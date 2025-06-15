use crate::services::core::infrastructure::persistence_layer::connection_pool::ConnectionManager;
use crate::services::core::infrastructure::persistence_layer::transaction_coordinator::TransactionCoordinator;
use crate::utils::now_system_time;
use crate::utils::ArbitrageResult;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use worker::Env;

/// OpenTelemetry-compliant metric types following semantic conventions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtelMetricType {
    /// Monotonically increasing counter
    Counter,
    /// Gauge for arbitrary values that can go up and down
    Gauge,
    /// Histogram for latency/size measurements with bucketed distribution
    Histogram,
    /// Summary for percentile calculations
    Summary,
    /// Timer for duration measurements (specialized histogram)
    Timer,
    /// Rate metrics (requests per second, etc.)
    Rate,
    /// Percentage metrics (CPU utilization, etc.)
    Percentage,
    /// Custom business metrics
    Custom(String),
}

impl OtelMetricType {
    /// Get OpenTelemetry semantic convention name
    pub fn otel_name(&self) -> &str {
        match self {
            OtelMetricType::Counter => "counter",
            OtelMetricType::Gauge => "gauge",
            OtelMetricType::Histogram => "histogram",
            OtelMetricType::Summary => "summary",
            OtelMetricType::Timer => "histogram", // OTel uses histogram for durations
            OtelMetricType::Rate => "gauge",      // Rates are typically gauges in OTel
            OtelMetricType::Percentage => "gauge",
            OtelMetricType::Custom(name) => name,
        }
    }

    /// Check if metric supports aggregation
    pub fn supports_aggregation(&self) -> bool {
        matches!(
            self,
            OtelMetricType::Counter
                | OtelMetricType::Gauge
                | OtelMetricType::Rate
                | OtelMetricType::Percentage
        )
    }

    /// Check if metric supports percentile calculations
    pub fn supports_percentiles(&self) -> bool {
        matches!(
            self,
            OtelMetricType::Histogram | OtelMetricType::Summary | OtelMetricType::Timer
        )
    }
}

/// RED (Rate, Errors, Duration) metrics following SRE best practices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedMetrics {
    /// Request rate (requests per second)
    pub rate: f64,
    /// Error rate (percentage of failed requests)
    pub error_rate: f64,
    /// Duration metrics (response time statistics)
    pub duration: DurationMetrics,
    /// Additional SLI metrics
    pub availability: f64,
    pub throughput: f64,
    pub saturation: f64,
}

/// Duration metrics with percentile distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurationMetrics {
    pub avg: f64,
    pub min: f64,
    pub max: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p99_9: f64,
    pub stddev: f64,
}

impl Default for DurationMetrics {
    fn default() -> Self {
        Self {
            avg: 0.0,
            min: 0.0,
            max: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            p99_9: 0.0,
            stddev: 0.0,
        }
    }
}

/// Business metrics for domain-specific monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessMetrics {
    /// Arbitrage opportunities detected
    pub opportunities_detected: u64,
    /// Successful trades executed
    pub trades_executed: u64,
    /// Trade success rate
    pub trade_success_rate: f64,
    /// Profit generated
    pub profit_generated: f64,
    /// User engagement metrics
    pub active_users: u64,
    /// Market data freshness
    pub data_freshness_seconds: f64,
}

impl Default for BusinessMetrics {
    fn default() -> Self {
        Self {
            opportunities_detected: 0,
            trades_executed: 0,
            trade_success_rate: 0.0,
            profit_generated: 0.0,
            active_users: 0,
            data_freshness_seconds: 0.0,
        }
    }
}

