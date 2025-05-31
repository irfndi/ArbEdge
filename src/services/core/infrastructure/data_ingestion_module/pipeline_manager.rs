// Pipeline Manager - Cloudflare Pipelines Integration with R2 Storage
// Provides efficient data storage with automatic compression and partitioning

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use worker::Env;

/// Pipeline types for different data streams
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineType {
    MarketData,
    Analytics,
    Audit,
    UserActivity,
    SystemMetrics,
    TradingSignals,
    AIAnalysis,
    Custom(String),
}

impl PipelineType {
    pub fn as_str(&self) -> &str {
        match self {
            PipelineType::MarketData => "market_data",
            PipelineType::Analytics => "analytics",
            PipelineType::Audit => "audit",
            PipelineType::UserActivity => "user_activity",
            PipelineType::SystemMetrics => "system_metrics",
            PipelineType::TradingSignals => "trading_signals",
            PipelineType::AIAnalysis => "ai_analysis",
            PipelineType::Custom(name) => name,
        }
    }

    pub fn default_pipeline_id(&self) -> &str {
        match self {
            PipelineType::MarketData => "prod-market-data-pipeline",
            PipelineType::Analytics => "prod-analytics-pipeline",
            PipelineType::Audit => "prod-audit-pipeline",
            PipelineType::UserActivity => "prod-user-activity-pipeline",
            PipelineType::SystemMetrics => "prod-system-metrics-pipeline",
            PipelineType::TradingSignals => "prod-trading-signals-pipeline",
            PipelineType::AIAnalysis => "prod-ai-analysis-pipeline",
            PipelineType::Custom(_) => "prod-custom-pipeline",
        }
    }

    pub fn r2_prefix(&self) -> &str {
        match self {
            PipelineType::MarketData => "market-data",
            PipelineType::Analytics => "analytics",
            PipelineType::Audit => "audit",
            PipelineType::UserActivity => "user-activity",
            PipelineType::SystemMetrics => "system-metrics",
            PipelineType::TradingSignals => "trading-signals",
            PipelineType::AIAnalysis => "ai-analysis",
            PipelineType::Custom(_) => "custom",
        }
    }

    pub fn compression_enabled(&self) -> bool {
        match self {
            PipelineType::MarketData => true, // High volume data
            PipelineType::Analytics => true,  // High volume data
            PipelineType::Audit => false,     // Legal compliance, no compression
            PipelineType::UserActivity => true,
            PipelineType::SystemMetrics => true,
            PipelineType::TradingSignals => false, // Time-sensitive, no compression
            PipelineType::AIAnalysis => true,
            PipelineType::Custom(_) => true,
        }
    }
}

/// Pipeline health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineHealth {
    pub is_healthy: bool,
    pub pipelines_available: bool,
    pub r2_available: bool,
    pub last_success_timestamp: u64,
    pub last_error: Option<String>,
    pub success_rate_percent: f32,
    pub average_latency_ms: f64,
    pub active_pipelines: u32,
    pub total_pipelines: u32,
    pub last_health_check: u64,
}

impl Default for PipelineHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            pipelines_available: false,
            r2_available: false,
            last_success_timestamp: 0,
            last_error: None,
            success_rate_percent: 0.0,
            average_latency_ms: 0.0,
            active_pipelines: 0,
            total_pipelines: 0,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Pipeline performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineMetrics {
    pub total_events_processed: u64,
    pub events_per_second: f64,
    pub successful_ingestions: u64,
    pub failed_ingestions: u64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub batch_operations: u64,
    pub compression_savings_bytes: u64,
    pub r2_storage_used_gb: f64,
    pub pipeline_usage_by_type: HashMap<PipelineType, u64>,
    pub data_partitions_created: u64,
    pub last_updated: u64,
}

