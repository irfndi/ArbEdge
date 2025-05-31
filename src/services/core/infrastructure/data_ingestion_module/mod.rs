// Data Ingestion Module - Modular Data Ingestion with Enhanced Streaming Capabilities
// Replaces cloudflare_pipelines.rs (948 lines) with 4 specialized components

pub mod data_transformer;
pub mod ingestion_coordinator;
pub mod pipeline_manager;
pub mod queue_manager;

// Re-export main types for easy access
pub use data_transformer::{
    DataFormat, DataTransformer, DataTransformerConfig, TransformationMetrics, TransformationRule,
};
pub use ingestion_coordinator::{
    IngestionCoordinator, IngestionCoordinatorConfig, IngestionMetrics, IngestionRequest,
    IngestionResponse,
};
pub use pipeline_manager::{
    PipelineHealth, PipelineManager, PipelineManagerConfig, PipelineMetrics, PipelineType,
};
pub use queue_manager::{
    MessagePriority, QueueHealth, QueueManager, QueueManagerConfig, QueueMetrics, QueueType,
};

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

/// Data ingestion event types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestionEventType {
    MarketData,
    Analytics,
    Audit,
    UserActivity,
    SystemMetrics,
    TradingSignals,
    AIAnalysis,
    Custom(String),
}

impl IngestionEventType {
    pub fn as_str(&self) -> &str {
        match self {
            IngestionEventType::MarketData => "market_data",
            IngestionEventType::Analytics => "analytics",
            IngestionEventType::Audit => "audit",
            IngestionEventType::UserActivity => "user_activity",
            IngestionEventType::SystemMetrics => "system_metrics",
            IngestionEventType::TradingSignals => "trading_signals",
            IngestionEventType::AIAnalysis => "ai_analysis",
            IngestionEventType::Custom(name) => name,
        }
    }

    pub fn default_pipeline_id(&self) -> &str {
        match self {
            IngestionEventType::MarketData => "prod-market-data-pipeline",
            IngestionEventType::Analytics => "prod-analytics-pipeline",
            IngestionEventType::Audit => "prod-audit-pipeline",
            IngestionEventType::UserActivity => "prod-user-activity-pipeline",
            IngestionEventType::SystemMetrics => "prod-system-metrics-pipeline",
            IngestionEventType::TradingSignals => "prod-trading-signals-pipeline",
            IngestionEventType::AIAnalysis => "prod-ai-analysis-pipeline",
            IngestionEventType::Custom(_) => "prod-custom-pipeline",
        }
    }

    pub fn default_queue_name(&self) -> &str {
        match self {
            IngestionEventType::MarketData => "market-data-queue",
            IngestionEventType::Analytics => "analytics-queue",
            IngestionEventType::Audit => "audit-queue",
            IngestionEventType::UserActivity => "user-activity-queue",
            IngestionEventType::SystemMetrics => "system-metrics-queue",
            IngestionEventType::TradingSignals => "trading-signals-queue",
            IngestionEventType::AIAnalysis => "ai-analysis-queue",
            IngestionEventType::Custom(_) => "custom-queue",
        }
    }

    pub fn default_r2_prefix(&self) -> &str {
        match self {
            IngestionEventType::MarketData => "market-data",
            IngestionEventType::Analytics => "analytics",
            IngestionEventType::Audit => "audit",
            IngestionEventType::UserActivity => "user-activity",
            IngestionEventType::SystemMetrics => "system-metrics",
            IngestionEventType::TradingSignals => "trading-signals",
            IngestionEventType::AIAnalysis => "ai-analysis",
            IngestionEventType::Custom(_) => "custom",
        }
    }
}

/// Ingestion event with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionEvent {
    pub event_id: String,
    pub event_type: IngestionEventType,
    pub timestamp: u64,
    pub source: String,
    pub data: serde_json::Value,
    pub metadata: HashMap<String, String>,
    pub priority: u8, // 1 = high, 2 = medium, 3 = low
    pub retry_count: u32,
    pub max_retries: u32,
    pub ttl_seconds: Option<u64>,
}