/// Infrastructure metrics following OpenTelemetry semantic conventions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfrastructureMetrics {
    /// CPU utilization percentage
    pub cpu_utilization: f64,
    /// Memory usage metrics
    pub memory_usage: MemoryMetrics,
    /// Network I/O metrics
    pub network_io: NetworkMetrics,
    /// Storage metrics
    pub storage_metrics: StorageMetrics,
    /// Container metrics (if applicable)
    pub container_metrics: Option<ContainerMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub utilization_percent: f64,
    pub gc_metrics: Option<GcMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcMetrics {
    pub gc_count: u64,
    pub gc_duration_ms: f64,
    pub heap_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub connection_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetrics {
    pub reads_per_second: f64,
    pub writes_per_second: f64,
    pub read_latency_ms: f64,
    pub write_latency_ms: f64,
    pub storage_used_bytes: u64,
    pub storage_available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetrics {
    pub cpu_limit: f64,
    pub memory_limit_bytes: u64,
    pub restart_count: u64,
    pub uptime_seconds: u64,
}

/// Cost tracking metrics for FinOps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostMetrics {
    /// Cloudflare Workers cost
    pub workers_cost_usd: f64,
    /// D1 database cost
    pub d1_cost_usd: f64,
    /// R2 storage cost
    pub r2_cost_usd: f64,
    /// KV storage cost
    pub kv_cost_usd: f64,
    /// Total infrastructure cost
    pub total_cost_usd: f64,
    /// Cost per request
    pub cost_per_request: f64,
    /// Cost per user
    pub cost_per_user: f64,
    /// Monthly burn rate projection
    pub monthly_burn_rate: f64,
}

impl Default for CostMetrics {
    fn default() -> Self {
        Self {
            workers_cost_usd: 0.0,
            d1_cost_usd: 0.0,
            r2_cost_usd: 0.0,
            kv_cost_usd: 0.0,
            total_cost_usd: 0.0,
            cost_per_request: 0.0,
            cost_per_user: 0.0,
            monthly_burn_rate: 0.0,
        }
    }
}

/// Enhanced metric value with OpenTelemetry attributes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedMetricValue {
    pub value: f64,
    pub timestamp: u64,
    pub attributes: HashMap<String, String>,
    pub resource_attributes: HashMap<String, String>,
    pub exemplars: Vec<Exemplar>,
    pub unit: String,
    pub description: String,
}

/// OpenTelemetry exemplar for trace correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exemplar {
    pub value: f64,
    pub timestamp: u64,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub filtered_attributes: HashMap<String, String>,
}

/// Configuration for the metrics collection engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsCollectionConfig {
    /// Enable OpenTelemetry compliance
    pub otel_compliance_enabled: bool,
    /// Collection interval for different metric types
    pub collection_intervals: HashMap<String, Duration>,
    /// Enable RED metrics collection
    pub red_metrics_enabled: bool,
    /// Enable business metrics collection
    pub business_metrics_enabled: bool,
    /// Enable infrastructure metrics collection
    pub infrastructure_metrics_enabled: bool,
    /// Enable cost tracking
    pub cost_tracking_enabled: bool,
    /// Maximum number of metric series
    pub max_metric_series: usize,
    /// Retention period for metrics
    pub retention_period: Duration,
    /// Export configuration
    pub export_config: ExportConfig,
    /// Sampling configuration
    pub sampling_config: SamplingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// Export to Prometheus format
    pub prometheus_enabled: bool,
    /// Export to OTLP format
    pub otlp_enabled: bool,
    /// Export to InfluxDB format
    pub influxdb_enabled: bool,
    /// Export interval
    pub export_interval: Duration,
    /// Batch size for exports
    pub batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// Sample rate for high-frequency metrics
    pub high_frequency_sample_rate: f64,
    /// Sample rate for medium-frequency metrics
    pub medium_frequency_sample_rate: f64,
    /// Sample rate for low-frequency metrics
    pub low_frequency_sample_rate: f64,
    /// Adaptive sampling enabled
    pub adaptive_sampling: bool,
}

impl Default for MetricsCollectionConfig {
    fn default() -> Self {
        let mut collection_intervals = HashMap::new();
        collection_intervals.insert("red".to_string(), Duration::from_secs(15));
        collection_intervals.insert("business".to_string(), Duration::from_secs(60));
        collection_intervals.insert("infrastructure".to_string(), Duration::from_secs(30));
        collection_intervals.insert("cost".to_string(), Duration::from_secs(300));

        Self {
            otel_compliance_enabled: true,
            collection_intervals,
            red_metrics_enabled: true,
            business_metrics_enabled: true,
            infrastructure_metrics_enabled: true,
            cost_tracking_enabled: true,
            max_metric_series: 10000,
            retention_period: Duration::from_secs(86400 * 7), // 7 days
            export_config: ExportConfig {
                prometheus_enabled: true,
                otlp_enabled: true,
                influxdb_enabled: false,
                export_interval: Duration::from_secs(60),
                batch_size: 1000,
            },
            sampling_config: SamplingConfig {
                high_frequency_sample_rate: 0.1,
                medium_frequency_sample_rate: 0.5,
                low_frequency_sample_rate: 1.0,
                adaptive_sampling: true,
            },
        }
    }
}

/// Health status of the metrics collection engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsCollectionHealth {
    pub is_healthy: bool,
    pub collection_rate_per_second: f64,
    pub export_rate_per_second: f64,
    pub error_rate_percent: f32,
    pub memory_usage_mb: f64,
    pub active_metric_series: u64,
    pub dropped_metrics_count: u64,
    pub last_collection_timestamp: u64,
    pub last_export_timestamp: u64,
    pub otel_compliance_status: String,
    pub last_error: Option<String>,
}