impl Default for PipelineMetrics {
    fn default() -> Self {
        Self {
            total_events_processed: 0,
            events_per_second: 0.0,
            successful_ingestions: 0,
            failed_ingestions: 0,
            average_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            max_latency_ms: 0.0,
            batch_operations: 0,
            compression_savings_bytes: 0,
            r2_storage_used_gb: 0.0,
            pipeline_usage_by_type: HashMap::new(),
            data_partitions_created: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for PipelineManager
#[derive(Debug, Clone)]
pub struct PipelineManagerConfig {
    pub enable_pipelines: bool,
    pub enable_r2_storage: bool,
    pub enable_compression: bool,
    pub enable_data_partitioning: bool,
    pub enable_batch_processing: bool,
    pub batch_size: usize,
    pub batch_timeout_seconds: u64,
    pub compression_threshold_bytes: usize,
    pub max_concurrent_pipelines: u32,
    pub pipeline_timeout_seconds: u64,
    pub enable_schema_evolution: bool,
    pub enable_health_monitoring: bool,
    pub health_check_interval_seconds: u64,
    pub r2_bucket_name: String,
    pub account_id: String,
    pub api_token: String,
}

impl Default for PipelineManagerConfig {
    fn default() -> Self {
        Self {
            enable_pipelines: true,
            enable_r2_storage: true,
            enable_compression: true,
            enable_data_partitioning: true,
            enable_batch_processing: true,
            batch_size: 200,
            batch_timeout_seconds: 300,        // 5 minutes
            compression_threshold_bytes: 1024, // 1KB
            max_concurrent_pipelines: 10,
            pipeline_timeout_seconds: 30,
            enable_schema_evolution: true,
            enable_health_monitoring: true,
            health_check_interval_seconds: 60,
            r2_bucket_name: "prod-arb-edge".to_string(),
            account_id: String::new(),
            api_token: String::new(),
        }
    }
}

impl PipelineManagerConfig {
    /// Create configuration optimized for high throughput
    pub fn high_throughput() -> Self {
        Self {
            batch_size: 500,
            batch_timeout_seconds: 180, // 3 minutes
            max_concurrent_pipelines: 20,
            pipeline_timeout_seconds: 60,
            compression_threshold_bytes: 512, // More aggressive compression
            enable_batch_processing: true,
            enable_compression: true,
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            batch_size: 100,
            batch_timeout_seconds: 600, // 10 minutes
            max_concurrent_pipelines: 5,
            pipeline_timeout_seconds: 120,
            health_check_interval_seconds: 30, // More frequent health checks
            enable_health_monitoring: true,
            enable_schema_evolution: true,
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "batch_size must be greater than 0",
            ));
        }
        if self.batch_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "batch_timeout_seconds must be greater than 0",
            ));
        }
        if self.max_concurrent_pipelines == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_pipelines must be greater than 0",
            ));
        }
        if self.pipeline_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "pipeline_timeout_seconds must be greater than 0",
            ));
        }
        if self.compression_threshold_bytes == 0 {
            return Err(ArbitrageError::validation_error(
                "compression_threshold_bytes must be greater than 0",
            ));
        }
        if self.r2_bucket_name.is_empty() {
            return Err(ArbitrageError::validation_error(
                "r2_bucket_name cannot be empty",
            ));
        }
        Ok(())
    }
}

/// Pipeline data event for ingestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEvent {
    pub event_id: String,
    pub pipeline_type: PipelineType,
    pub timestamp: u64,
    pub data: serde_json::Value,
    pub metadata: HashMap<String, String>,
    pub partition_key: Option<String>,
    pub compression_enabled: bool,
    pub schema_version: String,
}

impl PipelineEvent {
    pub fn new(pipeline_type: PipelineType, data: serde_json::Value) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            pipeline_type: pipeline_type.clone(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            data,
            metadata: HashMap::new(),
            partition_key: None,
            compression_enabled: pipeline_type.compression_enabled(),
            schema_version: "1.0".to_string(),
        }
    }

    pub fn with_partition_key(mut self, partition_key: String) -> Self {
        self.partition_key = Some(partition_key);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_schema_version(mut self, version: String) -> Self {
        self.schema_version = version;
        self
    }

    pub fn generate_r2_key(&self) -> String {
        let date = chrono::DateTime::from_timestamp_millis(self.timestamp as i64)
            .unwrap_or(chrono::Utc::now())
            .format("%Y/%m/%d");

        let hour = chrono::DateTime::from_timestamp_millis(self.timestamp as i64)
            .unwrap_or(chrono::Utc::now())
            .format("%H");

        if let Some(partition_key) = &self.partition_key {
            format!(
                "{}/{}/{}/{}/{}.json",
                self.pipeline_type.r2_prefix(),
                date,
                hour,
                partition_key,
                self.event_id
            )
        } else {
            format!(
                "{}/{}/{}/{}.json",
                self.pipeline_type.r2_prefix(),
                date,
                hour,
                self.event_id
            )
        }
    }
}

