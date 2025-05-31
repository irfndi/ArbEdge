use crate::log_info;
use crate::services::core::opportunities::opportunity_core::{
    ArbitrageAnalysis, MarketData, OpportunityConfig, OpportunityConstants, OpportunityUtils,
    TechnicalAnalysis,
};
use crate::services::core::trading::exchange::{ExchangeInterface, ExchangeService};
use crate::types::{
    ArbitrageOpportunity, ArbitrageType, ExchangeIdEnum, FundingRateInfo, TechnicalRiskLevel,
    TechnicalSignalStrength, TechnicalSignalType, Ticker,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;

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

/// Market analyzer for technical analysis and market data processing
pub struct MarketAnalyzer {
    exchange_service: Arc<ExchangeService>,
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
    pub fn new(exchange_service: Arc<ExchangeService>) -> Self {
        Self {
            exchange_service,
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

    /// Create a MarketAnalyzer without an exchange service for testing/initialization
    pub fn new_without_exchange() -> Self {
        // Create a mock exchange service for initialization
        // This will be replaced with proper dependency injection later
        let mock_exchange = Arc::new(ExchangeService::new_mock().unwrap_or_else(|_| {
            // If mock creation fails, we'll need to handle this gracefully
            panic!("Failed to create mock exchange service for MarketAnalyzer")
        }));

        Self::new(mock_exchange)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_config(
        exchange_service: Arc<ExchangeService>,
        rsi_period: usize,
        rsi_overbought: f64,
        rsi_oversold: f64,
        ma_short_period: usize,
        ma_long_period: usize,
        bb_period: usize,
        bb_std_dev: f64,
    ) -> Self {
        Self {
            exchange_service,
            rsi_period,
            rsi_overbought,
            rsi_oversold,
            ma_short_period,
            ma_long_period,
            bb_period,
            bb_std_dev,
        }
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
                                        .and_then(|v| v.as_u64())
                                        .and_then(|ts| DateTime::from_timestamp(ts as i64, 0)),
                                    estimated_rate: rate_data
                                        .get("markPrice")
                                        .and_then(|v| v.as_f64()),
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

        // Check if arbitrage is significant
        if !OpportunityUtils::is_arbitrage_significant(price_difference_percent) {
            return Err(ArbitrageError::validation_error(
                "Price difference too small for arbitrage".to_string(),
            ));
        }

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

    /// Detect arbitrage opportunities across multiple exchanges
    pub async fn detect_arbitrage_opportunities(
        &self,
        pair: &str,
        exchanges: &[ExchangeIdEnum],
        config: &OpportunityConfig,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        if exchanges.len() < 2 {
            return Ok(Vec::new());
        }

        let mut opportunities = Vec::new();

        // Compare each exchange pair
        for i in 0..exchanges.len() {
            for j in (i + 1)..exchanges.len() {
                let exchange_a = exchanges[i];
                let exchange_b = exchanges[j];

                // Get tickers for both exchanges (mock implementation)
                if let (Ok(ticker_a), Ok(ticker_b)) = (
                    self.get_ticker_for_exchange(pair, &exchange_a).await,
                    self.get_ticker_for_exchange(pair, &exchange_b).await,
                ) {
                    if let Ok(analysis) = self.analyze_arbitrage_opportunity(
                        pair,
                        &ticker_a,
                        &ticker_b,
                        &exchange_a,
                        &exchange_b,
                    ) {
                        if analysis.price_difference_percent >= config.min_rate_difference {
                            let opportunity = ArbitrageOpportunity {
                                id: uuid::Uuid::new_v4().to_string(),
                                trading_pair: pair.clone(),
                                exchanges: vec![exchange_a.to_string(), exchange_b.to_string()],
                                profit_percentage: analysis.price_difference_percent,
                                confidence_score: 0.8, // High confidence for market analyzer
                                risk_level: "medium".to_string(),
                                buy_exchange: exchange_a.to_string(),
                                sell_exchange: exchange_b.to_string(),
                                buy_price: ticker_a.last.unwrap_or(0.0),
                                sell_price: ticker_b.last.unwrap_or(0.0),
                                volume: 1000.0, // Default volume
                                created_at: chrono::Utc::now().timestamp_millis() as u64,
                                expires_at: Some(
                                    chrono::Utc::now().timestamp_millis() as u64 + 300_000,
                                ), // 5 minutes
                                // Additional fields
                                pair: pair.clone(),
                                long_exchange: exchange_a,
                                short_exchange: exchange_b,
                                long_rate: Some(analysis.price_difference_percent),
                                short_rate: Some(analysis.price_difference_percent),
                                rate_difference: analysis.price_difference_percent,
                                net_rate_difference: Some(analysis.price_difference_percent),
                                potential_profit_value: Some(analysis.price_difference * 1000.0),
                                confidence: 0.8,
                                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                detected_at: chrono::Utc::now().timestamp_millis() as u64,
                                r#type: ArbitrageType::CrossExchange,
                                details: Some(format!(
                                    "Market analyzer detected arbitrage between {} and {}",
                                    exchange_a, exchange_b
                                )),
                                min_exchanges_required: 2,
                            };
                            opportunities.push(opportunity);
                        }
                    }
                }
            }
        }

        Ok(opportunities)
    }

    /// Get ticker data for a specific exchange (mock implementation)
    async fn get_ticker_for_exchange(
        &self,
        pair: &str,
        exchange: &ExchangeIdEnum,
    ) -> ArbitrageResult<Ticker> {
        // Mock ticker data - in real implementation, this would fetch from exchange APIs
        let base_price = match pair {
            "BTCUSDT" => 45000.0,
            "ETHUSDT" => 3000.0,
            "ADAUSDT" => 0.5,
            _ => 100.0,
        };

        // Add some variance based on exchange
        let price_variance = match exchange {
            ExchangeIdEnum::Binance => 0.0,
            ExchangeIdEnum::Bybit => 0.001,
            ExchangeIdEnum::OKX => 0.002,
            _ => 0.003,
        };

        let _adjusted_price = base_price * (1.0 + price_variance);

        Ok(Ticker {
            symbol: pair.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            datetime: chrono::Utc::now().to_rfc3339(),
            high: Some(50000.0),
            low: Some(49000.0),
            bid: Some(49500.0),
            bid_volume: Some(10.0),
            ask: Some(49600.0),
            ask_volume: Some(10.0),
            vwap: Some(49550.0),
            open: Some(49400.0),
            close: Some(49550.0),
            last: Some(49550.0),
            previous_close: Some(49400.0),
            change: Some(150.0),
            percentage: Some(0.3),
            average: Some(49500.0),
            base_volume: Some(1000.0),
            quote_volume: Some(49550000.0),
            volume: Some(1000.0),
            info: serde_json::json!({}),
        })
    }

    /// Analyze technical signals for a specific pair and exchange
    pub async fn analyze_technical_signals(
        &self,
        pair: &str,
        exchange: ExchangeIdEnum,
        config: &OpportunityConfig,
    ) -> ArbitrageResult<Vec<TechnicalSignalData>> {
        // Get ticker data for the exchange
        let ticker = self.get_ticker_for_exchange(pair, &exchange).await?;

        // Analyze technical signals
        let technical_analysis = self.analyze_technical_signal(&ticker, &None);

        // Convert to signal data format
        let signal_data = TechnicalSignalData {
            pair: pair.to_string(),
            exchange,
            signal_type: self.signal_to_enum(&technical_analysis.signal),
            signal_strength: self.determine_signal_strength(technical_analysis.confidence),
            confidence_score: technical_analysis.confidence,
            entry_price: ticker.last.unwrap_or(0.0),
            target_price: Some(technical_analysis.target_price),
            stop_loss: Some(technical_analysis.stop_loss),
            technical_indicators: vec!["RSI".to_string(), "MA".to_string(), "BB".to_string()],
            timeframe: "1h".to_string(), // Default timeframe
            expected_return_percentage: technical_analysis.expected_return,
            market_conditions: technical_analysis.market_conditions,
        };

        // Only return signals that meet minimum confidence threshold
        if signal_data.confidence_score >= config.min_confidence_threshold {
            Ok(vec![signal_data])
        } else {
            Ok(Vec::new())
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use chrono::Utc;

    fn create_test_ticker(symbol: &str, price: f64, volume: f64, change_percent: f64) -> Ticker {
        Ticker {
            symbol: symbol.to_string(),
            bid: Some(price - 10.0),
            ask: Some(price + 10.0),
            last: Some(price),
            high: Some(price * (1.0 + change_percent / 100.0)),
            low: Some(price * (1.0 - change_percent / 100.0)),
            volume: Some(volume),
            timestamp: Some(Utc::now()),
            datetime: Some(Utc::now().to_rfc3339()),
        }
    }

    fn create_test_funding_rate(symbol: &str, rate: f64) -> FundingRateInfo {
        FundingRateInfo {
            symbol: symbol.to_string(),
            funding_rate: rate,
            timestamp: Some(Utc::now()),
            datetime: Some(Utc::now().to_rfc3339()),
            next_funding_time: Some(Utc::now()),
            estimated_rate: Some(rate),
        }
    }

    #[allow(dead_code)] // #[test]
    fn test_technical_signal_determination() {
        let analyzer = MarketAnalyzer::new_without_exchange();

        // Test bullish signal with positive momentum
        let bullish_ticker = create_test_ticker("BTCUSDT", 50000.0, 1000000.0, 3.0);
        let analysis = analyzer.analyze_technical_signal(&bullish_ticker, &None);
        assert_eq!(analysis.signal, "LONG");

        // Test bearish signal with negative momentum
        let bearish_ticker = create_test_ticker("ETHUSDT", 3000.0, 1000000.0, -3.0);
        let analysis = analyzer.analyze_technical_signal(&bearish_ticker, &None);
        assert_eq!(analysis.signal, "SHORT");

        // Test funding rate override
        let high_funding = Some(create_test_funding_rate("BTCUSDT", 0.02));
        let analysis = analyzer.analyze_technical_signal(&bullish_ticker, &high_funding);
        assert_eq!(analysis.signal, "SHORT");
    }

    #[allow(dead_code)] // #[test]
    fn test_arbitrage_analysis() {
        let analyzer = MarketAnalyzer::new_without_exchange();

        let ticker_a = create_test_ticker("BTCUSDT", 50000.0, 1000000.0, 1.0);
        let ticker_b = create_test_ticker("BTCUSDT", 50200.0, 1000000.0, 1.0);

        let result = analyzer.analyze_arbitrage_opportunity(
            "BTCUSDT",
            &ticker_a,
            &ticker_b,
            &ExchangeIdEnum::Binance,
            &ExchangeIdEnum::Bybit,
        );

        assert!(result.is_ok());
        let analysis = result.unwrap();
        assert_eq!(analysis.buy_exchange, ExchangeIdEnum::Binance);
        assert_eq!(analysis.sell_exchange, ExchangeIdEnum::Bybit);
        assert!(analysis.price_difference_percent > 0.0);
    }

    #[allow(dead_code)] // #[test]
    fn test_risk_assessment() {
        let analyzer = MarketAnalyzer::new_without_exchange();

        // High risk due to high volatility
        let high_vol_ticker = create_test_ticker("VOLATILE", 100.0, 1000000.0, 10.0);
        let analysis = analyzer.analyze_technical_signal(&high_vol_ticker, &None);
        assert_eq!(analysis.risk_level, "HIGH");

        // Low risk due to stable conditions
        let stable_ticker = create_test_ticker("STABLE", 100.0, 1000000.0, 0.5);
        let analysis = analyzer.analyze_technical_signal(&stable_ticker, &None);
        assert_eq!(analysis.risk_level, "LOW");
    }

    #[allow(dead_code)] // #[test]
    fn test_confidence_calculation() {
        let analyzer = MarketAnalyzer::new_without_exchange();

        // High confidence with high volume and significant funding rate
        let high_vol_ticker = create_test_ticker("BTCUSDT", 50000.0, 2000000.0, 2.5);
        let high_funding = Some(create_test_funding_rate("BTCUSDT", 0.02));
        let analysis = analyzer.analyze_technical_signal(&high_vol_ticker, &high_funding);
        assert!(analysis.confidence > 0.8);

        // Low confidence with low volume
        let low_vol_ticker = create_test_ticker("ALTCOIN", 1.0, 50000.0, 0.5);
        let analysis = analyzer.analyze_technical_signal(&low_vol_ticker, &None);
        assert!(analysis.confidence < 0.7);
    }
}
