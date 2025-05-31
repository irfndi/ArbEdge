// Monitoring Module - Advanced Observability with Real-time Dashboards and Alerting
// Replaces monitoring_observability.rs (1,691 lines) with 5 specialized components

pub mod alert_manager;
pub mod health_monitor;
pub mod metrics_collector;
pub mod observability_coordinator;
pub mod trace_collector;

// Re-export main types for easy access
pub use alert_manager::{
    AlertHealth, AlertManager, AlertManagerConfig, AlertRule, AlertSeverity, AlertStatus,
};
pub use health_monitor::{
    ComponentHealth, HealthCheck, HealthMetrics, HealthMonitor, HealthMonitorConfig, HealthStatus,
};
pub use metrics_collector::{
    MetricType, MetricValue, MetricsCollector, MetricsCollectorConfig, MetricsData, MetricsHealth,
};
pub use observability_coordinator::{
    ObservabilityCoordinator, ObservabilityCoordinatorConfig, ObservabilityHealth,
    ObservabilityMetrics,
};
pub use trace_collector::{
    TraceCollector, TraceCollectorConfig, TraceContext, TraceSpan, TracingHealth, TracingMetrics,
};

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

/// Observability event types for comprehensive monitoring
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservabilityEventType {
    SystemMetrics,
    ApplicationMetrics,
    BusinessMetrics,
    ServiceMetrics,
    PerformanceMetrics,
    SecurityMetrics,
    UserMetrics,
    ErrorMetrics,
    Custom(String),
}

impl ObservabilityEventType {
    pub fn as_str(&self) -> &str {
        match self {
            ObservabilityEventType::SystemMetrics => "system_metrics",
            ObservabilityEventType::ApplicationMetrics => "application_metrics",
            ObservabilityEventType::BusinessMetrics => "business_metrics",
            ObservabilityEventType::ServiceMetrics => "service_metrics",
            ObservabilityEventType::PerformanceMetrics => "performance_metrics",
            ObservabilityEventType::SecurityMetrics => "security_metrics",
            ObservabilityEventType::UserMetrics => "user_metrics",
            ObservabilityEventType::ErrorMetrics => "error_metrics",
            ObservabilityEventType::Custom(name) => name,
        }
    }

    pub fn default_collection_interval_seconds(&self) -> u64 {
        match self {
            ObservabilityEventType::SystemMetrics => 30,
            ObservabilityEventType::ApplicationMetrics => 60,
            ObservabilityEventType::BusinessMetrics => 300,
            ObservabilityEventType::ServiceMetrics => 15,
            ObservabilityEventType::PerformanceMetrics => 10,
            ObservabilityEventType::SecurityMetrics => 60,
            ObservabilityEventType::UserMetrics => 120,
            ObservabilityEventType::ErrorMetrics => 5,
            ObservabilityEventType::Custom(_) => 60,
        }
    }

    pub fn default_retention_days(&self) -> u32 {
        match self {
            ObservabilityEventType::SystemMetrics => 30,
            ObservabilityEventType::ApplicationMetrics => 90,
            ObservabilityEventType::BusinessMetrics => 365,
            ObservabilityEventType::ServiceMetrics => 30,
            ObservabilityEventType::PerformanceMetrics => 7,
            ObservabilityEventType::SecurityMetrics => 365,
            ObservabilityEventType::UserMetrics => 90,
            ObservabilityEventType::ErrorMetrics => 30,
            ObservabilityEventType::Custom(_) => 30,
        }
    }
}

/// Observability data point with comprehensive metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityDataPoint {
    pub id: String,
    pub event_type: ObservabilityEventType,
    pub timestamp: u64,
    pub source: String,
    pub component: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub unit: String,
    pub tags: HashMap<String, String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub severity: ObservabilitySeverity,
    pub ttl_seconds: Option<u64>,
}

impl ObservabilityDataPoint {
    pub fn new(
        event_type: ObservabilityEventType,
        source: String,
        component: String,
        metric_name: String,
        metric_value: f64,
        unit: String,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            event_type,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            source,
            component,
            metric_name,
            metric_value,
            unit,
            tags: HashMap::new(),
            metadata: HashMap::new(),
            severity: ObservabilitySeverity::Info,
            ttl_seconds: None,
        }
    }

    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }

    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_severity(mut self, severity: ObservabilitySeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_ttl(mut self, ttl_seconds: u64) -> Self {
        self.ttl_seconds = Some(ttl_seconds);
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

    pub fn age_seconds(&self) -> u64 {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        (now - self.timestamp) / 1000
    }
}

/// Observability severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ObservabilitySeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
    Warning,
    Error,
    Debug,
}

