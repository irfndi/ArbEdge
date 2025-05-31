// API Connector - Exchange API Integration with Rate Limiting Component
// Provides unified API access with per-exchange rate limiting and intelligent retry logic

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Supported exchanges for API integration
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeType {
    Binance,
    Bybit,
    OKX,
    Coinbase,
    Kraken,
    Huobi,
    KuCoin,
    Generic,
}

impl ExchangeType {
    pub fn as_str(&self) -> &str {
        match self {
            ExchangeType::Binance => "binance",
            ExchangeType::Bybit => "bybit",
            ExchangeType::OKX => "okx",
            ExchangeType::Coinbase => "coinbase",
            ExchangeType::Kraken => "kraken",
            ExchangeType::Huobi => "huobi",
            ExchangeType::KuCoin => "kucoin",
            ExchangeType::Generic => "generic",
        }
    }

    pub fn default_rate_limit_per_minute(&self) -> u32 {
        match self {
            ExchangeType::Binance => 1200,  // 1200 requests per minute
            ExchangeType::Bybit => 600,     // 600 requests per minute
            ExchangeType::OKX => 300,       // 300 requests per minute
            ExchangeType::Coinbase => 1000, // 1000 requests per minute
            ExchangeType::Kraken => 900,    // 900 requests per minute
            ExchangeType::Huobi => 600,     // 600 requests per minute
            ExchangeType::KuCoin => 1800,   // 1800 requests per minute
            ExchangeType::Generic => 300,   // Conservative default
        }
    }

    pub fn default_timeout_seconds(&self) -> u64 {
        match self {
            ExchangeType::Binance => 10,
            ExchangeType::Bybit => 15,
            ExchangeType::OKX => 12,
            ExchangeType::Coinbase => 20,
            ExchangeType::Kraken => 25,
            ExchangeType::Huobi => 15,
            ExchangeType::KuCoin => 18,
            ExchangeType::Generic => 30,
        }
    }

    pub fn base_url(&self) -> &str {
        match self {
            ExchangeType::Binance => "https://api.binance.com",
            ExchangeType::Bybit => "https://api.bybit.com",
            ExchangeType::OKX => "https://www.okx.com",
            ExchangeType::Coinbase => "https://api.exchange.coinbase.com",
            ExchangeType::Kraken => "https://api.kraken.com",
            ExchangeType::Huobi => "https://api.huobi.pro",
            ExchangeType::KuCoin => "https://api.kucoin.com",
            ExchangeType::Generic => "",
        }
    }
}

/// Rate limiting configuration for exchanges
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst_limit: u32,
    pub window_size_seconds: u64,
    pub enable_adaptive_limiting: bool,
    pub backoff_multiplier: f64,
    pub max_backoff_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 300,
            burst_limit: 10,
            window_size_seconds: 60,
            enable_adaptive_limiting: true,
            backoff_multiplier: 2.0,
            max_backoff_seconds: 300, // 5 minutes
        }
    }
}

/// Exchange-specific configuration
#[derive(Debug, Clone)]
pub struct ExchangeConfig {
    pub exchange_type: ExchangeType,
    pub base_url: String,
    pub api_key: Option<String>,
    pub secret_key: Option<String>,
    pub passphrase: Option<String>,
    pub rate_limit: RateLimitConfig,
    pub timeout_seconds: u64,
    pub enable_retry: bool,
    pub max_retries: u32,
    pub enable_health_check: bool,
    pub health_check_interval_seconds: u64,
}

impl ExchangeConfig {
    pub fn new(exchange_type: ExchangeType) -> Self {
        let rate_limit = RateLimitConfig {
            requests_per_minute: exchange_type.default_rate_limit_per_minute(),
            ..Default::default()
        };

        Self {
            base_url: exchange_type.base_url().to_string(),
            timeout_seconds: exchange_type.default_timeout_seconds(),
            exchange_type,
            api_key: None,
            secret_key: None,
            passphrase: None,
            rate_limit,
            enable_retry: true,
            max_retries: 3,
            enable_health_check: true,
            health_check_interval_seconds: 300, // 5 minutes
        }
    }