impl Default for MetricsCollectionHealth {
    fn default() -> Self {
        Self {
            is_healthy: true,
            collection_rate_per_second: 0.0,
            export_rate_per_second: 0.0,
            error_rate_percent: 0.0,
            memory_usage_mb: 0.0,
            active_metric_series: 0,
            dropped_metrics_count: 0,
            last_collection_timestamp: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            last_export_timestamp: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            otel_compliance_status: "compliant".to_string(),
            last_error: None,
        }
    }
}

/// Main metrics collection engine with OpenTelemetry compliance
pub struct MetricsCollectionEngine {
    config: MetricsCollectionConfig,
    #[allow(dead_code)]
    connection_manager: Arc<ConnectionManager>,
    #[allow(dead_code)]
    transaction_coordinator: Arc<TransactionCoordinator>,

    // Metrics storage
    red_metrics: Arc<RwLock<RedMetrics>>,
    business_metrics: Arc<RwLock<BusinessMetrics>>,
    infrastructure_metrics: Arc<RwLock<InfrastructureMetrics>>,
    cost_metrics: Arc<RwLock<CostMetrics>>,

    // Metric series registry
    #[allow(dead_code)]
    metric_series: Arc<RwLock<HashMap<String, EnhancedMetricValue>>>,

    // Performance tracking
    health: Arc<RwLock<MetricsCollectionHealth>>,
    #[allow(dead_code)]
    collection_count: Arc<RwLock<u64>>,
    #[allow(dead_code)]
    export_count: Arc<RwLock<u64>>,

    // OpenTelemetry compliance
    #[allow(dead_code)]
    otel_resource_attributes: HashMap<String, String>,
    #[allow(dead_code)]
    otel_semantic_conventions: HashSet<String>,

    #[allow(dead_code)]
    startup_time: u64,
}

// Implement Send + Sync for thread safety
unsafe impl Send for MetricsCollectionEngine {}
unsafe impl Sync for MetricsCollectionEngine {}

impl MetricsCollectionEngine {
    /// Create new metrics collection engine
    pub async fn new(
        config: MetricsCollectionConfig,
        connection_manager: Arc<ConnectionManager>,
        transaction_coordinator: Arc<TransactionCoordinator>,
        _env: &Env,
    ) -> ArbitrageResult<Self> {
        let startup_time = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Initialize OpenTelemetry resource attributes
        let mut otel_resource_attributes = HashMap::new();
        otel_resource_attributes.insert("service.name".to_string(), "arb-edge".to_string());
        otel_resource_attributes.insert("service.version".to_string(), "1.0.0".to_string());
        otel_resource_attributes.insert(
            "deployment.environment".to_string(),
            "production".to_string(),
        );

        // Initialize semantic conventions
        let mut otel_semantic_conventions = HashSet::new();
        otel_semantic_conventions.insert("http.request.duration".to_string());
        otel_semantic_conventions.insert("http.request.body.size".to_string());
        otel_semantic_conventions.insert("http.response.body.size".to_string());
        otel_semantic_conventions.insert("db.operation.duration".to_string());
        otel_semantic_conventions.insert("system.cpu.utilization".to_string());
        otel_semantic_conventions.insert("system.memory.utilization".to_string());

        Ok(Self {
            config,
            connection_manager,
            transaction_coordinator,
            red_metrics: Arc::new(RwLock::new(RedMetrics {
                rate: 0.0,
                error_rate: 0.0,
                duration: DurationMetrics::default(),
                availability: 100.0,
                throughput: 0.0,
                saturation: 0.0,
            })),
            business_metrics: Arc::new(RwLock::new(BusinessMetrics::default())),
            infrastructure_metrics: Arc::new(RwLock::new(InfrastructureMetrics {
                cpu_utilization: 0.0,
                memory_usage: MemoryMetrics {
                    used_bytes: 0,
                    available_bytes: 0,
                    utilization_percent: 0.0,
                    gc_metrics: None,
                },
                network_io: NetworkMetrics {
                    bytes_sent: 0,
                    bytes_received: 0,
                    packets_sent: 0,
                    packets_received: 0,
                    connection_count: 0,
                },
                storage_metrics: StorageMetrics {
                    reads_per_second: 0.0,
                    writes_per_second: 0.0,
                    read_latency_ms: 0.0,
                    write_latency_ms: 0.0,
                    storage_used_bytes: 0,
                    storage_available_bytes: 0,
                },
                container_metrics: None,
            })),
            cost_metrics: Arc::new(RwLock::new(CostMetrics::default())),
            metric_series: Arc::new(RwLock::new(HashMap::new())),
            health: Arc::new(RwLock::new(MetricsCollectionHealth::default())),
            collection_count: Arc::new(RwLock::new(0)),
            export_count: Arc::new(RwLock::new(0)),
            otel_resource_attributes,
            otel_semantic_conventions,
            startup_time,
        })
    }

