// Observability Coordinator - Unified Orchestration of All Monitoring Components
// Part of Monitoring Module replacing monitoring_observability.rs

use super::{
    AlertManager, AlertManagerConfig, HealthMonitor, HealthMonitorConfig, MetricsCollector,
    MetricsCollectorConfig, ObservabilityDataPoint, ObservabilityEventType, ObservabilitySeverity,
    TraceCollector, TraceCollectorConfig,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Observability health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityHealth {
    pub is_healthy: bool,
    pub overall_score: f32,
    pub component_health: HashMap<String, bool>,
    pub metrics_collector_healthy: bool,
    pub alert_manager_healthy: bool,
    pub trace_collector_healthy: bool,
    pub health_monitor_healthy: bool,
    pub data_collection_rate_per_second: f64,
    pub alert_processing_rate_per_second: f64,
    pub trace_processing_rate_per_second: f64,
    pub health_check_rate_per_second: f64,
    pub error_rate_percent: f32,
    pub storage_usage_percent: f32,
    pub kv_store_available: bool,
    pub last_health_check: u64,
    pub last_error: Option<String>,
}

impl Default for ObservabilityHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            overall_score: 0.0,
            component_health: HashMap::new(),
            metrics_collector_healthy: false,
            alert_manager_healthy: false,
            trace_collector_healthy: false,
            health_monitor_healthy: false,
            data_collection_rate_per_second: 0.0,
            alert_processing_rate_per_second: 0.0,
            trace_processing_rate_per_second: 0.0,
            health_check_rate_per_second: 0.0,
            error_rate_percent: 0.0,
            storage_usage_percent: 0.0,
            kv_store_available: false,
            last_health_check: 0,
            last_error: None,
        }
    }
}

/// Observability performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityMetrics {
    pub total_data_points_processed: u64,
    pub data_points_per_second: f64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub total_alerts_generated: u64,
    pub alerts_per_second: f64,
    pub total_traces_collected: u64,
    pub traces_per_second: f64,
    pub total_health_checks: u64,
    pub health_checks_per_second: f64,
    pub avg_processing_latency_ms: f64,
    pub min_processing_latency_ms: f64,
    pub max_processing_latency_ms: f64,
    pub data_points_by_type: HashMap<ObservabilityEventType, u64>,
    pub data_points_by_severity: HashMap<ObservabilitySeverity, u64>,
    pub storage_used_mb: f64,
    pub compression_ratio_percent: f32,
    pub uptime_seconds: u64,
    pub last_updated: u64,
}

impl Default for ObservabilityMetrics {
    fn default() -> Self {
        Self {
            total_data_points_processed: 0,
            data_points_per_second: 0.0,
            successful_operations: 0,
            failed_operations: 0,
            total_alerts_generated: 0,
            alerts_per_second: 0.0,
            total_traces_collected: 0,
            traces_per_second: 0.0,
            total_health_checks: 0,
            health_checks_per_second: 0.0,
            avg_processing_latency_ms: 0.0,
            min_processing_latency_ms: f64::MAX,
            max_processing_latency_ms: 0.0,
            data_points_by_type: HashMap::new(),
            data_points_by_severity: HashMap::new(),
            storage_used_mb: 0.0,
            compression_ratio_percent: 0.0,
            uptime_seconds: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Dashboard configuration for real-time observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub enable_real_time_dashboard: bool,
    pub refresh_interval_seconds: u64,
    pub max_data_points_displayed: usize,
    pub enable_charts: bool,
    pub enable_alerts_panel: bool,
    pub enable_health_panel: bool,
    pub enable_traces_panel: bool,
    pub enable_metrics_panel: bool,
    pub chart_types: Vec<ChartType>,
    pub time_ranges: Vec<TimeRange>,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enable_real_time_dashboard: true,
            refresh_interval_seconds: 30,
            max_data_points_displayed: 1000,
            enable_charts: true,
            enable_alerts_panel: true,
            enable_health_panel: true,
            enable_traces_panel: true,
            enable_metrics_panel: true,
            chart_types: vec![
                ChartType::LineChart,
                ChartType::BarChart,
                ChartType::PieChart,
                ChartType::Heatmap,
            ],
            time_ranges: vec![
                TimeRange::Last5Minutes,
                TimeRange::Last15Minutes,
                TimeRange::LastHour,
                TimeRange::Last6Hours,
                TimeRange::Last24Hours,
            ],
        }
    }
}

