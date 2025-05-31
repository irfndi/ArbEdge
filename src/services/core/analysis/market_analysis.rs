// Market Analysis Service
// Task 9.1: Technical Indicators Foundation for Hybrid Trading Platform
// Supports both arbitrage enhancement and standalone technical trading

use crate::services::core::infrastructure::data_ingestion_module::DataIngestionModule;
use crate::services::core::infrastructure::database_repositories::DatabaseManager;
use crate::services::core::user::user_trading_preferences::UserTradingPreferencesService;
use crate::services::core::user::user_trading_preferences::{TradingFocus, UserTradingPreferences};
use crate::utils::{logger::Logger, ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
// use worker::*; // TODO: Re-enable when implementing worker functionality

// ============= CORE DATA STRUCTURES =============

/// Price data point with timestamp
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PricePoint {
    pub timestamp: u64,       // Unix timestamp in milliseconds
    pub price: f64,           // Price value
    pub volume: Option<f64>,  // Trading volume (optional)
    pub exchange_id: String,  // Exchange identifier
    pub trading_pair: String, // Trading pair (e.g., "BTC/USDT")
}

/// Time series of price data for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceSeries {
    pub trading_pair: String,
    pub exchange_id: String,
    pub timeframe: TimeFrame,
    pub data_points: Vec<PricePoint>,
    pub last_updated: u64,
}

impl PriceSeries {
    pub fn new(trading_pair: String, exchange_id: String, timeframe: TimeFrame) -> Self {
        #[cfg(target_arch = "wasm32")]
        let now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            trading_pair,
            exchange_id,
            timeframe,
            data_points: Vec::new(),
            last_updated: now,
        }
    }

    pub fn add_price_point(&mut self, point: PricePoint) {
        // Use binary search to find the correct insertion position for sorted order
        let insertion_pos = self
            .data_points
            .binary_search_by(|probe| probe.timestamp.cmp(&point.timestamp))
            .unwrap_or_else(|e| e);

        self.data_points.insert(insertion_pos, point);

        #[cfg(target_arch = "wasm32")]
        {
            self.last_updated = js_sys::Date::now() as u64;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.last_updated = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
        }
    }

    /// Get the latest price point
    pub fn latest_price(&self) -> Option<&PricePoint> {
        self.data_points.last()
    }

    /// Get prices within a specific time range
    pub fn price_range(&self, start_time: u64, end_time: u64) -> Vec<&PricePoint> {
        self.data_points
            .iter()
            .filter(|point| point.timestamp >= start_time && point.timestamp <= end_time)
            .collect()
    }

    /// Get just the price values for calculations
    pub fn price_values(&self) -> Vec<f64> {
        self.data_points.iter().map(|p| p.price).collect()
    }
}

/// Time frame for price data aggregation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimeFrame {
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "15m")]
    FifteenMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "1d")]
    OneDay,
}

impl TimeFrame {
    /// Get the duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        match self {
            TimeFrame::OneMinute => 60 * 1000,
            TimeFrame::FiveMinutes => 5 * 60 * 1000,
            TimeFrame::FifteenMinutes => 15 * 60 * 1000,
            TimeFrame::OneHour => 60 * 60 * 1000,
            TimeFrame::FourHours => 4 * 60 * 60 * 1000,
            TimeFrame::OneDay => 24 * 60 * 60 * 1000,
        }
    }
}

/// Result of a technical indicator calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorResult {
    pub indicator_name: String,
    pub values: Vec<IndicatorValue>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub calculated_at: u64,
}

/// Individual indicator value with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorValue {
    pub timestamp: u64,
    pub value: f64,
    pub signal: Option<SignalType>, // Buy/Sell/Hold signal if applicable
}

/// Signal types for trading decisions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
    StrongBuy,
    StrongSell,
}

/// Types of trading opportunities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OpportunityType {
    #[serde(rename = "arbitrage")]
    Arbitrage, // Cross-exchange price differences
    #[serde(rename = "technical")]
    Technical, // Technical analysis signals
    #[serde(rename = "arbitrage_technical")]
    ArbitrageTechnical, // Arbitrage enhanced with technical analysis
}

impl std::fmt::Display for OpportunityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpportunityType::Arbitrage => write!(f, "Arbitrage"),
            OpportunityType::Technical => write!(f, "Technical"),
            OpportunityType::ArbitrageTechnical => write!(f, "Arbitrage+Technical"),
        }
    }
}

/// Trading opportunity with analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingOpportunity {
    pub opportunity_id: String,
    pub opportunity_type: OpportunityType,
    pub trading_pair: String,
    pub exchanges: Vec<String>, // Exchanges involved
    pub entry_price: f64,
    pub target_price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub confidence_score: f64, // 0.0 to 1.0
    pub risk_level: RiskLevel,
    pub expected_return: f64, // Percentage
    pub time_horizon: TimeHorizon,
    pub indicators_used: Vec<String>,
    pub analysis_data: serde_json::Value,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

/// Risk level assessment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Hash)]
pub enum RiskLevel {
    #[serde(rename = "low")]
    Low, // Conservative, arbitrage-focused
    #[serde(rename = "medium")]
    Medium, // Balanced risk-reward
    #[serde(rename = "high")]
    High, // Higher risk, higher potential return
}

