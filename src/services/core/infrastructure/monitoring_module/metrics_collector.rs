// MetricsCollector - Centralized metrics collection with multi-tier metrics and real-time processing
// Part of Monitoring Module replacing monitoring_observability.rs

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Metric types for comprehensive monitoring
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
    Timer,
    Rate,
    Percentage,
    Custom(String),
}

impl MetricType {
    pub fn as_str(&self) -> &str {
        match self {
            MetricType::Counter => "counter",
            MetricType::Gauge => "gauge",
            MetricType::Histogram => "histogram",
            MetricType::Summary => "summary",
            MetricType::Timer => "timer",
            MetricType::Rate => "rate",
            MetricType::Percentage => "percentage",
            MetricType::Custom(name) => name,
        }
    }

    pub fn supports_aggregation(&self) -> bool {
        matches!(
            self,
            MetricType::Counter | MetricType::Gauge | MetricType::Rate | MetricType::Percentage
        )
    }

    pub fn supports_percentiles(&self) -> bool {
        matches!(
            self,
            MetricType::Histogram | MetricType::Summary | MetricType::Timer
        )
    }
}

/// Metric value with statistical data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub value: f64,
    pub timestamp: u64,
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub rate_per_second: Option<f64>,
    pub tags: HashMap<String, String>,
}

impl MetricValue {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            count: 1,
            sum: value,
            min: value,
            max: value,
            avg: value,
            p50: None,
            p95: None,
            p99: None,
            rate_per_second: None,
            tags: HashMap::new(),
        }
    }

    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }

    pub fn with_percentiles(mut self, p50: f64, p95: f64, p99: f64) -> Self {
        self.p50 = Some(p50);
        self.p95 = Some(p95);
        self.p99 = Some(p99);
        self
    }

    pub fn with_rate(mut self, rate_per_second: f64) -> Self {
        self.rate_per_second = Some(rate_per_second);
        self
    }

    pub fn update(&mut self, new_value: f64) {
        self.count += 1;
        self.sum += new_value;
        self.min = self.min.min(new_value);
        self.max = self.max.max(new_value);
        self.avg = self.sum / self.count as f64;
        self.value = new_value;
        self.timestamp = chrono::Utc::now().timestamp_millis() as u64;
    }

    pub fn merge(&mut self, other: &MetricValue) {
        self.count += other.count;
        self.sum += other.sum;
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.avg = self.sum / self.count as f64;
        self.timestamp = self.timestamp.max(other.timestamp);
    }
}

/// Metrics data structure for organized storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsData {
    pub metric_name: String,
    pub metric_type: MetricType,
    pub component: String,
    pub values: Vec<MetricValue>,
    pub aggregated_value: Option<MetricValue>,
    pub retention_seconds: u64,
    pub last_updated: u64,
}

impl MetricsData {
    pub fn new(metric_name: String, metric_type: MetricType, component: String) -> Self {
        Self {
            metric_name,
            metric_type,
            component,
            values: Vec::new(),
            aggregated_value: None,
            retention_seconds: 3600, // 1 hour default
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn add_value(&mut self, value: MetricValue) {
        self.values.push(value);
        self.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        self.cleanup_old_values();
        self.update_aggregated_value();
    }

    pub fn cleanup_old_values(&mut self) {
        let cutoff_time =
            chrono::Utc::now().timestamp_millis() as u64 - (self.retention_seconds * 1000);
        self.values.retain(|v| v.timestamp >= cutoff_time);
    }

    pub fn update_aggregated_value(&mut self) {
        if self.values.is_empty() {
            self.aggregated_value = None;
            return;
        }

        let mut aggregated = MetricValue::new(0.0);
        aggregated.count = 0;
        aggregated.sum = 0.0;
        aggregated.min = f64::MAX;
        aggregated.max = f64::MIN;

        for value in &self.values {
            aggregated.count += value.count;
            aggregated.sum += value.sum;
            aggregated.min = aggregated.min.min(value.min);
            aggregated.max = aggregated.max.max(value.max);
            aggregated.timestamp = aggregated.timestamp.max(value.timestamp);
        }

        if aggregated.count > 0 {
            aggregated.avg = aggregated.sum / aggregated.count as f64;
            aggregated.value = self.values.last().unwrap().value;

            // Calculate percentiles for supported metric types
            if self.metric_type.supports_percentiles() {
                let mut sorted_values: Vec<f64> = self.values.iter().map(|v| v.value).collect();
                sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

                if !sorted_values.is_empty() {
                    let len = sorted_values.len();
                    aggregated.p50 = Some(sorted_values[len * 50 / 100]);
                    aggregated.p95 = Some(sorted_values[len * 95 / 100]);
                    aggregated.p99 = Some(sorted_values[len * 99 / 100]);
                }
            }

            // Calculate rate for rate-based metrics
            if matches!(self.metric_type, MetricType::Rate | MetricType::Counter) {
                let time_span_seconds =
                    (aggregated.timestamp - self.values.first().unwrap().timestamp) / 1000;
                if time_span_seconds > 0 {
                    aggregated.rate_per_second = Some(aggregated.sum / time_span_seconds as f64);
                }
            }
        }

        self.aggregated_value = Some(aggregated);
    }

    pub fn get_latest_value(&self) -> Option<&MetricValue> {
        self.values.last()
    }

    pub fn get_aggregated_value(&self) -> Option<&MetricValue> {
        self.aggregated_value.as_ref()
    }
}

/// Metrics collector health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsHealth {
    pub is_healthy: bool,
    pub collection_rate_per_second: f64,
    pub storage_usage_percent: f32,
    pub active_metrics_count: u64,
    pub failed_collections_count: u64,
    pub last_collection_timestamp: u64,
    pub kv_store_available: bool,
    pub compression_enabled: bool,
    pub export_enabled: bool,
    pub last_error: Option<String>,
}

impl Default for MetricsHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            collection_rate_per_second: 0.0,
            storage_usage_percent: 0.0,
            active_metrics_count: 0,
            failed_collections_count: 0,
            last_collection_timestamp: 0,
            kv_store_available: false,
            compression_enabled: false,
            export_enabled: false,
            last_error: None,
        }
    }
}