impl ObservabilitySeverity {
    pub fn priority_score(&self) -> u8 {
        match self {
            ObservabilitySeverity::Critical => 7,
            ObservabilitySeverity::High => 6,
            ObservabilitySeverity::Medium => 5,
            ObservabilitySeverity::Low => 4,
            ObservabilitySeverity::Info => 3,
            ObservabilitySeverity::Warning => 2,
            ObservabilitySeverity::Error => 1,
            ObservabilitySeverity::Debug => 0,
        }
    }
}

/// Monitoring module health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringModuleHealth {
    pub is_healthy: bool,
    pub overall_score: f32,
    pub component_health: HashMap<String, bool>,
    pub metrics_collector_available: bool,
    pub alert_manager_available: bool,
    pub trace_collector_available: bool,
    pub health_monitor_available: bool,
    pub observability_coordinator_available: bool,
    pub data_collection_rate_per_second: f64,
    pub alert_processing_rate_per_second: f64,
    pub trace_processing_rate_per_second: f64,
    pub error_rate_percent: f32,
    pub last_health_check: u64,
    pub last_error: Option<String>,
}

impl Default for MonitoringModuleHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            overall_score: 0.0,
            component_health: HashMap::new(),
            metrics_collector_available: false,
            alert_manager_available: false,
            trace_collector_available: false,
            health_monitor_available: false,
            observability_coordinator_available: false,
            data_collection_rate_per_second: 0.0,
            alert_processing_rate_per_second: 0.0,
            trace_processing_rate_per_second: 0.0,
            error_rate_percent: 0.0,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
            last_error: None,
        }
    }
}

/// Monitoring module performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringModuleMetrics {
    pub total_data_points_collected: u64,
    pub data_points_per_second: f64,
    pub successful_collections: u64,
    pub failed_collections: u64,
    pub total_alerts_generated: u64,
    pub alerts_per_second: f64,
    pub successful_alerts: u64,
    pub failed_alerts: u64,
    pub total_traces_collected: u64,
    pub traces_per_second: f64,
    pub successful_traces: u64,
    pub failed_traces: u64,
    pub avg_collection_latency_ms: f64,
    pub min_collection_latency_ms: f64,
    pub max_collection_latency_ms: f64,
    pub avg_alert_latency_ms: f64,
    pub min_alert_latency_ms: f64,
    pub max_alert_latency_ms: f64,
    pub avg_trace_latency_ms: f64,
    pub min_trace_latency_ms: f64,
    pub max_trace_latency_ms: f64,
    pub data_points_by_type: HashMap<ObservabilityEventType, u64>,
    pub data_points_by_severity: HashMap<ObservabilitySeverity, u64>,
    pub data_points_by_component: HashMap<String, u64>,
    pub storage_used_mb: f64,
    pub compression_ratio_percent: f32,
    pub last_updated: u64,
}