/// Time horizon for opportunity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimeHorizon {
    #[serde(rename = "immediate")]
    Immediate, // < 5 minutes
    #[serde(rename = "short")]
    Short, // 5-60 minutes
    #[serde(rename = "medium")]
    Medium, // 1-24 hours
    #[serde(rename = "long")]
    Long, // > 24 hours
}

/// Market data event for pipeline ingestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataEvent {
    pub timestamp: u64,
    pub exchange: String,
    pub symbol: String,
    pub price: f64,
    pub volume: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub change_24h: f64,
    pub data_type: String, // "price_data", "technical_indicator", "analysis_result"
}

/// Analysis result event for pipeline storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResultEvent {
    pub analysis_id: String,
    pub analysis_type: String, // "technical_indicators", "correlation", "market_sentiment"
    pub trading_pair: String,
    pub exchange: String,
    pub results: serde_json::Value,
    pub confidence_score: f64,
    pub timestamp: u64,
    pub data_type: String,
}

// ============= MATHEMATICAL FOUNDATION =============

/// Mathematical utility functions for technical indicators
pub struct MathUtils;

impl MathUtils {
    /// Calculate Simple Moving Average
    #[allow(clippy::result_large_err)]
    pub fn simple_moving_average(prices: &[f64], period: usize) -> ArbitrageResult<Vec<f64>> {
        if prices.is_empty() {
            return Err(ArbitrageError::validation_error("No price data provided"));
        }

        if period == 0 {
            return Err(ArbitrageError::validation_error(
                "Period must be greater than 0",
            ));
        }

        if prices.len() < period {
            return Err(ArbitrageError::validation_error(
                "Insufficient data for SMA calculation",
            ));
        }

        let mut sma_values = Vec::new();

        for i in (period - 1)..prices.len() {
            let start_idx = i + 1 - period; // This is safe because i >= period - 1
            let sum: f64 = prices[start_idx..=i].iter().sum();
            sma_values.push(sum / period as f64);
        }

        Ok(sma_values)
    }

    /// Calculate Exponential Moving Average
    #[allow(clippy::result_large_err)]
    pub fn exponential_moving_average(prices: &[f64], period: usize) -> ArbitrageResult<Vec<f64>> {
        if prices.is_empty() {
            return Err(ArbitrageError::validation_error("No price data provided"));
        }

        let alpha = 2.0 / (period as f64 + 1.0);
        let mut ema_values = Vec::with_capacity(prices.len());

        // First EMA value is the first price
        ema_values.push(prices[0]);

        // Calculate subsequent EMA values
        for i in 1..prices.len() {
            let ema = alpha * prices[i] + (1.0 - alpha) * ema_values[i - 1];
            ema_values.push(ema);
        }

        Ok(ema_values)
    }

    /// Calculate standard deviation
    #[allow(clippy::result_large_err)]
    pub fn standard_deviation(values: &[f64]) -> ArbitrageResult<f64> {
        if values.is_empty() {
            return Err(ArbitrageError::validation_error("No values provided"));
        }

        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / values.len() as f64;

        Ok(variance.sqrt())
    }

    /// Calculate Relative Strength Index (RSI)
    /// Calculate Relative Strength Index (RSI) using Cutler's method
    ///
    /// This implementation uses simple moving averages for gain/loss calculation (Cutler's method)
    /// rather than exponential moving averages (Wilder's method). Cutler's method is more responsive
    /// to recent price changes and is commonly used in modern trading platforms.
    ///
    /// Formula: RSI = 100 - (100 / (1 + RS))
    /// Where RS = Average Gain / Average Loss over the specified period
    #[allow(clippy::result_large_err)]
    pub fn relative_strength_index(prices: &[f64], period: usize) -> ArbitrageResult<Vec<f64>> {
        if prices.len() < period + 1 {
            return Err(ArbitrageError::validation_error(
                "Insufficient data for RSI calculation",
            ));
        }

        let mut gains = Vec::new();
        let mut losses = Vec::new();

        // Calculate price changes
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

        let mut rsi_values = Vec::new();

        // Calculate RSI for each period
        for i in (period - 1)..gains.len() {
            let avg_gain: f64 = gains[(i + 1 - period)..=i].iter().sum::<f64>() / period as f64;
            let avg_loss: f64 = losses[(i + 1 - period)..=i].iter().sum::<f64>() / period as f64;

            let rs = if avg_loss == 0.0 {
                100.0 // If no losses, RSI is 100
            } else {
                avg_gain / avg_loss
            };

            let rsi = 100.0 - (100.0 / (1.0 + rs));
            rsi_values.push(rsi);
        }

        Ok(rsi_values)
    }

    /// Calculate Bollinger Bands
    #[allow(clippy::result_large_err)]
    pub fn bollinger_bands(
        prices: &[f64],
        period: usize,
        std_dev_multiplier: f64,
    ) -> ArbitrageResult<(Vec<f64>, Vec<f64>, Vec<f64>)> {
        let sma = Self::simple_moving_average(prices, period)?;

        if prices.len() < period {
            return Err(ArbitrageError::validation_error(
                "Insufficient data for Bollinger Bands",
            ));
        }

        let mut upper_band = Vec::new();
        let mut lower_band = Vec::new();

        for i in (period - 1)..prices.len() {
            let price_slice = &prices[(i + 1 - period)..=i];
            let std_dev = Self::standard_deviation(price_slice)?;
            let sma_index = i - (period - 1);

            upper_band.push(sma[sma_index] + (std_dev_multiplier * std_dev));
            lower_band.push(sma[sma_index] - (std_dev_multiplier * std_dev));
        }

        Ok((upper_band, sma, lower_band))
    }