/// Metrics collector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsCollectorConfig {
    pub collection_interval_seconds: u64,
    pub batch_size: usize,
    pub max_metrics_in_memory: usize,
    pub default_retention_seconds: u64,
    pub enable_percentile_calculation: bool,
    pub enable_rate_calculation: bool,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_export: bool,
    pub export_format: String, // "prometheus", "grafana", "json"
    pub max_export_batch_size: usize,
    pub enable_real_time_processing: bool,
    pub aggregation_window_seconds: u64,
}

impl Default for MetricsCollectorConfig {
    fn default() -> Self {
        Self {
            collection_interval_seconds: 30,
            batch_size: 100,
            max_metrics_in_memory: 10000,
            default_retention_seconds: 3600, // 1 hour
            enable_percentile_calculation: true,
            enable_rate_calculation: true,
            enable_compression: true,
            compression_threshold_bytes: 5120, // 5KB
            enable_kv_storage: true,
            kv_key_prefix: "metrics:".to_string(),
            enable_export: true,
            export_format: "prometheus".to_string(),
            max_export_batch_size: 1000,
            enable_real_time_processing: true,
            aggregation_window_seconds: 300, // 5 minutes
        }
    }
}

impl MetricsCollectorConfig {
    pub fn high_performance() -> Self {
        Self {
            collection_interval_seconds: 10,
            batch_size: 200,
            max_metrics_in_memory: 50000,
            aggregation_window_seconds: 60,
            max_export_batch_size: 2000,
            ..Default::default()
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            collection_interval_seconds: 5,
            batch_size: 50,
            max_metrics_in_memory: 100000,
            default_retention_seconds: 86400, // 24 hours
            aggregation_window_seconds: 30,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.collection_interval_seconds == 0 {
            return Err(ArbitrageError::configuration_error(
                "Collection interval must be greater than 0".to_string(),
            ));
        }

        if self.batch_size == 0 {
            return Err(ArbitrageError::configuration_error(
                "Batch size must be greater than 0".to_string(),
            ));
        }

        if self.max_metrics_in_memory == 0 {
            return Err(ArbitrageError::configuration_error(
                "Max metrics in memory must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Centralized metrics collector with multi-tier metrics and real-time processing
pub struct MetricsCollector {
    config: MetricsCollectorConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Metrics storage
    metrics_data: Arc<Mutex<HashMap<String, MetricsData>>>,

    // Health and performance tracking
    health: Arc<Mutex<MetricsHealth>>,
    collection_count: Arc<Mutex<u64>>,
    last_collection_time: Arc<Mutex<u64>>,

    // Performance metrics
    startup_time: u64,
}

impl MetricsCollector {
    /// Create new metrics collector with comprehensive monitoring
    pub async fn new(
        config: MetricsCollectorConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let startup_start = chrono::Utc::now().timestamp_millis() as u64;

        // Validate configuration
        config.validate()?;

        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);
        logger.info("Initializing MetricsCollector with multi-tier metrics");

        let startup_time = chrono::Utc::now().timestamp_millis() as u64 - startup_start;

        logger.info(&format!(
            "MetricsCollector initialized successfully in {}ms",
            startup_time
        ));

        Ok(Self {
            config,
            logger,
            kv_store,
            metrics_data: Arc::new(Mutex::new(HashMap::new())),
            health: Arc::new(Mutex::new(MetricsHealth::default())),
            collection_count: Arc::new(Mutex::new(0)),
            last_collection_time: Arc::new(Mutex::new(0)),
            startup_time,
        })
    }

    /// Collect a single metric value
    pub async fn collect_metric(
        &self,
        metric_name: String,
        metric_type: MetricType,
        component: String,
        value: MetricValue,
    ) -> ArbitrageResult<()> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Create metric key
        let metric_key = format!("{}:{}:{}", component, metric_type.as_str(), metric_name);

        // Update metrics data
        {
            let mut metrics = match self.metrics_data.lock() {
                Ok(g) => g,
                Err(e) => {
                    self.logger.error(&format!("Metrics mutex poisoned: {}", e));
                    let mut health = self.health.lock().ok();
                    if let Some(h) = &mut health {
                        h.failed_collections_count += 1;
                        h.last_error = Some("metrics mutex poisoned".into());
                    }
                    return Err(ArbitrageError::internal_error("metrics mutex poisoned"));
                }
            };

            // Get or create metrics data
            let metrics_data = metrics.entry(metric_key.clone()).or_insert_with(|| {
                MetricsData::new(metric_name.clone(), metric_type.clone(), component.clone())
            });

            // Add new value
            metrics_data.add_value(value);

            // Check memory limits
            if metrics.len() > self.config.max_metrics_in_memory {
                self.logger.warn(&format!(
                    "Metrics in memory ({}) exceeds limit ({})",
                    metrics.len(),
                    self.config.max_metrics_in_memory
                ));

                // Remove oldest metrics
                let mut oldest_key = String::new();
                let mut oldest_time = u64::MAX;

                for (key, data) in metrics.iter() {
                    if data.last_updated < oldest_time {
                        oldest_time = data.last_updated;
                        oldest_key = key.clone();
                    }
                }

                if !oldest_key.is_empty() {
                    metrics.remove(&oldest_key);
                    self.logger
                        .info(&format!("Removed oldest metric: {}", oldest_key));
                }
            }
        }

        // Store in KV if enabled
        if self.config.enable_kv_storage {
            if let Err(e) = self.store_metric_in_kv(&metric_key).await {
                self.logger
                    .error(&format!("Failed to store metric in KV: {}", e));

                // Update health status
                {
                    let mut health = self.health.lock().unwrap();
                    health.failed_collections_count += 1;
                    health.last_error = Some(format!("KV storage error: {}", e));
                }
            }
        }

        // Update collection statistics
        {
            let mut count = self.collection_count.lock().unwrap();
            *count += 1;

            let mut last_time = self.last_collection_time.lock().unwrap();
            *last_time = chrono::Utc::now().timestamp_millis() as u64;
        }

        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        if end_time - start_time > 100 {
            self.logger.warn(&format!(
                "Slow metric collection: {}ms for {}",
                end_time - start_time,
                metric_key
            ));
        }

        Ok(())
    }

    /// Collect batch of metrics
    pub async fn collect_batch(
        &self,
        metrics: Vec<(String, MetricType, String, MetricValue)>,
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        let mut results = Vec::new();

        for (metric_name, metric_type, component, value) in metrics {
            let result = self
                .collect_metric(metric_name, metric_type, component, value)
                .await;
            results.push(result);
        }

        Ok(results)
    }

    /// Get metric data by key
    pub async fn get_metric(&self, metric_key: &str) -> Option<MetricsData> {
        let metrics = self.metrics_data.lock().unwrap();
        metrics.get(metric_key).cloned()
    }

    /// Get all metrics for a component
    pub async fn get_component_metrics(&self, component: &str) -> HashMap<String, MetricsData> {
        let metrics = self.metrics_data.lock().unwrap();
        metrics
            .iter()
            .filter(|(_, data)| data.component == component)
            .map(|(key, data)| (key.clone(), data.clone()))
            .collect()
    }

    /// Get aggregated metrics summary
    pub async fn get_aggregated_summary(&self) -> HashMap<String, MetricValue> {
        let metrics = self.metrics_data.lock().unwrap();
        metrics
            .iter()
            .filter_map(|(key, data)| {
                data.get_aggregated_value()
                    .map(|value| (key.clone(), value.clone()))
            })
            .collect()
    }

    /// Export metrics in specified format
    pub async fn export_metrics(&self, format: &str) -> ArbitrageResult<String> {
        // Get metrics snapshot to avoid holding lock across await
        let metrics_snapshot = {
            let metrics = self.metrics_data.lock().unwrap();
            metrics.clone()
        };

        match format.to_lowercase().as_str() {
            "json" => {
                let json_output = serde_json::to_string_pretty(&metrics_snapshot).map_err(|e| {
                    ArbitrageError::serialization_error(format!("JSON export failed: {}", e))
                })?;
                Ok(json_output)
            }
            "prometheus" => {
                let mut prometheus_output = String::new();

                // Add basic metrics
                prometheus_output
                    .push_str("# HELP metrics_total_collected Total metrics collected\n");
                prometheus_output.push_str("# TYPE metrics_total_collected counter\n");
                prometheus_output.push_str(&format!(
                    "metrics_total_collected {}\n",
                    metrics_snapshot.len()
                ));

                // Store in KV for caching
                let metrics_data = {
                    let metrics = self.metrics_data.lock().unwrap();
                    metrics.clone()
                };

                let serialized = serde_json::to_string(&metrics_data)
                    .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

                self.kv_store
                    .put("metrics_export", serialized)?
                    .expiration_ttl(3600) // 1 hour
                    .execute()
                    .await?;

                Ok(prometheus_output)
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Unsupported export format: {}",
                format
            ))),
        }
    }

    /// Store metric in KV store
    async fn store_metric_in_kv(&self, metric_key: &str) -> ArbitrageResult<()> {
        let metric_data = {
            let metrics = self.metrics_data.lock().unwrap();
            metrics.get(metric_key).cloned()
        };

        if let Some(metric_data) = metric_data {
            let kv_key = format!("{}{}", self.config.kv_key_prefix, metric_key);
            let serialized = serde_json::to_string(&metric_data)
                .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

            // Compress if enabled and above threshold
            let data_to_store = if self.config.enable_compression
                && serialized.len() > self.config.compression_threshold_bytes
            {
                // In a real implementation, you would use a compression library
                // For now, we'll just store the original data
                serialized
            } else {
                serialized
            };

            self.kv_store
                .put(&kv_key, data_to_store)?
                .execute()
                .await
                .map_err(|e| {
                    ArbitrageError::storage_error(format!("KV execute failed: {:?}", e))
                })?;
        }

        Ok(())
    }

    /// Export metrics in Prometheus format
    fn export_prometheus_format(
        &self,
        metrics: &HashMap<String, MetricsData>,
    ) -> ArbitrageResult<String> {
        let mut output = String::new();

        for data in metrics.values() {
            if let Some(value) = data.get_aggregated_value() {
                // Add metric help and type
                output.push_str(&format!(
                    "# HELP {} {}\n",
                    data.metric_name, data.metric_name
                ));
                output.push_str(&format!(
                    "# TYPE {} {}\n",
                    data.metric_name,
                    data.metric_type.as_str()
                ));

                // Add metric value with labels
                let mut labels = vec![format!("component=\"{}\"", data.component)];
                for (tag_key, tag_value) in &value.tags {
                    labels.push(format!("{}=\"{}\"", tag_key, tag_value));
                }

                output.push_str(&format!(
                    "{}{{{}}} {} {}\n",
                    data.metric_name,
                    labels.join(","),
                    value.value,
                    value.timestamp
                ));

                // Add percentiles if available
                if let Some(p50) = value.p50 {
                    output.push_str(&format!(
                        "{}_p50{{{}}} {} {}\n",
                        data.metric_name,
                        labels.join(","),
                        p50,
                        value.timestamp
                    ));
                }
                if let Some(p95) = value.p95 {
                    output.push_str(&format!(
                        "{}_p95{{{}}} {} {}\n",
                        data.metric_name,
                        labels.join(","),
                        p95,
                        value.timestamp
                    ));
                }
                if let Some(p99) = value.p99 {
                    output.push_str(&format!(
                        "{}_p99{{{}}} {} {}\n",
                        data.metric_name,
                        labels.join(","),
                        p99,
                        value.timestamp
                    ));
                }
            }
        }

        Ok(output)
    }

    /// Export metrics in Grafana format
    fn export_grafana_format(
        &self,
        metrics: &HashMap<String, MetricsData>,
    ) -> ArbitrageResult<String> {
        let mut grafana_metrics = Vec::new();

        for (key, data) in metrics {
            if let Some(value) = data.get_aggregated_value() {
                let metric = serde_json::json!({
                    "target": key,
                    "datapoints": [[value.value, value.timestamp]],
                    "tags": value.tags,
                    "meta": {
                        "component": data.component,
                        "type": data.metric_type.as_str(),
                        "count": value.count,
                        "min": value.min,
                        "max": value.max,
                        "avg": value.avg,
                        "p50": value.p50,
                        "p95": value.p95,
                        "p99": value.p99
                    }
                });
                grafana_metrics.push(metric);
            }
        }

        serde_json::to_string_pretty(&grafana_metrics)
            .map_err(|e| ArbitrageError::serialization_error(e.to_string()))
    }

    /// Export metrics in JSON format
    fn export_json_format(
        &self,
        metrics: &HashMap<String, MetricsData>,
    ) -> ArbitrageResult<String> {
        serde_json::to_string_pretty(metrics)
            .map_err(|e| ArbitrageError::serialization_error(e.to_string()))
    }

    /// Get metrics collector health
    pub async fn get_health(&self) -> MetricsHealth {
        let mut health = self.health.lock().unwrap();

        // Update health metrics
        let metrics_count = self.metrics_data.lock().unwrap().len() as u64;
        let collection_count = *self.collection_count.lock().unwrap();
        let last_collection = *self.last_collection_time.lock().unwrap();

        // Calculate collection rate
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let time_since_startup = (now - self.startup_time) / 1000; // seconds
        let collection_rate = if time_since_startup > 0 {
            collection_count as f64 / time_since_startup as f64
        } else {
            0.0
        };

        // Calculate storage usage
        let storage_usage =
            (metrics_count as f32 / self.config.max_metrics_in_memory as f32) * 100.0;

        // Determine overall health
        let is_healthy = storage_usage < 90.0
            && (now - last_collection) < (self.config.collection_interval_seconds * 2 * 1000);

        health.is_healthy = is_healthy;
        health.collection_rate_per_second = collection_rate;
        health.storage_usage_percent = storage_usage;
        health.active_metrics_count = metrics_count;
        health.last_collection_timestamp = last_collection;
        health.kv_store_available = self.config.enable_kv_storage;
        health.compression_enabled = self.config.enable_compression;
        health.export_enabled = self.config.enable_export;

        health.clone()
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let health = self.get_health().await;
        Ok(health.is_healthy)
    }

    /// Cleanup old metrics
    pub async fn cleanup_old_metrics(&self) -> ArbitrageResult<u64> {
        let mut metrics = self.metrics_data.lock().unwrap();
        let mut cleaned_count = 0;

        let cutoff_time = chrono::Utc::now().timestamp_millis() as u64
            - (self.config.default_retention_seconds * 1000);

        metrics.retain(|_, data| {
            if data.last_updated < cutoff_time {
                cleaned_count += 1;
                false
            } else {
                true
            }
        });

        if cleaned_count > 0 {
            self.logger
                .info(&format!("Cleaned up {} old metrics", cleaned_count));
        }

        Ok(cleaned_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_type_properties() {
        assert!(MetricType::Counter.supports_aggregation());
        assert!(MetricType::Histogram.supports_percentiles());
        assert!(!MetricType::Gauge.supports_percentiles());
    }

    #[test]
    fn test_metric_value_creation() {
        let value = MetricValue::new(42.0)
            .with_tag("env".to_string(), "prod".to_string())
            .with_percentiles(40.0, 50.0, 55.0);

        assert_eq!(value.value, 42.0);
        assert_eq!(value.count, 1);
        assert!(value.tags.contains_key("env"));
        assert_eq!(value.p50, Some(40.0));
    }

    #[test]
    fn test_metric_value_update() {
        let mut value = MetricValue::new(10.0);
        value.update(20.0);

        assert_eq!(value.count, 2);
        assert_eq!(value.sum, 30.0);
        assert_eq!(value.avg, 15.0);
        assert_eq!(value.min, 10.0);
        assert_eq!(value.max, 20.0);
    }

    #[test]
    fn test_metrics_data_aggregation() {
        let mut data = MetricsData::new(
            "test_metric".to_string(),
            MetricType::Gauge,
            "test_component".to_string(),
        );

        data.add_value(MetricValue::new(10.0));
        data.add_value(MetricValue::new(20.0));
        data.add_value(MetricValue::new(30.0));

        let aggregated = data.get_aggregated_value().unwrap();
        assert_eq!(aggregated.count, 3);
        assert_eq!(aggregated.avg, 20.0);
        assert_eq!(aggregated.min, 10.0);
        assert_eq!(aggregated.max, 30.0);
    }

    #[test]
    fn test_metrics_collector_config_validation() {
        let mut config = MetricsCollectorConfig::default();
        assert!(config.validate().is_ok());

        config.collection_interval_seconds = 0;
        assert!(config.validate().is_err());
    }
}