    pub fn with_credentials(
        mut self,
        api_key: String,
        secret_key: String,
        passphrase: Option<String>,
    ) -> Self {
        self.api_key = Some(api_key);
        self.secret_key = Some(secret_key);
        self.passphrase = passphrase;
        self
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.base_url.is_empty() {
            return Err(ArbitrageError::validation_error("base_url cannot be empty"));
        }
        if self.timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "timeout_seconds must be greater than 0",
            ));
        }
        if self.rate_limit.requests_per_minute == 0 {
            return Err(ArbitrageError::validation_error(
                "requests_per_minute must be greater than 0",
            ));
        }
        if self.max_retries > 10 {
            return Err(ArbitrageError::validation_error(
                "max_retries must not exceed 10",
            ));
        }
        Ok(())
    }
}

/// API request information
#[derive(Debug, Clone)]
pub struct APIRequest {
    pub exchange: ExchangeType,
    pub endpoint: String,
    pub method: String,
    pub params: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub priority: u8, // 1 = high, 2 = medium, 3 = low
}

impl APIRequest {
    pub fn new(exchange: ExchangeType, endpoint: String, method: String) -> Self {
        Self {
            exchange,
            endpoint,
            method,
            params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            timeout_seconds: None,
            priority: 2, // Default to medium priority
        }
    }