    /// Calculate price correlation between two price series
    #[allow(clippy::result_large_err)]
    pub fn price_correlation(prices1: &[f64], prices2: &[f64]) -> ArbitrageResult<f64> {
        if prices1.len() != prices2.len() || prices1.is_empty() {
            return Err(ArbitrageError::validation_error(
                "Price series must be same length and non-empty",
            ));
        }

        let n = prices1.len() as f64;
        let mean1 = prices1.iter().sum::<f64>() / n;
        let mean2 = prices2.iter().sum::<f64>() / n;

        let numerator: f64 = prices1
            .iter()
            .zip(prices2.iter())
            .map(|(x, y)| (x - mean1) * (y - mean2))
            .sum();

        let sum_sq1: f64 = prices1.iter().map(|x| (x - mean1).powi(2)).sum();
        let sum_sq2: f64 = prices2.iter().map(|y| (y - mean2).powi(2)).sum();

        let denominator = (sum_sq1 * sum_sq2).sqrt();

        if denominator == 0.0 {
            Ok(0.0)
        } else {
            Ok(numerator / denominator)
        }
    }
}

// ============= MARKET ANALYSIS SERVICE =============

/// Main service for market analysis and technical indicators
pub struct MarketAnalysisService {
    #[allow(dead_code)] // TODO: Will be used for database operations in future implementation
    database_repositories: DatabaseManager,
    preferences_service: UserTradingPreferencesService,
    pipelines_service: Option<DataIngestionModule>, // For market data ingestion and analysis storage
    logger: Logger,
    price_cache: HashMap<String, PriceSeries>, // In-memory cache for recent price data
    cache_max_size: usize,                     // Maximum number of cached series
    cache_ttl_ms: u64,                         // Time-to-live for cache entries in milliseconds
}

impl MarketAnalysisService {
    pub fn new(
        database_repositories: DatabaseManager,
        preferences_service: UserTradingPreferencesService,
        logger: Logger,
    ) -> Self {
        Self {
            database_repositories,
            preferences_service,
            pipelines_service: None,
            logger,
            price_cache: HashMap::new(),
            cache_max_size: 1000, // Default: cache up to 1000 price series
            cache_ttl_ms: 24 * 60 * 60 * 1000, // Default: 24 hours TTL
        }
    }

    /// Set pipelines service for market data ingestion and analysis storage
    pub fn set_pipelines_service(&mut self, pipelines_service: DataIngestionModule) {
        self.pipelines_service = Some(pipelines_service);
    }

    /// Store market data to pipelines for centralized data management
    #[allow(clippy::too_many_arguments)]
    pub async fn store_market_data_to_pipeline(
        &self,
        exchange: &str,
        symbol: &str,
        price: f64,
        volume: f64,
        high_24h: f64,
        low_24h: f64,
        change_24h: f64,
    ) -> ArbitrageResult<()> {
        if let Some(ref pipelines_service) = self.pipelines_service {
            let market_data = MarketDataEvent {
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                exchange: exchange.to_string(),
                symbol: symbol.to_string(),
                price,
                volume,
                high_24h,
                low_24h,
                change_24h,
                data_type: "price_data".to_string(),
            };

            // Store to pipelines for historical data and analytics
            let market_data_json = serde_json::to_value(&market_data)?;
            pipelines_service
                .store_market_data(exchange, symbol, &serde_json::to_string(&market_data_json)?)
                .await?;

            self.logger.info(&format!(
                "Stored market data to pipeline: {}/{} at ${}",
                exchange, symbol, price
            ));
        }
        Ok(())
    }

    /// Store analysis results to pipelines for historical tracking
    pub async fn store_analysis_results_to_pipeline(
        &self,
        analysis_type: &str,
        trading_pair: &str,
        exchange: &str,
        results: serde_json::Value,
        confidence_score: f64,
    ) -> ArbitrageResult<()> {
        if let Some(ref pipelines_service) = self.pipelines_service {
            let analysis_result = AnalysisResultEvent {
                analysis_id: Uuid::new_v4().to_string(),
                analysis_type: analysis_type.to_string(),
                trading_pair: trading_pair.to_string(),
                exchange: exchange.to_string(),
                results,
                confidence_score,
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                data_type: "analysis_result".to_string(),
            };

            // Store to pipelines for historical analysis tracking
            let analysis_result_json = serde_json::to_value(&analysis_result)?;
            pipelines_service
                .store_analysis_results(
                    analysis_type,
                    &serde_json::to_string(&analysis_result_json)?,
                )
                .await?;

            self.logger.info(&format!(
                "Stored analysis results to pipeline: {} for {}/{}",
                analysis_type, exchange, trading_pair
            ));
        }
        Ok(())
    }

