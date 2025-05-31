// Data Access Layer - Unified Data Access with Intelligent Routing and Caching
// Provides hierarchical data access with fallback strategies and comprehensive monitoring

pub mod api_connector;
pub mod cache_layer;
pub mod data_coordinator;
pub mod data_source_manager;
pub mod data_validator;

// Re-export main types for easy access
pub use api_connector::{
    APIConnector, APIConnectorConfig, APIHealth, APIMetrics, APIRequest, APIResponse, ExchangeType,
};
pub use cache_layer::{
    CacheEntryType, CacheHealth, CacheLayer, CacheLayerConfig, CacheMetrics, CacheStats,
};
pub use data_coordinator::{
    CacheStrategy, CoordinationMetrics, DataAccessRequest, DataAccessResponse, DataCoordinator,
    DataCoordinatorConfig, DataSourceType,
};
pub use data_source_manager::{
    DataSourceHealth, DataSourceManager, DataSourceManagerConfig, DataSourceMetrics,
};
pub use data_validator::{
    DataValidator, DataValidatorConfig, FreshnessRule, ValidationMetrics, ValidationResult,
    ValidationRule, ValidationRuleType,
};

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Overall health status for the entire Data Access Layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAccessLayerHealth {
    pub is_healthy: bool,
    pub overall_score: f32,
    pub component_health: HashMap<String, bool>,
    pub active_connections: u32,
    pub cache_hit_rate_percent: f32,
    pub average_latency_ms: f64,
    pub total_requests: u64,
    pub error_rate_percent: f32,
    pub last_health_check: u64,
}

impl Default for DataAccessLayerHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            overall_score: 0.0,
            component_health: HashMap::new(),
            active_connections: 0,
            cache_hit_rate_percent: 0.0,
            average_latency_ms: 0.0,
            total_requests: 0,
            error_rate_percent: 0.0,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for the entire Data Access Layer
#[derive(Debug, Clone)]
pub struct DataAccessLayerConfig {
    pub data_source_config: DataSourceManagerConfig,
    pub cache_config: CacheLayerConfig,
    pub api_config: APIConnectorConfig,
    pub validator_config: DataValidatorConfig,
    pub coordinator_config: DataCoordinatorConfig,
    pub enable_comprehensive_monitoring: bool,
    pub health_check_interval_seconds: u64,
    pub enable_performance_optimization: bool,
    pub enable_chaos_engineering: bool,
}

impl Default for DataAccessLayerConfig {
    fn default() -> Self {
        Self {
            data_source_config: DataSourceManagerConfig::default(),
            cache_config: CacheLayerConfig::default(),
            api_config: APIConnectorConfig::default(),
            validator_config: DataValidatorConfig::default(),
            coordinator_config: DataCoordinatorConfig::default(),
            enable_comprehensive_monitoring: true,
            health_check_interval_seconds: 300, // 5 minutes
            enable_performance_optimization: true,
            enable_chaos_engineering: true,
        }
    }
}

impl DataAccessLayerConfig {
    /// Create configuration optimized for high concurrency (1000-2500 users)
    pub fn high_concurrency() -> Self {
        Self {
            data_source_config: DataSourceManagerConfig::high_concurrency(),
            cache_config: CacheLayerConfig::high_concurrency(),
            api_config: APIConnectorConfig::high_concurrency(),
            validator_config: DataValidatorConfig::high_performance(),
            coordinator_config: DataCoordinatorConfig::high_throughput(),
            health_check_interval_seconds: 180, // 3 minutes
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability and data quality
    pub fn high_reliability() -> Self {
        Self {
            data_source_config: DataSourceManagerConfig::high_reliability(),
            cache_config: CacheLayerConfig::high_reliability(),
            api_config: APIConnectorConfig::high_reliability(),
            validator_config: DataValidatorConfig::high_quality(),
            coordinator_config: DataCoordinatorConfig::high_reliability(),
            health_check_interval_seconds: 600, // 10 minutes
            ..Default::default()
        }
    }

    /// Validate the entire configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        self.data_source_config.validate()?;
        self.cache_config.validate()?;
        self.api_config.validate()?;
        self.validator_config.validate()?;
        self.coordinator_config.validate()?;

        if self.health_check_interval_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "health_check_interval_seconds must be greater than 0",
            ));
        }

        Ok(())
    }
}

/// Main Data Access Layer providing unified interface to all data access operations
#[derive(Clone)]
pub struct DataAccessLayer {
    config: DataAccessLayerConfig,
    logger: crate::utils::logger::Logger,