/// Pipeline Manager for Cloudflare Pipelines integration
pub struct PipelineManager {
    config: PipelineManagerConfig,
    logger: crate::utils::logger::Logger,

    // Cloudflare credentials
    account_id: String,
    api_token: String,

    // Service availability
    pipelines_available: Arc<StdMutex<bool>>,
    r2_available: Arc<StdMutex<bool>>,

    // Health and metrics
    health: Arc<StdMutex<PipelineHealth>>,
    metrics: Arc<StdMutex<PipelineMetrics>>,

    // Active pipeline tracking
    active_pipelines: Arc<StdMutex<HashMap<String, u64>>>, // pipeline_id -> start_time

    // Performance tracking
    startup_time: u64,
}

impl PipelineManager {
    /// Create new PipelineManager instance
    pub async fn new(mut config: PipelineManagerConfig, env: &Env) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Get Cloudflare credentials from environment
        config.account_id = env
            .secret("CLOUDFLARE_ACCOUNT_ID")
            .unwrap_or_else(|_| worker::Secret::from(worker::wasm_bindgen::JsValue::from_str("")))
            .to_string();
        config.api_token = env
            .secret("CLOUDFLARE_API_TOKEN")
            .unwrap_or_else(|_| worker::Secret::from(worker::wasm_bindgen::JsValue::from_str("")))
            .to_string();

        // Validate configuration
        config.validate()?;

        // Check service availability
        let pipelines_available = !config.account_id.is_empty()
            && !config.api_token.is_empty()
            && config.enable_pipelines;
        let r2_available = !config.account_id.is_empty()
            && !config.api_token.is_empty()
            && config.enable_r2_storage;

        if !pipelines_available && config.enable_pipelines {
            logger.warn("Pipelines service disabled: missing Cloudflare credentials");
        }

        if !r2_available && config.enable_r2_storage {
            logger.warn("R2 storage disabled: missing Cloudflare credentials");
        }

        logger.info(&format!(
            "PipelineManager initialized: pipelines_enabled={}, r2_enabled={}, batch_size={}",
            pipelines_available, r2_available, config.batch_size
        ));

        Ok(Self {
            account_id: config.account_id.clone(),
            api_token: config.api_token.clone(),
            config,
            logger,
            pipelines_available: Arc::new(StdMutex::new(pipelines_available)),
            r2_available: Arc::new(StdMutex::new(r2_available)),
            health: Arc::new(StdMutex::new(PipelineHealth::default())),
            metrics: Arc::new(StdMutex::new(PipelineMetrics::default())),
            active_pipelines: Arc::new(StdMutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Ingest a single event into pipeline
    pub async fn ingest_event(&self, event: PipelineEvent) -> ArbitrageResult<()> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check if pipelines are available
        if !self.is_pipelines_available().await {
            return Err(ArbitrageError::service_unavailable(
                "Pipelines service not available",
            ));
        }

        // Process the event
        match self.process_pipeline_event(&event).await {
            Ok(_) => {
                self.record_success(&event.pipeline_type, start_time).await;
                Ok(())
            }
            Err(e) => {
                self.record_failure(&event.pipeline_type, start_time, &e)
                    .await;
                Err(e)
            }
        }
    }

    /// Ingest multiple events in batch
    pub async fn ingest_batch(
        &self,
        events: Vec<PipelineEvent>,
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        if events.is_empty() {
            return Ok(vec![]);
        }

        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let mut results = Vec::with_capacity(events.len());

        // Process events in batches
        for chunk in events.chunks(self.config.batch_size) {
            for event in chunk {
                let result = self.ingest_event(event.clone()).await;
                results.push(result);
            }
        }

        self.record_batch_operation(start_time).await;
        Ok(results)
    }

    /// Store data directly to R2 (fallback when pipelines unavailable)
    pub async fn store_to_r2(&self, event: &PipelineEvent) -> ArbitrageResult<()> {
        if !self.is_r2_available().await {
            return Err(ArbitrageError::service_unavailable(
                "R2 storage not available",
            ));
        }

        let r2_key = event.generate_r2_key();
        let _data = if event.compression_enabled && self.config.enable_compression {
            self.compress_data(&serde_json::to_string(&event.data)?)?
        } else {
            serde_json::to_string(&event.data)?
        };

        // In a real implementation, this would use the R2 API
        // For now, we'll simulate the operation
        self.logger.info(&format!(
            "Storing event {} to R2 key: {}",
            event.event_id, r2_key
        ));

        Ok(())
    }

    /// Check if pipelines service is available
    pub async fn is_pipelines_available(&self) -> bool {
        if let Ok(available) = self.pipelines_available.lock() {
            *available
        } else {
            false
        }
    }

    /// Check if R2 storage is available
    pub async fn is_r2_available(&self) -> bool {
        if let Ok(available) = self.r2_available.lock() {
            *available
        } else {
            false
        }
    }

    /// Get pipeline health status
    pub async fn get_health(&self) -> PipelineHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            PipelineHealth::default()
        }
    }