    /// Collect RED metrics for HTTP requests
    pub async fn collect_red_metrics(
        &self,
        request_count: u64,
        error_count: u64,
        duration_samples: Vec<f64>,
    ) -> ArbitrageResult<()> {
        if !self.config.red_metrics_enabled {
            return Ok(());
        }

        let mut red_metrics = self.red_metrics.write();

        // Calculate rate (requests per second)
        red_metrics.rate = request_count as f64;

        // Calculate error rate
        red_metrics.error_rate = if request_count > 0 {
            (error_count as f64 / request_count as f64) * 100.0
        } else {
            0.0
        };

        // Calculate duration metrics
        if !duration_samples.is_empty() {
            red_metrics.duration = self.calculate_duration_metrics(&duration_samples);
        }

        // Calculate availability (SLI)
        red_metrics.availability = 100.0 - red_metrics.error_rate;

        // Update collection count
        let mut count = self.collection_count.write();
        *count += 1;

        Ok(())
    }

    /// Collect business metrics
    pub async fn collect_business_metrics(
        &self,
        opportunities: u64,
        trades: u64,
        profit: f64,
        users: u64,
        data_age: f64,
    ) -> ArbitrageResult<()> {
        if !self.config.business_metrics_enabled {
            return Ok(());
        }

        let mut business_metrics = self.business_metrics.write();

        business_metrics.opportunities_detected = opportunities;
        business_metrics.trades_executed = trades;
        business_metrics.trade_success_rate = if opportunities > 0 {
            (trades as f64 / opportunities as f64) * 100.0
        } else {
            0.0
        };
        business_metrics.profit_generated = profit;
        business_metrics.active_users = users;
        business_metrics.data_freshness_seconds = data_age;

        Ok(())
    }

    /// Collect infrastructure metrics
    pub async fn collect_infrastructure_metrics(&self, _env: &Env) -> ArbitrageResult<()> {
        if !self.config.infrastructure_metrics_enabled {
            return Ok(());
        }

        // In a real implementation, these would be collected from system APIs
        // For now, we'll simulate some metrics
        let mut infra_metrics = self.infrastructure_metrics.write();

        // Simulate CPU utilization
        infra_metrics.cpu_utilization = 45.0; // Would be actual CPU usage

        // Simulate memory metrics
        infra_metrics.memory_usage.used_bytes = 1024 * 1024 * 256; // 256MB
        infra_metrics.memory_usage.available_bytes = 1024 * 1024 * 768; // 768MB
        infra_metrics.memory_usage.utilization_percent = 25.0;

        // Simulate network metrics
        infra_metrics.network_io.bytes_sent += 1024;
        infra_metrics.network_io.bytes_received += 2048;
        infra_metrics.network_io.connection_count = 10;

        Ok(())
    }

    /// Calculate cost metrics
    pub async fn calculate_cost_metrics(
        &self,
        requests_count: u64,
        users_count: u64,
    ) -> ArbitrageResult<()> {
        if !self.config.cost_tracking_enabled {
            return Ok(());
        }

        let mut cost_metrics = self.cost_metrics.write();

        // Cloudflare Workers pricing (example rates)
        cost_metrics.workers_cost_usd = (requests_count as f64) * 0.0000005; // $0.50 per million requests

        // D1 pricing (simulated)
        cost_metrics.d1_cost_usd = 0.01; // Base cost

        // R2 pricing (simulated)
        cost_metrics.r2_cost_usd = 0.005; // Storage cost

        // KV pricing (simulated)
        cost_metrics.kv_cost_usd = 0.002; // KV operations cost

        cost_metrics.total_cost_usd = cost_metrics.workers_cost_usd
            + cost_metrics.d1_cost_usd
            + cost_metrics.r2_cost_usd
            + cost_metrics.kv_cost_usd;

        // Calculate per-unit costs
        if requests_count > 0 {
            cost_metrics.cost_per_request = cost_metrics.total_cost_usd / requests_count as f64;
        }

        if users_count > 0 {
            cost_metrics.cost_per_user = cost_metrics.total_cost_usd / users_count as f64;
        }

        // Project monthly burn rate (assume current rate continues)
        cost_metrics.monthly_burn_rate = cost_metrics.total_cost_usd * 30.0;

        Ok(())
    }