impl IngestionEvent {
    pub fn new(event_type: IngestionEventType, source: String, data: serde_json::Value) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            source,
            data,
            metadata: HashMap::new(),
            priority: 2, // Default to medium priority
            retry_count: 0,
            max_retries: 3,
            ttl_seconds: None,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_ttl(mut self, ttl_seconds: u64) -> Self {
        self.ttl_seconds = Some(ttl_seconds);
        self
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_seconds {
            let now = chrono::Utc::now().timestamp_millis() as u64;
            let age_seconds = (now - self.timestamp) / 1000;
            age_seconds > ttl
        } else {
            false
        }
    }

    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries && !self.is_expired()
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

/// Data ingestion health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataIngestionHealth {
    pub is_healthy: bool,
    pub overall_score: f32,
    pub component_health: HashMap<String, bool>,
    pub pipeline_availability: bool,
    pub queue_availability: bool,
    pub r2_availability: bool,
    pub kv_fallback_available: bool,
    pub ingestion_rate_per_second: f64,
    pub error_rate_percent: f32,
    pub last_health_check: u64,
    pub last_error: Option<String>,
}

impl Default for DataIngestionHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            overall_score: 0.0,
            component_health: HashMap::new(),
            pipeline_availability: false,
            queue_availability: false,
            r2_availability: false,
            kv_fallback_available: false,
            ingestion_rate_per_second: 0.0,
            error_rate_percent: 0.0,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
            last_error: None,
        }
    }
}

/// Data ingestion performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataIngestionMetrics {
    pub total_events_ingested: u64,
    pub events_per_second: f64,
    pub successful_ingestions: u64,
    pub failed_ingestions: u64,
    pub retried_ingestions: u64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub events_by_type: HashMap<IngestionEventType, u64>,
    pub events_by_source: HashMap<String, u64>,
    pub pipeline_usage_percent: f32,
    pub queue_usage_percent: f32,
    pub r2_storage_used_gb: f64,
    pub compression_ratio_percent: f32,
    pub last_updated: u64,
}

impl Default for DataIngestionMetrics {
    fn default() -> Self {
        Self {
            total_events_ingested: 0,
            events_per_second: 0.0,
            successful_ingestions: 0,
            failed_ingestions: 0,
            retried_ingestions: 0,
            average_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            max_latency_ms: 0.0,
            events_by_type: HashMap::new(),
            events_by_source: HashMap::new(),
            pipeline_usage_percent: 0.0,
            queue_usage_percent: 0.0,
            r2_storage_used_gb: 0.0,
            compression_ratio_percent: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for Data Ingestion Module
#[derive(Debug, Clone)]
pub struct DataIngestionModuleConfig {
    pub pipeline_config: PipelineManagerConfig,
    pub queue_config: QueueManagerConfig,
    pub transformer_config: DataTransformerConfig,
    pub coordinator_config: IngestionCoordinatorConfig,
    pub enable_comprehensive_monitoring: bool,
    pub enable_chaos_engineering: bool,
    pub enable_performance_optimization: bool,
    pub health_check_interval_seconds: u64,
    pub r2_bucket_name: String,
    pub enable_kv_fallback: bool,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
}

impl Default for DataIngestionModuleConfig {
    fn default() -> Self {
        Self {
            pipeline_config: PipelineManagerConfig::default(),
            queue_config: QueueManagerConfig::default(),
            transformer_config: DataTransformerConfig::default(),
            coordinator_config: IngestionCoordinatorConfig::default(),
            enable_comprehensive_monitoring: true,
            enable_chaos_engineering: true,
            enable_performance_optimization: true,
            health_check_interval_seconds: 30,
            r2_bucket_name: "prod-arb-edge".to_string(),
            enable_kv_fallback: true,
            enable_compression: true,
            compression_threshold_bytes: 1024, // 1KB
        }
    }
}

impl DataIngestionModuleConfig {
    /// Create configuration optimized for high throughput
    pub fn high_throughput() -> Self {
        Self {
            pipeline_config: PipelineManagerConfig::high_throughput(),
            queue_config: QueueManagerConfig::high_throughput(),
            transformer_config: DataTransformerConfig::high_performance(),
            coordinator_config: IngestionCoordinatorConfig::high_throughput(),
            enable_performance_optimization: true,
            compression_threshold_bytes: 512, // More aggressive compression
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            pipeline_config: PipelineManagerConfig::high_reliability(),
            queue_config: QueueManagerConfig::high_reliability(),
            transformer_config: DataTransformerConfig::high_reliability(),
            coordinator_config: IngestionCoordinatorConfig::high_reliability(),
            enable_chaos_engineering: true,
            enable_kv_fallback: true,
            health_check_interval_seconds: 15, // More frequent health checks
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        self.pipeline_config.validate()?;
        self.queue_config.validate()?;
        self.transformer_config.validate()?;
        self.coordinator_config.validate()?;

        if self.health_check_interval_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "health_check_interval_seconds must be greater than 0",
            ));
        }

        if self.r2_bucket_name.is_empty() {
            return Err(ArbitrageError::validation_error(
                "r2_bucket_name cannot be empty",
            ));
        }

        if self.compression_threshold_bytes == 0 {
            return Err(ArbitrageError::validation_error(
                "compression_threshold_bytes must be greater than 0",
            ));
        }

        Ok(())
    }
}

/// Main Data Ingestion Module orchestrating all components
pub struct DataIngestionModule {
    config: DataIngestionModuleConfig,
    logger: crate::utils::logger::Logger,