    pub fn with_params(mut self, params: HashMap<String, String>) -> Self {
        self.params = params;
        self
    }

    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    pub fn with_body(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// API response information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub latency_ms: u64,
    pub exchange: ExchangeType,
    pub endpoint: String,
    pub timestamp: u64,
    pub rate_limit_remaining: Option<u32>,
    pub rate_limit_reset: Option<u64>,
}

/// API health status for exchanges
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIHealth {
    pub exchange: ExchangeType,
    pub is_healthy: bool,
    pub last_success_timestamp: u64,
    pub last_error: Option<String>,
    pub success_rate_percent: f32,
    pub average_latency_ms: f64,
    pub rate_limit_status: String,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub last_health_check: u64,
}

impl Default for APIHealth {
    fn default() -> Self {
        Self {
            exchange: ExchangeType::Generic,
            is_healthy: false,
            last_success_timestamp: 0,
            last_error: None,
            success_rate_percent: 0.0,
            average_latency_ms: 0.0,
            rate_limit_status: "unknown".to_string(),
            total_requests: 0,
            failed_requests: 0,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// API performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIMetrics {
    pub exchange: ExchangeType,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rate_limited_requests: u64,
    pub timeout_requests: u64,
    pub retry_requests: u64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub requests_per_minute: f64,
    pub success_rate_percent: f32,
    pub last_updated: u64,
}

impl Default for APIMetrics {
    fn default() -> Self {
        Self {
            exchange: ExchangeType::Generic,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            rate_limited_requests: 0,
            timeout_requests: 0,
            retry_requests: 0,
            average_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            max_latency_ms: 0.0,
            requests_per_minute: 0.0,
            success_rate_percent: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Rate limiter for tracking API usage
#[derive(Debug)]
struct RateLimiter {
    requests: Vec<u64>,
    config: RateLimitConfig,
    last_request_time: u64,
    current_backoff: u64,
}

impl RateLimiter {
    fn new(config: RateLimitConfig) -> Self {
        Self {
            requests: Vec::new(),
            config,
            last_request_time: 0,
            current_backoff: 0,
        }
    }

    fn can_make_request(&mut self) -> bool {
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Remove old requests outside the window
        let window_start = now - (self.config.window_size_seconds * 1000);
        self.requests.retain(|&timestamp| timestamp > window_start);

        // Check if we're within rate limits
        let requests_in_window = self.requests.len() as u32;

        // Check burst limit
        if requests_in_window >= self.config.burst_limit {
            return false;
        }

        // Check rate limit
        let max_requests_in_window = (((self.config.requests_per_minute as f64)
            * (self.config.window_size_seconds as f64 / 60.0))
            .ceil()) as u32;
        if requests_in_window >= max_requests_in_window {
            return false;
        }

        // Check backoff
        if self.current_backoff > 0
            && (now - self.last_request_time) < (self.current_backoff * 1000)
        {
            return false;
        }

        true
    }

    fn record_request(&mut self) {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        self.requests.push(now);
        self.last_request_time = now;

        // Reset backoff on successful request
        self.current_backoff = 0;
    }

    fn record_rate_limit(&mut self) {
        if self.config.enable_adaptive_limiting {
            self.current_backoff = if self.current_backoff == 0 {
                1
            } else {
                ((self.current_backoff as f64 * self.config.backoff_multiplier) as u64)
                    .min(self.config.max_backoff_seconds)
            };
        }
    }

    fn get_wait_time_seconds(&self) -> u64 {
        if self.current_backoff > 0 {
            let now = chrono::Utc::now().timestamp_millis() as u64;
            let elapsed = (now - self.last_request_time) / 1000;
            if elapsed < self.current_backoff {
                return self.current_backoff - elapsed;
            }
        }
        0
    }
}

/// Configuration for APIConnector
#[derive(Debug, Clone)]
pub struct APIConnectorConfig {
    pub enable_rate_limiting: bool,
    pub enable_retry_logic: bool,
    pub enable_health_monitoring: bool,
    pub enable_performance_tracking: bool,
    pub default_timeout_seconds: u64,
    pub max_concurrent_requests: u32,
    pub connection_pool_size: u32,
    pub enable_request_logging: bool,
    pub enable_response_caching: bool,
    pub cache_ttl_seconds: u64,
}

impl Default for APIConnectorConfig {
    fn default() -> Self {
        Self {
            enable_rate_limiting: true,
            enable_retry_logic: true,
            enable_health_monitoring: true,
            enable_performance_tracking: true,
            default_timeout_seconds: 30,
            max_concurrent_requests: 100,
            connection_pool_size: 20,
            enable_request_logging: true,
            enable_response_caching: false,
            cache_ttl_seconds: 300, // 5 minutes
        }
    }
}

impl APIConnectorConfig {
    /// Create configuration optimized for high concurrency
    pub fn high_concurrency() -> Self {
        Self {
            max_concurrent_requests: 200,
            connection_pool_size: 50,
            default_timeout_seconds: 15,
            enable_response_caching: true,
            cache_ttl_seconds: 60, // 1 minute
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            default_timeout_seconds: 60,
            max_concurrent_requests: 50,
            connection_pool_size: 10,
            enable_response_caching: true,
            cache_ttl_seconds: 600, // 10 minutes
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.default_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "default_timeout_seconds must be greater than 0",
            ));
        }
        if self.max_concurrent_requests == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_requests must be greater than 0",
            ));
        }
        if self.connection_pool_size == 0 {
            return Err(ArbitrageError::validation_error(
                "connection_pool_size must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// API connector for exchange integration with rate limiting
pub struct APIConnector {
    config: APIConnectorConfig,
    logger: crate::utils::logger::Logger,

    // Exchange configurations
    exchanges: Arc<std::sync::Mutex<HashMap<ExchangeType, ExchangeConfig>>>,

    // Rate limiters for each exchange
    rate_limiters: Arc<std::sync::Mutex<HashMap<ExchangeType, RateLimiter>>>,

    // Health status for each exchange
    health_status: Arc<std::sync::Mutex<HashMap<ExchangeType, APIHealth>>>,

    // Performance metrics for each exchange
    metrics: Arc<std::sync::Mutex<HashMap<ExchangeType, APIMetrics>>>,

    // Active requests tracking
    active_requests: Arc<std::sync::Mutex<u32>>,

    // Performance tracking
    startup_time: u64,
}

impl APIConnector {
    /// Create new APIConnector instance
    pub fn new(config: APIConnectorConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        let connector = Self {
            config,
            logger,
            exchanges: Arc::new(std::sync::Mutex::new(HashMap::new())),
            rate_limiters: Arc::new(std::sync::Mutex::new(HashMap::new())),
            health_status: Arc::new(std::sync::Mutex::new(HashMap::new())),
            metrics: Arc::new(std::sync::Mutex::new(HashMap::new())),
            active_requests: Arc::new(std::sync::Mutex::new(0)),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        };

        connector.logger.info(&format!(
            "APIConnector initialized: rate_limiting={}, retry_logic={}, max_concurrent={}",
            connector.config.enable_rate_limiting,
            connector.config.enable_retry_logic,
            connector.config.max_concurrent_requests
        ));

        Ok(connector)
    }

    /// Add exchange configuration
    pub async fn add_exchange(&self, exchange_config: ExchangeConfig) -> ArbitrageResult<()> {
        exchange_config.validate()?;

        let exchange_type = exchange_config.exchange_type.clone();

        // Add exchange configuration
        if let Ok(mut exchanges) = self.exchanges.lock() {
            exchanges.insert(exchange_type.clone(), exchange_config.clone());
        }

        // Initialize rate limiter
        if let Ok(mut limiters) = self.rate_limiters.lock() {
            limiters.insert(
                exchange_type.clone(),
                RateLimiter::new(exchange_config.rate_limit),
            );
        }

        // Initialize health status
        if let Ok(mut health) = self.health_status.lock() {
            let status = APIHealth {
                exchange: exchange_type.clone(),
                ..Default::default()
            };
            health.insert(exchange_type.clone(), status);
        }

        // Initialize metrics
        if let Ok(mut metrics) = self.metrics.lock() {
            let metric = APIMetrics {
                exchange: exchange_type.clone(),
                ..Default::default()
            };
            metrics.insert(exchange_type, metric);
        }

        self.logger.info(&format!(
            "Added exchange configuration: {}",
            exchange_config.exchange_type.as_str()
        ));

        Ok(())
    }

    /// Make API request with rate limiting and retry logic
    pub async fn make_request(&self, request: APIRequest) -> ArbitrageResult<APIResponse> {
        // Check concurrent request limit
        if !self.check_concurrent_limit().await {
            return Err(ArbitrageError::parse_error(
                "Maximum concurrent requests reached",
            ));
        }

        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let mut last_error = None;
        let max_retries = if self.config.enable_retry_logic {
            self.get_max_retries(&request.exchange).await
        } else {
            1
        };

        for attempt in 0..max_retries {
            // Check rate limiting
            if self.config.enable_rate_limiting && !self.check_rate_limit(&request.exchange).await {
                let wait_time = self.get_rate_limit_wait_time(&request.exchange).await;
                if wait_time > 0 {
                    self.logger.warn(&format!(
                        "Rate limited for {}, waiting {} seconds",
                        request.exchange.as_str(),
                        wait_time
                    ));
                    self.record_rate_limited(&request.exchange, start_time)
                        .await;

                    // For now, we'll return an error instead of actually waiting
                    // In a real implementation, you might want to implement async waiting
                    self.decrement_active_requests().await;
                    return Err(ArbitrageError::parse_error(format!(
                        "Rate limited, wait {} seconds",
                        wait_time
                    )));
                }
            }

            // Record rate limiter usage
            self.record_rate_limit_usage(&request.exchange).await;

            // Make the actual request
            match self.execute_request(&request).await {
                Ok(response) => {
                    self.record_success(&request.exchange, start_time, response.latency_ms)
                        .await;
                    self.decrement_active_requests().await;
                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e.clone());

                    // Check if we should retry
                    if attempt < max_retries - 1 && self.should_retry(&e) {
                        self.record_retry(&request.exchange, start_time).await;
                        self.logger.warn(&format!(
                            "Request failed, retrying (attempt {}/{}): {}",
                            attempt + 1,
                            max_retries,
                            e
                        ));

                        // Exponential backoff
                        let _backoff_seconds = (2.0_f64.powf(attempt as f64)).clamp(1.0, 60.0);
                        continue;
                    } else {
                        self.record_failure(&request.exchange, start_time, &e).await;
                        break;
                    }
                }
            }
        }

        self.decrement_active_requests().await;
        Err(last_error
            .unwrap_or_else(|| ArbitrageError::parse_error("Request failed after all retries")))
    }

    /// Execute the actual HTTP request (placeholder implementation)
    async fn execute_request(&self, request: &APIRequest) -> ArbitrageResult<APIResponse> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Get exchange configuration
        let exchange_config = if let Ok(exchanges) = self.exchanges.lock() {
            exchanges.get(&request.exchange).cloned()
        } else {
            None
        };

        let config = exchange_config.ok_or_else(|| {
            ArbitrageError::configuration_error(format!(
                "Exchange {} not configured",
                request.exchange.as_str()
            ))
        })?;

        // Build full URL
        let _full_url = if request.endpoint.starts_with("http") {
            request.endpoint.clone()
        } else {
            format!("{}{}", config.base_url, request.endpoint)
        };

        // In a real implementation, this would make an actual HTTP request
        // For now, we'll simulate a response
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let latency = end_time - start_time;

        // Simulate different response scenarios
        let response = APIResponse {
            status_code: 200,
            headers: HashMap::new(),
            body: r#"{"status":"success","data":{}}"#.to_string(),
            latency_ms: latency,
            exchange: request.exchange.clone(),
            endpoint: request.endpoint.clone(),
            timestamp: end_time,
            rate_limit_remaining: Some(100),
            rate_limit_reset: Some(end_time + 60000), // 1 minute from now
        };

        Ok(response)
    }

    /// Check if request should be retried based on error type
    fn should_retry(&self, error: &ArbitrageError) -> bool {
        let error_msg = error.to_string().to_lowercase();

        // Network and timeout errors are retryable
        if error_msg.contains("network")
            || error_msg.contains("timeout")
            || error_msg.contains("service")
        {
            return true;
        }

        // Rate limit and auth errors are not retryable
        if error_msg.contains("rate limit") || error_msg.contains("authentication") {
            return false;
        }

        // Default to not retryable for unknown errors
        false
    }

    /// Check concurrent request limit
    async fn check_concurrent_limit(&self) -> bool {
        if let Ok(mut active) = self.active_requests.lock() {
            if *active >= self.config.max_concurrent_requests {
                false
            } else {
                *active += 1;
                true
            }
        } else {
            false
        }
    }

    /// Decrement active requests counter
    async fn decrement_active_requests(&self) {
        if let Ok(mut active) = self.active_requests.lock() {
            if *active > 0 {
                *active -= 1;
            }
        }
    }

    /// Check rate limit for exchange
    async fn check_rate_limit(&self, exchange: &ExchangeType) -> bool {
        if let Ok(mut limiters) = self.rate_limiters.lock() {
            if let Some(limiter) = limiters.get_mut(exchange) {
                limiter.can_make_request()
            } else {
                true
            }
        } else {
            true
        }
    }

    /// Get rate limit wait time
    async fn get_rate_limit_wait_time(&self, exchange: &ExchangeType) -> u64 {
        if let Ok(limiters) = self.rate_limiters.lock() {
            if let Some(limiter) = limiters.get(exchange) {
                limiter.get_wait_time_seconds()
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Record rate limiter usage
    async fn record_rate_limit_usage(&self, exchange: &ExchangeType) {
        if let Ok(mut limiters) = self.rate_limiters.lock() {
            if let Some(limiter) = limiters.get_mut(exchange) {
                limiter.record_request();
            }
        }
    }

    /// Get max retries for exchange
    async fn get_max_retries(&self, exchange: &ExchangeType) -> u32 {
        if let Ok(exchanges) = self.exchanges.lock() {
            if let Some(config) = exchanges.get(exchange) {
                config.max_retries
            } else {
                3
            }
        } else {
            3
        }
    }

    /// Record successful request
    async fn record_success(&self, exchange: &ExchangeType, _start_time: u64, latency_ms: u64) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        // Update health status
        if let Ok(mut health) = self.health_status.lock() {
            if let Some(status) = health.get_mut(exchange) {
                status.is_healthy = true;
                status.last_success_timestamp = end_time;
                status.total_requests += 1;
                status.success_rate_percent = (status.total_requests - status.failed_requests)
                    as f32
                    / status.total_requests as f32
                    * 100.0;
                status.average_latency_ms = (status.average_latency_ms
                    * (status.total_requests - 1) as f64
                    + latency_ms as f64)
                    / status.total_requests as f64;
                status.rate_limit_status = "ok".to_string();
                status.last_health_check = end_time;
            }
        }

        // Update metrics
        if self.config.enable_performance_tracking {
            if let Ok(mut metrics) = self.metrics.lock() {
                if let Some(metric) = metrics.get_mut(exchange) {
                    metric.total_requests += 1;
                    metric.successful_requests += 1;
                    metric.average_latency_ms = (metric.average_latency_ms
                        * (metric.total_requests - 1) as f64
                        + latency_ms as f64)
                        / metric.total_requests as f64;
                    metric.min_latency_ms = metric.min_latency_ms.min(latency_ms as f64);
                    metric.max_latency_ms = metric.max_latency_ms.max(latency_ms as f64);
                    metric.success_rate_percent =
                        metric.successful_requests as f32 / metric.total_requests as f32 * 100.0;

                    // Calculate requests per minute
                    let elapsed_minutes = (end_time - self.startup_time) as f64 / 60000.0;
                    if elapsed_minutes > 0.0 {
                        metric.requests_per_minute = metric.total_requests as f64 / elapsed_minutes;
                    }

                    metric.last_updated = end_time;
                }
            }
        }
    }

    /// Record failed request
    async fn record_failure(
        &self,
        exchange: &ExchangeType,
        start_time: u64,
        error: &ArbitrageError,
    ) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let latency = end_time - start_time;

        // Update health status
        if let Ok(mut health) = self.health_status.lock() {
            if let Some(status) = health.get_mut(exchange) {
                status.is_healthy = false;
                status.last_error = Some(error.to_string());
                status.total_requests += 1;
                status.failed_requests += 1;
                status.success_rate_percent = (status.total_requests - status.failed_requests)
                    as f32
                    / status.total_requests as f32
                    * 100.0;
                status.average_latency_ms = (status.average_latency_ms
                    * (status.total_requests - 1) as f64
                    + latency as f64)
                    / status.total_requests as f64;
                status.last_health_check = end_time;
            }
        }

        // Update metrics
        if self.config.enable_performance_tracking {
            if let Ok(mut metrics) = self.metrics.lock() {
                if let Some(metric) = metrics.get_mut(exchange) {
                    metric.total_requests += 1;
                    metric.failed_requests += 1;
                    metric.average_latency_ms = (metric.average_latency_ms
                        * (metric.total_requests - 1) as f64
                        + latency as f64)
                        / metric.total_requests as f64;
                    metric.min_latency_ms = metric.min_latency_ms.min(latency as f64);
                    metric.max_latency_ms = metric.max_latency_ms.max(latency as f64);
                    metric.success_rate_percent =
                        metric.successful_requests as f32 / metric.total_requests as f32 * 100.0;

                    // Update specific error counters
                    if let ArbitrageError {
                        kind: crate::utils::error::ErrorKind::NetworkError,
                        ..
                    } = error
                    {
                        metric.timeout_requests += 1
                    }
                }
            }
        }
    }

    /// Record rate limited request
    async fn record_rate_limited(&self, exchange: &ExchangeType, _start_time: u64) {
        // Update rate limiter
        if let Ok(mut limiters) = self.rate_limiters.lock() {
            if let Some(limiter) = limiters.get_mut(exchange) {
                limiter.record_rate_limit();
            }
        }

        // Update metrics
        if let Ok(mut metrics) = self.metrics.lock() {
            if let Some(metric) = metrics.get_mut(exchange) {
                metric.rate_limited_requests += 1;
                metric.last_updated = chrono::Utc::now().timestamp_millis() as u64;
            }
        }

        // Update health status
        if let Ok(mut health) = self.health_status.lock() {
            if let Some(status) = health.get_mut(exchange) {
                status.rate_limit_status = "limited".to_string();
                status.last_health_check = chrono::Utc::now().timestamp_millis() as u64;
            }
        }
    }

    /// Record retry attempt
    async fn record_retry(&self, exchange: &ExchangeType, _start_time: u64) {
        if let Ok(mut metrics) = self.metrics.lock() {
            if let Some(metric) = metrics.get_mut(exchange) {
                metric.retry_requests += 1;
                metric.last_updated = chrono::Utc::now().timestamp_millis() as u64;
            }
        }
    }

    /// Get health status for all exchanges
    pub async fn get_health_status(&self) -> HashMap<ExchangeType, APIHealth> {
        if let Ok(health) = self.health_status.lock() {
            health.clone()
        } else {
            HashMap::new()
        }
    }

    /// Get performance metrics for all exchanges
    pub async fn get_metrics(&self) -> HashMap<ExchangeType, APIMetrics> {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            HashMap::new()
        }
    }

    /// Health check for API connector
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let health_status = self.get_health_status().await;

        if health_status.is_empty() {
            return Ok(false);
        }

        // Consider healthy if at least 50% of exchanges are healthy
        let total_exchanges = health_status.len();
        let healthy_exchanges = health_status.values().filter(|h| h.is_healthy).count();

        Ok(healthy_exchanges as f32 / total_exchanges as f32 >= 0.5)
    }

    /// Get overall health summary
    pub async fn get_health_summary(&self) -> ArbitrageResult<serde_json::Value> {
        let health_status = self.get_health_status().await;
        let metrics = self.get_metrics().await;

        let total_exchanges = health_status.len();
        let healthy_exchanges = health_status.values().filter(|h| h.is_healthy).count();

        let overall_success_rate = if !metrics.is_empty() {
            metrics
                .values()
                .map(|m| m.success_rate_percent)
                .sum::<f32>()
                / metrics.len() as f32
        } else {
            0.0
        };

        let overall_latency = if !metrics.is_empty() {
            metrics.values().map(|m| m.average_latency_ms).sum::<f64>() / metrics.len() as f64
        } else {
            0.0
        };

        let active_requests = if let Ok(active) = self.active_requests.lock() {
            *active
        } else {
            0
        };

        Ok(serde_json::json!({
            "overall_health": healthy_exchanges as f32 / total_exchanges.max(1) as f32 * 100.0,
            "healthy_exchanges": healthy_exchanges,
            "total_exchanges": total_exchanges,
            "active_requests": active_requests,
            "max_concurrent_requests": self.config.max_concurrent_requests,
            "overall_success_rate_percent": overall_success_rate,
            "overall_average_latency_ms": overall_latency,
            "exchange_details": health_status,
            "performance_metrics": metrics,
            "last_updated": chrono::Utc::now().timestamp_millis()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exchange_type_rate_limits() {
        assert_eq!(ExchangeType::Binance.default_rate_limit_per_minute(), 1200);
        assert_eq!(ExchangeType::Bybit.default_rate_limit_per_minute(), 600);
        assert_eq!(ExchangeType::OKX.default_rate_limit_per_minute(), 300);
    }

    #[test]
    fn test_exchange_config_validation() {
        let config = ExchangeConfig::new(ExchangeType::Binance);
        assert!(config.validate().is_ok());

        let mut invalid_config = config;
        invalid_config.base_url = "".to_string();
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_api_request_builder() {
        let request = APIRequest::new(
            ExchangeType::Binance,
            "/api/v3/ticker/price".to_string(),
            "GET".to_string(),
        )
        .with_priority(1);

        assert_eq!(request.exchange, ExchangeType::Binance);
        assert_eq!(request.priority, 1);
    }

    #[test]
    fn test_rate_limiter() {
        let config = RateLimitConfig::default();
        let mut limiter = RateLimiter::new(config);

        // Should be able to make initial requests
        assert!(limiter.can_make_request());
        limiter.record_request();
    }

    #[test]
    fn test_api_connector_config_validation() {
        let config = APIConnectorConfig::default();
        assert!(config.validate().is_ok());

        let mut invalid_config = config;
        invalid_config.default_timeout_seconds = 0;
        assert!(invalid_config.validate().is_err());
    }
}
