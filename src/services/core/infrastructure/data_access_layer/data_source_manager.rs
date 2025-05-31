// Data Source Manager - Multi-source Data Coordination Component
// Provides hierarchical data access with Pipeline → KV → Database → API fallback

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

/// Data source types in priority order
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceType {
    Pipeline, // Cloudflare Pipelines (highest priority)
    Cache,    // KV Store (second priority)
    Database, // D1 Database (third priority)
    API,      // External APIs (lowest priority)
}

impl DataSourceType {
    pub fn as_str(&self) -> &str {
        match self {
            DataSourceType::Pipeline => "pipeline",
            DataSourceType::Cache => "cache",
            DataSourceType::Database => "database",
            DataSourceType::API => "api",
        }
    }

    pub fn priority(&self) -> u8 {
        match self {
            DataSourceType::Pipeline => 1,
            DataSourceType::Cache => 2,
            DataSourceType::Database => 3,
            DataSourceType::API => 4,
        }
    }
}

/// Data source health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceHealth {
    pub source_type: DataSourceType,
    pub is_healthy: bool,
    pub last_success_timestamp: u64,
    pub last_error: Option<String>,
    pub success_rate_percent: f32,
    pub average_latency_ms: f64,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub circuit_breaker_open: bool,
    pub last_health_check: u64,
}

impl Default for DataSourceHealth {
    fn default() -> Self {
        Self {
            source_type: DataSourceType::Cache,
            is_healthy: false,
            last_success_timestamp: 0,
            last_error: None,
            success_rate_percent: 0.0,
            average_latency_ms: 0.0,
            total_requests: 0,
            failed_requests: 0,
            circuit_breaker_open: false,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Circuit breaker state for data sources
#[derive(Debug, Clone)]
enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Circuit breaker is open, requests fail fast
    HalfOpen, // Testing if service is back
}

/// Circuit breaker for individual data sources
#[derive(Debug, Clone)]
struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    failure_threshold: u32,
    timeout_ms: u64,
    last_failure_time: Option<u64>,
    success_count: u32,
    half_open_max_calls: u32,
}

impl CircuitBreaker {
    fn new(failure_threshold: u32, timeout_ms: u64) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            failure_threshold,
            timeout_ms,
            last_failure_time: None,
            success_count: 0,
            half_open_max_calls: 3,
        }
    }

    fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    let now = chrono::Utc::now().timestamp_millis() as u64;
                    if now - last_failure > self.timeout_ms {
                        self.state = CircuitBreakerState::HalfOpen;
                        self.success_count = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => self.success_count < self.half_open_max_calls,
        }
    }

    fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.half_open_max_calls {
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                }
            }
            CircuitBreakerState::Open => {}
        }
    }

    fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(chrono::Utc::now().timestamp_millis() as u64);

        match self.state {
            CircuitBreakerState::Closed => {
                if self.failure_count >= self.failure_threshold {
                    self.state = CircuitBreakerState::Open;
                }
            }
            CircuitBreakerState::HalfOpen => {
                self.state = CircuitBreakerState::Open;
            }
            CircuitBreakerState::Open => {}
        }
    }

    fn is_open(&self) -> bool {
        matches!(self.state, CircuitBreakerState::Open)
    }
}

/// Configuration for the data source manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceManagerConfig {
    pub enable_kv_store: bool,
    pub enable_d1_database: bool,
    pub enable_external_apis: bool,
    pub enable_caching: bool,
    pub enable_fallback_chain: bool,
    pub enable_automatic_failover: bool,
    pub enable_circuit_breakers: bool,
    pub enable_performance_tracking: bool,
    pub connection_pool_size: u32,
    pub default_timeout_seconds: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub cache_ttl_seconds: u64,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
    pub enable_metrics: bool,
    pub enable_health_checks: bool,
    pub health_check_interval_seconds: u64,
}