    // Core components
    pipeline_manager: Arc<PipelineManager>,
    queue_manager: Arc<QueueManager>,
    data_transformer: Arc<DataTransformer>,
    coordinator: Arc<IngestionCoordinator>,

    // Health and metrics
    health: Arc<std::sync::Mutex<DataIngestionHealth>>,
    metrics: Arc<std::sync::Mutex<DataIngestionMetrics>>,

    // Performance tracking
    startup_time: u64,
}

impl DataIngestionModule {
    /// Create new DataIngestionModule instance
    pub async fn new(
        config: DataIngestionModuleConfig,
        kv_store: KvStore,
        env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize components
        let pipeline_manager =
            Arc::new(PipelineManager::new(config.pipeline_config.clone(), env).await?);
        let queue_manager = Arc::new(QueueManager::new(config.queue_config.clone(), env).await?);
        let data_transformer = Arc::new(DataTransformer::new(config.transformer_config.clone())?);
        let coordinator = Arc::new(
            IngestionCoordinator::new(
                config.coordinator_config.clone(),
                kv_store,
                pipeline_manager.clone(),
                queue_manager.clone(),
                data_transformer.clone(),
            )
            .await?,
        );

        logger.info(&format!(
            "DataIngestionModule initialized: pipeline_enabled={}, queue_enabled={}, compression_enabled={}",
            config.pipeline_config.enable_pipelines,
            config.queue_config.enable_queues,
            config.enable_compression
        ));

        Ok(Self {
            config,
            logger,
            pipeline_manager,
            queue_manager,
            data_transformer,
            coordinator,
            health: Arc::new(std::sync::Mutex::new(DataIngestionHealth::default())),
            metrics: Arc::new(std::sync::Mutex::new(DataIngestionMetrics::default())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Ingest a single event
    pub async fn ingest_event(&self, event: IngestionEvent) -> ArbitrageResult<()> {
        self.coordinator.ingest_event(event).await
    }

    /// Ingest multiple events in batch
    pub async fn ingest_batch(
        &self,
        events: Vec<IngestionEvent>,
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        self.coordinator.ingest_batch(events).await
    }

    // ============= LEGACY COMPATIBILITY METHODS =============

    /// Store market data (legacy compatibility method)
    pub async fn store_market_data(
        &self,
        exchange: &str,
        symbol: &str,
        data: &str,
    ) -> ArbitrageResult<()> {
        let market_data: serde_json::Value = serde_json::from_str(data).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse market data: {}", e))
        })?;

        let event = IngestionEvent::new(
            IngestionEventType::MarketData,
            format!("{}_{}", exchange, symbol),
            market_data,
        )
        .with_priority(2) // Medium priority
        .with_metadata("exchange".to_string(), exchange.to_string())
        .with_metadata("symbol".to_string(), symbol.to_string());

        self.ingest_event(event).await
    }

    /// Store analysis results (legacy compatibility method)
    pub async fn store_analysis_results(
        &self,
        analysis_type: &str,
        data: &str,
    ) -> ArbitrageResult<()> {
        let analysis_data: serde_json::Value = serde_json::from_str(data).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse analysis data: {}", e))
        })?;

        let event = IngestionEvent::new(
            IngestionEventType::Analytics,
            format!("analysis_{}", analysis_type),
            analysis_data,
        )
        .with_priority(2) // Medium priority
        .with_metadata("analysis_type".to_string(), analysis_type.to_string());

        self.ingest_event(event).await
    }