    // Main coordinator
    coordinator: Arc<DataCoordinator>,

    // Health monitoring
    health: Arc<std::sync::Mutex<DataAccessLayerHealth>>,

    // Performance tracking
    startup_time: u64,
}

impl DataAccessLayer {
    /// Create new DataAccessLayer instance
    pub async fn new(
        config: DataAccessLayerConfig,
        kv_store: worker::kv::KvStore,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize the coordinator with all components
        let coordinator = Arc::new(
            DataCoordinator::new(
                config.coordinator_config.clone(),
                config.data_source_config.clone(),
                config.cache_config.clone(),
                config.api_config.clone(),
                config.validator_config.clone(),
                kv_store,
            )
            .await?,
        );

        let layer = Self {
            config,
            logger,
            coordinator,
            health: Arc::new(std::sync::Mutex::new(DataAccessLayerHealth::default())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        };

        layer.logger.info(&format!(
            "DataAccessLayer initialized: monitoring={}, performance_optimization={}, chaos_engineering={}",
            layer.config.enable_comprehensive_monitoring,
            layer.config.enable_performance_optimization,
            layer.config.enable_chaos_engineering
        ));

        Ok(layer)
    }

    /// Process a single data access request
    pub async fn get_data(
        &self,
        request: DataAccessRequest,
    ) -> ArbitrageResult<DataAccessResponse> {
        self.coordinator.process_request(request).await
    }

    /// Process multiple data access requests in batch
    pub async fn get_data_batch(
        &self,
        requests: Vec<DataAccessRequest>,
    ) -> ArbitrageResult<Vec<DataAccessResponse>> {
        self.coordinator.batch_process_requests(requests).await
    }

    /// Convenience method for simple data retrieval with default settings
    pub async fn get_simple(
        &self,
        key: &str,
        data_type: &str,
    ) -> ArbitrageResult<serde_json::Value> {
        let request = DataAccessRequest::new(
            format!("simple_{}", chrono::Utc::now().timestamp_millis()),
            data_type.to_string(),
            key.to_string(),
        );

        let response = self.get_data(request).await?;
        response
            .data
            .ok_or_else(|| ArbitrageError::not_found("Data not found"))
    }

    /// Convenience method for data retrieval with validation
    pub async fn get_validated(
        &self,
        key: &str,
        data_type: &str,
    ) -> ArbitrageResult<(serde_json::Value, ValidationResult)> {
        let request = DataAccessRequest::new(
            format!("validated_{}", chrono::Utc::now().timestamp_millis()),
            data_type.to_string(),
            key.to_string(),
        )
        .with_validation(true)
        .with_freshness_check(true);

        let response = self.get_data(request).await?;
        let data = response
            .data
            .ok_or_else(|| ArbitrageError::not_found("Data not found"))?;
        let validation = response
            .validation_result
            .ok_or_else(|| ArbitrageError::parse_error("Validation result missing"))?;

        Ok((data, validation))
    }

    /// Convenience method for cache-first data retrieval
    pub async fn get_cached(
        &self,
        key: &str,
        data_type: &str,
    ) -> ArbitrageResult<serde_json::Value> {
        let request = DataAccessRequest::new(
            format!("cached_{}", chrono::Utc::now().timestamp_millis()),
            data_type.to_string(),
            key.to_string(),
        )
        .with_cache_strategy(CacheStrategy::CacheFirst);

        let response = self.get_data(request).await?;
        response
            .data
            .ok_or_else(|| ArbitrageError::not_found("Data not found"))
    }

    /// Convenience method for fresh data retrieval (bypassing cache)
    pub async fn get_fresh(
        &self,
        key: &str,
        data_type: &str,
    ) -> ArbitrageResult<serde_json::Value> {
        let request = DataAccessRequest::new(
            format!("fresh_{}", chrono::Utc::now().timestamp_millis()),
            data_type.to_string(),
            key.to_string(),
        )
        .with_cache_strategy(CacheStrategy::SourceFirst);

        let response = self.get_data(request).await?;
        response
            .data
            .ok_or_else(|| ArbitrageError::not_found("Data not found"))
    }

    /// Get comprehensive health status
    pub async fn health_check(&self) -> ArbitrageResult<DataAccessLayerHealth> {
        let coordinator_healthy = match self.coordinator.health_check().await {
            Ok(v) => v,
            Err(e) => {
                self.logger
                    .error(&format!("Coordinator health check failed: {}", e));
                false
            }
        };
        let coordinator_metrics = self.coordinator.get_metrics().await;
        let health_summary = self
            .coordinator
            .get_health_summary()
            .await
            .unwrap_or_default();

        let mut component_health = HashMap::new();
        component_health.insert("coordinator".to_string(), coordinator_healthy);

        // Extract component health from summary if available
        if let Some(components) = health_summary.get("component_health") {
            if let Some(data_source) = components.get("data_source_manager") {
                component_health.insert(
                    "data_source_manager".to_string(),
                    data_source
                        .get("is_healthy")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                );
            }
            if let Some(cache) = components.get("cache_layer") {
                component_health.insert(
                    "cache_layer".to_string(),
                    cache
                        .get("is_healthy")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                );
            }
            if let Some(api) = components.get("api_connector") {
                component_health.insert(
                    "api_connector".to_string(),
                    api.get("overall_health")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
                        >= 50.0,
                );
            }
            if let Some(validator) = components.get("data_validator") {
                component_health.insert(
                    "data_validator".to_string(),
                    validator
                        .get("is_healthy")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                );
            }
        }

        let healthy_components = component_health.values().filter(|&&h| h).count();
        let total_components = component_health.len();
        let overall_score = if total_components > 0 {
            healthy_components as f32 / total_components as f32
        } else {
            0.0
        };

        let cache_hit_rate =
            if coordinator_metrics.cache_hits + coordinator_metrics.cache_misses > 0 {
                coordinator_metrics.cache_hits as f32
                    / (coordinator_metrics.cache_hits + coordinator_metrics.cache_misses) as f32
                    * 100.0
            } else {
                0.0
            };

        let error_rate = if coordinator_metrics.total_requests > 0 {
            coordinator_metrics.failed_requests as f32 / coordinator_metrics.total_requests as f32
                * 100.0
        } else {
            0.0
        };

        let health = DataAccessLayerHealth {
            is_healthy: overall_score >= 0.6, // 60% of components must be healthy
            overall_score,
            component_health,
            active_connections: 0, // Would be populated from actual connection pools
            cache_hit_rate_percent: cache_hit_rate,
            average_latency_ms: coordinator_metrics.average_latency_ms,
            total_requests: coordinator_metrics.total_requests,
            error_rate_percent: error_rate,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
        };

        // Update stored health
        if let Ok(mut stored_health) = self.health.lock() {
            *stored_health = health.clone();
        }

        Ok(health)
    }

    /// Get performance metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<serde_json::Value> {
        let coordinator_metrics = self.coordinator.get_metrics().await;
        let health_summary = self
            .coordinator
            .get_health_summary()
            .await
            .unwrap_or_default();
        let uptime_seconds =
            (chrono::Utc::now().timestamp_millis() as u64 - self.startup_time) / 1000;

        Ok(serde_json::json!({
            "uptime_seconds": uptime_seconds,
            "coordinator_metrics": coordinator_metrics,
            "component_health": health_summary,
            "configuration": {
                "monitoring_enabled": self.config.enable_comprehensive_monitoring,
                "performance_optimization": self.config.enable_performance_optimization,
                "chaos_engineering": self.config.enable_chaos_engineering,
                "health_check_interval": self.config.health_check_interval_seconds
            },
            "last_updated": chrono::Utc::now().timestamp_millis()
        }))
    }