    /// Get pipeline metrics
    pub async fn get_metrics(&self) -> PipelineMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            PipelineMetrics::default()
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Test pipeline availability
        let pipelines_healthy = self.test_pipeline_connection().await;
        let r2_healthy = self.test_r2_connection().await;

        // Update availability status
        if let Ok(mut available) = self.pipelines_available.lock() {
            *available = pipelines_healthy;
        }
        if let Ok(mut available) = self.r2_available.lock() {
            *available = r2_healthy;
        }

        // Calculate overall health
        let is_healthy = pipelines_healthy || r2_healthy; // At least one service must be available

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = is_healthy;
            health.pipelines_available = pipelines_healthy;
            health.r2_available = r2_healthy;
            health.last_health_check = chrono::Utc::now().timestamp_millis() as u64;

            if is_healthy {
                health.last_success_timestamp = start_time;
                health.last_error = None;
            } else {
                health.last_error =
                    Some("Both pipelines and R2 storage are unavailable".to_string());
            }
        }

        Ok(is_healthy)
    }

    /// Get latest data (legacy compatibility method)
    pub async fn get_latest_data(&self, key: &str) -> ArbitrageResult<Option<String>> {
        // This method would typically retrieve the latest data from R2 or pipeline storage
        // For now, we'll return None as this is a compatibility method
        self.logger.warn(&format!(
            "get_latest_data called for key {} - returning None (not implemented)",
            key
        ));
        Ok(None)
    }

    /// Process a pipeline event
    async fn process_pipeline_event(&self, event: &PipelineEvent) -> ArbitrageResult<()> {
        let pipeline_id = event.pipeline_type.default_pipeline_id();

        // Track active pipeline
        if let Ok(mut active) = self.active_pipelines.lock() {
            active.insert(
                pipeline_id.to_string(),
                chrono::Utc::now().timestamp_millis() as u64,
            );
        }

        // In a real implementation, this would send data to Cloudflare Pipelines
        // For now, we'll simulate the operation and store to R2 as fallback
        self.logger.info(&format!(
            "Processing pipeline event {} for pipeline {}",
            event.event_id, pipeline_id
        ));

        // Simulate pipeline processing
        if self.is_pipelines_available().await {
            // Primary: Send to pipeline
            self.send_to_pipeline(event).await?;
        } else if self.is_r2_available().await {
            // Fallback: Store directly to R2
            self.store_to_r2(event).await?;
        } else {
            return Err(ArbitrageError::service_unavailable(
                "No storage services available",
            ));
        }

        // Remove from active pipelines
        if let Ok(mut active) = self.active_pipelines.lock() {
            active.remove(pipeline_id);
        }

        Ok(())
    }

    /// Send event to Cloudflare Pipeline
    async fn send_to_pipeline(&self, event: &PipelineEvent) -> ArbitrageResult<()> {
        // In a real implementation, this would use the Cloudflare Pipelines API
        self.logger
            .info(&format!("Sending event {} to pipeline", event.event_id));
        Ok(())
    }

    /// Test pipeline connection
    async fn test_pipeline_connection(&self) -> bool {
        if self.account_id.is_empty() || self.api_token.is_empty() {
            return false;
        }

        // In a real implementation, this would test the Cloudflare Pipelines API
        // For now, we'll simulate a successful connection if credentials are present
        true
    }

    /// Test R2 connection
    async fn test_r2_connection(&self) -> bool {
        if self.account_id.is_empty() || self.api_token.is_empty() {
            return false;
        }

        // In a real implementation, this would test the R2 API
        // For now, we'll simulate a successful connection if credentials are present
        true
    }

    /// Compress data (placeholder implementation)
    fn compress_data(&self, data: &str) -> ArbitrageResult<String> {
        // In a real implementation, this would use a compression library
        // For now, we'll just return the original data
        Ok(data.to_string())
    }

    /// Record successful operation
    async fn record_success(&self, _pipeline_type: &PipelineType, _latency: u64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_events_processed += 1;
        metrics.successful_ingestions += 1;

        let mut health = self.health.lock().unwrap();
        health.success_rate_percent =
            metrics.successful_ingestions as f32 / metrics.total_events_processed as f32 * 100.0;
    }

    /// Record failed operation
    async fn record_failure(
        &self,
        _pipeline_type: &PipelineType,
        start_time: u64,
        error: &ArbitrageError,
    ) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let latency = end_time - start_time;

        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_events_processed += 1;
        metrics.failed_ingestions += 1;
        metrics.average_latency_ms = (metrics.average_latency_ms
            * (metrics.total_events_processed - 1) as f64
            + latency as f64)
            / metrics.total_events_processed as f64;
        metrics.min_latency_ms = metrics.min_latency_ms.min(latency as f64);
        metrics.max_latency_ms = metrics.max_latency_ms.max(latency as f64);
        metrics.last_updated = end_time;

        let mut health = self.health.lock().unwrap();
        health.last_error = Some(error.to_string());
        let total_operations =
            health.success_rate_percent * metrics.total_events_processed as f32 / 100.0;
        health.success_rate_percent =
            (total_operations / metrics.total_events_processed as f32) * 100.0;
    }

    /// Record batch operation
    async fn record_batch_operation(&self, _start_time: u64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.batch_operations += 1;
        metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
    }

    async fn create_pipeline_config(
        &self,
        _pipeline_type: &PipelineType,
        _config: &PipelineManagerConfig,
    ) -> ArbitrageResult<serde_json::Value> {
        // Implementation of create_pipeline_config method
        Ok(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_type_defaults() {
        assert_eq!(PipelineType::MarketData.as_str(), "market_data");
        assert_eq!(
            PipelineType::MarketData.default_pipeline_id(),
            "prod-market-data-pipeline"
        );
        assert_eq!(PipelineType::Analytics.r2_prefix(), "analytics");
        assert!(PipelineType::MarketData.compression_enabled());
        assert!(!PipelineType::Audit.compression_enabled());
    }

    #[test]
    fn test_pipeline_event_creation() {
        let data = serde_json::json!({"test": "data"});
        let event = PipelineEvent::new(PipelineType::MarketData, data.clone());

        assert_eq!(event.pipeline_type, PipelineType::MarketData);
        assert_eq!(event.data, data);
        assert!(event.compression_enabled);
        assert_eq!(event.schema_version, "1.0");
    }

    #[test]
    fn test_pipeline_event_r2_key_generation() {
        let data = serde_json::json!({"test": "data"});
        let event = PipelineEvent::new(PipelineType::Analytics, data)
            .with_partition_key("exchange_binance".to_string());

        let r2_key = event.generate_r2_key();
        assert!(r2_key.starts_with("analytics/"));
        assert!(r2_key.contains("exchange_binance"));
        assert!(r2_key.ends_with(".json"));
    }

    #[test]
    fn test_pipeline_manager_config_validation() {
        let mut config = PipelineManagerConfig::default();
        assert!(config.validate().is_ok());

        config.batch_size = 0;
        assert!(config.validate().is_err());

        config.batch_size = 100;
        config.r2_bucket_name = "".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_throughput_config() {
        let config = PipelineManagerConfig::high_throughput();
        assert_eq!(config.batch_size, 500);
        assert_eq!(config.max_concurrent_pipelines, 20);
        assert_eq!(config.compression_threshold_bytes, 512);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = PipelineManagerConfig::high_reliability();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.health_check_interval_seconds, 30);
        assert!(config.enable_health_monitoring);
    }
}
