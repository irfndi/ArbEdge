use crate::services::core::infrastructure::cloudflare_pipelines::CloudflarePipelinesService;
use crate::types::{ExchangeIdEnum, FundingRateInfo};
use crate::utils::logger::Logger;
use crate::{utils::ArbitrageError, utils::ArbitrageResult};
// Remove circular dependency - define MarketDataSnapshot locally
// use crate::services::core::market_data::market_data_ingestion::{MarketDataSnapshot, MarketDataIngestionService};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::kv::KvStore;
use worker::*;

// Local MarketDataSnapshot definition to avoid circular dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataSnapshot {
    pub exchange: ExchangeIdEnum,
    pub symbol: String,
    pub timestamp: u64,
    pub price: f64,
    pub volume_24h: Option<f64>,
    pub funding_rate: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub high_24h: Option<f64>,
    pub low_24h: Option<f64>,
    pub change_24h: Option<f64>,
    pub source: DataAccessSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataAccessSource {
    Pipeline,
    Cache,
    RealAPI,
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridDataAccessConfig {
    pub cache_ttl_seconds: u64,
    pub pipeline_timeout_seconds: u32,
    pub api_timeout_seconds: u32,
    pub max_retries: u32,
    pub enable_data_freshness_validation: bool,
    pub freshness_threshold_seconds: u64,
    pub enable_automatic_refresh: bool,
    pub refresh_interval_seconds: u64,
    pub enable_health_monitoring: bool,
    pub health_check_interval_seconds: u64,
}

impl Default for HybridDataAccessConfig {
    fn default() -> Self {
        Self {
            cache_ttl_seconds: 300,       // 5 minutes cache TTL
            pipeline_timeout_seconds: 10, // 10 seconds pipeline timeout
            api_timeout_seconds: 30,      // 30 seconds API timeout
            max_retries: 3,               // 3 retry attempts
            enable_data_freshness_validation: true,
            freshness_threshold_seconds: 600, // 10 minutes freshness threshold
            enable_automatic_refresh: true,
            refresh_interval_seconds: 300, // 5 minutes refresh interval
            enable_health_monitoring: true,
            health_check_interval_seconds: 60, // 1 minute health checks
        }
    }
}

impl HybridDataAccessConfig {
    /// Validate configuration values
    pub fn validate(&self) -> ArbitrageResult<()> {
        // Validate timeout values (should be positive and within reasonable limits)
        if self.pipeline_timeout_seconds == 0 || self.pipeline_timeout_seconds > 300 {
            return Err(ArbitrageError::validation_error(
                "pipeline_timeout_seconds must be between 1 and 300 seconds",
            ));
        }

        if self.api_timeout_seconds == 0 || self.api_timeout_seconds > 600 {
            return Err(ArbitrageError::validation_error(
                "api_timeout_seconds must be between 1 and 600 seconds",
            ));
        }

        // Validate cache TTL (should be positive and reasonable)
        if self.cache_ttl_seconds == 0 || self.cache_ttl_seconds > 86400 {
            return Err(ArbitrageError::validation_error(
                "cache_ttl_seconds must be between 1 and 86400 seconds (24 hours)",
            ));
        }

        // Validate retry count
        if self.max_retries > 10 {
            return Err(ArbitrageError::validation_error(
                "max_retries must not exceed 10",
            ));
        }

        // Validate freshness threshold
        if self.freshness_threshold_seconds == 0 || self.freshness_threshold_seconds > 3600 {
            return Err(ArbitrageError::validation_error(
                "freshness_threshold_seconds must be between 1 and 3600 seconds (1 hour)",
            ));
        }

        // Validate refresh interval
        if self.refresh_interval_seconds == 0 || self.refresh_interval_seconds > 3600 {
            return Err(ArbitrageError::validation_error(
                "refresh_interval_seconds must be between 1 and 3600 seconds (1 hour)",
            ));
        }

        // Validate health check interval
        if self.health_check_interval_seconds == 0 || self.health_check_interval_seconds > 300 {
            return Err(ArbitrageError::validation_error(
                "health_check_interval_seconds must be between 1 and 300 seconds",
            ));
        }

        Ok(())
    }

    /// Create a new validated configuration
    #[allow(clippy::too_many_arguments)]
    pub fn new_validated(
        cache_ttl_seconds: u64,
        pipeline_timeout_seconds: u32,
        api_timeout_seconds: u32,
        max_retries: u32,
        enable_data_freshness_validation: bool,
        freshness_threshold_seconds: u64,
        enable_automatic_refresh: bool,
        refresh_interval_seconds: u64,
        enable_health_monitoring: bool,
        health_check_interval_seconds: u64,
    ) -> ArbitrageResult<Self> {
        let config = Self {
            cache_ttl_seconds,
            pipeline_timeout_seconds,
            api_timeout_seconds,
            max_retries,
            enable_data_freshness_validation,
            freshness_threshold_seconds,
            enable_automatic_refresh,
            refresh_interval_seconds,
            enable_health_monitoring,
            health_check_interval_seconds,
        };

        config.validate()?;
        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceHealth {
    pub source: DataAccessSource,
    pub is_healthy: bool,
    pub last_success_timestamp: u64,
    pub last_error: Option<String>,
    pub success_rate: f64,
    pub average_latency_ms: f64,
    pub total_requests: u64,
    pub failed_requests: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAccessMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub pipeline_hits: u64,
    pub cache_hits: u64,
    pub api_calls: u64,
    pub fallback_calls: u64,
    pub average_latency_ms: f64,
    pub success_rate: f64,
    pub last_updated: u64,
}

#[derive(Debug, Clone)]
pub struct SuperAdminApiConfig {
    pub exchange_id: String,
    pub api_key: String,
    pub secret: String,
    pub is_read_only: bool,
}

impl SuperAdminApiConfig {
    pub fn new_read_only(exchange_id: String, api_key: String, secret: String) -> Self {
        Self {
            exchange_id,
            api_key,
            secret,
            is_read_only: true,
        }
    }
}

/// Comprehensive hybrid data access service implementing the standardized pattern:
/// Pipeline (primary) → KV Cache (fallback) → Real API (last resort)
pub struct HybridDataAccessService {
    pipelines_service: Option<CloudflarePipelinesService>,
    super_admin_configs: HashMap<String, SuperAdminApiConfig>,
    kv_store: KvStore,
    logger: Logger,
    metrics: DataAccessMetrics,
    config: HybridDataAccessConfig,
}

impl HybridDataAccessService {
    /// Alias for new_from_env for backward compatibility
    pub fn new(env: &worker::Env) -> ArbitrageResult<Self> {
        Self::new_from_env(env)
    }

    /// Constructor for compatibility with services expecting new(env)
    pub fn new_from_env(env: &worker::Env) -> ArbitrageResult<Self> {
        // Try MARKET_DATA_KV first, then fallback to ArbEdgeKV
        let kv_store = env
            .kv("MARKET_DATA_KV")
            .or_else(|_| env.kv("ArbEdgeKV"))
            .map_err(|e| {
                ArbitrageError::configuration_error(format!("Failed to get KV store: {}", e))
            })?;
        let logger = Logger::new(crate::utils::logger::LogLevel::Info);
        let config = HybridDataAccessConfig::default();

        Ok(Self {
            pipelines_service: None,
            super_admin_configs: HashMap::new(),
            kv_store,
            logger,
            metrics: DataAccessMetrics {
                total_requests: 0,
                successful_requests: 0,
                pipeline_hits: 0,
                cache_hits: 0,
                api_calls: 0,
                fallback_calls: 0,
                average_latency_ms: 0.0,
                success_rate: 0.0,
                last_updated: 0,
            },
            config,
        })
    }

    /// Constructor with configuration
    pub fn new_with_config(
        env: &worker::Env,
        config: HybridDataAccessConfig,
    ) -> ArbitrageResult<Self> {
        config.validate()?;
        // Try MARKET_DATA_KV first, then fallback to ArbEdgeKV
        let kv_store = env
            .kv("MARKET_DATA_KV")
            .or_else(|_| env.kv("ArbEdgeKV"))
            .map_err(|e| {
                ArbitrageError::configuration_error(format!("Failed to get KV store: {}", e))
            })?;
        let logger = Logger::new(crate::utils::logger::LogLevel::Info);

        Ok(Self {
            pipelines_service: None,
            super_admin_configs: HashMap::new(),
            kv_store,
            logger,
            metrics: DataAccessMetrics {
                total_requests: 0,
                successful_requests: 0,
                pipeline_hits: 0,
                cache_hits: 0,
                api_calls: 0,
                fallback_calls: 0,
                average_latency_ms: 0.0,
                success_rate: 0.0,
                last_updated: 0,
            },
            config,
        })
    }

    /// Constructor with services for flexibility
    pub fn new_with_services(
        pipelines_service: Option<CloudflarePipelinesService>,
        kv_store: KvStore,
        logger: Logger,
    ) -> Self {
        Self {
            pipelines_service,
            super_admin_configs: HashMap::new(),
            kv_store,
            logger,
            metrics: DataAccessMetrics {
                total_requests: 0,
                successful_requests: 0,
                pipeline_hits: 0,
                cache_hits: 0,
                api_calls: 0,
                fallback_calls: 0,
                average_latency_ms: 0.0,
                success_rate: 0.0,
                last_updated: 0,
            },
            config: HybridDataAccessConfig::default(),
        }
    }

    /// Configure super admin API for an exchange
    pub fn configure_super_admin_api(
        &mut self,
        exchange_id: String,
        api_key: String,
        secret: String,
    ) -> ArbitrageResult<()> {
        let config = SuperAdminApiConfig::new_read_only(exchange_id.clone(), api_key, secret);
        self.super_admin_configs.insert(exchange_id.clone(), config);

        self.logger.info(&format!(
            "Configured super admin API for exchange: {}",
            exchange_id
        ));

        Ok(())
    }

    /// Transform symbol for Binance API format
    fn transform_symbol_for_binance(symbol: &str) -> ArbitrageResult<String> {
        if symbol.is_empty() {
            return Err(ArbitrageError::validation_error("Symbol cannot be empty"));
        }

        let transformed = symbol.replace("-", "").replace("_", "").to_uppercase();

        // Enhanced validation for Binance symbol format
        if transformed.len() < 6 || transformed.len() > 12 {
            return Err(ArbitrageError::validation_error(
                "Invalid symbol length for Binance",
            ));
        }

        // Basic pattern validation (letters and numbers only)
        if !transformed.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ArbitrageError::validation_error(
                "Symbol contains invalid characters for Binance",
            ));
        }

        Ok(transformed)
    }

    /// Transform symbol for Bybit API format
    fn transform_symbol_for_bybit(symbol: &str) -> ArbitrageResult<String> {
        if symbol.is_empty() {
            return Err(ArbitrageError::validation_error("Symbol cannot be empty"));
        }

        let transformed = symbol.replace("-", "").replace("_", "").to_uppercase();

        // Enhanced validation for Bybit symbol format
        if transformed.len() < 6 || transformed.len() > 12 {
            return Err(ArbitrageError::validation_error(
                "Invalid symbol length for Bybit",
            ));
        }

        // Basic pattern validation (letters and numbers only)
        if !transformed.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ArbitrageError::validation_error(
                "Symbol contains invalid characters for Bybit",
            ));
        }

        Ok(transformed)
    }

    /// Transform symbol for OKX API format
    fn transform_symbol_for_okx(symbol: &str) -> ArbitrageResult<String> {
        if symbol.is_empty() {
            return Err(ArbitrageError::validation_error("Symbol cannot be empty"));
        }

        let transformed = symbol.to_uppercase();

        // Enhanced validation for OKX symbol format
        if transformed.len() < 6 || transformed.len() > 15 {
            return Err(ArbitrageError::validation_error(
                "Invalid symbol length for OKX",
            ));
        }

        // Basic pattern validation (letters, numbers, and hyphens only)
        if !transformed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(ArbitrageError::validation_error(
                "Symbol contains invalid characters for OKX",
            ));
        }

        Ok(transformed)
    }

    /// Fetch with retry logic and timeout
    async fn fetch_with_retry(&self, url: &str) -> ArbitrageResult<Response> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            let request = Request::new_with_init(url, RequestInit::new().with_method(Method::Get))?;

            match self.fetch_with_timeout(request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.config.max_retries {
                        // Exponential backoff: 1s, 2s, 4s, etc.
                        let delay_ms = (2_u64.pow(attempt) * 1000).min(10000);
                        #[cfg(target_arch = "wasm32")]
                        {
                            use gloo_timers::future::TimeoutFuture;
                            TimeoutFuture::new(delay_ms as u32).await;
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        }
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ArbitrageError::api_error("All retry attempts failed")))
    }

    /// Fetch with timeout handling using WASM-compatible approach
    async fn fetch_with_timeout(&self, request: Request) -> ArbitrageResult<Response> {
        // For WASM targets, use gloo-timers for timeout handling
        #[cfg(target_arch = "wasm32")]
        {
            use futures::future::{select, Either};
            use gloo_timers::future::TimeoutFuture;

            let timeout_ms = (self.config.api_timeout_seconds as u64) * 1000;
            let timeout_future = TimeoutFuture::new(timeout_ms as u32);

            // Create a longer-lived binding for the fetch request to avoid lifetime issues
            let fetch_request = Fetch::Request(request);
            let request_future = async move { fetch_request.send().await };

            match select(Box::pin(request_future), Box::pin(timeout_future)).await {
                Either::Left((result, _)) => match result {
                    Ok(response) => Ok(response),
                    Err(e) => Err(ArbitrageError::api_error(format!("Request failed: {}", e))),
                },
                Either::Right((_, _)) => Err(ArbitrageError::api_error(format!(
                    "Request timeout after {} seconds",
                    self.config.api_timeout_seconds
                ))),
            }
        }

        // For non-WASM targets, use tokio timeout
        #[cfg(not(target_arch = "wasm32"))]
        {
            let timeout_duration =
                std::time::Duration::from_secs(self.config.api_timeout_seconds as u64);
            match tokio::time::timeout(timeout_duration, Fetch::Request(request).send()).await {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(e)) => Err(ArbitrageError::api_error(format!("Request failed: {}", e))),
                Err(_) => Err(ArbitrageError::api_error(format!(
                    "Request timeout after {} seconds",
                    self.config.api_timeout_seconds
                ))),
            }
        }
    }

    /// Get market data using hybrid access pattern
    pub async fn get_market_data(
        &mut self,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        let start_time = js_sys::Date::now();
        self.metrics.total_requests += 1;

        // 1. Try pipelines (primary)
        if let Some(ref pipelines) = self.pipelines_service {
            // Check if Pipelines service is available before using it
            if pipelines.is_service_available().await {
                match self
                    .get_pipeline_market_data(pipelines, exchange, symbol)
                    .await
                {
                    Ok(mut data) => {
                        data.source = DataAccessSource::Pipeline;
                        self.metrics.pipeline_hits += 1;
                        self.update_metrics(start_time, true);

                        self.logger.info(&format!(
                            "Retrieved market data from pipeline: {}:{}",
                            exchange, symbol
                        ));

                        return Ok(data);
                    }
                    Err(e) => {
                        self.logger.warn(&format!(
                            "Pipeline data access failed for {}:{}: {}",
                            exchange, symbol, e
                        ));
                    }
                }
            } else {
                self.logger.debug(&format!(
                    "Pipelines service unavailable, skipping pipeline data access for {}:{}",
                    exchange, symbol
                ));
            }
        }

        // 2. Try KV cache (fallback)
        match self.get_cached_market_data(exchange, symbol).await {
            Ok(mut data) => {
                data.source = DataAccessSource::Cache;
                self.metrics.cache_hits += 1;
                self.update_metrics(start_time, true);

                self.logger.info(&format!(
                    "Retrieved market data from cache: {}:{}",
                    exchange, symbol
                ));

                return Ok(data);
            }
            Err(e) => {
                self.logger.warn(&format!(
                    "Cache data access failed for {}:{}: {}",
                    exchange, symbol, e
                ));
            }
        }

        // 3. Try super admin API (last resort)
        match self.fetch_from_super_admin_api(exchange, symbol).await {
            Ok(mut data) => {
                data.source = DataAccessSource::RealAPI;
                self.metrics.api_calls += 1;

                // Cache for future use
                let _ = self.cache_market_data(&data).await;

                // Store to pipeline if available
                if let Some(ref pipelines) = self.pipelines_service {
                    let _ = self.store_market_data_to_pipeline(pipelines, &data).await;
                }

                self.update_metrics(start_time, true);

                self.logger.info(&format!(
                    "Retrieved market data from real API: {}:{}",
                    exchange, symbol
                ));

                return Ok(data);
            }
            Err(e) => {
                self.logger.error(&format!(
                    "Real API data access failed for {}:{}: {}",
                    exchange, symbol, e
                ));
            }
        }

        self.metrics.fallback_calls += 1;
        self.update_metrics(start_time, false);

        Err(ArbitrageError::not_found(format!(
            "No data sources available for {}:{}",
            exchange, symbol
        )))
    }

    /// Get funding rate data using hybrid access pattern
    pub async fn get_funding_rate_data(
        &mut self,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        let start_time = js_sys::Date::now();
        self.metrics.total_requests += 1;

        // 1. Try pipelines (primary)
        if let Some(ref pipelines) = self.pipelines_service {
            // Check if Pipelines service is available before using it
            if pipelines.is_service_available().await {
                match self
                    .get_pipeline_funding_rate(pipelines, exchange, symbol)
                    .await
                {
                    Ok(funding_rate) => {
                        self.metrics.pipeline_hits += 1;
                        self.update_metrics(start_time, true);

                        self.logger.info(&format!(
                            "Retrieved funding rate from pipeline: {}:{}",
                            exchange, symbol
                        ));

                        return Ok(funding_rate);
                    }
                    Err(e) => {
                        self.logger.warn(&format!(
                            "Pipeline funding rate access failed for {}:{}: {}",
                            exchange, symbol, e
                        ));
                    }
                }
            } else {
                self.logger.debug(&format!(
                    "Pipelines service unavailable, skipping pipeline funding rate access for {}:{}",
                    exchange, symbol
                ));
            }
        }

        // 2. Try KV cache (fallback)
        match self.get_cached_funding_rate(exchange, symbol).await {
            Ok(funding_rate) => {
                self.metrics.cache_hits += 1;
                self.update_metrics(start_time, true);

                self.logger.info(&format!(
                    "Retrieved funding rate from cache: {}:{}",
                    exchange, symbol
                ));

                return Ok(funding_rate);
            }
            Err(e) => {
                self.logger.warn(&format!(
                    "Cache funding rate access failed for {}:{}: {}",
                    exchange, symbol, e
                ));
            }
        }

        // 3. Try super admin API (last resort)
        match self.fetch_funding_rate_from_api(exchange, symbol).await {
            Ok(funding_rate) => {
                self.metrics.api_calls += 1;

                // Cache for future use
                let _ = self.cache_funding_rate(&funding_rate, exchange).await;

                // Store to pipeline if available
                if let Some(ref pipelines) = self.pipelines_service {
                    let _ = self
                        .store_funding_rate_to_pipeline(pipelines, &funding_rate)
                        .await;
                }

                self.update_metrics(start_time, true);

                self.logger.info(&format!(
                    "Retrieved funding rate from real API: {}:{}",
                    exchange, symbol
                ));

                return Ok(funding_rate);
            }
            Err(e) => {
                self.logger.error(&format!(
                    "Real API funding rate access failed for {}:{}: {}",
                    exchange, symbol, e
                ));
            }
        }

        self.metrics.fallback_calls += 1;
        self.update_metrics(start_time, false);

        Err(ArbitrageError::not_found(format!(
            "No funding rate data sources available for {}:{}",
            exchange, symbol
        )))
    }

    /// Get pipeline market data
    async fn get_pipeline_market_data(
        &self,
        pipelines: &CloudflarePipelinesService,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        let data = pipelines
            .get_latest_data(&format!("{}:{}", exchange, symbol))
            .await?;

        // Parse pipeline data to MarketDataSnapshot
        if let Some(price) = data.get("price").and_then(|v| v.as_f64()) {
            Ok(MarketDataSnapshot {
                exchange: *exchange,
                symbol: symbol.to_string(),
                timestamp: data.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0),
                price,
                volume_24h: data.get("volume_24h").and_then(|v| v.as_f64()),
                funding_rate: data.get("funding_rate").and_then(|v| v.as_f64()),
                bid: data.get("bid").and_then(|v| v.as_f64()),
                ask: data.get("ask").and_then(|v| v.as_f64()),
                high_24h: data.get("high_24h").and_then(|v| v.as_f64()),
                low_24h: data.get("low_24h").and_then(|v| v.as_f64()),
                change_24h: data.get("change_24h").and_then(|v| v.as_f64()),
                source: DataAccessSource::Pipeline,
            })
        } else {
            Err(ArbitrageError::parse_error("Invalid pipeline data format"))
        }
    }

    /// Get cached market data
    async fn get_cached_market_data(
        &self,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        let cache_key = format!("hybrid_market_data:{}:{}", exchange, symbol);

        match self.kv_store.get(&cache_key).text().await {
            Ok(Some(cached_data)) => {
                match serde_json::from_str::<MarketDataSnapshot>(&cached_data) {
                    Ok(data) => Ok(data),
                    Err(e) => Err(ArbitrageError::parse_error(format!(
                        "Failed to parse cached market data: {}",
                        e
                    ))),
                }
            }
            Ok(None) => Err(ArbitrageError::not_found("Cache miss for market data")),
            Err(e) => Err(ArbitrageError::cache_error(format!(
                "Failed to retrieve cached market data: {}",
                e
            ))),
        }
    }

    /// Fetch from super admin API
    async fn fetch_from_super_admin_api(
        &self,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        let exchange_str = match exchange {
            ExchangeIdEnum::Binance => "binance",
            ExchangeIdEnum::Bybit => "bybit",
            ExchangeIdEnum::OKX => "okx",
            _ => {
                return Err(ArbitrageError::not_implemented(format!(
                    "Exchange {:?} not supported for super admin API",
                    exchange
                )))
            }
        };

        if !self.super_admin_configs.contains_key(exchange_str) {
            return Err(ArbitrageError::configuration_error(format!(
                "No super admin API configured for {}",
                exchange_str
            )));
        }

        match exchange {
            ExchangeIdEnum::Binance => self.fetch_binance_data(symbol).await,
            ExchangeIdEnum::Bybit => self.fetch_bybit_data(symbol).await,
            ExchangeIdEnum::OKX => self.fetch_okx_data(symbol).await,
            _ => Err(ArbitrageError::not_implemented(format!(
                "Exchange {:?} not implemented",
                exchange
            ))),
        }
    }

    /// Fetch Binance data
    async fn fetch_binance_data(&self, symbol: &str) -> ArbitrageResult<MarketDataSnapshot> {
        let binance_symbol = Self::transform_symbol_for_binance(symbol)?;
        let url = format!(
            "https://api.binance.com/api/v3/ticker/24hr?symbol={}",
            binance_symbol
        );

        // Apply retry logic with timeout
        let mut response = self.fetch_with_retry(&url).await?;

        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(format!(
                "Binance API error: {}",
                response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let data: serde_json::Value = serde_json::from_str(&response_text)?;

        // Parse price with proper error handling
        let price = data["lastPrice"]
            .as_str()
            .ok_or_else(|| ArbitrageError::parse_error("Missing lastPrice field"))?
            .parse::<f64>()
            .map_err(|e| ArbitrageError::parse_error(format!("Invalid lastPrice format: {}", e)))?;

        Ok(MarketDataSnapshot {
            exchange: ExchangeIdEnum::Binance,
            symbol: symbol.to_string(),
            timestamp: Utc::now().timestamp_millis() as u64,
            price,
            volume_24h: data["volume"].as_str().and_then(|s| s.parse().ok()),
            funding_rate: None, // Will be fetched separately
            bid: data["bidPrice"].as_str().and_then(|s| s.parse().ok()),
            ask: data["askPrice"].as_str().and_then(|s| s.parse().ok()),
            high_24h: data["highPrice"].as_str().and_then(|s| s.parse().ok()),
            low_24h: data["lowPrice"].as_str().and_then(|s| s.parse().ok()),
            change_24h: data["priceChange"].as_str().and_then(|s| s.parse().ok()),
            source: DataAccessSource::RealAPI,
        })
    }

    /// Fetch Bybit data
    async fn fetch_bybit_data(&self, symbol: &str) -> ArbitrageResult<MarketDataSnapshot> {
        let bybit_symbol = Self::transform_symbol_for_bybit(symbol)?;
        let url = format!(
            "https://api.bybit.com/v5/market/tickers?category=spot&symbol={}",
            bybit_symbol
        );

        // Apply retry logic with timeout
        let mut response = self.fetch_with_retry(&url).await?;

        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(format!(
                "Bybit API error: {}",
                response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(result) = response_json.get("result") {
            if let Some(list) = result.get("list").and_then(|l| l.as_array()) {
                if let Some(ticker) = list.first() {
                    // Parse price with proper error handling
                    let price = ticker["lastPrice"]
                        .as_str()
                        .ok_or_else(|| {
                            ArbitrageError::parse_error("Missing lastPrice field in Bybit response")
                        })?
                        .parse::<f64>()
                        .map_err(|e| {
                            ArbitrageError::parse_error(format!(
                                "Invalid Bybit lastPrice format: {}",
                                e
                            ))
                        })?;

                    return Ok(MarketDataSnapshot {
                        exchange: ExchangeIdEnum::Bybit,
                        symbol: symbol.to_string(),
                        timestamp: Utc::now().timestamp_millis() as u64,
                        price,
                        volume_24h: ticker["volume24h"].as_str().and_then(|s| s.parse().ok()),
                        funding_rate: None, // Will be fetched separately
                        bid: ticker["bid1Price"].as_str().and_then(|s| s.parse().ok()),
                        ask: ticker["ask1Price"].as_str().and_then(|s| s.parse().ok()),
                        high_24h: ticker["highPrice24h"].as_str().and_then(|s| s.parse().ok()),
                        low_24h: ticker["lowPrice24h"].as_str().and_then(|s| s.parse().ok()),
                        change_24h: ticker["price24hPcnt"].as_str().and_then(|s| s.parse().ok()),
                        source: DataAccessSource::RealAPI,
                    });
                }
            }
        }

        Err(ArbitrageError::parse_error(
            "Failed to parse Bybit ticker data",
        ))
    }

    /// Fetch OKX data
    async fn fetch_okx_data(&self, symbol: &str) -> ArbitrageResult<MarketDataSnapshot> {
        let okx_symbol = Self::transform_symbol_for_okx(symbol)?;
        let url = format!(
            "https://www.okx.com/api/v5/market/ticker?instId={}",
            okx_symbol
        );

        // Apply retry logic with timeout
        let mut response = self.fetch_with_retry(&url).await?;

        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(format!(
                "OKX API error: {}",
                response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(data) = response_json.get("data").and_then(|d| d.as_array()) {
            if let Some(ticker) = data.first() {
                // Parse price with proper error handling
                let price = ticker["last"]
                    .as_str()
                    .ok_or_else(|| {
                        ArbitrageError::parse_error("Missing last field in OKX response")
                    })?
                    .parse::<f64>()
                    .map_err(|e| {
                        ArbitrageError::parse_error(format!("Invalid OKX last price format: {}", e))
                    })?;

                return Ok(MarketDataSnapshot {
                    exchange: ExchangeIdEnum::OKX,
                    symbol: symbol.to_string(),
                    timestamp: Utc::now().timestamp_millis() as u64,
                    price,
                    volume_24h: ticker["vol24h"].as_str().and_then(|s| s.parse().ok()),
                    funding_rate: None, // Will be fetched separately
                    bid: ticker["bidPx"].as_str().and_then(|s| s.parse().ok()),
                    ask: ticker["askPx"].as_str().and_then(|s| s.parse().ok()),
                    high_24h: ticker["high24h"].as_str().and_then(|s| s.parse().ok()),
                    low_24h: ticker["low24h"].as_str().and_then(|s| s.parse().ok()),
                    change_24h: ticker["open24h"].as_str().and_then(|s| s.parse().ok()),
                    source: DataAccessSource::RealAPI,
                });
            }
        }

        Err(ArbitrageError::parse_error(
            "Failed to parse OKX ticker data",
        ))
    }

    /// Cache market data
    async fn cache_market_data(&self, data: &MarketDataSnapshot) -> ArbitrageResult<()> {
        let cache_key = format!("hybrid_market_data:{}:{}", data.exchange, data.symbol);
        let cache_value = serde_json::to_string(data)?;

        self.kv_store
            .put(&cache_key, cache_value)?
            .expiration_ttl(self.config.cache_ttl_seconds)
            .execute()
            .await?;

        Ok(())
    }

    /// Store market data to pipeline
    async fn store_market_data_to_pipeline(
        &self,
        pipelines: &CloudflarePipelinesService,
        data: &MarketDataSnapshot,
    ) -> ArbitrageResult<()> {
        let pipeline_data = serde_json::json!({
            "exchange": data.exchange,
            "symbol": data.symbol,
            "timestamp": data.timestamp,
            "price": data.price,
            "volume_24h": data.volume_24h,
            "funding_rate": data.funding_rate,
            "bid": data.bid,
            "ask": data.ask,
            "high_24h": data.high_24h,
            "low_24h": data.low_24h,
            "change_24h": data.change_24h,
            "source": "hybrid_data_access",
            "data_type": "market_data"
        });

        pipelines
            .store_market_data(&data.exchange.to_string(), &data.symbol, &pipeline_data)
            .await?;

        Ok(())
    }

    /// Get pipeline funding rate
    async fn get_pipeline_funding_rate(
        &self,
        pipelines: &CloudflarePipelinesService,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        let data = pipelines
            .get_latest_data(&format!("funding_rate:{}:{}", exchange, symbol))
            .await?;

        if let Some(funding_rate) = data.get("funding_rate").and_then(|v| v.as_f64()) {
            Ok(FundingRateInfo {
                symbol: symbol.to_string(),
                funding_rate,
                timestamp: data
                    .get("timestamp")
                    .and_then(|v| v.as_u64())
                    .and_then(|ts| chrono::DateTime::from_timestamp((ts / 1000) as i64, 0)),
                datetime: data
                    .get("datetime")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                next_funding_time: data
                    .get("next_funding_time")
                    .and_then(|v| v.as_u64())
                    .and_then(|ts| chrono::DateTime::from_timestamp((ts / 1000) as i64, 0)),
                estimated_rate: data.get("estimated_rate").and_then(|v| v.as_f64()),
            })
        } else {
            Err(ArbitrageError::parse_error(
                "Invalid pipeline funding rate data",
            ))
        }
    }

    /// Get cached funding rate
    async fn get_cached_funding_rate(
        &self,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        let cache_key = format!("hybrid_funding_rate:{}:{}", exchange, symbol);

        match self.kv_store.get(&cache_key).text().await {
            Ok(Some(cached_data)) => match serde_json::from_str::<FundingRateInfo>(&cached_data) {
                Ok(data) => Ok(data),
                Err(e) => Err(ArbitrageError::parse_error(format!(
                    "Failed to parse cached funding rate: {}",
                    e
                ))),
            },
            Ok(None) => Err(ArbitrageError::not_found("Cache miss for funding rate")),
            Err(e) => Err(ArbitrageError::cache_error(format!(
                "Failed to retrieve cached funding rate: {}",
                e
            ))),
        }
    }

    /// Fetch funding rate from API
    async fn fetch_funding_rate_from_api(
        &self,
        exchange: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        match exchange {
            ExchangeIdEnum::Binance => self.fetch_binance_funding_rate(symbol).await,
            ExchangeIdEnum::Bybit => self.fetch_bybit_funding_rate(symbol).await,
            _ => Err(ArbitrageError::not_implemented(format!(
                "Funding rate not implemented for {:?}",
                exchange
            ))),
        }
    }

    /// Fetch Binance funding rate
    async fn fetch_binance_funding_rate(&self, symbol: &str) -> ArbitrageResult<FundingRateInfo> {
        let binance_symbol = Self::transform_symbol_for_binance(symbol)?;
        let url = format!(
            "https://fapi.binance.com/fapi/v1/premiumIndex?symbol={}",
            binance_symbol
        );

        let mut response = self.fetch_with_retry(&url).await?;

        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(format!(
                "Binance funding rate API error: {}",
                response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let data: serde_json::Value = serde_json::from_str(&response_text)?;

        let funding_rate = data["lastFundingRate"]
            .as_str()
            .ok_or_else(|| {
                ArbitrageError::parse_error("Missing lastFundingRate field in Binance response")
            })?
            .parse::<f64>()
            .map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Invalid Binance lastFundingRate format: {}",
                    e
                ))
            })?;

        Ok(FundingRateInfo {
            symbol: symbol.to_string(),
            funding_rate,
            timestamp: Some(Utc::now()),
            datetime: Some(Utc::now().to_rfc3339()),
            next_funding_time: data["nextFundingTime"]
                .as_u64()
                .and_then(|ts| chrono::DateTime::from_timestamp((ts / 1000) as i64, 0)),
            estimated_rate: data["estimatedSettlePrice"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok()),
        })
    }

    /// Fetch Bybit funding rate
    async fn fetch_bybit_funding_rate(&self, symbol: &str) -> ArbitrageResult<FundingRateInfo> {
        let bybit_symbol = Self::transform_symbol_for_bybit(symbol)?;
        let url = format!(
            "https://api.bybit.com/v5/market/funding/history?category=linear&symbol={}&limit=1",
            bybit_symbol
        );

        let mut response = self.fetch_with_retry(&url).await?;

        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(format!(
                "Bybit funding rate API error: {}",
                response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(result) = response_json.get("result") {
            if let Some(list) = result.get("list").and_then(|l| l.as_array()) {
                if let Some(funding_data) = list.first() {
                    let funding_rate = funding_data["fundingRate"]
                        .as_str()
                        .ok_or_else(|| {
                            ArbitrageError::parse_error(
                                "Missing fundingRate field in Bybit response",
                            )
                        })?
                        .parse::<f64>()
                        .map_err(|e| {
                            ArbitrageError::parse_error(format!(
                                "Invalid Bybit fundingRate format: {}",
                                e
                            ))
                        })?;

                    return Ok(FundingRateInfo {
                        symbol: symbol.to_string(),
                        funding_rate,
                        timestamp: Some(Utc::now()),
                        datetime: Some(Utc::now().to_rfc3339()),
                        next_funding_time: None,
                        estimated_rate: None,
                    });
                }
            }
        }

        Err(ArbitrageError::parse_error(
            "Failed to parse Bybit funding rate",
        ))
    }

    /// Cache funding rate
    async fn cache_funding_rate(
        &self,
        funding_rate: &FundingRateInfo,
        exchange: &ExchangeIdEnum,
    ) -> ArbitrageResult<()> {
        let cache_key = format!("hybrid_funding_rate:{}:{}", exchange, funding_rate.symbol);
        let cache_value = serde_json::to_string(funding_rate)?;

        self.kv_store
            .put(&cache_key, cache_value)?
            .expiration_ttl(self.config.cache_ttl_seconds)
            .execute()
            .await?;

        Ok(())
    }

    /// Store funding rate to pipeline
    async fn store_funding_rate_to_pipeline(
        &self,
        pipelines: &CloudflarePipelinesService,
        funding_rate: &FundingRateInfo,
    ) -> ArbitrageResult<()> {
        let pipeline_data = serde_json::json!({
            "symbol": funding_rate.symbol,
            "funding_rate": funding_rate.funding_rate,
            "timestamp": funding_rate.timestamp.map(|t| t.timestamp_millis()),
            "datetime": funding_rate.datetime,
            "next_funding_time": funding_rate.next_funding_time.map(|t| t.timestamp_millis()),
            "estimated_rate": funding_rate.estimated_rate,
            "source": "hybrid_data_access",
            "data_type": "funding_rate"
        });

        pipelines
            .store_analysis_results("funding_rate_data", &pipeline_data)
            .await?;
        Ok(())
    }

    /// Update metrics
    fn update_metrics(&mut self, start_time: f64, success: bool) {
        let latency = js_sys::Date::now() - start_time;

        // Update average latency
        let total_requests = self.metrics.total_requests as f64;
        if total_requests > 0.0 {
            self.metrics.average_latency_ms =
                (self.metrics.average_latency_ms * (total_requests - 1.0) + latency)
                    / total_requests;
        } else {
            self.metrics.average_latency_ms = latency;
        }

        // Track success/failure using separate counter to avoid precision errors
        if success {
            self.metrics.successful_requests += 1;
        }

        // Calculate success rate using integer counters
        if self.metrics.total_requests > 0 {
            self.metrics.success_rate =
                (self.metrics.successful_requests as f64) / (self.metrics.total_requests as f64);
        } else {
            self.metrics.success_rate = 0.0;
        }

        self.metrics.last_updated = Utc::now().timestamp_millis() as u64;
    }

    /// Get metrics
    pub fn get_metrics(&self) -> &DataAccessMetrics {
        &self.metrics
    }

    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = DataAccessMetrics {
            total_requests: 0,
            successful_requests: 0,
            pipeline_hits: 0,
            cache_hits: 0,
            api_calls: 0,
            fallback_calls: 0,
            average_latency_ms: 0.0,
            success_rate: 0.0,
            last_updated: 0,
        };
    }

    /// Helper function to execute operations with timeout for both WASM and non-WASM targets
    async fn execute_with_timeout<F, T>(
        &self,
        future: F,
        timeout_seconds: u64,
    ) -> std::result::Result<T, &'static str>
    where
        F: std::future::Future<Output = T>,
    {
        #[cfg(target_arch = "wasm32")]
        {
            use futures::future::{select, Either};
            use gloo_timers::future::TimeoutFuture;

            let timeout_future = TimeoutFuture::new((timeout_seconds * 1000) as u32);
            match select(Box::pin(future), Box::pin(timeout_future)).await {
                Either::Left((result, _)) => Ok(result),
                Either::Right((_, _)) => Err("timeout"),
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let timeout_duration = std::time::Duration::from_secs(timeout_seconds);
            match tokio::time::timeout(timeout_duration, future).await {
                Ok(result) => Ok(result),
                Err(_) => Err("timeout"),
            }
        }
    }

    /// Health check
    pub async fn health_check(&self) -> ArbitrageResult<serde_json::Value> {
        let test_key = format!("health_check_{}", Utc::now().timestamp_millis());

        let mut health_status = serde_json::json!({
            "status": "healthy",
            "pipelines_available": self.pipelines_service.is_some(),
            "super_admin_apis_configured": self.super_admin_configs.len(),
            "cache_available": true,
            "metrics": self.metrics
        });

        // Test pipeline connectivity if available
        if let Some(ref pipelines) = self.pipelines_service {
            match self
                .execute_with_timeout(pipelines.get_latest_data(&test_key), 5)
                .await
            {
                Ok(Ok(_)) => {
                    health_status["pipelines_status"] = serde_json::json!("connected");
                }
                Ok(Err(_)) | Err(_) => {
                    health_status["pipelines_status"] = serde_json::json!("disconnected");
                    health_status["status"] = serde_json::json!("degraded");
                }
            }
        }

        // Test KV store connectivity (both write and read capability)
        let test_value = "health_check_test";
        match self
            .execute_with_timeout(self.kv_store.put(&test_key, test_value)?.execute(), 5)
            .await
        {
            Ok(Ok(_)) => {
                // Now test read capability
                match self
                    .execute_with_timeout(self.kv_store.get(&test_key).text(), 5)
                    .await
                {
                    Ok(Ok(_)) => {
                        health_status["kv_status"] = serde_json::json!("connected");
                    }
                    Ok(Err(_)) | Err(_) => {
                        health_status["kv_status"] = serde_json::json!("read_failed");
                        health_status["status"] = serde_json::json!("degraded");
                    }
                }
            }
            Ok(Err(_)) | Err(_) => {
                health_status["kv_status"] = serde_json::json!("write_failed");
                health_status["status"] = serde_json::json!("degraded");
            }
        }

        // Cleanup test data (ignore errors)
        let _ = self.kv_store.delete(&test_key).await;

        Ok(health_status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_data_snapshot_creation() {
        let snapshot = MarketDataSnapshot {
            exchange: ExchangeIdEnum::Binance,
            symbol: "BTC-USDT".to_string(),
            timestamp: 1640995200000,
            price: 50000.0,
            volume_24h: Some(1000.0),
            funding_rate: Some(0.0001),
            bid: Some(49999.0),
            ask: Some(50001.0),
            high_24h: Some(51000.0),
            low_24h: Some(49000.0),
            change_24h: Some(1000.0),
            source: DataAccessSource::Pipeline,
        };

        assert_eq!(snapshot.exchange, ExchangeIdEnum::Binance);
        assert_eq!(snapshot.symbol, "BTC-USDT");
        assert_eq!(snapshot.price, 50000.0);
        assert!(matches!(snapshot.source, DataAccessSource::Pipeline));
    }

    #[test]
    fn test_data_access_metrics_creation() {
        let metrics = DataAccessMetrics {
            total_requests: 100,
            successful_requests: 100,
            pipeline_hits: 60,
            cache_hits: 30,
            api_calls: 10,
            fallback_calls: 0,
            average_latency_ms: 150.0,
            success_rate: 1.0,
            last_updated: 1640995200000,
        };

        assert_eq!(metrics.total_requests, 100);
        assert_eq!(metrics.successful_requests, 100);
        assert_eq!(metrics.pipeline_hits, 60);
        assert_eq!(metrics.cache_hits, 30);
        assert_eq!(metrics.api_calls, 10);
        assert_eq!(metrics.success_rate, 1.0);
    }

    #[test]
    fn test_super_admin_api_config_creation() {
        let config = SuperAdminApiConfig::new_read_only(
            "binance".to_string(),
            "test_api_key".to_string(),
            "test_secret".to_string(),
        );

        assert_eq!(config.exchange_id, "binance");
        assert_eq!(config.api_key, "test_api_key");
        assert_eq!(config.secret, "test_secret");
        assert!(config.is_read_only);
    }

    #[test]
    fn test_data_access_source_enum() {
        let sources = [
            DataAccessSource::Pipeline,
            DataAccessSource::Cache,
            DataAccessSource::RealAPI,
            DataAccessSource::Fallback,
        ];

        assert_eq!(sources.len(), 4);
        assert!(matches!(sources[0], DataAccessSource::Pipeline));
        assert!(matches!(sources[1], DataAccessSource::Cache));
        assert!(matches!(sources[2], DataAccessSource::RealAPI));
        assert!(matches!(sources[3], DataAccessSource::Fallback));
    }

    #[test]
    fn test_hybrid_data_access_config_validation() {
        // Test invalid cache TTL (0)
        let result = HybridDataAccessConfig::new_validated(
            0, // Invalid cache TTL
            10, 30, 3, true, 600, true, 300, true, 60,
        );
        assert!(result.is_err());

        // Test invalid pipeline timeout (0)
        let result = HybridDataAccessConfig::new_validated(
            300, 0, // Invalid pipeline timeout
            30, 3, true, 600, true, 300, true, 60,
        );
        assert!(result.is_err());

        // Test invalid API timeout (too high)
        let result = HybridDataAccessConfig::new_validated(
            300, 10, 700, // Invalid API timeout (> 600)
            3, true, 600, true, 300, true, 60,
        );
        assert!(result.is_err());

        // Test invalid max retries (too high)
        let result = HybridDataAccessConfig::new_validated(
            300, 10, 30, 15, // Invalid max retries (> 10)
            true, 600, true, 300, true, 60,
        );
        assert!(result.is_err());

        // Test valid configuration
        let result =
            HybridDataAccessConfig::new_validated(300, 10, 30, 3, true, 600, true, 300, true, 60);
        assert!(result.is_ok());
    }

    #[test]
    fn test_symbol_transformation() {
        // Test Binance symbol transformation
        assert_eq!(
            HybridDataAccessService::transform_symbol_for_binance("BTC-USDT").unwrap(),
            "BTCUSDT"
        );
        assert_eq!(
            HybridDataAccessService::transform_symbol_for_binance("ETH_USD").unwrap(),
            "ETHUSD"
        );

        // Test Bybit symbol transformation
        assert_eq!(
            HybridDataAccessService::transform_symbol_for_bybit("BTC-USDT").unwrap(),
            "BTCUSDT"
        );

        // Test OKX symbol transformation (preserves hyphens)
        assert_eq!(
            HybridDataAccessService::transform_symbol_for_okx("BTC-USDT").unwrap(),
            "BTC-USDT"
        );

        // Test invalid symbols
        assert!(HybridDataAccessService::transform_symbol_for_binance("").is_err());
        assert!(HybridDataAccessService::transform_symbol_for_bybit("").is_err());
        assert!(HybridDataAccessService::transform_symbol_for_okx("").is_err());

        // Test symbols with invalid characters
        assert!(HybridDataAccessService::transform_symbol_for_binance("BTC@USDT").is_err());
        assert!(HybridDataAccessService::transform_symbol_for_bybit("BTC@USDT").is_err());
        assert!(HybridDataAccessService::transform_symbol_for_okx("BTC@USDT").is_err());

        // Test symbols with invalid length
        assert!(HybridDataAccessService::transform_symbol_for_binance("BTC").is_err());
        assert!(HybridDataAccessService::transform_symbol_for_bybit("BTC").is_err());
        assert!(HybridDataAccessService::transform_symbol_for_okx("BTC").is_err());
    }

    #[test]
    fn test_config_default_values() {
        let config = HybridDataAccessConfig::default();

        assert_eq!(config.cache_ttl_seconds, 300);
        assert_eq!(config.pipeline_timeout_seconds, 10);
        assert_eq!(config.api_timeout_seconds, 30);
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_data_freshness_validation);
        assert_eq!(config.freshness_threshold_seconds, 600);
        assert!(config.enable_automatic_refresh);
        assert_eq!(config.refresh_interval_seconds, 300);
        assert!(config.enable_health_monitoring);
        assert_eq!(config.health_check_interval_seconds, 60);
    }

    #[test]
    fn test_data_source_health_creation() {
        let health = DataSourceHealth {
            source: DataAccessSource::Pipeline,
            is_healthy: true,
            last_success_timestamp: 1640995200000,
            last_error: None,
            success_rate: 0.95,
            average_latency_ms: 150.0,
            total_requests: 100,
            failed_requests: 5,
        };

        assert!(matches!(health.source, DataAccessSource::Pipeline));
        assert!(health.is_healthy);
        assert_eq!(health.success_rate, 0.95);
        assert_eq!(health.total_requests, 100);
        assert_eq!(health.failed_requests, 5);
    }
}