/// Chart types for dashboard visualization
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    LineChart,
    BarChart,
    PieChart,
    Heatmap,
    Gauge,
    Table,
}

/// Time ranges for dashboard data
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeRange {
    Last5Minutes,
    Last15Minutes,
    LastHour,
    Last6Hours,
    Last24Hours,
    Last7Days,
    Last30Days,
}

impl TimeRange {
    pub fn to_seconds(&self) -> u64 {
        match self {
            TimeRange::Last5Minutes => 300,
            TimeRange::Last15Minutes => 900,
            TimeRange::LastHour => 3600,
            TimeRange::Last6Hours => 21600,
            TimeRange::Last24Hours => 86400,
            TimeRange::Last7Days => 604800,
            TimeRange::Last30Days => 2592000,
        }
    }
}

/// Automated response configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomatedResponseConfig {
    pub enable_automated_responses: bool,
    pub enable_auto_scaling: bool,
    pub enable_circuit_breaker: bool,
    pub enable_failover: bool,
    pub response_timeout_seconds: u64,
    pub max_concurrent_responses: usize,
    pub response_cooldown_seconds: u64,
    pub escalation_thresholds: HashMap<String, f32>,
}

impl Default for AutomatedResponseConfig {
    fn default() -> Self {
        let mut escalation_thresholds = HashMap::new();
        escalation_thresholds.insert("error_rate".to_string(), 10.0);
        escalation_thresholds.insert("response_time".to_string(), 1000.0);
        escalation_thresholds.insert("health_score".to_string(), 0.7);

        Self {
            enable_automated_responses: false,
            enable_auto_scaling: false,
            enable_circuit_breaker: true,
            enable_failover: true,
            response_timeout_seconds: 30,
            max_concurrent_responses: 10,
            response_cooldown_seconds: 300,
            escalation_thresholds,
        }
    }
}

/// Configuration for ObservabilityCoordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityCoordinatorConfig {
    pub enable_coordination: bool,
    pub enable_real_time_processing: bool,
    pub enable_intelligent_routing: bool,
    pub enable_data_correlation: bool,
    pub coordination_interval_seconds: u64,
    pub max_concurrent_operations: usize,
    pub operation_timeout_seconds: u64,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
    pub dashboard_config: DashboardConfig,
    pub automated_response_config: AutomatedResponseConfig,
    pub enable_export: bool,
    pub export_formats: Vec<String>,
    pub export_endpoints: Vec<String>,
    pub enable_anomaly_detection: bool,
    pub anomaly_threshold: f32,
}

impl Default for ObservabilityCoordinatorConfig {
    fn default() -> Self {
        Self {
            enable_coordination: true,
            enable_real_time_processing: true,
            enable_intelligent_routing: true,
            enable_data_correlation: true,
            coordination_interval_seconds: 10,
            max_concurrent_operations: 100,
            operation_timeout_seconds: 30,
            enable_kv_storage: true,
            kv_key_prefix: "observability:".to_string(),
            enable_compression: true,
            compression_threshold_bytes: 1024,
            dashboard_config: DashboardConfig::default(),
            automated_response_config: AutomatedResponseConfig::default(),
            enable_export: false,
            export_formats: vec!["json".to_string(), "prometheus".to_string()],
            export_endpoints: Vec::new(),
            enable_anomaly_detection: true,
            anomaly_threshold: 2.0, // 2 standard deviations
        }
    }
}