    /// Store price data for analysis
    pub async fn store_price_data(&mut self, price_point: PricePoint) -> ArbitrageResult<()> {
        let cache_key = format!("{}_{}", price_point.exchange_id, price_point.trading_pair);

        // Evict expired entries before adding new data
        self.evict_expired_cache_entries();

        // Check if cache is at capacity and evict LRU entries if needed
        if self.price_cache.len() >= self.cache_max_size
            && !self.price_cache.contains_key(&cache_key)
        {
            self.evict_lru_cache_entries(1);
        }

        // Update in-memory cache
        match self.price_cache.get_mut(&cache_key) {
            Some(series) => {
                series.add_price_point(price_point.clone());
            }
            None => {
                let mut series = PriceSeries::new(
                    price_point.trading_pair.clone(),
                    price_point.exchange_id.clone(),
                    TimeFrame::OneMinute, // Default timeframe
                );
                series.add_price_point(price_point.clone());
                self.price_cache.insert(cache_key, series);
            }
        }

        self.logger.info(&format!(
            "Stored price data for {}/{}: ${}",
            price_point.exchange_id, price_point.trading_pair, price_point.price
        ));

        Ok(())
    }

    /// Get price series for analysis
    pub fn get_price_series(&self, exchange_id: &str, trading_pair: &str) -> Option<&PriceSeries> {
        let cache_key = format!("{}_{}", exchange_id, trading_pair);
        self.price_cache.get(&cache_key)
    }

    /// Evict expired cache entries based on TTL
    fn evict_expired_cache_entries(&mut self) {
        #[cfg(target_arch = "wasm32")]
        let now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let expired_keys: Vec<String> = self
            .price_cache
            .iter()
            .filter(|(_, series)| now.saturating_sub(series.last_updated) > self.cache_ttl_ms)
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired_keys {
            self.price_cache.remove(&key);
            self.logger
                .debug(&format!("Evicted expired cache entry: {}", key));
        }
    }

    /// Evict least recently used cache entries
    fn evict_lru_cache_entries(&mut self, count: usize) {
        // Find the oldest entries by last_updated timestamp
        let mut entries: Vec<(String, u64)> = self
            .price_cache
            .iter()
            .map(|(key, series)| (key.clone(), series.last_updated))
            .collect();

        // Sort by last_updated (oldest first)
        entries.sort_by_key(|(_, timestamp)| *timestamp);

        // Remove the oldest entries
        for (key, _) in entries.into_iter().take(count) {
            self.price_cache.remove(&key);
            self.logger
                .debug(&format!("Evicted LRU cache entry: {}", key));
        }
    }

    /// Configure cache settings
    pub fn configure_cache(&mut self, max_size: usize, ttl_ms: u64) {
        self.cache_max_size = max_size;
        self.cache_ttl_ms = ttl_ms;
        self.logger.info(&format!(
            "Cache configured: max_size={}, ttl_ms={}",
            max_size, ttl_ms
        ));
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();
        stats.insert(
            "cache_size".to_string(),
            serde_json::Value::Number(serde_json::Number::from(self.price_cache.len())),
        );
        stats.insert(
            "cache_max_size".to_string(),
            serde_json::Value::Number(serde_json::Number::from(self.cache_max_size)),
        );
        stats.insert(
            "cache_ttl_ms".to_string(),
            serde_json::Value::Number(serde_json::Number::from(self.cache_ttl_ms)),
        );

        #[cfg(target_arch = "wasm32")]
        let now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let expired_count = self
            .price_cache
            .values()
            .filter(|series| now.saturating_sub(series.last_updated) > self.cache_ttl_ms)
            .count();

        stats.insert(
            "expired_entries".to_string(),
            serde_json::Value::Number(serde_json::Number::from(expired_count)),
        );
        stats
    }