    /// Get latest data (legacy compatibility method)
    pub async fn get_latest_data(&self, key: &str) -> ArbitrageResult<Option<String>> {
        // This method would typically retrieve the latest data from storage
        // For now, we'll return None as this is a compatibility method
        self.logger.warn(&format!(
            "get_latest_data called for key {} - returning None (not implemented)",
            key
        ));
        Ok(None)
    }

    // ============= HEALTH AND METRICS METHODS =============

    /// Get ingestion health status
    pub async fn get_health(&self) -> DataIngestionHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            DataIngestionHealth::default()
        }
    }

    /// Get ingestion metrics
    pub async fn get_metrics(&self) -> DataIngestionMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            DataIngestionMetrics::default()
        }
    }

    /// Perform comprehensive health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let _start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check all components and log errors for better observability
        let pipeline_healthy = match self.pipeline_manager.health_check().await {
            Ok(healthy) => healthy,
            Err(e) => {
                self.logger
                    .error(&format!("Pipeline manager health check failed: {}", e));
                false
            }
        };

        let queue_healthy = match self.queue_manager.health_check().await {
            Ok(healthy) => healthy,
            Err(e) => {
                self.logger
                    .error(&format!("Queue manager health check failed: {}", e));
                false
            }
        };

        let transformer_healthy = match self.data_transformer.health_check().await {
            Ok(healthy) => healthy,
            Err(e) => {
                self.logger
                    .error(&format!("Data transformer health check failed: {}", e));
                false
            }
        };

        let coordinator_healthy = match self.coordinator.health_check().await {
            Ok(healthy) => healthy,
            Err(e) => {
                self.logger
                    .error(&format!("Ingestion coordinator health check failed: {}", e));
                false
            }
        };

        // Calculate overall health
        let healthy_components = [
            pipeline_healthy,
            queue_healthy,
            transformer_healthy,
            coordinator_healthy,
        ]
        .iter()
        .filter(|&&h| h)
        .count();
        let total_components = 4;
        let overall_score = healthy_components as f32 / total_components as f32;
        let is_healthy = overall_score >= 0.75; // 75% of components must be healthy

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = is_healthy;
            health.overall_score = overall_score;
            health
                .component_health
                .insert("pipeline_manager".to_string(), pipeline_healthy);
            health
                .component_health
                .insert("queue_manager".to_string(), queue_healthy);
            health
                .component_health
                .insert("data_transformer".to_string(), transformer_healthy);
            health
                .component_health
                .insert("ingestion_coordinator".to_string(), coordinator_healthy);
            health.pipeline_availability = pipeline_healthy;
            health.queue_availability = queue_healthy;
            health.last_health_check = chrono::Utc::now().timestamp_millis() as u64;

            if !is_healthy {
                health.last_error = Some(format!(
                    "Health check failed: {}/{} components healthy",
                    healthy_components, total_components
                ));
            } else {
                health.last_error = None;
            }
        }

        Ok(is_healthy)
    }

    /// Get comprehensive health summary
    pub async fn get_health_summary(&self) -> ArbitrageResult<serde_json::Value> {
        let health = self.get_health().await;
        let metrics = self.get_metrics().await;
        let pipeline_health = self.pipeline_manager.get_health().await;
        let queue_health = self.queue_manager.get_health().await;
        let coordinator_metrics = self.coordinator.get_metrics().await;

        Ok(serde_json::json!({
            "overall_health": {
                "is_healthy": health.is_healthy,
                "score": health.overall_score,
                "last_check": health.last_health_check,
                "error": health.last_error
            },
            "components": {
                "pipeline_manager": {
                    "healthy": health.pipeline_availability,
                    "details": pipeline_health
                },
                "queue_manager": {
                    "healthy": health.queue_availability,
                    "details": queue_health
                },
                "data_transformer": {
                    "healthy": health.component_health.get("data_transformer").unwrap_or(&false)
                },
                "ingestion_coordinator": {
                    "healthy": health.component_health.get("ingestion_coordinator").unwrap_or(&false),
                    "metrics": coordinator_metrics
                }
            },
            "performance": {
                "events_per_second": metrics.events_per_second,
                "average_latency_ms": metrics.average_latency_ms,
                "error_rate_percent": (metrics.failed_ingestions as f64 / metrics.total_events_ingested.max(1) as f64) * 100.0,
                "total_events": metrics.total_events_ingested
            },
            "storage": {
                "r2_usage_gb": metrics.r2_storage_used_gb,
                "compression_ratio": metrics.compression_ratio_percent
            }
        }))
    }

    /// Get component references for advanced usage
    pub fn get_pipeline_manager(&self) -> Arc<PipelineManager> {
        self.pipeline_manager.clone()
    }

    pub fn get_queue_manager(&self) -> Arc<QueueManager> {
        self.queue_manager.clone()
    }

    pub fn get_data_transformer(&self) -> Arc<DataTransformer> {
        self.data_transformer.clone()
    }

    pub fn get_coordinator(&self) -> Arc<IngestionCoordinator> {
        self.coordinator.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingestion_event_type_defaults() {
        assert_eq!(IngestionEventType::MarketData.as_str(), "market_data");
        assert_eq!(
            IngestionEventType::MarketData.default_pipeline_id(),
            "prod-market-data-pipeline"
        );
        assert_eq!(
            IngestionEventType::Analytics.default_queue_name(),
            "analytics-queue"
        );
        assert_eq!(IngestionEventType::Audit.default_r2_prefix(), "audit");
    }

    #[test]
    fn test_ingestion_event_creation() {
        let data = serde_json::json!({"test": "data"});
        let event = IngestionEvent::new(
            IngestionEventType::MarketData,
            "test_source".to_string(),
            data.clone(),
        );

        assert_eq!(event.event_type, IngestionEventType::MarketData);
        assert_eq!(event.source, "test_source");
        assert_eq!(event.data, data);
        assert_eq!(event.priority, 2);
        assert_eq!(event.retry_count, 0);
        assert_eq!(event.max_retries, 3);
    }

    #[test]
    fn test_ingestion_event_expiration() {
        let mut event = IngestionEvent::new(
            IngestionEventType::Analytics,
            "test".to_string(),
            serde_json::json!({}),
        );

        // Event without TTL should not expire
        assert!(!event.is_expired());

        // Event with future TTL should not expire
        event = event.with_ttl(3600); // 1 hour
        assert!(!event.is_expired());

        // Simulate old timestamp
        event.timestamp = chrono::Utc::now().timestamp_millis() as u64 - 7200000; // 2 hours ago
        event = event.with_ttl(3600); // 1 hour TTL
        assert!(event.is_expired());
    }

    #[test]
    fn test_data_ingestion_module_config_validation() {
        let mut config = DataIngestionModuleConfig::default();
        assert!(config.validate().is_ok());

        config.health_check_interval_seconds = 0;
        assert!(config.validate().is_err());

        config.health_check_interval_seconds = 30;
        config.r2_bucket_name = "".to_string();
        assert!(config.validate().is_err());

        config.r2_bucket_name = "test-bucket".to_string();
        config.compression_threshold_bytes = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_throughput_config() {
        let config = DataIngestionModuleConfig::high_throughput();
        assert!(config.enable_performance_optimization);
        assert_eq!(config.compression_threshold_bytes, 512);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = DataIngestionModuleConfig::high_reliability();
        assert!(config.enable_chaos_engineering);
        assert!(config.enable_kv_fallback);
        assert_eq!(config.health_check_interval_seconds, 15);
    }
}