    /// Get the underlying KvStore for direct access when needed
    pub fn get_kv_store(&self) -> worker::kv::KvStore {
        self.coordinator.get_kv_store()
    }

    /// Get coordinator reference
    pub fn get_coordinator(&self) -> Arc<DataCoordinator> {
        self.coordinator.clone()
    }

    /// Check if the layer is healthy
    pub async fn is_healthy(&self) -> bool {
        self.health_check()
            .await
            .map(|h| h.is_healthy)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_access_layer_config_validation() {
        let config = DataAccessLayerConfig::default();
        assert!(config.validate().is_ok());

        let mut invalid_config = config;
        invalid_config.health_check_interval_seconds = 0;
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_data_access_layer_health_default() {
        let health = DataAccessLayerHealth::default();
        assert!(!health.is_healthy);
        assert_eq!(health.overall_score, 0.0);
        assert_eq!(health.cache_hit_rate_percent, 0.0);
    }

    #[test]
    fn test_high_concurrency_config() {
        let config = DataAccessLayerConfig::high_concurrency();
        assert!(config.enable_performance_optimization);
        assert_eq!(config.health_check_interval_seconds, 180);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = DataAccessLayerConfig::high_reliability();
        assert!(config.enable_chaos_engineering);
        assert_eq!(config.health_check_interval_seconds, 600);
    }
}