    /// Generate trading opportunities based on user preferences
    pub async fn generate_opportunities(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Vec<TradingOpportunity>> {
        let preferences = self
            .preferences_service
            .get_or_create_preferences(user_id)
            .await?;

        let mut opportunities = Vec::new();

        match preferences.trading_focus {
            TradingFocus::Arbitrage => {
                // Focus on arbitrage opportunities
                opportunities.extend(self.detect_arbitrage_opportunities(&preferences).await?);
            }
            TradingFocus::Technical => {
                // Focus on technical trading opportunities
                opportunities.extend(self.detect_technical_opportunities(&preferences).await?);
            }
            TradingFocus::Hybrid => {
                // Both arbitrage and technical opportunities
                opportunities.extend(self.detect_arbitrage_opportunities(&preferences).await?);
                opportunities.extend(self.detect_technical_opportunities(&preferences).await?);
                opportunities.extend(self.detect_hybrid_opportunities(&preferences).await?);
            }
        }

        // Filter by user's risk tolerance and experience level
        opportunities = self.filter_opportunities_by_preferences(opportunities, &preferences);

        self.logger.info(&format!(
            "Generated {} opportunities for user {} (focus: {:?})",
            opportunities.len(),
            user_id,
            preferences.trading_focus
        ));

        Ok(opportunities)
    }

    /// Detect arbitrage opportunities with enhanced cross-exchange analysis
    async fn detect_arbitrage_opportunities(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<Vec<TradingOpportunity>> {
        let mut opportunities = Vec::new();

        // Define exchanges and trading pairs for cross-exchange arbitrage analysis
        let exchanges = vec!["binance", "bybit", "kraken"];
        let trading_pairs = vec!["BTC/USDT", "ETH/USDT", "ADA/USDT"];

        for pair in &trading_pairs {
            let mut exchange_prices = Vec::new();

            // Collect price data from multiple exchanges
            for exchange in &exchanges {
                if let Some(price_series) = self.get_price_series(exchange, pair) {
                    if let Some(latest_price) = price_series.latest_price() {
                        exchange_prices.push((
                            exchange,
                            latest_price.price,
                            latest_price.volume.unwrap_or(0.0),
                        ));
                    }
                }
            }

            // Need at least 2 exchanges to detect arbitrage
            if exchange_prices.len() < 2 {
                continue;
            }

            // Find minimum and maximum prices
            let (min_exchange, min_price, min_volume) = exchange_prices
                .iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap();
            let (max_exchange, max_price, max_volume) = exchange_prices
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap();

            // Calculate spread percentage
            let spread_percentage = ((max_price - min_price) / min_price) * 100.0;

            // Only consider opportunities with spread > 0.1% (to cover transaction costs)
            let min_spread_threshold = 0.1;
            if spread_percentage <= min_spread_threshold {
                continue;
            }

            // Calculate confidence based on spread size and volume
            let volume_confidence = (min_volume.min(*max_volume) / 1000.0).min(1.0); // Normalize volume
            let spread_confidence = (spread_percentage / 2.0).min(1.0); // Higher spreads = higher confidence
            let base_confidence = 0.6 + (volume_confidence * 0.2) + (spread_confidence * 0.2);

            // Incorporate transaction costs (approximately 0.075% per exchange)
            let transaction_costs = 0.15; // 0.075% * 2 exchanges
            let net_return = spread_percentage - transaction_costs;

            // Only create opportunity if net return is positive
            if net_return > 0.0 {
                let opportunity = TradingOpportunity {
                    opportunity_id: format!(
                        "arb_{}_{}_{}_{}",
                        pair.replace('/', ""),
                        min_exchange,
                        max_exchange,
                        uuid::Uuid::new_v4()
                    ),
                    opportunity_type: OpportunityType::Arbitrage,
                    trading_pair: pair.to_string(),
                    exchanges: vec![min_exchange.to_string(), max_exchange.to_string()],
                    entry_price: *min_price,
                    target_price: Some(*max_price),
                    stop_loss: Some(min_price * 0.995), // 0.5% stop loss
                    confidence_score: base_confidence,
                    risk_level: if spread_percentage > 1.0 {
                        RiskLevel::Medium
                    } else {
                        RiskLevel::Low
                    },
                    expected_return: net_return,
                    time_horizon: TimeHorizon::Immediate, // Arbitrage should be immediate
                    indicators_used: vec![
                        "cross_exchange_price_analysis".to_string(),
                        "volume_analysis".to_string(),
                    ],
                    analysis_data: serde_json::json!({
                        "buy_exchange": min_exchange,
                        "sell_exchange": max_exchange,
                        "buy_price": min_price,
                        "sell_price": max_price,
                        "gross_spread_percent": spread_percentage,
                        "transaction_costs_percent": transaction_costs,
                        "net_return_percent": net_return,
                        "min_volume": min_volume,
                        "max_volume": max_volume,
                        "detection_method": "enhanced_cross_exchange_analysis",
                        "market_depth_checked": *min_volume > 0.0 && *max_volume > 0.0
                    }),
                    created_at: chrono::Utc::now().timestamp_millis() as u64,
                    expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + 300000), // 5 minutes
                };

                // Only include if it matches user preferences and has sufficient volume
                if self.matches_user_risk_tolerance(&opportunity, preferences) && *min_volume > 10.0
                {
                    opportunities.push(opportunity);
                }
            }
        }

        self.logger.info(&format!(
            "Detected {} arbitrage opportunities with enhanced cross-exchange analysis",
            opportunities.len()
        ));
        Ok(opportunities)
    }

    /// Detect technical trading opportunities
    async fn detect_technical_opportunities(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<Vec<TradingOpportunity>> {
        // Basic technical analysis implementation
        let mut opportunities = Vec::new();

        // Sample analysis for cached price series
        for price_series in self.price_cache.values() {
            if price_series.data_points.len() < 20 {
                continue; // Need enough data for technical analysis
            }

            // Calculate RSI and SMA for signal generation
            let prices = price_series.price_values();
            if let (Ok(rsi_values), Ok(sma_values)) = (
                MathUtils::relative_strength_index(&prices, 14),
                MathUtils::simple_moving_average(&prices, 20),
            ) {
                // Check for oversold RSI condition (RSI < 30)
                if let (Some(&latest_rsi), Some(&latest_price)) = (rsi_values.last(), prices.last())
                {
                    if latest_rsi < 30.0 && latest_price > *sma_values.last().unwrap_or(&0.0) {
                        let opportunity = TradingOpportunity {
                            opportunity_id: format!(
                                "tech_{}_{}",
                                price_series.trading_pair.replace('/', ""),
                                uuid::Uuid::new_v4()
                            ),
                            opportunity_type: OpportunityType::Technical,
                            trading_pair: price_series.trading_pair.clone(),
                            exchanges: vec![price_series.exchange_id.clone()],
                            entry_price: latest_price,
                            target_price: Some(latest_price * 1.02), // 2% target
                            stop_loss: Some(latest_price * 0.98),    // 2% stop loss
                            confidence_score: 0.65,
                            risk_level: RiskLevel::Medium,
                            expected_return: 2.0, // 2%
                            time_horizon: TimeHorizon::Medium,
                            indicators_used: vec!["RSI_14".to_string(), "SMA_20".to_string()],
                            analysis_data: serde_json::json!({
                                "rsi_value": latest_rsi,
                                "sma_value": sma_values.last(),
                                "signal": "oversold_recovery",
                                "detection_method": "technical_indicators"
                            }),
                            created_at: chrono::Utc::now().timestamp_millis() as u64,
                            expires_at: Some(
                                chrono::Utc::now().timestamp_millis() as u64 + 3600000,
                            ), // 1 hour
                        };

                        if self.matches_user_risk_tolerance(&opportunity, preferences) {
                            opportunities.push(opportunity);
                        }
                    }
                }
            }
        }

        self.logger.info(&format!(
            "Detected {} technical opportunities",
            opportunities.len()
        ));
        Ok(opportunities)
    }

    /// Detect hybrid opportunities (arbitrage + technical signals)
    async fn detect_hybrid_opportunities(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<Vec<TradingOpportunity>> {
        // Combine arbitrage and technical analysis
        let arbitrage_ops = self.detect_arbitrage_opportunities(preferences).await?;
        let technical_ops = self.detect_technical_opportunities(preferences).await?;

        let mut hybrid_opportunities = Vec::new();

        // Find pairs that have both arbitrage and technical signals
        for arb_op in &arbitrage_ops {
            for tech_op in &technical_ops {
                if arb_op.trading_pair == tech_op.trading_pair {
                    // Create hybrid opportunity combining both signals
                    let hybrid_opportunity = TradingOpportunity {
                        opportunity_id: format!(
                            "hybrid_{}_{}",
                            arb_op.trading_pair.replace('/', ""),
                            uuid::Uuid::new_v4()
                        ),
                        opportunity_type: OpportunityType::ArbitrageTechnical,
                        trading_pair: arb_op.trading_pair.clone(),
                        exchanges: arb_op.exchanges.clone(),
                        entry_price: arb_op.entry_price,
                        target_price: arb_op.target_price,
                        stop_loss: arb_op.stop_loss,
                        confidence_score: (arb_op.confidence_score + tech_op.confidence_score)
                            / 2.0,
                        risk_level: RiskLevel::Medium, // Hybrid typically medium risk
                        expected_return: arb_op.expected_return + tech_op.expected_return,
                        time_horizon: TimeHorizon::Short,
                        indicators_used: {
                            let mut combined = arb_op.indicators_used.clone();
                            combined.extend(tech_op.indicators_used.clone());
                            combined
                        },
                        analysis_data: serde_json::json!({
                            "arbitrage_data": arb_op.analysis_data,
                            "technical_data": tech_op.analysis_data,
                            "detection_method": "hybrid_analysis",
                            "combined_confidence": (arb_op.confidence_score + tech_op.confidence_score) / 2.0
                        }),
                        created_at: chrono::Utc::now().timestamp_millis() as u64,
                        expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + 1800000), // 30 minutes
                    };

                    if self.matches_user_risk_tolerance(&hybrid_opportunity, preferences) {
                        hybrid_opportunities.push(hybrid_opportunity);
                    }
                }
            }
        }

        self.logger.info(&format!(
            "Detected {} hybrid opportunities",
            hybrid_opportunities.len()
        ));
        Ok(hybrid_opportunities)
    }

    /// Check if opportunity matches user's risk tolerance
    fn matches_user_risk_tolerance(
        &self,
        opportunity: &TradingOpportunity,
        preferences: &UserTradingPreferences,
    ) -> bool {
        match preferences.risk_tolerance {
            crate::services::core::user::user_trading_preferences::RiskTolerance::Conservative => {
                opportunity.risk_level == RiskLevel::Low && opportunity.confidence_score >= 0.8
            }
            crate::services::core::user::user_trading_preferences::RiskTolerance::Balanced => {
                (opportunity.risk_level == RiskLevel::Low
                    || opportunity.risk_level == RiskLevel::Medium)
                    && opportunity.confidence_score >= 0.6
            }
            crate::services::core::user::user_trading_preferences::RiskTolerance::Aggressive => {
                opportunity.confidence_score >= 0.5 // Accept most opportunities
            }
        }
    }

    /// Filter opportunities based on user preferences
    fn filter_opportunities_by_preferences(
        &self,
        opportunities: Vec<TradingOpportunity>,
        preferences: &UserTradingPreferences,
    ) -> Vec<TradingOpportunity> {
        opportunities
            .into_iter()
            .filter(|opp| {
                // Filter by risk tolerance
                match preferences.risk_tolerance {
                    crate::services::core::user::user_trading_preferences::RiskTolerance::Conservative => {
                        opp.risk_level == RiskLevel::Low
                    }
                    crate::services::core::user::user_trading_preferences::RiskTolerance::Balanced => {
                        opp.risk_level == RiskLevel::Low || opp.risk_level == RiskLevel::Medium
                    }
                    crate::services::core::user::user_trading_preferences::RiskTolerance::Aggressive => {
                        true // All risk levels allowed
                    }
                }
            })
            .filter(|opp| {
                // Filter by experience level (beginners get simpler opportunities)
                match preferences.experience_level {
                    crate::services::core::user::user_trading_preferences::ExperienceLevel::Beginner => {
                        opp.opportunity_type == OpportunityType::Arbitrage
                            && opp.confidence_score >= 0.8
                    }
                    crate::services::core::user::user_trading_preferences::ExperienceLevel::Intermediate => {
                        opp.confidence_score >= 0.6
                    }
                    crate::services::core::user::user_trading_preferences::ExperienceLevel::Advanced => {
                        true // All opportunities allowed
                    }
                }
            })
            .collect()
    }

    /// Calculate technical indicators for a price series
    #[allow(clippy::result_large_err)]
    pub fn calculate_indicators(
        &self,
        series: &PriceSeries,
        indicators: &[&str],
    ) -> ArbitrageResult<Vec<IndicatorResult>> {
        let prices = series.price_values();
        let mut results = Vec::new();

        #[cfg(target_arch = "wasm32")]
        let now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        for &indicator in indicators {
            match indicator {
                "sma_20" => {
                    if let Ok(sma_values) = MathUtils::simple_moving_average(&prices, 20) {
                        let indicator_values: Vec<IndicatorValue> = sma_values
                            .into_iter()
                            .enumerate()
                            .filter_map(|(i, value)| {
                                // Add bounds check to prevent panic
                                if i + 19 < series.data_points.len() {
                                    Some(IndicatorValue {
                                        timestamp: series.data_points[i + 19].timestamp, // Offset by period
                                        value,
                                        signal: None,
                                    })
                                } else {
                                    None
                                }
                            })
                            .collect();

                        results.push(IndicatorResult {
                            indicator_name: "SMA_20".to_string(),
                            values: indicator_values,
                            metadata: HashMap::new(),
                            calculated_at: now,
                        });
                    }
                }
                "rsi_14" => {
                    if let Ok(rsi_values) = MathUtils::relative_strength_index(&prices, 14) {
                        let indicator_values: Vec<IndicatorValue> = rsi_values
                            .into_iter()
                            .enumerate()
                            .filter_map(|(i, value)| {
                                // Add bounds check to prevent panic
                                if i + 14 < series.data_points.len() {
                                    let signal = if value > 70.0 {
                                        Some(SignalType::Sell)
                                    } else if value < 30.0 {
                                        Some(SignalType::Buy)
                                    } else {
                                        Some(SignalType::Hold)
                                    };

                                    Some(IndicatorValue {
                                        timestamp: series.data_points[i + 14].timestamp, // Offset by period
                                        value,
                                        signal,
                                    })
                                } else {
                                    None
                                }
                            })
                            .collect();

                        results.push(IndicatorResult {
                            indicator_name: "RSI_14".to_string(),
                            values: indicator_values,
                            metadata: HashMap::new(),
                            calculated_at: now,
                        });
                    }
                }
                _ => {
                    self.logger
                        .warn(&format!("Unknown indicator requested: {}", indicator));
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_series_creation() {
        let series = PriceSeries::new(
            "BTC/USDT".to_string(),
            "binance".to_string(),
            TimeFrame::OneMinute,
        );

        assert_eq!(series.trading_pair, "BTC/USDT");
        assert_eq!(series.exchange_id, "binance");
        assert_eq!(series.timeframe, TimeFrame::OneMinute);
        assert!(series.data_points.is_empty());
    }

    #[test]
    fn test_price_point_addition() {
        let mut series = PriceSeries::new(
            "BTC/USDT".to_string(),
            "binance".to_string(),
            TimeFrame::OneMinute,
        );

        let point = PricePoint {
            timestamp: 1640995200000, // 2022-01-01 00:00:00 UTC
            price: 46000.0,
            volume: Some(1.5),
            exchange_id: "binance".to_string(),
            trading_pair: "BTC/USDT".to_string(),
        };

        series.add_price_point(point.clone());
        assert_eq!(series.data_points.len(), 1);
        assert_eq!(series.latest_price().unwrap().price, 46000.0);
    }

    #[test]
    fn test_simple_moving_average() {
        let prices = vec![10.0, 12.0, 14.0, 16.0, 18.0, 20.0];
        let sma = MathUtils::simple_moving_average(&prices, 3).unwrap();

        assert_eq!(sma.len(), 4); // 6 prices - 3 period + 1
        assert_eq!(sma[0], 12.0); // (10+12+14)/3
        assert_eq!(sma[1], 14.0); // (12+14+16)/3
        assert_eq!(sma[2], 16.0); // (14+16+18)/3
        assert_eq!(sma[3], 18.0); // (16+18+20)/3
    }

    #[test]
    fn test_exponential_moving_average() {
        let prices = vec![10.0, 12.0, 14.0, 16.0, 18.0];
        let ema = MathUtils::exponential_moving_average(&prices, 3).unwrap();

        assert_eq!(ema.len(), 5);
        assert_eq!(ema[0], 10.0); // First value is the first price
                                  // Subsequent values should follow EMA formula
        assert!(ema[1] > 10.0 && ema[1] < 12.0);
    }

    #[test]
    fn test_rsi_calculation() {
        let prices = vec![
            44.0, 44.25, 44.5, 43.75, 44.5, 44.0, 44.25, 45.0, 47.0, 46.75, 46.5, 46.25, 47.75,
            47.5, 47.0, 46.5, 46.0, 47.0, 47.25, 48.0,
        ];

        let rsi = MathUtils::relative_strength_index(&prices, 14).unwrap();
        assert!(!rsi.is_empty());

        // RSI should be between 0 and 100
        for value in rsi {
            assert!((0.0..=100.0).contains(&value));
        }
    }

    #[test]
    fn test_bollinger_bands() {
        let prices = vec![10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0];
        let (upper, middle, lower) = MathUtils::bollinger_bands(&prices, 5, 2.0).unwrap();

        assert_eq!(upper.len(), middle.len());
        assert_eq!(middle.len(), lower.len());

        // Upper band should be above middle, lower should be below
        for i in 0..upper.len() {
            assert!(upper[i] > middle[i]);
            assert!(lower[i] < middle[i]);
        }
    }

    #[test]
    fn test_price_correlation() {
        let prices1 = vec![10.0, 12.0, 14.0, 16.0, 18.0];
        let prices2 = vec![20.0, 24.0, 28.0, 32.0, 36.0]; // Perfect positive correlation

        let correlation = MathUtils::price_correlation(&prices1, &prices2).unwrap();
        assert!((correlation - 1.0).abs() < 0.001); // Should be very close to 1.0
    }

    #[test]
    fn test_timeframe_duration() {
        assert_eq!(TimeFrame::OneMinute.duration_ms(), 60 * 1000);
        assert_eq!(TimeFrame::FiveMinutes.duration_ms(), 5 * 60 * 1000);
        assert_eq!(TimeFrame::OneHour.duration_ms(), 60 * 60 * 1000);
        assert_eq!(TimeFrame::OneDay.duration_ms(), 24 * 60 * 60 * 1000);
    }

    // Note: Integration tests that require D1Service and other complex dependencies
    // are disabled in unit tests. These should be run in proper integration test environment
    // with proper mocking or actual Cloudflare Workers environment.

    #[test]
    fn test_error_handling_invalid_inputs() {
        // Test mathematical functions with invalid inputs that don't require service setup
        let empty_prices: Vec<f64> = vec![];
        let sma_result = MathUtils::simple_moving_average(&empty_prices, 5);
        assert!(sma_result.is_err());

        let insufficient_prices = vec![1.0, 2.0];
        let rsi_result = MathUtils::relative_strength_index(&insufficient_prices, 14);
        assert!(rsi_result.is_err());

        // Test invalid input edge cases
        let zero_period_sma = MathUtils::simple_moving_average(&[1.0, 2.0, 3.0], 0);
        assert!(zero_period_sma.is_err());

        let correlation_mismatched = MathUtils::price_correlation(&[1.0, 2.0], &[1.0]);
        assert!(correlation_mismatched.is_err());
    }

    #[test]
    fn test_price_series_functionality() {
        let mut series = PriceSeries::new(
            "BTC/USDT".to_string(),
            "binance".to_string(),
            TimeFrame::OneHour,
        );

        // Test adding price points
        let price_point1 = PricePoint {
            timestamp: 1000,
            price: 50000.0,
            volume: Some(1.0),
            exchange_id: "binance".to_string(),
            trading_pair: "BTC/USDT".to_string(),
        };

        let price_point2 = PricePoint {
            timestamp: 2000,
            price: 51000.0,
            volume: Some(1.5),
            exchange_id: "binance".to_string(),
            trading_pair: "BTC/USDT".to_string(),
        };

        series.add_price_point(price_point1);
        series.add_price_point(price_point2);

        // Test latest price
        let latest = series.latest_price().unwrap();
        assert_eq!(latest.price, 51000.0);
        assert_eq!(latest.timestamp, 2000);

        // Test price values extraction
        let prices = series.price_values();
        assert_eq!(prices, vec![50000.0, 51000.0]);

        // Test price range filtering
        let range = series.price_range(1500, 2500);
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].price, 51000.0);
    }

    #[test]
    fn test_trading_opportunity_creation() {
        let opportunity = TradingOpportunity {
            opportunity_id: "test_123".to_string(),
            opportunity_type: OpportunityType::Arbitrage,
            trading_pair: "BTC/USDT".to_string(),
            exchanges: vec!["binance".to_string(), "bybit".to_string()],
            entry_price: 50000.0,
            target_price: Some(51000.0),
            stop_loss: Some(49000.0),
            confidence_score: 0.85,
            risk_level: RiskLevel::Medium,
            expected_return: 2.0,
            time_horizon: TimeHorizon::Short,
            indicators_used: vec!["price_diff".to_string()],
            analysis_data: serde_json::json!({"spread": 1000.0}),
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: None,
        };

        assert_eq!(opportunity.opportunity_type, OpportunityType::Arbitrage);
        assert_eq!(opportunity.confidence_score, 0.85);
        assert_eq!(opportunity.expected_return, 2.0);
    }

    #[test]
    fn test_opportunity_type_display() {
        assert_eq!(OpportunityType::Arbitrage.to_string(), "Arbitrage");
        assert_eq!(OpportunityType::Technical.to_string(), "Technical");
        assert_eq!(
            OpportunityType::ArbitrageTechnical.to_string(),
            "Arbitrage+Technical"
        );
    }
}