    /// Export metrics in Prometheus format
    pub async fn export_prometheus_format(&self) -> ArbitrageResult<String> {
        let mut output = String::new();

        // Export RED metrics
        let red_metrics = self.red_metrics.read();
        output.push_str(&format!(
            "# HELP http_requests_per_second Request rate\n# TYPE http_requests_per_second gauge\nhttp_requests_per_second{} {}\n",
            "", red_metrics.rate
        ));
        output.push_str(&format!(
            "# HELP http_error_rate_percent Error rate percentage\n# TYPE http_error_rate_percent gauge\nhttp_error_rate_percent{} {}\n",
            "", red_metrics.error_rate
        ));

        // Export business metrics
        let business_metrics = self.business_metrics.read();
        output.push_str(&format!(
            "# HELP arbitrage_opportunities_detected Opportunities detected\n# TYPE arbitrage_opportunities_detected counter\narbitrage_opportunities_detected{} {}\n",
            "", business_metrics.opportunities_detected
        ));

        // Export cost metrics
        let cost_metrics = self.cost_metrics.read();
        output.push_str(&format!(
            "# HELP infrastructure_cost_total_usd Total infrastructure cost in USD\n# TYPE infrastructure_cost_total_usd gauge\ninfrastructure_cost_total_usd{} {}\n",
            "", cost_metrics.total_cost_usd
        ));

        Ok(output)
    }

    /// Export metrics in OTLP format
    pub async fn export_otlp_format(&self) -> ArbitrageResult<serde_json::Value> {
        let mut metrics = serde_json::Map::new();

        // Add resource attributes
        metrics.insert(
            "resource".to_string(),
            serde_json::to_value(&self.otel_resource_attributes)?,
        );

        // Add metric data
        let red_metrics = self.red_metrics.read();
        metrics.insert(
            "red_metrics".to_string(),
            serde_json::to_value(&*red_metrics)?,
        );

        let business_metrics = self.business_metrics.read();
        metrics.insert(
            "business_metrics".to_string(),
            serde_json::to_value(&*business_metrics)?,
        );

        Ok(serde_json::Value::Object(metrics))
    }

    /// Calculate duration metrics from samples
    fn calculate_duration_metrics(&self, samples: &[f64]) -> DurationMetrics {
        if samples.is_empty() {
            return DurationMetrics::default();
        }

        let mut sorted_samples = samples.to_vec();
        sorted_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let sum: f64 = sorted_samples.iter().sum();
        let count = sorted_samples.len();
        let avg = sum / count as f64;

        let variance: f64 = sorted_samples
            .iter()
            .map(|value| {
                let diff = avg - value;
                diff * diff
            })
            .sum::<f64>()
            / count as f64;

        DurationMetrics {
            avg,
            min: sorted_samples[0],
            max: sorted_samples[count - 1],
            p50: sorted_samples[count * 50 / 100],
            p95: sorted_samples[count * 95 / 100],
            p99: sorted_samples[count * 99 / 100],
            p99_9: sorted_samples[count * 999 / 1000],
            stddev: variance.sqrt(),
        }
    }

    /// Get current health status
    pub async fn get_health(&self) -> MetricsCollectionHealth {
        self.health.read().clone()
    }

    /// Get RED metrics
    pub async fn get_red_metrics(&self) -> RedMetrics {
        self.red_metrics.read().clone()
    }

    /// Get business metrics
    pub async fn get_business_metrics(&self) -> BusinessMetrics {
        self.business_metrics.read().clone()
    }

    /// Get infrastructure metrics
    pub async fn get_infrastructure_metrics(&self) -> InfrastructureMetrics {
        self.infrastructure_metrics.read().clone()
    }

    /// Get cost metrics
    pub async fn get_cost_metrics(&self) -> CostMetrics {
        self.cost_metrics.read().clone()
    }
}

impl MetricsCollectionEngine {
    /// Health check for the metrics collection engine
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let health = self.health.read();

        // Check if collection is working
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let time_since_last_collection = now - health.last_collection_timestamp;

        // Consider unhealthy if no collection in last 5 minutes
        if time_since_last_collection > 300_000 {
            return Ok(false);
        }

        // Check error rate
        if health.error_rate_percent > 10.0 {
            return Ok(false);
        }

        Ok(health.is_healthy)
    }
}
