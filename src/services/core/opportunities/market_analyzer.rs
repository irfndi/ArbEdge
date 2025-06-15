use crate::log_info;
use crate::services::core::infrastructure::data_ingestion_module::{
    DataIngestionModule, IngestionEvent, IngestionEventType, PipelineManager,
};
use crate::services::core::opportunities::opportunity_core::{
    ArbitrageAnalysis, MarketData, OpportunityConstants, OpportunityUtils, TechnicalAnalysis,
};
use crate::services::core::trading::exchange::{ExchangeInterface, ExchangeService};
use crate::services::CacheManager;
use crate::types::{
    ArbitrageOpportunity, ArbitrageType, ExchangeIdEnum, FundingRateInfo, TechnicalRiskLevel,
    TechnicalSignalStrength, TechnicalSignalType, Ticker,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::Utc;
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
// Removed unused serde imports
use serde::{Deserialize, Serialize};
use worker::console_log;

/// Enhanced technical signal with detailed analysis
#[derive(Debug, Clone)]
pub struct EnhancedTechnicalSignal {
    pub signal_type: TechnicalSignalType,
    pub signal_strength: TechnicalSignalStrength,
    pub confidence_score: f64,
    pub indicator_source: String,
    pub entry_price: f64,
    pub target_price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub metadata: HashMap<String, f64>,
}

/// Technical indicator results
#[derive(Debug, Clone)]
pub struct TechnicalIndicators {
    pub rsi: Option<f64>,
    pub ma_short: Option<f64>,
    pub ma_long: Option<f64>,
    pub bb_upper: Option<f64>,
    pub bb_lower: Option<f64>,
    pub bb_middle: Option<f64>,
    pub momentum: Option<f64>,
    pub volatility: Option<f64>,
}

/// Market data source priority for pipeline-first architecture
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataSource {
    Pipeline,  // Cloudflare Pipelines (highest priority)
    Cache,     // KV Cache (medium priority)
    DirectAPI, // Direct exchange API (lowest priority, causes uncached subrequests)
}

/// Market analyzer with pipeline-first data architecture
/// Reduces uncached subrequests by consuming data from pipelines before direct API calls
#[derive(Clone)]
pub struct MarketAnalyzer {
    exchange_service: Arc<ExchangeService>,
    cache_manager: Option<Arc<CacheManager>>,
    pipeline_manager: Option<Arc<PipelineManager>>,
    data_ingestion_module: Option<Arc<DataIngestionModule>>,
    supported_exchanges: Vec<ExchangeIdEnum>,
    pipeline_first_enabled: bool,
    // Technical analysis configuration
    pub rsi_period: usize,
    pub rsi_overbought: f64,
    pub rsi_oversold: f64,
    pub ma_short_period: usize,
    pub ma_long_period: usize,
    pub bb_period: usize,
    pub bb_std_dev: f64,
}

impl MarketAnalyzer {
    /// Create new MarketAnalyzer with pipeline-first architecture
    pub fn new(exchange_service: Arc<ExchangeService>) -> Self {
        Self {
            exchange_service,
            cache_manager: None,
            pipeline_manager: None,
            data_ingestion_module: None,
            supported_exchanges: vec![
                ExchangeIdEnum::Binance,
                ExchangeIdEnum::Bybit,
                ExchangeIdEnum::OKX,
                ExchangeIdEnum::Coinbase,
            ],
            pipeline_first_enabled: true,
            // Default technical analysis configuration
            rsi_period: 14,
            rsi_overbought: 70.0,
            rsi_oversold: 30.0,
            ma_short_period: 10,
            ma_long_period: 20,
            bb_period: 20,
            bb_std_dev: 2.0,
        }
    }

    /// Inject cache manager for cache-first patterns
    pub fn with_cache_manager(mut self, cache_manager: Arc<CacheManager>) -> Self {
        self.cache_manager = Some(cache_manager);
        self
    }

    /// Inject pipeline manager for pipeline-first patterns
    pub fn with_pipeline_manager(mut self, pipeline_manager: Arc<PipelineManager>) -> Self {
        self.pipeline_manager = Some(pipeline_manager);
        self
    }

    /// Inject data ingestion module for comprehensive pipeline integration
    pub fn with_data_ingestion_module(
        mut self,
        data_ingestion_module: Arc<DataIngestionModule>,
    ) -> Self {
        self.data_ingestion_module = Some(data_ingestion_module);
        self
    }

    /// Enable or disable pipeline-first architecture
    pub fn set_pipeline_first_enabled(mut self, enabled: bool) -> Self {
        self.pipeline_first_enabled = enabled;
        self
    }

    /// Get market data using pipeline-first architecture (REDUCES UNCACHED SUBREQUESTS)
    pub async fn get_market_data_pipeline_first(
        &self,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<(Ticker, DataSource)> {
        console_log!(
            "🔄 PIPELINE-FIRST DATA RETRIEVAL - {} {} (reducing uncached subrequests)",
            exchange.as_str(),
            symbol
        );

        // 1. Try Pipeline Data First (HIGHEST PRIORITY - NO UNCACHED SUBREQUESTS)
        if self.pipeline_first_enabled {
            if let Some(ticker) = self.get_from_pipeline(exchange, symbol).await? {
                console_log!("🎯 PIPELINE SUCCESS - {} {} from Cloudflare Pipelines (0 uncached subrequests)", 
                    exchange.as_str(), symbol);
                return Ok((ticker, DataSource::Pipeline));
            }
        }

        // 2. Try Cache Second (MEDIUM PRIORITY - NO UNCACHED SUBREQUESTS)
        if let Some(cache_manager) = &self.cache_manager {
            let cache_key = format!("market_data:{}:{}", exchange.as_str(), symbol);
            if let Ok(Some(cached_ticker)) = cache_manager.get::<Ticker>(&cache_key).await {
                console_log!(
                    "🎯 CACHE SUCCESS - {} {} from KV cache (0 uncached subrequests)",
                    exchange.as_str(),
                    symbol
                );
                return Ok((cached_ticker, DataSource::Cache));
            }
        }

        // 3. Fallback to Direct API (LOWEST PRIORITY - CAUSES UNCACHED SUBREQUESTS)
        console_log!(
            "⚠️ FALLBACK TO DIRECT API - {} {} (WILL CAUSE UNCACHED SUBREQUEST)",
            exchange.as_str(),
            symbol
        );

        let ticker = match exchange {
            ExchangeIdEnum::Binance => self.exchange_service.get_ticker("binance", symbol).await?,
            ExchangeIdEnum::Bybit => self.exchange_service.get_ticker("bybit", symbol).await?,
            ExchangeIdEnum::OKX => self.exchange_service.get_ticker("okx", symbol).await?,
            _ => {
                return Err(ArbitrageError::exchange_error(
                    exchange.as_str(),
                    "Exchange not supported for direct API calls".to_string(),
                ))
            }
        };

        // Cache the result for future requests
        if let Some(cache_manager) = &self.cache_manager {
            let cache_key = format!("market_data:{}:{}", exchange.as_str(), symbol);
            let _ = cache_manager.set(&cache_key, &ticker, Some(60)).await; // 1 minute TTL
        }

        Ok((ticker, DataSource::DirectAPI))
    }

    /// Get market data from Cloudflare Pipelines (NO UNCACHED SUBREQUESTS)
    async fn get_from_pipeline(
        &self,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<Option<Ticker>> {
        if let Some(pipeline_manager) = &self.pipeline_manager {
            // Check if pipelines are available
            if !pipeline_manager.is_pipelines_available().await {
                console_log!("⚠️ PIPELINES UNAVAILABLE - Skipping pipeline data retrieval");
                return Ok(None);
            }

            // Generate pipeline key for market data
            let pipeline_key = format!(
                "market-data/{}/{}/latest",
                exchange.as_str().to_lowercase(),
                symbol.to_uppercase()
            );

            // Try to get latest data from R2 storage via pipeline
            if let Ok(Some(data)) = pipeline_manager.get_latest_data(&pipeline_key).await {
                match serde_json::from_str::<Ticker>(&data) {
                    Ok(ticker) => {
                        console_log!(
                            "✅ PIPELINE DATA FOUND - {} {} from R2 storage",
                            exchange.as_str(),
                            symbol
                        );
                        return Ok(Some(ticker));
                    }
                    Err(e) => {
                        console_log!("⚠️ PIPELINE DATA PARSE ERROR - {}: {}", pipeline_key, e);
                    }
                }
            }
        }

        Ok(None)
    }

    /// Analyze price arbitrage opportunities using pipeline-first data
    pub async fn build_price_arbitrage(
        &self,
        symbol: &str,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        console_log!(
            "🔍 PRICE ARBITRAGE ANALYSIS - {} (pipeline-first approach)",
            symbol
        );

        let mut opportunities = Vec::new();
        let mut exchange_prices: HashMap<ExchangeIdEnum, f64> = HashMap::new();
        let mut data_sources: HashMap<ExchangeIdEnum, DataSource> = HashMap::new();

        // Collect prices from all supported exchanges using pipeline-first approach
        for exchange in &self.supported_exchanges {
            match self.get_market_data_pipeline_first(exchange, symbol).await {
                Ok((ticker, source)) => {
                    if let Some(price) = ticker.last.or(ticker.close) {
                        exchange_prices.insert(*exchange, price);
                        let source_clone = source.clone(); // Clone before moving
                        data_sources.insert(*exchange, source);
                        console_log!(
                            "📊 PRICE COLLECTED - {} {} = ${:.4} (source: {:?})",
                            exchange.as_str(),
                            symbol,
                            price,
                            source_clone
                        );
                    }
                }
                Err(e) => {
                    console_log!(
                        "❌ PRICE COLLECTION FAILED - {} {}: {}",
                        exchange.as_str(),
                        symbol,
                        e
                    );
                }
            }
        }

        // Find arbitrage opportunities
        let exchanges: Vec<_> = exchange_prices.keys().cloned().collect();
        for (i, long_exchange) in exchanges.iter().enumerate() {
            for short_exchange in exchanges.iter().skip(i + 1) {
                if let (Some(&long_price), Some(&short_price)) = (
                    exchange_prices.get(long_exchange),
                    exchange_prices.get(short_exchange),
                ) {
                    let price_diff = (short_price - long_price) / long_price * 100.0;

                    if price_diff.abs() > 0.1 {
                        // Minimum 0.1% difference
                        let (buy_exchange, sell_exchange, profit_pct) = if price_diff > 0.0 {
                            (*long_exchange, *short_exchange, price_diff)
                        } else {
                            (*short_exchange, *long_exchange, -price_diff)
                        };

                        // Calculate confidence based on data sources
                        let confidence = self.calculate_confidence_score(
                            data_sources
                                .get(&buy_exchange)
                                .unwrap_or(&DataSource::DirectAPI),
                            data_sources
                                .get(&sell_exchange)
                                .unwrap_or(&DataSource::DirectAPI),
                        );

                        let opportunity = ArbitrageOpportunity {
                            id: uuid::Uuid::new_v4().to_string(),
                            trading_pair: symbol.to_string(),
                            exchanges: vec![
                                buy_exchange.as_str().to_string(),
                                sell_exchange.as_str().to_string(),
                            ],
                            profit_percentage: profit_pct,
                            confidence_score: confidence,
                            risk_level: if profit_pct > 1.0 {
                                "medium".to_string()
                            } else {
                                "low".to_string()
                            },
                            buy_exchange: buy_exchange.as_str().to_string(),
                            sell_exchange: sell_exchange.as_str().to_string(),
                            buy_price: exchange_prices[&buy_exchange],
                            sell_price: exchange_prices[&sell_exchange],
                            volume: 1000.0, // Default volume
                            created_at: chrono::Utc::now().timestamp_millis() as u64,
                            expires_at: Some(
                                chrono::Utc::now().timestamp_millis() as u64 + 300_000,
                            ), // 5 minutes
                            pair: symbol.to_string(),
                            long_exchange: buy_exchange,
                            short_exchange: sell_exchange,
                            long_rate: Some(exchange_prices[&buy_exchange]),
                            short_rate: Some(exchange_prices[&sell_exchange]),
                            rate_difference: profit_pct,
                            net_rate_difference: Some(profit_pct),
                            potential_profit_value: Some(profit_pct * 10.0), // Assume $1000 position
                            timestamp: chrono::Utc::now().timestamp_millis() as u64,
                            detected_at: chrono::Utc::now().timestamp_millis() as u64,
                            r#type: ArbitrageType::Price,
                            details: Some(format!(
                                "Pipeline-first analysis: Buy at ${:.4}, Sell at ${:.4}",
                                exchange_prices[&buy_exchange], exchange_prices[&sell_exchange]
                            )),
                            min_exchanges_required: 2,
                        };

                        opportunities.push(opportunity);
                        console_log!("💰 ARBITRAGE OPPORTUNITY - {} {:.2}% profit (Buy: {} ${:.4}, Sell: {} ${:.4})", 
                            symbol, profit_pct, buy_exchange.as_str(), exchange_prices[&buy_exchange],
                            sell_exchange.as_str(), exchange_prices[&sell_exchange]);
                    }
                }
            }
        }

        // Ingest analysis results into pipeline for future reference
        if let Some(data_ingestion) = &self.data_ingestion_module {
            let analysis_event = IngestionEvent::new(
                IngestionEventType::Analytics,
                "market_analyzer".to_string(),
                serde_json::json!({
                    "symbol": symbol,
                    "opportunities_found": opportunities.len(),
                    "exchanges_analyzed": exchange_prices.len(),
                    "data_sources": data_sources,
                    "timestamp": chrono::Utc::now().timestamp_millis()
                }),
            );

            let _ = data_ingestion.ingest_event(analysis_event).await;
        }

        console_log!(
            "✅ PRICE ARBITRAGE COMPLETE - {} opportunities found for {}",
            opportunities.len(),
            symbol
        );
        Ok(opportunities)
    }

    /// Calculate confidence score based on data sources
    fn calculate_confidence_score(&self, source1: &DataSource, source2: &DataSource) -> f64 {
        let score1 = match source1 {
            DataSource::Pipeline => 0.95,  // Highest confidence - pipeline data
            DataSource::Cache => 0.85,     // Medium confidence - cached data
            DataSource::DirectAPI => 0.75, // Lower confidence - direct API (may be stale)
        };

        let score2 = match source2 {
            DataSource::Pipeline => 0.95,
            DataSource::Cache => 0.85,
            DataSource::DirectAPI => 0.75,
        };

        // Average the scores and add small random variation for realism
        let base_score = (score1 + score2) / 2.0;
        let variation = (chrono::Utc::now().timestamp_millis() % 100) as f64 / 1000.0; // 0-0.099
        (base_score + variation).min(0.99)
    }

    /// Get supported exchanges
    pub fn get_supported_exchanges(&self) -> &[ExchangeIdEnum] {
        &self.supported_exchanges
    }

    /// Check if pipeline-first mode is enabled
    pub fn is_pipeline_first_enabled(&self) -> bool {
        self.pipeline_first_enabled
    }

    /// Get a reference to the exchange service for dynamic symbol discovery
    pub fn get_exchange_service(&self) -> Arc<ExchangeService> {
        Arc::clone(&self.exchange_service)
    }

    /// Fetch market data for multiple symbols across multiple exchanges
    pub async fn fetch_market_data(
        &self,
        symbols: &[String],
        exchanges: &[ExchangeIdEnum],
        user_id: &str,
    ) -> ArbitrageResult<HashMap<String, MarketData>> {
        let mut market_data = HashMap::new();

        for symbol in symbols {
            let mut exchange_tickers = HashMap::new();
            let mut funding_rates = HashMap::new();

            // Fetch tickers and funding rates concurrently
            let mut ticker_tasks = Vec::new();
            let mut funding_tasks = Vec::new();

            for exchange_id in exchanges {
                // Ticker task
                let exchange_service = Arc::clone(&self.exchange_service);
                let symbol_clone = symbol.clone();
                let exchange_id_clone = *exchange_id;
                let user_id_clone = user_id.to_string();

                let ticker_task = Box::pin(async move {
                    let result = exchange_service
                        .get_ticker(&exchange_id_clone.to_string(), &symbol_clone)
                        .await;
                    (
                        exchange_id_clone,
                        symbol_clone.clone(),
                        result,
                        user_id_clone,
                    )
                });
                ticker_tasks.push(ticker_task);

                // Funding rate task
                let exchange_service = Arc::clone(&self.exchange_service);
                let symbol_clone = symbol.clone();
                let exchange_id_clone = *exchange_id;

                let funding_task = Box::pin(async move {
                    let result = exchange_service
                        .fetch_funding_rates(&exchange_id_clone.to_string(), Some(&symbol_clone))
                        .await;
                    (exchange_id_clone, symbol_clone, result)
                });
                funding_tasks.push(funding_task);
            }

            // Execute ticker tasks
            let ticker_results = join_all(ticker_tasks).await;
            for (exchange_id, symbol_name, result, user_id_ref) in ticker_results {
                match result {
                    Ok(ticker) => {
                        exchange_tickers.insert(exchange_id, ticker);
                    }
                    Err(e) => {
                        log_info!(
                            "Failed to fetch ticker",
                            serde_json::json!({
                                "user_id": user_id_ref,
                                "exchange": format!("{:?}", exchange_id),
                                "symbol": symbol_name,
                                "error": e.to_string()
                            })
                        );
                    }
                }
            }

            // Execute funding rate tasks
            let funding_results = join_all(funding_tasks).await;
            for (exchange_id, symbol_name, result) in funding_results {
                let funding_info = match result {
                    Ok(rates) => {
                        if let Some(rate_data) = rates.first() {
                            rate_data.get("fundingRate").and_then(|v| v.as_f64()).map(
                                |funding_rate| FundingRateInfo {
                                    symbol: symbol_name.clone(),
                                    funding_rate,
                                    timestamp: Utc::now().timestamp_millis() as u64,
                                    datetime: Utc::now().to_rfc3339(),
                                    next_funding_time: rate_data
                                        .get("fundingTime")
                                        .and_then(|v| v.as_u64()),
                                    estimated_rate: rate_data
                                        .get("markPrice")
                                        .and_then(|v| v.as_f64()),
                                    info: serde_json::json!({}),
                                    estimated_settle_price: rate_data
                                        .get("settlePrice")
                                        .and_then(|v| v.as_f64()),
                                    exchange: exchange_id,
                                    funding_interval_hours: 8, // Default 8 hours for most exchanges
                                    mark_price: rate_data.get("markPrice").and_then(|v| v.as_f64()),
                                    index_price: rate_data
                                        .get("indexPrice")
                                        .and_then(|v| v.as_f64()),
                                    funding_countdown: rate_data
                                        .get("fundingCountdown")
                                        .and_then(|v| v.as_u64()),
                                },
                            )
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                };
                funding_rates.insert(exchange_id, funding_info);
            }

            market_data.insert(
                symbol.clone(),
                MarketData {
                    symbol: symbol.clone(),
                    exchange_tickers,
                    funding_rates,
                    timestamp: Utc::now().timestamp_millis() as u64,
                },
            );
        }

        Ok(market_data)
    }

    /// Perform technical analysis on market data
    pub fn analyze_technical_signal(
        &self,
        ticker: &Ticker,
        funding_rate: &Option<FundingRateInfo>,
    ) -> TechnicalAnalysis {
        let price_change_percent = OpportunityUtils::calculate_price_change_percent(ticker);
        let last_price = ticker.last.unwrap_or(0.0);
        let volume_24h = ticker.volume.unwrap_or(0.0);

        // Determine signal based on funding rate and price momentum
        let signal = self.determine_technical_signal(price_change_percent, funding_rate);

        // Calculate confidence based on multiple factors
        let confidence = OpportunityUtils::calculate_base_confidence(
            volume_24h,
            price_change_percent,
            funding_rate.as_ref().map(|fr| fr.funding_rate),
        );

        // Calculate target and stop loss prices
        let target_price = self.calculate_target_price(last_price, &signal);
        let stop_loss = self.calculate_stop_loss(last_price, &signal);

        // Calculate expected return
        let expected_return = self.calculate_expected_return(price_change_percent);

        // Assess risk level
        let risk_level = self.assess_risk_level(price_change_percent, volume_24h);

        // Analyze market conditions
        let market_conditions =
            self.analyze_market_conditions(price_change_percent, volume_24h, funding_rate);

        TechnicalAnalysis {
            signal,
            confidence,
            target_price,
            stop_loss,
            expected_return,
            risk_level,
            market_conditions,
        }
    }

    /// Analyze arbitrage opportunity between two exchanges
    pub fn analyze_arbitrage_opportunity(
        &self,
        _symbol: &str,
        ticker_a: &Ticker,
        ticker_b: &Ticker,
        exchange_a: &ExchangeIdEnum,
        exchange_b: &ExchangeIdEnum,
    ) -> ArbitrageResult<ArbitrageAnalysis> {
        let price_a = ticker_a.last.unwrap_or(0.0);
        let price_b = ticker_b.last.unwrap_or(0.0);

        if price_a <= 0.0 || price_b <= 0.0 {
            return Err(ArbitrageError::validation_error(
                "Invalid ticker prices for arbitrage analysis".to_string(),
            ));
        }

        let price_difference = (price_b - price_a).abs();
        let price_difference_percent =
            OpportunityUtils::calculate_price_difference_percent(price_a, price_b);

        // Log arbitrage calculation for debugging
        log::debug!(
            "💰 ARBITRAGE DEBUG - {:?} vs {:?}: Price A: ${:.2}, Price B: ${:.2}, Diff: ${:.2}, Diff%: {:.4}%, Threshold: 0.1%",
            exchange_a,
            exchange_b,
            price_a,
            price_b,
            price_difference,
            price_difference_percent
        );

        // Check if arbitrage is significant
        if !OpportunityUtils::is_arbitrage_significant(price_difference_percent) {
            log::warn!(
                "❌ ARBITRAGE REJECTED - Rate difference {:.4}% below minimum threshold 0.1000%",
                price_difference_percent
            );
            return Err(ArbitrageError::validation_error(format!(
                "Rate difference {:.4}% below minimum threshold 0.1000%",
                price_difference_percent
            )));
        }

        log::info!(
            "✅ ARBITRAGE ACCEPTED - Rate difference {:.4}% above threshold",
            price_difference_percent
        );

        // Determine buy/sell exchanges
        let (buy_exchange, sell_exchange) = if price_a < price_b {
            (*exchange_a, *exchange_b)
        } else {
            (*exchange_b, *exchange_a)
        };

        // Calculate confidence based on price difference
        let confidence = self.calculate_arbitrage_confidence(price_difference_percent);

        // Identify risk factors
        let risk_factors = self.identify_arbitrage_risk_factors(ticker_a, ticker_b);

        // Calculate liquidity score
        let liquidity_score = self.calculate_liquidity_score(ticker_a, ticker_b);

        Ok(ArbitrageAnalysis {
            buy_exchange,
            sell_exchange,
            price_difference,
            price_difference_percent,
            confidence,
            risk_factors,
            liquidity_score,
        })
    }

    /// Convert technical analysis to signal type enum
    pub fn signal_to_enum(&self, signal: &str) -> TechnicalSignalType {
        match signal.to_uppercase().as_str() {
            "LONG" | "BUY" => TechnicalSignalType::Buy,
            "SHORT" | "SELL" => TechnicalSignalType::Sell,
            _ => TechnicalSignalType::Hold,
        }
    }

    /// Convert risk level to enum
    pub fn risk_to_enum(&self, risk_level: &str) -> TechnicalRiskLevel {
        match risk_level.to_uppercase().as_str() {
            "HIGH" => TechnicalRiskLevel::High,
            "MEDIUM" => TechnicalRiskLevel::Medium,
            _ => TechnicalRiskLevel::Low,
        }
    }

    /// Determine signal strength based on confidence
    pub fn determine_signal_strength(&self, confidence: f64) -> TechnicalSignalStrength {
        if confidence > 0.8 {
            TechnicalSignalStrength::Strong
        } else if confidence > 0.6 {
            TechnicalSignalStrength::Moderate
        } else {
            TechnicalSignalStrength::Weak
        }
    }

    /// Calculate comprehensive technical indicators for price data
    pub fn calculate_technical_indicators(&self, prices: &[f64]) -> TechnicalIndicators {
        TechnicalIndicators {
            rsi: self.calculate_rsi(prices),
            ma_short: self.calculate_moving_average(prices, self.ma_short_period),
            ma_long: self.calculate_moving_average(prices, self.ma_long_period),
            bb_upper: self
                .calculate_bollinger_bands(prices)
                .map(|(_, upper, _)| upper),
            bb_lower: self
                .calculate_bollinger_bands(prices)
                .map(|(lower, _, _)| lower),
            bb_middle: self
                .calculate_bollinger_bands(prices)
                .map(|(_, _, middle)| middle),
            momentum: self.calculate_momentum(prices),
            volatility: self.calculate_volatility(prices),
        }
    }

    /// Generate enhanced technical signals with multiple indicators
    pub fn generate_enhanced_technical_signals(
        &self,
        prices: &[f64],
        current_price: f64,
        symbol: &str,
        exchange: &str,
    ) -> ArbitrageResult<Vec<EnhancedTechnicalSignal>> {
        let mut signals = Vec::new();

        if prices.len() < self.bb_period.max(self.ma_long_period).max(self.rsi_period) {
            return Ok(signals); // Not enough data
        }

        let indicators = self.calculate_technical_indicators(prices);

        // RSI signals
        if let Some(rsi) = indicators.rsi {
            if let Some(signal) = self.generate_rsi_signal(rsi, current_price, symbol, exchange) {
                signals.push(signal);
            }
        }

        // Moving average crossover signals
        if let (Some(ma_short), Some(ma_long)) = (indicators.ma_short, indicators.ma_long) {
            if let Some(signal) = self.generate_ma_crossover_signal(
                ma_short,
                ma_long,
                current_price,
                symbol,
                exchange,
            ) {
                signals.push(signal);
            }
        }

        // Bollinger Bands signals
        if let (Some(bb_upper), Some(bb_lower)) = (indicators.bb_upper, indicators.bb_lower) {
            if let Some(signal) =
                self.generate_bollinger_signal(current_price, bb_upper, bb_lower, symbol, exchange)
            {
                signals.push(signal);
            }
        }

        // Momentum signals
        if let Some(momentum) = indicators.momentum {
            if let Some(signal) =
                self.generate_momentum_signal(momentum, current_price, symbol, exchange)
            {
                signals.push(signal);
            }
        }

        Ok(signals)
    }

    /// Calculate RSI (Relative Strength Index)
    fn calculate_rsi(&self, prices: &[f64]) -> Option<f64> {
        if prices.len() < self.rsi_period + 1 {
            return None;
        }

        let mut gains = Vec::new();
        let mut losses = Vec::new();

        for i in 1..prices.len() {
            let change = prices[i] - prices[i - 1];
            if change > 0.0 {
                gains.push(change);
                losses.push(0.0);
            } else {
                gains.push(0.0);
                losses.push(-change);
            }
        }

        if gains.len() < self.rsi_period {
            return None;
        }

        let avg_gain: f64 =
            gains.iter().take(self.rsi_period).sum::<f64>() / self.rsi_period as f64;
        let avg_loss: f64 =
            losses.iter().take(self.rsi_period).sum::<f64>() / self.rsi_period as f64;

        if avg_loss == 0.0 {
            return Some(100.0);
        }

        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }

    /// Calculate Simple Moving Average
    fn calculate_moving_average(&self, prices: &[f64], period: usize) -> Option<f64> {
        if prices.len() < period {
            return None;
        }

        let sum: f64 = prices.iter().rev().take(period).sum();
        Some(sum / period as f64)
    }

    /// Calculate Bollinger Bands (lower, upper, middle)
    fn calculate_bollinger_bands(&self, prices: &[f64]) -> Option<(f64, f64, f64)> {
        if prices.len() < self.bb_period {
            return None;
        }

        let recent_prices: Vec<f64> = prices.iter().rev().take(self.bb_period).cloned().collect();
        let middle: f64 = recent_prices.iter().sum::<f64>() / self.bb_period as f64;

        let variance: f64 = recent_prices
            .iter()
            .map(|price| (price - middle).powi(2))
            .sum::<f64>()
            / self.bb_period as f64;

        let std_dev = variance.sqrt();
        let upper = middle + (self.bb_std_dev * std_dev);
        let lower = middle - (self.bb_std_dev * std_dev);

        Some((lower, upper, middle))
    }

    /// Calculate momentum indicator
    fn calculate_momentum(&self, prices: &[f64]) -> Option<f64> {
        if prices.len() < 10 {
            return None;
        }

        let current = prices[prices.len() - 1];
        let previous = prices[prices.len() - 10];

        if previous != 0.0 {
            Some((current - previous) / previous)
        } else {
            None
        }
    }

    /// Calculate price volatility
    fn calculate_volatility(&self, prices: &[f64]) -> Option<f64> {
        if prices.len() < 20 {
            return None;
        }

        let recent_prices: Vec<f64> = prices.iter().rev().take(20).cloned().collect();
        let mean: f64 = recent_prices.iter().sum::<f64>() / recent_prices.len() as f64;

        let variance: f64 = recent_prices
            .iter()
            .map(|price| (price - mean).powi(2))
            .sum::<f64>()
            / recent_prices.len() as f64;

        Some(variance.sqrt() / mean)
    }

    // Signal generation methods

    fn generate_rsi_signal(
        &self,
        rsi: f64,
        current_price: f64,
        _symbol: &str,
        _exchange: &str,
    ) -> Option<EnhancedTechnicalSignal> {
        let (signal_type, signal_strength) = if rsi > self.rsi_overbought {
            (TechnicalSignalType::Sell, TechnicalSignalStrength::Moderate)
        } else if rsi < self.rsi_oversold {
            (TechnicalSignalType::Buy, TechnicalSignalStrength::Moderate)
        } else {
            (TechnicalSignalType::Hold, TechnicalSignalStrength::Weak)
        };

        let rsi_confidence = if !(30.0..=70.0).contains(&rsi) {
            0.8 // Strong signal when RSI is outside normal range
        } else if !(40.0..=60.0).contains(&rsi) {
            0.7 // Medium confidence for moderate RSI values
        } else {
            0.4 // Low confidence for neutral RSI values
        };

        let (target_price, stop_loss_price) =
            self.calculate_price_targets(current_price, &signal_type);

        let mut metadata = HashMap::new();
        metadata.insert("rsi_value".to_string(), rsi);
        metadata.insert(
            "rsi_threshold".to_string(),
            if matches!(signal_type, TechnicalSignalType::Buy) {
                self.rsi_oversold
            } else {
                self.rsi_overbought
            },
        );

        Some(EnhancedTechnicalSignal {
            signal_type,
            signal_strength,
            confidence_score: rsi_confidence,
            indicator_source: "RSI".to_string(),
            entry_price: current_price,
            target_price,
            stop_loss: stop_loss_price,
            metadata,
        })
    }

    fn generate_ma_crossover_signal(
        &self,
        ma_short: f64,
        ma_long: f64,
        current_price: f64,
        _symbol: &str,
        _exchange: &str,
    ) -> Option<EnhancedTechnicalSignal> {
        let crossover_strength = ((ma_short - ma_long) / ma_long).abs();

        if crossover_strength < 0.01 {
            // Less than 1% difference
            return None;
        }

        let signal_type = if ma_short > ma_long {
            TechnicalSignalType::Buy
        } else {
            TechnicalSignalType::Sell
        };

        let confidence = (crossover_strength * 10.0).min(1.0);
        let signal_strength = if confidence > 0.7 {
            TechnicalSignalStrength::Strong
        } else if confidence > 0.4 {
            TechnicalSignalStrength::Moderate
        } else {
            TechnicalSignalStrength::Weak
        };

        let (target_price, stop_loss) = self.calculate_price_targets(current_price, &signal_type);

        let mut metadata = HashMap::new();
        metadata.insert("ma_short".to_string(), ma_short);
        metadata.insert("ma_long".to_string(), ma_long);
        metadata.insert("crossover_strength".to_string(), crossover_strength);

        Some(EnhancedTechnicalSignal {
            signal_type,
            signal_strength,
            confidence_score: confidence,
            indicator_source: "MA_Crossover".to_string(),
            entry_price: current_price,
            target_price,
            stop_loss,
            metadata,
        })
    }

    fn generate_bollinger_signal(
        &self,
        current_price: f64,
        bb_upper: f64,
        bb_lower: f64,
        _symbol: &str,
        _exchange: &str,
    ) -> Option<EnhancedTechnicalSignal> {
        let bb_width = bb_upper - bb_lower;
        let bb_middle = (bb_upper + bb_lower) / 2.0;

        let (signal_type, signal_strength) = if current_price > bb_upper {
            let _overshoot = (current_price - bb_upper) / bb_width;
            (TechnicalSignalType::Sell, TechnicalSignalStrength::Strong)
        } else if current_price < bb_lower {
            let _undershoot = (bb_lower - current_price) / bb_width;
            (TechnicalSignalType::Buy, TechnicalSignalStrength::Strong)
        } else {
            return None;
        };

        let signal_strength = if signal_strength == TechnicalSignalStrength::Strong {
            TechnicalSignalStrength::Strong
        } else {
            TechnicalSignalStrength::Moderate
        };

        let (target_price, stop_loss) = self.calculate_price_targets(current_price, &signal_type);

        let mut metadata = HashMap::new();
        metadata.insert("bb_upper".to_string(), bb_upper);
        metadata.insert("bb_lower".to_string(), bb_lower);
        metadata.insert("bb_middle".to_string(), bb_middle);
        metadata.insert(
            "bb_position".to_string(),
            (current_price - bb_lower) / bb_width,
        );

        Some(EnhancedTechnicalSignal {
            signal_type,
            signal_strength,
            confidence_score: 1.0,
            indicator_source: "Bollinger_Bands".to_string(),
            entry_price: current_price,
            target_price,
            stop_loss,
            metadata,
        })
    }

    fn generate_momentum_signal(
        &self,
        momentum: f64,
        current_price: f64,
        _symbol: &str,
        _exchange: &str,
    ) -> Option<EnhancedTechnicalSignal> {
        let momentum_threshold = 0.02; // 2% momentum threshold

        if momentum.abs() < momentum_threshold {
            return None;
        }

        let signal_type = if momentum > 0.0 {
            TechnicalSignalType::Buy
        } else {
            TechnicalSignalType::Sell
        };

        let confidence = (momentum.abs() / 0.1).min(1.0); // Normalize to 10% momentum
        let signal_strength = if confidence > 0.7 {
            TechnicalSignalStrength::Strong
        } else if confidence > 0.4 {
            TechnicalSignalStrength::Moderate
        } else {
            TechnicalSignalStrength::Weak
        };

        let (target_price, stop_loss) = self.calculate_price_targets(current_price, &signal_type);

        let mut metadata = HashMap::new();
        metadata.insert("momentum".to_string(), momentum);
        metadata.insert("momentum_threshold".to_string(), momentum_threshold);

        Some(EnhancedTechnicalSignal {
            signal_type,
            signal_strength,
            confidence_score: confidence,
            indicator_source: "Momentum".to_string(),
            entry_price: current_price,
            target_price,
            stop_loss,
            metadata,
        })
    }

    fn calculate_price_targets(
        &self,
        entry_price: f64,
        signal_type: &TechnicalSignalType,
    ) -> (Option<f64>, Option<f64>) {
        let target_percentage = 0.02; // 2% target
        let stop_loss_percentage = 0.01; // 1% stop loss

        match signal_type {
            TechnicalSignalType::Buy => {
                let target = entry_price * (1.0 + target_percentage);
                let stop_loss = entry_price * (1.0 - stop_loss_percentage);
                (Some(target), Some(stop_loss))
            }
            TechnicalSignalType::Sell => {
                let target = entry_price * (1.0 - target_percentage);
                let stop_loss = entry_price * (1.0 + stop_loss_percentage);
                (Some(target), Some(stop_loss))
            }
            TechnicalSignalType::Hold => (None, None),
            // Handle all other technical signal types with default behavior
            _ => {
                let target = entry_price * (1.0 + target_percentage);
                let stop_loss = entry_price * (1.0 - stop_loss_percentage);
                (Some(target), Some(stop_loss))
            }
        }
    }

    // Private helper methods

    fn determine_technical_signal(
        &self,
        price_change_percent: f64,
        funding_rate: &Option<FundingRateInfo>,
    ) -> String {
        // Check funding rate first (higher priority)
        if let Some(fr) = funding_rate {
            if fr.funding_rate > OpportunityConstants::FUNDING_RATE_THRESHOLD {
                return "SHORT".to_string();
            } else if fr.funding_rate < -OpportunityConstants::FUNDING_RATE_THRESHOLD {
                return "LONG".to_string();
            }
        }

        // Use price momentum as fallback
        if price_change_percent > OpportunityConstants::PRICE_MOMENTUM_THRESHOLD {
            "LONG".to_string()
        } else if price_change_percent < -OpportunityConstants::PRICE_MOMENTUM_THRESHOLD {
            "SHORT".to_string()
        } else {
            "NEUTRAL".to_string()
        }
    }

    fn calculate_target_price(&self, last_price: f64, signal: &str) -> f64 {
        match signal.to_uppercase().as_str() {
            "LONG" | "BUY" => last_price * 1.02,   // 2% above for long
            "SHORT" | "SELL" => last_price * 0.98, // 2% below for short
            _ => last_price,                       // No change for neutral
        }
    }

    fn calculate_stop_loss(&self, last_price: f64, signal: &str) -> f64 {
        match signal.to_uppercase().as_str() {
            "LONG" | "BUY" => last_price * 0.99,   // 1% below for long
            "SHORT" | "SELL" => last_price * 1.01, // 1% above for short
            _ => last_price,                       // No change for neutral
        }
    }

    fn calculate_expected_return(&self, price_change_percent: f64) -> f64 {
        // Conservative estimate: half of the recent price movement
        price_change_percent.abs() * 0.5
    }

    fn assess_risk_level(&self, price_change_percent: f64, volume: f64) -> String {
        let volatility_risk = price_change_percent.abs() > 5.0;
        let liquidity_risk = volume < OpportunityConstants::MIN_VOLUME_THRESHOLD;

        if volatility_risk || liquidity_risk {
            "HIGH".to_string()
        } else if price_change_percent.abs() > OpportunityConstants::PRICE_MOMENTUM_THRESHOLD {
            "MEDIUM".to_string()
        } else {
            "LOW".to_string()
        }
    }

    fn analyze_market_conditions(
        &self,
        price_change_percent: f64,
        volume: f64,
        funding_rate: &Option<FundingRateInfo>,
    ) -> String {
        let mut conditions = Vec::new();

        // Price momentum conditions
        if price_change_percent > OpportunityConstants::PRICE_MOMENTUM_THRESHOLD {
            conditions.push("Bullish momentum");
        } else if price_change_percent < -OpportunityConstants::PRICE_MOMENTUM_THRESHOLD {
            conditions.push("Bearish momentum");
        }

        // Funding rate conditions
        if let Some(fr) = funding_rate {
            if fr.funding_rate > OpportunityConstants::FUNDING_RATE_THRESHOLD {
                conditions.push("High funding rate");
            } else if fr.funding_rate < -OpportunityConstants::FUNDING_RATE_THRESHOLD {
                conditions.push("Negative funding rate");
            }
        }

        // Volume conditions
        if volume > OpportunityConstants::HIGH_VOLUME_THRESHOLD {
            conditions.push("High volume");
        } else if volume < OpportunityConstants::MIN_VOLUME_THRESHOLD {
            conditions.push("Low volume");
        }

        if conditions.is_empty() {
            "Neutral market conditions".to_string()
        } else {
            conditions.join(", ")
        }
    }

    fn calculate_arbitrage_confidence(&self, price_diff_percent: f64) -> f64 {
        // Higher price difference = higher confidence, capped at 1.0
        (price_diff_percent / 5.0).min(1.0)
    }

    fn identify_arbitrage_risk_factors(&self, ticker_a: &Ticker, ticker_b: &Ticker) -> Vec<String> {
        let mut risks = Vec::new();

        let volume_a = ticker_a.volume.unwrap_or(0.0);
        let volume_b = ticker_b.volume.unwrap_or(0.0);

        // Low liquidity risk
        if !OpportunityUtils::is_volume_sufficient(volume_a)
            || !OpportunityUtils::is_volume_sufficient(volume_b)
        {
            risks.push("Low liquidity".to_string());
        }

        // Volatility divergence risk
        let change_a = OpportunityUtils::calculate_price_change_percent(ticker_a);
        let change_b = OpportunityUtils::calculate_price_change_percent(ticker_b);
        if (change_a - change_b).abs() > 5.0 {
            risks.push("High volatility divergence".to_string());
        }

        // Spread risk
        let spread_a = ticker_a.ask.unwrap_or(0.0) - ticker_a.bid.unwrap_or(0.0);
        let spread_b = ticker_b.ask.unwrap_or(0.0) - ticker_b.bid.unwrap_or(0.0);
        let avg_price_a = (ticker_a.ask.unwrap_or(0.0) + ticker_a.bid.unwrap_or(0.0)) / 2.0;
        let avg_price_b = (ticker_b.ask.unwrap_or(0.0) + ticker_b.bid.unwrap_or(0.0)) / 2.0;

        if avg_price_a > 0.0 && avg_price_b > 0.0 {
            let spread_percent_a = (spread_a / avg_price_a) * 100.0;
            let spread_percent_b = (spread_b / avg_price_b) * 100.0;
            if spread_percent_a > 0.5 || spread_percent_b > 0.5 {
                risks.push("Wide bid-ask spread".to_string());
            }
        }

        risks
    }

    fn calculate_liquidity_score(&self, ticker_a: &Ticker, ticker_b: &Ticker) -> f64 {
        let volume_a = ticker_a.volume.unwrap_or(0.0);
        let volume_b = ticker_b.volume.unwrap_or(0.0);
        let avg_volume = (volume_a + volume_b) / 2.0;

        // Normalize to 0-1 scale based on high volume threshold
        (avg_volume / OpportunityConstants::HIGH_VOLUME_THRESHOLD).min(1.0)
    }
}

/// Technical signal data structure
#[derive(Debug, Clone)]
pub struct TechnicalSignalData {
    pub pair: String,
    pub exchange: ExchangeIdEnum,
    pub signal_type: TechnicalSignalType,
    pub signal_strength: TechnicalSignalStrength,
    pub confidence_score: f64,
    pub entry_price: f64,
    pub target_price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub technical_indicators: Vec<String>,
    pub timeframe: String,
    pub expected_return_percentage: f64,
    pub market_conditions: String,
}