impl ObservabilityCoordinatorConfig {
    pub fn high_performance() -> Self {
        Self {
            coordination_interval_seconds: 5,
            max_concurrent_operations: 200,
            operation_timeout_seconds: 15,
            dashboard_config: DashboardConfig {
                refresh_interval_seconds: 15,
                max_data_points_displayed: 500,
                ..Default::default()
            },
            automated_response_config: AutomatedResponseConfig {
                enable_automated_responses: true,
                enable_auto_scaling: true,
                response_timeout_seconds: 15,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            coordination_interval_seconds: 5,
            max_concurrent_operations: 50,
            operation_timeout_seconds: 60,
            dashboard_config: DashboardConfig {
                refresh_interval_seconds: 10,
                max_data_points_displayed: 2000,
                ..Default::default()
            },
            automated_response_config: AutomatedResponseConfig {
                enable_automated_responses: true,
                enable_circuit_breaker: true,
                enable_failover: true,
                response_cooldown_seconds: 180,
                ..Default::default()
            },
            enable_anomaly_detection: true,
            anomaly_threshold: 1.5,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.coordination_interval_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "coordination_interval_seconds must be greater than 0",
            ));
        }
        if self.max_concurrent_operations == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_operations must be greater than 0",
            ));
        }
        if self.operation_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "operation_timeout_seconds must be greater than 0",
            ));
        }
        if self.anomaly_threshold <= 0.0 {
            return Err(ArbitrageError::validation_error(
                "anomaly_threshold must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Observability Coordinator for unified monitoring orchestration
pub struct ObservabilityCoordinator {
    config: ObservabilityCoordinatorConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Component references
    metrics_collector: Arc<MetricsCollector>,
    alert_manager: Arc<AlertManager>,
    trace_collector: Arc<TraceCollector>,
    health_monitor: Arc<HealthMonitor>,

    // Coordination state
    health: Arc<Mutex<ObservabilityHealth>>,
    metrics: Arc<Mutex<ObservabilityMetrics>>,

    // Data correlation
    data_correlation_cache: Arc<Mutex<HashMap<String, Vec<ObservabilityDataPoint>>>>,

    // Performance tracking
    startup_time: u64,
}

impl ObservabilityCoordinator {
    /// Create new ObservabilityCoordinator instance
    pub async fn new(
        config: ObservabilityCoordinatorConfig,
        kv_store: KvStore,
        env: &worker::Env,
        metrics_collector_config: MetricsCollectorConfig,
        alert_manager_config: AlertManagerConfig,
        trace_collector_config: TraceCollectorConfig,
        health_monitor_config: HealthMonitorConfig,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize all monitoring components
        let metrics_collector =
            Arc::new(MetricsCollector::new(metrics_collector_config, kv_store.clone(), env).await?);

        let alert_manager =
            Arc::new(AlertManager::new(alert_manager_config, kv_store.clone(), env).await?);

        let trace_collector =
            Arc::new(TraceCollector::new(trace_collector_config, kv_store.clone(), env).await?);

        let health_monitor =
            Arc::new(HealthMonitor::new(health_monitor_config, kv_store.clone(), env).await?);

        logger.info(&format!(
            "ObservabilityCoordinator initialized: coordination={}, real_time={}, intelligent_routing={}",
            config.enable_coordination, config.enable_real_time_processing, config.enable_intelligent_routing
        ));

        Ok(Self {
            config,
            logger,
            kv_store,
            metrics_collector,
            alert_manager,
            trace_collector,
            health_monitor,
            health: Arc::new(Mutex::new(ObservabilityHealth::default())),
            metrics: Arc::new(Mutex::new(ObservabilityMetrics::default())),
            data_correlation_cache: Arc::new(Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Process observability data point through all relevant components
    pub async fn process_data_point(
        &self,
        data_point: ObservabilityDataPoint,
    ) -> ArbitrageResult<()> {
        if !self.config.enable_coordination {
            return Ok(());
        }

        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Route data point to appropriate components based on type
        let mut processing_results = Vec::new();

        // Route to metrics collector
        if matches!(
            data_point.event_type,
            ObservabilityEventType::SystemMetrics | ObservabilityEventType::PerformanceMetrics
        ) {
            processing_results.push((
                "metrics_collector",
                self.metrics_collector
                    .collect_metric(
                        data_point.component.clone(),
                        crate::services::core::infrastructure::monitoring_module::metrics_collector::MetricType::Counter,
                        data_point.component.clone(),
                        crate::services::core::infrastructure::monitoring_module::metrics_collector::MetricValue::new(data_point.metric_value),
                    )
                    .await,
            ));
        }

        // Send to alert manager if severity is high
        if matches!(
            data_point.severity,
            ObservabilitySeverity::High | ObservabilitySeverity::Critical
        ) {
            let alert_result = self
                .alert_manager
                .evaluate_metric(
                    &data_point.component,
                    &data_point.metric_name,
                    data_point.metric_value,
                )
                .await;
            processing_results.push(("alert_manager", alert_result.map(|_| ())));
        }

        // Store in correlation cache for analysis
        if self.config.enable_data_correlation {
            self.add_to_correlation_cache(data_point.clone()).await;
        }

        // Update processing metrics
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let processing_time = (end_time - start_time) as f64;
        self.update_processing_metrics(processing_time, &processing_results)
            .await;

        // Check for anomalies if enabled
        if self.config.enable_anomaly_detection {
            self.detect_anomalies(&data_point).await?;
        }

        Ok(())
    }

    /// Process batch of observability data points
    pub async fn process_batch(
        &self,
        data_points: Vec<ObservabilityDataPoint>,
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        if !self.config.enable_coordination {
            return Ok(vec![Ok(()); data_points.len()]);
        }

        let mut results = Vec::new();

        for data_point in data_points {
            let result = self.process_data_point(data_point).await;
            results.push(result);
        }

        Ok(results)
    }

    /// Get comprehensive observability health status
    pub async fn get_health(&self) -> ObservabilityHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            ObservabilityHealth::default()
        }
    }

    /// Get comprehensive observability metrics
    pub async fn get_metrics(&self) -> ObservabilityMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            ObservabilityMetrics::default()
        }
    }

    /// Perform comprehensive health check of all components
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check health of all components
        let metrics_health = match self.metrics_collector.health_check().await {
            Ok(ok) => ok,
            Err(e) => {
                self.logger
                    .error(&format!("Metrics health-check failed: {e}"));
                false
            }
        };
        let alert_health = self.alert_manager.health_check().await.unwrap_or(false);
        let trace_health = self.trace_collector.health_check().await.unwrap_or(false);
        let health_monitor_health = self.health_monitor.health_check().await.unwrap_or(false);

        // Test KV store connectivity
        let kv_health = self.test_kv_store().await.unwrap_or(false);

        // Calculate overall health
        let component_scores = [
            metrics_health as u8,
            alert_health as u8,
            trace_health as u8,
            health_monitor_health as u8,
            kv_health as u8,
        ];

        let total_score = component_scores.iter().sum::<u8>() as f32;
        let max_score = component_scores.len() as f32;
        let overall_score = total_score / max_score;
        let is_healthy = overall_score >= 0.8;

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = is_healthy;
            health.overall_score = overall_score;
            health.metrics_collector_healthy = metrics_health;
            health.alert_manager_healthy = alert_health;
            health.trace_collector_healthy = trace_health;
            health.health_monitor_healthy = health_monitor_health;
            health.kv_store_available = kv_health;
            health.last_health_check = start_time;
            health.last_error = None;
        }

        Ok(is_healthy)
    }

    /// Get real-time dashboard data
    pub async fn get_dashboard_data(
        &self,
        time_range: TimeRange,
    ) -> ArbitrageResult<serde_json::Value> {
        if !self.config.dashboard_config.enable_real_time_dashboard {
            return Ok(serde_json::json!({"error": "Dashboard disabled"}));
        }

        let mut dashboard_data = serde_json::Map::new();

        // Get health summary
        if self.config.dashboard_config.enable_health_panel {
            let health = self.get_health().await;
            dashboard_data.insert("health".to_string(), serde_json::to_value(health)?);
        }

        // Get metrics summary
        if self.config.dashboard_config.enable_metrics_panel {
            let metrics = self.get_metrics().await;
            dashboard_data.insert("metrics".to_string(), serde_json::to_value(metrics)?);
        }

        // Get alerts summary
        if self.config.dashboard_config.enable_alerts_panel {
            let alert_health = self.alert_manager.get_health().await;
            dashboard_data.insert("alerts".to_string(), serde_json::to_value(alert_health)?);
        }

        // Get traces summary
        if self.config.dashboard_config.enable_traces_panel {
            let trace_health = self.trace_collector.get_health().await;
            dashboard_data.insert("traces".to_string(), serde_json::to_value(trace_health)?);
        }

        // Add time range info
        dashboard_data.insert("time_range".to_string(), serde_json::to_value(time_range)?);
        dashboard_data.insert(
            "refresh_interval".to_string(),
            serde_json::to_value(self.config.dashboard_config.refresh_interval_seconds)?,
        );

        Ok(serde_json::Value::Object(dashboard_data))
    }

    /// Detect anomalies in observability data
    async fn detect_anomalies(&self, data_point: &ObservabilityDataPoint) -> ArbitrageResult<()> {
        if !self.config.enable_anomaly_detection {
            return Ok(());
        }

        // Simple anomaly detection based on metric value deviation
        // In a real implementation, this would use more sophisticated algorithms
        let metric_value = data_point.metric_value;

        // Get historical data for comparison
        let historical_data = {
            if let Ok(correlation_cache) = self.data_correlation_cache.lock() {
                correlation_cache.get(&data_point.metric_name).cloned()
            } else {
                None
            }
        };

        if let Some(historical_data) = historical_data {
            if historical_data.len() >= 10 {
                let values: Vec<f64> = historical_data.iter().map(|dp| dp.metric_value).collect();
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                let variance =
                    values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
                let std_dev = variance.sqrt();

                // Avoid divide-by-zero – if there is no variance, nothing can be an outlier.
                if std_dev == 0.0 {
                    return Ok(());
                }

                let z_score = (metric_value - mean) / std_dev;

                if z_score.abs() > self.config.anomaly_threshold as f64 {
                    self.logger.warn(&format!(
                        "Anomaly detected in {}: value={}, z_score={:.2}, threshold={}",
                        data_point.metric_name,
                        metric_value,
                        z_score,
                        self.config.anomaly_threshold
                    ));

                    // Could trigger automated response here
                    if self
                        .config
                        .automated_response_config
                        .enable_automated_responses
                    {
                        self.trigger_automated_response("anomaly_detected", data_point)
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Trigger automated response to observability events
    async fn trigger_automated_response(
        &self,
        response_type: &str,
        data_point: &ObservabilityDataPoint,
    ) -> ArbitrageResult<()> {
        if !self
            .config
            .automated_response_config
            .enable_automated_responses
        {
            return Ok(());
        }

        self.logger.info(&format!(
            "Triggering automated response: {} for metric: {}",
            response_type, data_point.metric_name
        ));

        // In a real implementation, this would trigger actual automated responses
        // such as scaling, circuit breaking, failover, etc.

        Ok(())
    }

    /// Add data point to correlation cache for analysis
    async fn add_to_correlation_cache(&self, data_point: ObservabilityDataPoint) {
        if let Ok(mut cache) = self.data_correlation_cache.lock() {
            let metric_name = data_point.metric_name.clone();
            let entry = cache.entry(metric_name).or_insert_with(Vec::new);

            entry.push(data_point);

            // Keep only recent data points (last 100)
            if entry.len() > 100 {
                entry.remove(0);
            }
        }
    }

    /// Update processing metrics
    async fn update_processing_metrics(
        &self,
        processing_time: f64,
        results: &[(&str, ArbitrageResult<()>)],
    ) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_data_points_processed += 1;

            let successful = results.iter().all(|(_, result)| result.is_ok());
            if successful {
                metrics.successful_operations += 1;
            } else {
                metrics.failed_operations += 1;
            }

            // Update latency metrics
            metrics.avg_processing_latency_ms = (metrics.avg_processing_latency_ms
                * (metrics.total_data_points_processed - 1) as f64
                + processing_time)
                / metrics.total_data_points_processed as f64;
            metrics.min_processing_latency_ms =
                metrics.min_processing_latency_ms.min(processing_time);
            metrics.max_processing_latency_ms =
                metrics.max_processing_latency_ms.max(processing_time);

            // Calculate rates
            let now = chrono::Utc::now().timestamp_millis() as u64;
            let uptime_seconds = (now - self.startup_time) / 1000;
            metrics.uptime_seconds = uptime_seconds;

            if uptime_seconds > 0 {
                metrics.data_points_per_second =
                    metrics.total_data_points_processed as f64 / uptime_seconds as f64;
            }

            metrics.last_updated = now;
        }
    }

    /// Test KV store connectivity
    async fn test_kv_store(&self) -> ArbitrageResult<bool> {
        let test_key = format!("{}health_check_test", self.config.kv_key_prefix);
        let test_value = "coordinator_test";

        // Try to write and read a test value
        match self
            .kv_store
            .put(&test_key, test_value)?
            .expiration_ttl(60)
            .execute()
            .await
        {
            Ok(_) => match self.kv_store.get(&test_key).text().await {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            },
            Err(_) => Ok(false),
        }
    }

    /// Get component references for direct access
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

    /// Export observability data in specified format
    pub async fn export_data(&self, format: &str) -> ArbitrageResult<String> {
        // Get correlation cache snapshot to avoid holding lock across await
        let correlation_snapshot = {
            if let Ok(correlation_cache) = self.data_correlation_cache.lock() {
                correlation_cache.clone()
            } else {
                HashMap::new()
            }
        };

        match format.to_lowercase().as_str() {
            "json" => {
                let export_data = serde_json::json!({
                    "timestamp": chrono::Utc::now().timestamp(),
                    "correlation_data": correlation_snapshot,
                    "export_format": "json"
                });

                let json_output = serde_json::to_string_pretty(&export_data).map_err(|e| {
                    ArbitrageError::serialization_error(format!("JSON export failed: {}", e))
                })?;

                // Store export in KV for caching
                // TODO: Implement store_export_in_kv method if needed
                // self.store_export_in_kv(format, &json_output).await?;

                Ok(json_output)
            }
            "prometheus" => {
                let mut prometheus_output = String::new();

                // Add observability metrics
                prometheus_output.push_str(
                    "# HELP observability_data_points_total Total data points processed\n",
                );
                prometheus_output.push_str("# TYPE observability_data_points_total counter\n");
                prometheus_output.push_str(&format!(
                    "observability_data_points_total {}\n",
                    correlation_snapshot.len()
                ));

                prometheus_output.push_str("# HELP observability_processing_latency_ms Processing latency in milliseconds\n");
                prometheus_output.push_str("# TYPE observability_processing_latency_ms gauge\n");
                prometheus_output
                    .push_str(&format!("observability_processing_latency_ms {}\n", 0.0));

                Ok(prometheus_output)
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Unsupported export format: {}",
                format
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_range_conversion() {
        assert_eq!(TimeRange::Last5Minutes.to_seconds(), 300);
        assert_eq!(TimeRange::LastHour.to_seconds(), 3600);
        assert_eq!(TimeRange::Last24Hours.to_seconds(), 86400);
    }

    #[test]
    fn test_dashboard_config_defaults() {
        let config = DashboardConfig::default();
        assert!(config.enable_real_time_dashboard);
        assert_eq!(config.refresh_interval_seconds, 30);
        assert!(config.enable_charts);
        assert!(config.chart_types.contains(&ChartType::LineChart));
    }

    #[test]
    fn test_automated_response_config_defaults() {
        let config = AutomatedResponseConfig::default();
        assert!(!config.enable_automated_responses);
        assert!(config.enable_circuit_breaker);
        assert!(config.enable_failover);
        assert!(config.escalation_thresholds.contains_key("error_rate"));
    }

    #[test]
    fn test_observability_coordinator_config_validation() {
        let mut config = ObservabilityCoordinatorConfig::default();
        assert!(config.validate().is_ok());

        config.coordination_interval_seconds = 0;
        assert!(config.validate().is_err());

        config.coordination_interval_seconds = 10;
        config.max_concurrent_operations = 0;
        assert!(config.validate().is_err());

        config.max_concurrent_operations = 100;
        config.anomaly_threshold = 0.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = ObservabilityCoordinatorConfig::high_performance();
        assert_eq!(config.coordination_interval_seconds, 5);
        assert_eq!(config.max_concurrent_operations, 200);
        assert!(config.automated_response_config.enable_automated_responses);
        assert!(config.automated_response_config.enable_auto_scaling);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = ObservabilityCoordinatorConfig::high_reliability();
        assert_eq!(config.coordination_interval_seconds, 5);
        assert_eq!(config.anomaly_threshold, 1.5);
        assert!(config.automated_response_config.enable_circuit_breaker);
        assert!(config.automated_response_config.enable_failover);
    }
}