impl Default for DataSourceManagerConfig {
    fn default() -> Self {
        Self {
            enable_kv_store: true,
            enable_d1_database: true,
            enable_external_apis: true,
            enable_caching: true,
            enable_fallback_chain: true,
            enable_automatic_failover: true,
            enable_circuit_breakers: true,
            enable_performance_tracking: true,
            connection_pool_size: 15,
            default_timeout_seconds: 30,
            max_retries: 3,
            retry_delay_ms: 1000,
            cache_ttl_seconds: 300,
            enable_compression: true,
            compression_threshold_bytes: 10240, // 10KB
            enable_metrics: true,
            enable_health_checks: true,
            health_check_interval_seconds: 60,
        }
    }
}

impl DataSourceManagerConfig {
    /// High-concurrency configuration for 1000-2500 concurrent users
    pub fn high_concurrency() -> Self {
        Self {
            connection_pool_size: 25,
            default_timeout_seconds: 15,
            max_retries: 2,
            retry_delay_ms: 500,
            cache_ttl_seconds: 180,
            ..Default::default()
        }
    }

    /// High-reliability configuration with enhanced error handling
    pub fn high_reliability() -> Self {
        Self {
            connection_pool_size: 10,
            default_timeout_seconds: 60,
            max_retries: 5,
            retry_delay_ms: 2000,
            cache_ttl_seconds: 600,
            ..Default::default()
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.connection_pool_size == 0 {
            return Err(ArbitrageError::configuration_error(
                "connection_pool_size must be greater than 0".to_string(),
            ));
        }
        if self.default_timeout_seconds == 0 {
            return Err(ArbitrageError::configuration_error(
                "default_timeout_seconds must be greater than 0".to_string(),
            ));
        }
        if self.compression_threshold_bytes == 0 {
            return Err(ArbitrageError::configuration_error(
                "compression_threshold_bytes must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Performance metrics for data sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceMetrics {
    pub source_type: DataSourceType,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub success_rate_percent: f32,
    pub circuit_breaker_trips: u64,
    pub failover_count: u64,
    pub last_updated: u64,
}

impl Default for DataSourceMetrics {
    fn default() -> Self {
        Self {
            source_type: DataSourceType::Cache,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            max_latency_ms: 0.0,
            success_rate_percent: 100.0,
            circuit_breaker_trips: 0,
            failover_count: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Data Source Manager for hierarchical data access
pub struct DataSourceManager {
    config: DataSourceManagerConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Circuit breakers for each data source
    circuit_breakers: Arc<std::sync::Mutex<HashMap<DataSourceType, CircuitBreaker>>>,

    // Health status for each data source
    health_status: Arc<std::sync::Mutex<HashMap<DataSourceType, DataSourceHealth>>>,

    // Performance metrics for each data source
    metrics: Arc<std::sync::Mutex<HashMap<DataSourceType, DataSourceMetrics>>>,

    // Active connections count
    active_connections: Arc<std::sync::Mutex<u32>>,
}

impl DataSourceManager {
    /// Create new DataSourceManager instance
    pub fn new(kv_store: KvStore, config: DataSourceManagerConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Initialize circuit breakers for each data source
        let mut circuit_breakers = HashMap::new();
        circuit_breakers.insert(
            DataSourceType::Pipeline,
            CircuitBreaker::new(5, 60000), // 5 failures, 60s timeout
        );
        circuit_breakers.insert(DataSourceType::Cache, CircuitBreaker::new(5, 60000));
        circuit_breakers.insert(DataSourceType::Database, CircuitBreaker::new(5, 60000));
        circuit_breakers.insert(DataSourceType::API, CircuitBreaker::new(5, 60000));

        // Initialize health status for each data source
        let mut health_status = HashMap::new();
        for source_type in [
            DataSourceType::Pipeline,
            DataSourceType::Cache,
            DataSourceType::Database,
            DataSourceType::API,
        ] {
            let health = DataSourceHealth {
                source_type: source_type.clone(),
                ..Default::default()
            };
            health_status.insert(source_type, health);
        }

        // Initialize metrics for each data source
        let mut metrics = HashMap::new();
        for source_type in [
            DataSourceType::Pipeline,
            DataSourceType::Cache,
            DataSourceType::Database,
            DataSourceType::API,
        ] {
            let metric = DataSourceMetrics {
                source_type: source_type.clone(),
                ..Default::default()
            };
            metrics.insert(source_type, metric);
        }

        logger.info(&format!(
            "DataSourceManager initialized: pool_size={}, timeout={}s, fallback_enabled={}",
            config.connection_pool_size,
            config.default_timeout_seconds,
            config.enable_fallback_chain
        ));

        Ok(Self {
            config,
            logger,
            kv_store,
            circuit_breakers: Arc::new(std::sync::Mutex::new(circuit_breakers)),
            health_status: Arc::new(std::sync::Mutex::new(health_status)),
            metrics: Arc::new(std::sync::Mutex::new(metrics)),
            active_connections: Arc::new(std::sync::Mutex::new(0)),
        })
    }

    /// Get data with hierarchical fallback: Pipeline → KV → Database → API
    pub async fn get_data<T>(&self, key: &str, data_type: &str) -> ArbitrageResult<Option<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check connection limit
        if !self.check_connection_limit().await {
            return Err(ArbitrageError::service_unavailable(
                "Connection pool exhausted".to_string(),
            ));
        }

        // Try data sources in priority order
        let sources = if self.config.enable_fallback_chain {
            vec![
                DataSourceType::Pipeline,
                DataSourceType::Cache,
                DataSourceType::Database,
                DataSourceType::API,
            ]
        } else {
            vec![DataSourceType::Cache] // Default to KV only
        };

        for source_type in sources {
            if !self.can_use_source(&source_type).await {
                continue;
            }

            match self.get_from_source(&source_type, key, data_type).await {
                Ok(Some(data)) => {
                    self.record_success(&source_type, start_time).await;
                    return Ok(Some(data));
                }
                Ok(None) => {
                    // Data not found in this source, try next
                    continue;
                }
                Err(error) => {
                    self.record_failure(&source_type, start_time, &error).await;
                    if !self.config.enable_fallback_chain {
                        return Err(error);
                    }
                    // Continue to next source
                }
            }
        }

        Ok(None)
    }

    /// Set data to multiple sources for redundancy
    pub async fn set_data<T>(
        &self,
        key: &str,
        data: &T,
        data_type: &str,
        ttl_seconds: Option<u64>,
    ) -> ArbitrageResult<()>
    where
        T: serde::Serialize,
    {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check connection limit
        if !self.check_connection_limit().await {
            return Err(ArbitrageError::service_unavailable(
                "Connection pool exhausted".to_string(),
            ));
        }

        // Always try to store in KV (primary cache)
        let mut success = false;
        if self.can_use_source(&DataSourceType::Cache).await {
            match self
                .set_to_source(&DataSourceType::Cache, key, data, data_type, ttl_seconds)
                .await
            {
                Ok(()) => {
                    self.record_success(&DataSourceType::Cache, start_time)
                        .await;
                    success = true;
                }
                Err(error) => {
                    self.record_failure(&DataSourceType::Cache, start_time, &error)
                        .await;
                }
            }
        }

        // Optionally store in other sources for redundancy
        if self.config.enable_fallback_chain {
            for source_type in [DataSourceType::Database, DataSourceType::Pipeline] {
                if self.can_use_source(&source_type).await {
                    let _ = self
                        .set_to_source(&source_type, key, data, data_type, ttl_seconds)
                        .await;
                }
            }
        }

        if success {
            Ok(())
        } else {
            Err(ArbitrageError::storage_error(
                "Failed to store data in any source".to_string(),
            ))
        }
    }

    /// Get data from specific source
    async fn get_from_source<T>(
        &self,
        source_type: &DataSourceType,
        key: &str,
        _data_type: &str,
    ) -> ArbitrageResult<Option<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        match source_type {
            DataSourceType::Cache => {
                // Get from KV store
                match self.kv_store.get(key).text().await {
                    Ok(Some(data_str)) => match serde_json::from_str::<T>(&data_str) {
                        Ok(data) => Ok(Some(data)),
                        Err(e) => Err(ArbitrageError::serialization_error(e.to_string())),
                    },
                    Ok(None) => Ok(None),
                    Err(e) => Err(ArbitrageError::storage_error(format!(
                        "KV get error: {:?}",
                        e
                    ))),
                }
            }
            DataSourceType::Pipeline => {
                // Placeholder for Pipeline data access
                self.logger
                    .debug("Pipeline data access not yet implemented");
                Ok(None)
            }
            DataSourceType::Database => {
                // Placeholder for D1 database access
                self.logger
                    .debug("Database data access not yet implemented");
                Ok(None)
            }
            DataSourceType::API => {
                // Placeholder for external API access
                self.logger.debug("API data access not yet implemented");
                Ok(None)
            }
        }
    }

    /// Set data to specific source
    async fn set_to_source<T>(
        &self,
        source_type: &DataSourceType,
        key: &str,
        data: &T,
        _data_type: &str,
        ttl_seconds: Option<u64>,
    ) -> ArbitrageResult<()>
    where
        T: serde::Serialize,
    {
        match source_type {
            DataSourceType::Cache => {
                // Set to KV store
                let data_str = serde_json::to_string(data)
                    .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

                let mut put_request = self.kv_store.put(key, data_str)?;
                if let Some(ttl) = ttl_seconds {
                    put_request = put_request.expiration_ttl(ttl);
                }
                put_request
                    .execute()
                    .await
                    .map_err(|e| ArbitrageError::storage_error(format!("KV put error: {:?}", e)))?;
                Ok(())
            }
            DataSourceType::Pipeline => {
                // Placeholder for Pipeline data storage
                self.logger
                    .debug("Pipeline data storage not yet implemented");
                Ok(())
            }
            DataSourceType::Database => {
                // Placeholder for D1 database storage
                self.logger
                    .debug("Database data storage not yet implemented");
                Ok(())
            }
            DataSourceType::API => {
                // External APIs are typically read-only
                Err(ArbitrageError::not_implemented(
                    "Cannot write to external API".to_string(),
                ))
            }
        }
    }

    /// Check if a data source can be used (circuit breaker check)
    async fn can_use_source(&self, source_type: &DataSourceType) -> bool {
        if !self.config.enable_circuit_breakers {
            return true;
        }

        if let Ok(mut circuit_breakers) = self.circuit_breakers.lock() {
            if let Some(circuit_breaker) = circuit_breakers.get_mut(source_type) {
                return circuit_breaker.can_execute();
            }
        }
        true
    }

    /// Check if we're within connection limits
    async fn check_connection_limit(&self) -> bool {
        if let Ok(active_connections) = self.active_connections.lock() {
            *active_connections < self.config.connection_pool_size
        } else {
            false
        }
    }

    /// Record successful operation
    async fn record_success(&self, source_type: &DataSourceType, start_time: u64) {
        let latency = chrono::Utc::now().timestamp_millis() as u64 - start_time;

        // Update circuit breaker
        if self.config.enable_circuit_breakers {
            if let Ok(mut circuit_breakers) = self.circuit_breakers.lock() {
                if let Some(circuit_breaker) = circuit_breakers.get_mut(source_type) {
                    circuit_breaker.record_success();
                }
            }
        }

        // Update health status
        if let Ok(mut health_status) = self.health_status.lock() {
            if let Some(health) = health_status.get_mut(source_type) {
                health.is_healthy = true;
                health.last_success_timestamp = chrono::Utc::now().timestamp_millis() as u64;
                health.total_requests += 1;
                health.last_error = None;
                health.circuit_breaker_open = false;

                // Update success rate
                let total = health.total_requests;
                let failed = health.failed_requests;
                health.success_rate_percent = if total > 0 {
                    ((total - failed) as f32 / total as f32) * 100.0
                } else {
                    100.0
                };

                // Update average latency
                health.average_latency_ms = (health.average_latency_ms * (total - 1) as f64
                    + latency as f64)
                    / total as f64;
            }
        }

        // Update metrics
        if self.config.enable_performance_tracking {
            if let Ok(mut metrics) = self.metrics.lock() {
                if let Some(metric) = metrics.get_mut(source_type) {
                    metric.total_requests += 1;
                    metric.successful_requests += 1;

                    // Update latency metrics
                    metric.min_latency_ms = metric.min_latency_ms.min(latency as f64);
                    metric.max_latency_ms = metric.max_latency_ms.max(latency as f64);
                    metric.average_latency_ms = (metric.average_latency_ms
                        * (metric.total_requests - 1) as f64
                        + latency as f64)
                        / metric.total_requests as f64;

                    // Update success rate
                    metric.success_rate_percent =
                        (metric.successful_requests as f32 / metric.total_requests as f32) * 100.0;

                    metric.last_updated = chrono::Utc::now().timestamp_millis() as u64;
                }
            }
        }
    }

    /// Record failed operation
    async fn record_failure(
        &self,
        source_type: &DataSourceType,
        start_time: u64,
        error: &ArbitrageError,
    ) {
        let latency = chrono::Utc::now().timestamp_millis() as u64 - start_time;

        // Update circuit breaker
        if self.config.enable_circuit_breakers {
            if let Ok(mut circuit_breakers) = self.circuit_breakers.lock() {
                if let Some(circuit_breaker) = circuit_breakers.get_mut(source_type) {
                    circuit_breaker.record_failure();
                }
            }
        }

        // Update health status
        if let Ok(mut health_status) = self.health_status.lock() {
            if let Some(health) = health_status.get_mut(source_type) {
                health.total_requests += 1;
                health.failed_requests += 1;
                health.last_error = Some(error.to_string());

                // Check if circuit breaker is open
                if let Ok(circuit_breakers) = self.circuit_breakers.lock() {
                    if let Some(circuit_breaker) = circuit_breakers.get(source_type) {
                        health.circuit_breaker_open = circuit_breaker.is_open();
                        health.is_healthy = !circuit_breaker.is_open();
                    }
                }

                // Update success rate
                let total = health.total_requests;
                let failed = health.failed_requests;
                health.success_rate_percent = if total > 0 {
                    ((total - failed) as f32 / total as f32) * 100.0
                } else {
                    100.0
                };

                // Update average latency
                health.average_latency_ms = (health.average_latency_ms * (total - 1) as f64
                    + latency as f64)
                    / total as f64;
            }
        }

        // Update metrics
        if self.config.enable_performance_tracking {
            if let Ok(mut metrics) = self.metrics.lock() {
                if let Some(metric) = metrics.get_mut(source_type) {
                    metric.total_requests += 1;
                    metric.failed_requests += 1;

                    // Update latency metrics
                    metric.min_latency_ms = metric.min_latency_ms.min(latency as f64);
                    metric.max_latency_ms = metric.max_latency_ms.max(latency as f64);
                    metric.average_latency_ms = (metric.average_latency_ms
                        * (metric.total_requests - 1) as f64
                        + latency as f64)
                        / metric.total_requests as f64;

                    // Update success rate
                    metric.success_rate_percent =
                        (metric.successful_requests as f32 / metric.total_requests as f32) * 100.0;

                    // Check for circuit breaker trip
                    if let Ok(circuit_breakers) = self.circuit_breakers.lock() {
                        if let Some(circuit_breaker) = circuit_breakers.get(source_type) {
                            if circuit_breaker.is_open() {
                                metric.circuit_breaker_trips += 1;
                            }
                        }
                    }

                    metric.last_updated = chrono::Utc::now().timestamp_millis() as u64;
                }
            }
        }

        self.logger.warn(&format!(
            "Data source {} failed: {} (latency: {}ms)",
            source_type.as_str(),
            error,
            latency
        ));
    }

    /// Get health status for all data sources
    pub async fn get_health_status(&self) -> HashMap<DataSourceType, DataSourceHealth> {
        if let Ok(health_status) = self.health_status.lock() {
            health_status.clone()
        } else {
            HashMap::new()
        }
    }

    /// Get performance metrics for all data sources
    pub async fn get_metrics(&self) -> HashMap<DataSourceType, DataSourceMetrics> {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            HashMap::new()
        }
    }

    /// Get comprehensive health summary
    pub async fn get_health_summary(&self) -> ArbitrageResult<serde_json::Value> {
        let health_status = self.get_health_status().await;
        let metrics = self.get_metrics().await;

        let total_sources = health_status.len();
        let healthy_sources = health_status.values().filter(|h| h.is_healthy).count();

        let overall_health_percent = if total_sources > 0 {
            (healthy_sources as f32 / total_sources as f32) * 100.0
        } else {
            0.0
        };

        let summary = serde_json::json!({
            "overall_health_percent": overall_health_percent,
            "healthy_sources": healthy_sources,
            "total_sources": total_sources,
            "sources": health_status,
            "metrics": metrics,
            "last_updated": chrono::Utc::now().timestamp_millis()
        });

        Ok(summary)
    }

    /// Perform health check on all data sources
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let health_status = self.get_health_status().await;
        let healthy_count = health_status.values().filter(|h| h.is_healthy).count();
        let total_count = health_status.len();

        // Consider healthy if at least 60% of sources are healthy
        Ok(healthy_count as f32 / total_count as f32 >= 0.6)
    }

    /// Get the underlying KV store for direct access
    pub fn get_kv_store(&self) -> worker::kv::KvStore {
        self.kv_store.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_source_manager_config_default() {
        let config = DataSourceManagerConfig::default();
        assert!(config.enable_fallback_chain);
        assert_eq!(config.connection_pool_size, 15);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_data_source_manager_config_high_concurrency() {
        let config = DataSourceManagerConfig::high_concurrency();
        assert_eq!(config.connection_pool_size, 15); // or set to 25 in the builder
        assert_eq!(config.default_timeout_seconds, 15);
        // remove obsolete circuit-breaker threshold assertion
    }

    #[test]
    fn test_data_source_type_priority() {
        assert_eq!(DataSourceType::Pipeline.priority(), 1);
        assert_eq!(DataSourceType::Cache.priority(), 2);
        assert_eq!(DataSourceType::Database.priority(), 3);
        assert_eq!(DataSourceType::API.priority(), 4);
    }

    #[test]
    fn test_circuit_breaker_functionality() {
        let mut circuit_breaker = CircuitBreaker::new(3, 60000);

        // Initially closed
        assert!(circuit_breaker.can_execute());
        assert!(!circuit_breaker.is_open());

        // Record failures
        circuit_breaker.record_failure();
        circuit_breaker.record_failure();
        assert!(circuit_breaker.can_execute());

        circuit_breaker.record_failure();
        assert!(circuit_breaker.is_open());
        assert!(!circuit_breaker.can_execute());
    }

    #[test]
    fn test_data_source_health_default() {
        let health = DataSourceHealth::default();
        assert!(!health.is_healthy);
        assert_eq!(health.success_rate_percent, 0.0);
        assert!(!health.circuit_breaker_open);
    }

    #[test]
    fn test_data_source_metrics_default() {
        let metrics = DataSourceMetrics::default();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.success_rate_percent, 100.0);
        assert_eq!(metrics.circuit_breaker_trips, 0);
    }
}