impl Default for MonitoringModuleMetrics {
    fn default() -> Self {
        Self {
            total_data_points_collected: 0,
            data_points_per_second: 0.0,
            successful_collections: 0,
            failed_collections: 0,
            total_alerts_generated: 0,
            alerts_per_second: 0.0,
            successful_alerts: 0,
            failed_alerts: 0,
            total_traces_collected: 0,
            traces_per_second: 0.0,
            successful_traces: 0,
            failed_traces: 0,
            avg_collection_latency_ms: 0.0,
            min_collection_latency_ms: 0.0,
            max_collection_latency_ms: 0.0,
            avg_alert_latency_ms: 0.0,
            min_alert_latency_ms: 0.0,
            max_alert_latency_ms: 0.0,
            avg_trace_latency_ms: 0.0,
            min_trace_latency_ms: 0.0,
            max_trace_latency_ms: 0.0,
            data_points_by_type: HashMap::new(),
            data_points_by_severity: HashMap::new(),
            data_points_by_component: HashMap::new(),
            storage_used_mb: 0.0,
            compression_ratio_percent: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Monitoring module configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringModuleConfig {
    pub metrics_collector_config: MetricsCollectorConfig,
    pub alert_manager_config: AlertManagerConfig,
    pub trace_collector_config: TraceCollectorConfig,
    pub health_monitor_config: HealthMonitorConfig,
    pub observability_coordinator_config: ObservabilityCoordinatorConfig,
    pub enable_comprehensive_monitoring: bool,
    pub enable_real_time_dashboards: bool,
    pub enable_intelligent_alerting: bool,
    pub enable_distributed_tracing: bool,
    pub enable_predictive_health: bool,
    pub health_check_interval_seconds: u64,
    pub metrics_retention_days: u32,
    pub traces_retention_days: u32,
    pub alerts_retention_days: u32,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
    pub enable_export_to_external: bool,
    pub external_export_endpoints: Vec<String>,
}

impl Default for MonitoringModuleConfig {
    fn default() -> Self {
        Self {
            metrics_collector_config: MetricsCollectorConfig::default(),
            alert_manager_config: AlertManagerConfig::default(),
            trace_collector_config: TraceCollectorConfig::default(),
            health_monitor_config: HealthMonitorConfig::default(),
            observability_coordinator_config: ObservabilityCoordinatorConfig::default(),
            enable_comprehensive_monitoring: true,
            enable_real_time_dashboards: true,
            enable_intelligent_alerting: true,
            enable_distributed_tracing: true,
            enable_predictive_health: true,
            health_check_interval_seconds: 30,
            metrics_retention_days: 30,
            traces_retention_days: 7,
            alerts_retention_days: 90,
            enable_compression: true,
            compression_threshold_bytes: 10240, // 10KB
            enable_export_to_external: false,
            external_export_endpoints: Vec::new(),
        }
    }
}

impl MonitoringModuleConfig {
    /// High-performance configuration for production environments
    pub fn high_performance() -> Self {
        Self {
            enable_comprehensive_monitoring: true,
            enable_real_time_dashboards: true,
            enable_intelligent_alerting: true,
            enable_distributed_tracing: true,
            enable_predictive_health: true,
            health_check_interval_seconds: 15,
            metrics_retention_days: 90,
            traces_retention_days: 30,
            alerts_retention_days: 365,
            enable_compression: true,
            compression_threshold_bytes: 5120, // 5KB
            enable_export_to_external: true,
            ..Default::default()
        }
    }

    /// High-reliability configuration with enhanced monitoring
    pub fn high_reliability() -> Self {
        Self {
            enable_comprehensive_monitoring: true,
            enable_real_time_dashboards: true,
            enable_intelligent_alerting: true,
            enable_distributed_tracing: true,
            enable_predictive_health: true,
            health_check_interval_seconds: 10,
            metrics_retention_days: 365,
            traces_retention_days: 90,
            alerts_retention_days: 730,
            enable_compression: true,
            compression_threshold_bytes: 2048, // 2KB
            enable_export_to_external: true,
            ..Default::default()
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.health_check_interval_seconds == 0 {
            return Err(ArbitrageError::configuration_error(
                "Health check interval must be greater than 0".to_string(),
            ));
        }

        if self.metrics_retention_days == 0 {
            return Err(ArbitrageError::configuration_error(
                "Metrics retention days must be greater than 0".to_string(),
            ));
        }

        if self.compression_threshold_bytes == 0 {
            return Err(ArbitrageError::configuration_error(
                "Compression threshold must be greater than 0".to_string(),
            ));
        }

        // Validate component configurations
        self.metrics_collector_config.validate()?;
        self.alert_manager_config.validate()?;
        self.trace_collector_config.validate()?;
        self.health_monitor_config.validate()?;
        self.observability_coordinator_config.validate()?;

        Ok(())
    }
}

/// Main monitoring module orchestrating all observability components
pub struct MonitoringModule {
    config: MonitoringModuleConfig,
    logger: crate::utils::logger::Logger,

    // Core components
    metrics_collector: Arc<MetricsCollector>,
    alert_manager: Arc<AlertManager>,
    trace_collector: Arc<TraceCollector>,
    health_monitor: Arc<HealthMonitor>,
    coordinator: Arc<ObservabilityCoordinator>,

    // Health and metrics
    health: Arc<std::sync::Mutex<MonitoringModuleHealth>>,
    metrics: Arc<std::sync::Mutex<MonitoringModuleMetrics>>,

    // Performance tracking
    startup_time: u64,
}

impl MonitoringModule {
    /// Create new monitoring module with comprehensive observability
    pub async fn new(
        config: MonitoringModuleConfig,
        kv_store: KvStore,
        env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let startup_start = chrono::Utc::now().timestamp_millis() as u64;

        // Validate configuration
        config.validate()?;

        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);
        logger.info("Initializing Monitoring Module with comprehensive observability");

        // Initialize core components
        let metrics_collector = Arc::new(
            MetricsCollector::new(
                config.metrics_collector_config.clone(),
                kv_store.clone(),
                env,
            )
            .await?,
        );
        let alert_manager = Arc::new(
            AlertManager::new(config.alert_manager_config.clone(), kv_store.clone(), env).await?,
        );
        let trace_collector = Arc::new(
            TraceCollector::new(config.trace_collector_config.clone(), kv_store.clone(), env)
                .await?,
        );
        let health_monitor = Arc::new(
            HealthMonitor::new(config.health_monitor_config.clone(), kv_store.clone(), env).await?,
        );
        let coordinator = Arc::new(
            ObservabilityCoordinator::new(
                config.observability_coordinator_config.clone(),
                kv_store.clone(),
                env,
                config.metrics_collector_config.clone(),
                config.alert_manager_config.clone(),
                config.trace_collector_config.clone(),
                config.health_monitor_config.clone(),
            )
            .await?,
        );

        let startup_time = chrono::Utc::now().timestamp_millis() as u64 - startup_start;

        logger.info(&format!(
            "Monitoring Module initialized successfully in {}ms",
            startup_time
        ));

        Ok(Self {
            config,
            logger,
            metrics_collector,
            alert_manager,
            trace_collector,
            health_monitor,
            coordinator,
            health: Arc::new(std::sync::Mutex::new(MonitoringModuleHealth::default())),
            metrics: Arc::new(std::sync::Mutex::new(MonitoringModuleMetrics::default())),
            startup_time,
        })
    }

    /// Collect observability data point
    pub async fn collect_data_point(
        &self,
        data_point: ObservabilityDataPoint,
    ) -> ArbitrageResult<()> {
        self.coordinator.process_data_point(data_point).await
    }

    /// Collect batch of observability data points
    pub async fn collect_batch(
        &self,
        data_points: Vec<ObservabilityDataPoint>,
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        self.coordinator.process_batch(data_points).await
    }

    /// Get monitoring module health
    pub async fn get_health(&self) -> MonitoringModuleHealth {
        let health = self.health.lock().unwrap().clone();
        health
    }

    /// Get monitoring module metrics
    pub async fn get_metrics(&self) -> MonitoringModuleMetrics {
        let metrics = self.metrics.lock().unwrap().clone();
        metrics
    }

    /// Perform comprehensive health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check all components
        let metrics_collector_healthy =
            self.metrics_collector.health_check().await.unwrap_or(false);
        let alert_manager_healthy = self.alert_manager.health_check().await.unwrap_or(false);
        let trace_collector_healthy = self.trace_collector.health_check().await.unwrap_or(false);
        let health_monitor_healthy = self.health_monitor.health_check().await.unwrap_or(false);
        let coordinator_healthy = self.coordinator.health_check().await.unwrap_or(false);

        // Calculate overall health score
        let healthy_components = [
            metrics_collector_healthy,
            alert_manager_healthy,
            trace_collector_healthy,
            health_monitor_healthy,
            coordinator_healthy,
        ]
        .iter()
        .filter(|&&h| h)
        .count();

        let total_components = 5;
        let health_score = (healthy_components as f32 / total_components as f32) * 100.0;
        let is_healthy = health_score >= 80.0; // 80% threshold

        // Update health status
        {
            let mut health = self.health.lock().unwrap();
            health.is_healthy = is_healthy;
            health.overall_score = health_score;
            health.metrics_collector_available = metrics_collector_healthy;
            health.alert_manager_available = alert_manager_healthy;
            health.trace_collector_available = trace_collector_healthy;
            health.health_monitor_available = health_monitor_healthy;
            health.observability_coordinator_available = coordinator_healthy;
            health.last_health_check = chrono::Utc::now().timestamp_millis() as u64;

            // Update component health map
            health
                .component_health
                .insert("metrics_collector".to_string(), metrics_collector_healthy);
            health
                .component_health
                .insert("alert_manager".to_string(), alert_manager_healthy);
            health
                .component_health
                .insert("trace_collector".to_string(), trace_collector_healthy);
            health
                .component_health
                .insert("health_monitor".to_string(), health_monitor_healthy);
            health
                .component_health
                .insert("observability_coordinator".to_string(), coordinator_healthy);
        }

        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        self.logger.info(&format!(
            "Health check completed in {}ms - Overall health: {:.1}%",
            end_time - start_time,
            health_score
        ));

        Ok(is_healthy)
    }

    /// Get comprehensive health summary
    pub async fn get_health_summary(&self) -> ArbitrageResult<serde_json::Value> {
        let health = self.get_health().await;
        let metrics = self.get_metrics().await;

        let summary = serde_json::json!({
            "module": "monitoring_module",
            "status": if health.is_healthy { "healthy" } else { "unhealthy" },
            "overall_score": health.overall_score,
            "components": {
                "metrics_collector": {
                    "healthy": health.metrics_collector_available,
                    "health": self.metrics_collector.get_health().await
                },
                "alert_manager": {
                    "healthy": health.alert_manager_available,
                    "health": self.alert_manager.get_health().await
                },
                "trace_collector": {
                    "healthy": health.trace_collector_available,
                    "health": self.trace_collector.get_health().await
                },
                "health_monitor": {
                    "healthy": health.health_monitor_available,
                    "health": self.health_monitor.get_overall_health_score().await
                },
                "observability_coordinator": {
                    "healthy": health.observability_coordinator_available,
                    "health": self.coordinator.get_health().await
                }
            },
            "performance": {
                "data_collection_rate_per_second": health.data_collection_rate_per_second,
                "alert_processing_rate_per_second": health.alert_processing_rate_per_second,
                "trace_processing_rate_per_second": health.trace_processing_rate_per_second,
                "error_rate_percent": health.error_rate_percent,
                "total_data_points_collected": metrics.total_data_points_collected,
                "total_alerts_generated": metrics.total_alerts_generated,
                "total_traces_collected": metrics.total_traces_collected
            },
            "configuration": {
                "comprehensive_monitoring_enabled": self.config.enable_comprehensive_monitoring,
                "real_time_dashboards_enabled": self.config.enable_real_time_dashboards,
                "intelligent_alerting_enabled": self.config.enable_intelligent_alerting,
                "distributed_tracing_enabled": self.config.enable_distributed_tracing,
                "predictive_health_enabled": self.config.enable_predictive_health,
                "health_check_interval_seconds": self.config.health_check_interval_seconds,
                "metrics_retention_days": self.config.metrics_retention_days,
                "compression_enabled": self.config.enable_compression
            },
            "startup_time_ms": self.startup_time,
            "last_health_check": health.last_health_check,
            "last_error": health.last_error
        });

        Ok(summary)
    }

    /// Get component references for advanced usage
    pub fn get_metrics_collector(&self) -> Arc<MetricsCollector> {
        self.metrics_collector.clone()
    }

    pub fn get_alert_manager(&self) -> Arc<AlertManager> {
        self.alert_manager.clone()
    }

    pub fn get_trace_collector(&self) -> Arc<TraceCollector> {
        self.trace_collector.clone()
    }

    pub fn get_health_monitor(&self) -> Arc<HealthMonitor> {
        self.health_monitor.clone()
    }

    pub fn get_coordinator(&self) -> Arc<ObservabilityCoordinator> {
        self.coordinator.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observability_event_type_defaults() {
        assert_eq!(
            ObservabilityEventType::SystemMetrics.as_str(),
            "system_metrics"
        );
        assert_eq!(
            ObservabilityEventType::SystemMetrics.default_collection_interval_seconds(),
            30
        );
        assert_eq!(
            ObservabilityEventType::SystemMetrics.default_retention_days(),
            30
        );
    }

    #[test]
    fn test_observability_data_point_creation() {
        let data_point = ObservabilityDataPoint::new(
            ObservabilityEventType::SystemMetrics,
            "test_source".to_string(),
            "test_component".to_string(),
            "cpu_usage".to_string(),
            75.5,
            "percent".to_string(),
        )
        .with_tag("environment".to_string(), "production".to_string())
        .with_severity(ObservabilitySeverity::Warning);

        assert_eq!(data_point.event_type, ObservabilityEventType::SystemMetrics);
        assert_eq!(data_point.metric_name, "cpu_usage");
        assert_eq!(data_point.metric_value, 75.5);
        assert_eq!(data_point.severity, ObservabilitySeverity::Warning);
        assert!(data_point.tags.contains_key("environment"));
    }

    #[test]
    fn test_observability_severity_ordering() {
        assert!(ObservabilitySeverity::Critical > ObservabilitySeverity::Error);
        assert!(ObservabilitySeverity::Error > ObservabilitySeverity::Warning);
        assert!(ObservabilitySeverity::Warning > ObservabilitySeverity::Info);
        assert!(ObservabilitySeverity::Info > ObservabilitySeverity::Debug);
    }

    #[test]
    fn test_monitoring_module_config_validation() {
        let mut config = MonitoringModuleConfig::default();
        assert!(config.validate().is_ok());

        config.health_check_interval_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = MonitoringModuleConfig::high_performance();
        assert_eq!(config.health_check_interval_seconds, 15);
        assert_eq!(config.metrics_retention_days, 90);
        assert!(config.enable_export_to_external);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = MonitoringModuleConfig::high_reliability();
        assert_eq!(config.health_check_interval_seconds, 10);
        assert_eq!(config.metrics_retention_days, 365);
        assert_eq!(config.alerts_retention_days, 730);
    }
}
