// src/services/core/infrastructure/analytics_module/data_processor.rs

//! Data Processor - Real-Time Data Processing and Stream Analytics
//!
//! This component provides real-time data processing capabilities for the ArbEdge platform,
//! handling stream analytics, time-series aggregation, and statistical analysis with
//! sub-second latency for high-concurrency trading operations.
//!
//! ## Revolutionary Features:
//! - **Stream Processing**: Real-time analytics with sub-second latency
//! - **Time-Series Aggregation**: 1m, 5m, 1h, 1d window aggregations
//! - **Statistical Analysis**: Mean, median, percentiles, standard deviation
//! - **Trend Detection**: Pattern recognition and anomaly detection
//! - **Data Enrichment**: Context and metadata addition

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use worker::{kv::KvStore, Env};

/// Data Processor Configuration
#[derive(Debug, Clone)]
pub struct DataProcessorConfig {
    // Stream processing settings
    pub enable_stream_processing: bool,
    pub stream_buffer_size: usize,
    pub processing_interval_ms: u64,
    pub max_processing_latency_ms: u64,

    // Aggregation settings
    pub enable_time_series_aggregation: bool,
    pub aggregation_windows: Vec<u64>, // in seconds: [60, 300, 3600, 86400]
    pub max_data_points_per_window: usize,

    // Statistical analysis settings
    pub enable_statistical_analysis: bool,
    pub enable_trend_detection: bool,
    pub enable_anomaly_detection: bool,
    pub anomaly_threshold_std_dev: f64,

    // Performance settings
    pub batch_processing_size: usize,
    pub cache_ttl_seconds: u64,
    pub max_concurrent_streams: u32,
}

impl Default for DataProcessorConfig {
    fn default() -> Self {
        Self {
            enable_stream_processing: true,
            stream_buffer_size: 1000,
            processing_interval_ms: 100,
            max_processing_latency_ms: 500,
            enable_time_series_aggregation: true,
            aggregation_windows: vec![60, 300, 3600, 86400], // 1m, 5m, 1h, 1d
            max_data_points_per_window: 1000,
            enable_statistical_analysis: true,
            enable_trend_detection: true,
            enable_anomaly_detection: true,
            anomaly_threshold_std_dev: 2.0,
            batch_processing_size: 100,
            cache_ttl_seconds: 300,
            max_concurrent_streams: 50,
        }
    }
}

impl DataProcessorConfig {
    /// High-performance configuration for 1000-2500 concurrent users
    pub fn high_performance() -> Self {
        Self {
            enable_stream_processing: true,
            stream_buffer_size: 2000,
            processing_interval_ms: 50,
            max_processing_latency_ms: 200,
            enable_time_series_aggregation: true,
            aggregation_windows: vec![60, 300, 3600],
            max_data_points_per_window: 2000,
            enable_statistical_analysis: true,
            enable_trend_detection: true,
            enable_anomaly_detection: true,
            anomaly_threshold_std_dev: 1.5,
            batch_processing_size: 200,
            cache_ttl_seconds: 180,
            max_concurrent_streams: 100,
        }
    }

    /// High-reliability configuration with enhanced data retention
    pub fn high_reliability() -> Self {
        Self {
            enable_stream_processing: true,
            stream_buffer_size: 500,
            processing_interval_ms: 200,
            max_processing_latency_ms: 1000,
            enable_time_series_aggregation: true,
            aggregation_windows: vec![60, 300, 3600, 86400, 604800], // Include weekly
            max_data_points_per_window: 5000,
            enable_statistical_analysis: true,
            enable_trend_detection: false, // Disable for stability
            enable_anomaly_detection: true,
            anomaly_threshold_std_dev: 3.0,
            batch_processing_size: 50,
            cache_ttl_seconds: 600,
            max_concurrent_streams: 25,
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.stream_buffer_size == 0 {
            return Err(ArbitrageError::configuration_error(
                "stream_buffer_size must be greater than 0".to_string(),
            ));
        }
        if self.batch_processing_size == 0 {
            return Err(ArbitrageError::configuration_error(
                "batch_processing_size must be greater than 0".to_string(),
            ));
        }
        if self.max_concurrent_streams == 0 {
            return Err(ArbitrageError::configuration_error(
                "max_concurrent_streams must be greater than 0".to_string(),
            ));
        }
        if self.aggregation_windows.is_empty() {
            return Err(ArbitrageError::configuration_error(
                "aggregation_windows cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Data Processor Health Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataProcessorHealth {
    pub is_healthy: bool,
    pub stream_processing_healthy: bool,
    pub aggregation_healthy: bool,
    pub statistical_analysis_healthy: bool,
    pub active_streams: u32,
    pub buffer_utilization_percent: f64,
    pub average_processing_latency_ms: f64,
    pub last_health_check: u64,
}

/// Data Processor Performance Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataProcessorMetrics {
    // Processing metrics
    pub queries_processed: u64,
    pub data_points_processed: u64,
    pub average_processing_time_ms: f64,
    pub processing_rate_per_second: f64,

    // Stream metrics
    pub active_streams: u32,
    pub total_streams_created: u64,
    pub stream_errors: u64,
    pub buffer_overflows: u64,

    // Aggregation metrics
    pub aggregations_computed: u64,
    pub aggregation_cache_hits: u64,
    pub aggregation_cache_misses: u64,

    // Statistical metrics
    pub anomalies_detected: u64,
    pub trends_identified: u64,
    pub statistical_computations: u64,

    // Performance metrics
    pub cache_hit_rate: f64,
    pub error_rate: f64,
    pub throughput_mbps: f64,
    pub last_updated: u64,
}

/// Time-series data point for aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesDataPoint {
    pub timestamp: u64,
    pub value: f64,
    pub metadata: HashMap<String, String>,
}

/// Aggregated time-series window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedWindow {
    pub window_size_seconds: u64,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub count: u64,
    pub sum: f64,
    pub mean: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub std_dev: f64,
    pub percentile_50: f64,
    pub percentile_95: f64,
    pub percentile_99: f64,
}

/// Statistical analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalAnalysis {
    pub data_points: u64,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub variance: f64,
    pub skewness: f64,
    pub kurtosis: f64,
    pub min: f64,
    pub max: f64,
    pub range: f64,
    pub percentiles: HashMap<String, f64>, // "p50", "p95", "p99", etc.
}

/// Trend detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub trend_direction: String, // "up", "down", "stable"
    pub trend_strength: f64,     // 0.0 to 1.0
    pub slope: f64,
    pub r_squared: f64,
    pub confidence_level: f64,
    pub prediction_next_value: Option<f64>,
    pub prediction_confidence: Option<f64>,
}

/// Anomaly detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetection {
    pub is_anomaly: bool,
    pub anomaly_score: f64,
    pub threshold: f64,
    pub deviation_from_mean: f64,
    pub z_score: f64,
    pub anomaly_type: String, // "high", "low", "pattern"
}

/// Data Processor for real-time analytics
#[derive(Clone)]
pub struct DataProcessor {
    config: DataProcessorConfig,
    kv_store: Option<KvStore>,

    // Stream processing state
    active_streams: HashMap<String, VecDeque<TimeSeriesDataPoint>>,
    stream_metadata: HashMap<String, HashMap<String, String>>,

    // Aggregation cache
    aggregation_cache: HashMap<String, AggregatedWindow>,

    // Performance tracking
    metrics: DataProcessorMetrics,
    last_processing_time: u64,
    is_initialized: bool,
}

impl DataProcessor {
    /// Create new Data Processor with configuration
    pub fn new(config: DataProcessorConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        Ok(Self {
            config,
            kv_store: None,
            active_streams: HashMap::new(),
            stream_metadata: HashMap::new(),
            aggregation_cache: HashMap::new(),
            metrics: DataProcessorMetrics::default(),
            last_processing_time: worker::Date::now().as_millis(),
            is_initialized: false,
        })
    }

    /// Initialize the Data Processor with environment
    pub async fn initialize(&mut self, env: &Env) -> ArbitrageResult<()> {
        // Initialize KV store for caching
        self.kv_store = Some(env.kv("ANALYTICS_CACHE").map_err(|e| {
            ArbitrageError::configuration_error(format!("Failed to initialize KV store: {:?}", e))
        })?);

        self.is_initialized = true;
        Ok(())
    }

    /// Process real-time data stream
    pub async fn process_stream_data(
        &mut self,
        stream_id: &str,
        data_points: Vec<TimeSeriesDataPoint>,
    ) -> ArbitrageResult<()> {
        let start_time = worker::Date::now().as_millis();

        // Get or create stream buffer
        let stream_buffer = self
            .active_streams
            .entry(stream_id.to_string())
            .or_insert_with(|| VecDeque::with_capacity(self.config.stream_buffer_size));

        // Add new data points to stream
        for data_point in data_points {
            // Check buffer capacity
            if stream_buffer.len() >= self.config.stream_buffer_size {
                stream_buffer.pop_front(); // Remove oldest point
                self.metrics.buffer_overflows += 1;
            }

            stream_buffer.push_back(data_point);
            self.metrics.data_points_processed += 1;
        }

        // Clone the buffer for use in methods to avoid borrow checker issues
        let buffer_clone = stream_buffer.clone();

        // Process aggregations if enabled
        if self.config.enable_time_series_aggregation {
            self.compute_aggregations(stream_id, &buffer_clone).await?;
        }

        // Perform statistical analysis if enabled
        if self.config.enable_statistical_analysis {
            self.perform_statistical_analysis(stream_id, &buffer_clone)
                .await?;
        }

        // Update metrics
        let processing_time = worker::Date::now().as_millis() - start_time;
        self.update_processing_metrics(processing_time);

        Ok(())
    }

    /// Compute time-series aggregations for different windows
    async fn compute_aggregations(
        &mut self,
        stream_id: &str,
        stream_buffer: &VecDeque<TimeSeriesDataPoint>,
    ) -> ArbitrageResult<()> {
        let current_time = worker::Date::now().as_millis();

        for &window_size in &self.config.aggregation_windows {
            let window_start = current_time - (window_size * 1000);
            let cache_key = format!("{}:{}:{}", stream_id, window_size, window_start / 1000);

            // Check cache first
            if let Some(cached_aggregation) = self.aggregation_cache.get(&cache_key) {
                if current_time - cached_aggregation.end_timestamp
                    < (self.config.cache_ttl_seconds * 1000)
                {
                    self.metrics.aggregation_cache_hits += 1;
                    continue;
                }
            }

            // Filter data points within window
            let window_data: Vec<f64> = stream_buffer
                .iter()
                .filter(|point| point.timestamp >= window_start)
                .map(|point| point.value)
                .collect();

            if !window_data.is_empty() {
                let aggregation = self.compute_window_aggregation(
                    window_size,
                    window_start,
                    current_time,
                    &window_data,
                )?;

                // Cache the aggregation
                self.aggregation_cache
                    .insert(cache_key.clone(), aggregation.clone());
                self.metrics.aggregations_computed += 1;
                self.metrics.aggregation_cache_misses += 1;

                // Store in KV if available
                if let Some(kv) = &self.kv_store {
                    let serialized = serde_json::to_string(&aggregation)
                        .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

                    kv.put(&cache_key, serialized)?
                        .expiration_ttl(self.config.cache_ttl_seconds)
                        .execute()
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Compute aggregation statistics for a window
    fn compute_window_aggregation(
        &self,
        window_size: u64,
        start_timestamp: u64,
        end_timestamp: u64,
        data: &[f64],
    ) -> ArbitrageResult<AggregatedWindow> {
        if data.is_empty() {
            return Err(ArbitrageError::processing_error(
                "No data points for aggregation".to_string(),
            ));
        }

        let count = data.len() as u64;
        let sum: f64 = data.iter().sum();
        let mean = sum / count as f64;

        // Sort data for percentile calculations
        let mut sorted_data = data.to_vec();
        sorted_data.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let min = sorted_data[0];
        let max = sorted_data[sorted_data.len() - 1];
        let median = self.calculate_percentile(&sorted_data, 50.0);
        let percentile_50 = median;
        let percentile_95 = self.calculate_percentile(&sorted_data, 95.0);
        let percentile_99 = self.calculate_percentile(&sorted_data, 99.0);

        // Calculate standard deviation
        let variance: f64 = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / count as f64;
        let std_dev = variance.sqrt();

        Ok(AggregatedWindow {
            window_size_seconds: window_size,
            start_timestamp,
            end_timestamp,
            count,
            sum,
            mean,
            median,
            min,
            max,
            std_dev,
            percentile_50,
            percentile_95,
            percentile_99,
        })
    }

    /// Calculate percentile from sorted data
    fn calculate_percentile(&self, sorted_data: &[f64], percentile: f64) -> f64 {
        if sorted_data.is_empty() {
            return 0.0;
        }

        let index = (percentile / 100.0) * (sorted_data.len() - 1) as f64;
        let lower_index = index.floor() as usize;
        let upper_index = index.ceil() as usize;

        if lower_index == upper_index {
            sorted_data[lower_index]
        } else {
            let weight = index - lower_index as f64;
            sorted_data[lower_index] * (1.0 - weight) + sorted_data[upper_index] * weight
        }
    }

    /// Perform statistical analysis on stream data
    async fn perform_statistical_analysis(
        &mut self,
        _stream_id: &str,
        stream_buffer: &VecDeque<TimeSeriesDataPoint>,
    ) -> ArbitrageResult<()> {
        if stream_buffer.len() < 10 {
            return Ok(()); // Need minimum data points for analysis
        }

        let values: Vec<f64> = stream_buffer.iter().map(|point| point.value).collect();

        // Compute statistical analysis
        let analysis = self.compute_statistical_analysis(&values)?;
        self.metrics.statistical_computations += 1;

        // Perform trend detection if enabled
        if self.config.enable_trend_detection && values.len() >= 20 {
            let trend = self.detect_trend(&values)?;
            if trend.trend_strength > 0.7 {
                self.metrics.trends_identified += 1;
            }
        }

        // Perform anomaly detection if enabled
        if self.config.enable_anomaly_detection {
            let latest_value = values[values.len() - 1];
            let anomaly = self.detect_anomaly(latest_value, &analysis)?;
            if anomaly.is_anomaly {
                self.metrics.anomalies_detected += 1;
            }
        }

        Ok(())
    }

    /// Compute comprehensive statistical analysis
    fn compute_statistical_analysis(&self, data: &[f64]) -> ArbitrageResult<StatisticalAnalysis> {
        if data.is_empty() {
            return Err(ArbitrageError::processing_error(
                "No data for statistical analysis".to_string(),
            ));
        }

        let n = data.len() as f64;
        let mean: f64 = data.iter().sum::<f64>() / n;

        let variance: f64 = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        let mut sorted_data = data.to_vec();
        sorted_data.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let median = self.calculate_percentile(&sorted_data, 50.0);
        let min = sorted_data[0];
        let max = sorted_data[sorted_data.len() - 1];
        let range = max - min;

        // Calculate skewness and kurtosis
        let skewness = if std_dev > 0.0 {
            data.iter()
                .map(|x| ((x - mean) / std_dev).powi(3))
                .sum::<f64>()
                / n
        } else {
            0.0
        };

        let kurtosis = if std_dev > 0.0 {
            data.iter()
                .map(|x| ((x - mean) / std_dev).powi(4))
                .sum::<f64>()
                / n
                - 3.0
        } else {
            0.0
        };

        // Calculate percentiles
        let mut percentiles = HashMap::new();
        percentiles.insert(
            "p10".to_string(),
            self.calculate_percentile(&sorted_data, 10.0),
        );
        percentiles.insert(
            "p25".to_string(),
            self.calculate_percentile(&sorted_data, 25.0),
        );
        percentiles.insert("p50".to_string(), median);
        percentiles.insert(
            "p75".to_string(),
            self.calculate_percentile(&sorted_data, 75.0),
        );
        percentiles.insert(
            "p90".to_string(),
            self.calculate_percentile(&sorted_data, 90.0),
        );
        percentiles.insert(
            "p95".to_string(),
            self.calculate_percentile(&sorted_data, 95.0),
        );
        percentiles.insert(
            "p99".to_string(),
            self.calculate_percentile(&sorted_data, 99.0),
        );

        Ok(StatisticalAnalysis {
            data_points: n as u64,
            mean,
            median,
            std_dev,
            variance,
            skewness,
            kurtosis,
            min,
            max,
            range,
            percentiles,
        })
    }

    /// Detect trend in time series data
    fn detect_trend(&self, data: &[f64]) -> ArbitrageResult<TrendAnalysis> {
        if data.len() < 2 {
            return Err(ArbitrageError::processing_error(
                "Insufficient data for trend analysis".to_string(),
            ));
        }

        let n = data.len() as f64;
        let x_values: Vec<f64> = (0..data.len()).map(|i| i as f64).collect();

        // Calculate linear regression
        let x_mean: f64 = x_values.iter().sum::<f64>() / n;
        let y_mean: f64 = data.iter().sum::<f64>() / n;

        let numerator: f64 = x_values
            .iter()
            .zip(data.iter())
            .map(|(x, y)| (x - x_mean) * (y - y_mean))
            .sum();

        let denominator: f64 = x_values.iter().map(|x| (x - x_mean).powi(2)).sum();

        let slope = if denominator != 0.0 {
            numerator / denominator
        } else {
            0.0
        };

        // Calculate R-squared
        let ss_tot: f64 = data.iter().map(|y| (y - y_mean).powi(2)).sum();
        let ss_res: f64 = x_values
            .iter()
            .zip(data.iter())
            .map(|(x, y)| {
                let predicted = y_mean + slope * (x - x_mean);
                (y - predicted).powi(2)
            })
            .sum();

        let r_squared = if ss_tot != 0.0 {
            1.0 - (ss_res / ss_tot)
        } else {
            0.0
        };

        // Determine trend direction and strength
        let trend_direction = if slope > 0.01 {
            "up"
        } else if slope < -0.01 {
            "down"
        } else {
            "stable"
        };

        let trend_strength = r_squared.abs().min(1.0);
        let confidence_level = if r_squared > 0.8 {
            0.95
        } else if r_squared > 0.6 {
            0.80
        } else {
            0.60
        };

        // Predict next value
        let prediction_next_value = if trend_strength > 0.5 {
            Some(y_mean + slope * n)
        } else {
            None
        };

        let prediction_confidence = if prediction_next_value.is_some() {
            Some(confidence_level)
        } else {
            None
        };

        Ok(TrendAnalysis {
            trend_direction: trend_direction.to_string(),
            trend_strength,
            slope,
            r_squared,
            confidence_level,
            prediction_next_value,
            prediction_confidence,
        })
    }

    /// Detect anomalies using statistical methods
    fn detect_anomaly(
        &self,
        value: f64,
        analysis: &StatisticalAnalysis,
    ) -> ArbitrageResult<AnomalyDetection> {
        let z_score = if analysis.std_dev > 0.0 {
            (value - analysis.mean) / analysis.std_dev
        } else {
            0.0
        };

        let threshold = self.config.anomaly_threshold_std_dev;
        let is_anomaly = z_score.abs() > threshold;
        let anomaly_score = z_score.abs() / threshold;
        let deviation_from_mean = (value - analysis.mean).abs();

        let anomaly_type = if z_score > threshold {
            "high"
        } else if z_score < -threshold {
            "low"
        } else {
            "normal"
        };

        Ok(AnomalyDetection {
            is_anomaly,
            anomaly_score,
            threshold,
            deviation_from_mean,
            z_score,
            anomaly_type: anomaly_type.to_string(),
        })
    }

    /// Update processing performance metrics
    fn update_processing_metrics(&mut self, processing_time_ms: u64) {
        self.metrics.queries_processed += 1;

        // Update average processing time (exponential moving average)
        let alpha = 0.1;
        self.metrics.average_processing_time_ms = alpha * processing_time_ms as f64
            + (1.0 - alpha) * self.metrics.average_processing_time_ms;

        // Calculate processing rate
        let current_time = worker::Date::now().as_millis();
        let time_diff_seconds = (current_time - self.last_processing_time) as f64 / 1000.0;
        if time_diff_seconds > 0.0 {
            self.metrics.processing_rate_per_second = 1.0 / time_diff_seconds;
        }
        self.last_processing_time = current_time;

        // Update cache hit rate
        let total_cache_requests =
            self.metrics.aggregation_cache_hits + self.metrics.aggregation_cache_misses;
        if total_cache_requests > 0 {
            self.metrics.cache_hit_rate =
                self.metrics.aggregation_cache_hits as f64 / total_cache_requests as f64;
        }

        // Update error rate
        let total_operations = self.metrics.queries_processed + self.metrics.stream_errors;
        if total_operations > 0 {
            self.metrics.error_rate = self.metrics.stream_errors as f64 / total_operations as f64;
        }

        self.metrics.last_updated = current_time;
    }

    /// Get health status
    pub async fn health_check(&self) -> ArbitrageResult<DataProcessorHealth> {
        let active_streams = self.active_streams.len() as u32;
        let max_buffer_size = self
            .active_streams
            .values()
            .map(|buffer| buffer.len())
            .max()
            .unwrap_or(0);

        let buffer_utilization_percent = if self.config.stream_buffer_size > 0 {
            (max_buffer_size as f64 / self.config.stream_buffer_size as f64) * 100.0
        } else {
            0.0
        };

        let stream_processing_healthy = active_streams <= self.config.max_concurrent_streams;
        let aggregation_healthy =
            self.metrics.average_processing_time_ms < self.config.max_processing_latency_ms as f64;
        let statistical_analysis_healthy = self.metrics.error_rate < 0.05; // 5% error threshold

        let is_healthy =
            stream_processing_healthy && aggregation_healthy && statistical_analysis_healthy;

        Ok(DataProcessorHealth {
            is_healthy,
            stream_processing_healthy,
            aggregation_healthy,
            statistical_analysis_healthy,
            active_streams,
            buffer_utilization_percent,
            average_processing_latency_ms: self.metrics.average_processing_time_ms,
            last_health_check: worker::Date::now().as_millis(),
        })
    }

    /// Get performance metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<DataProcessorMetrics> {
        Ok(self.metrics.clone())
    }

    /// Get aggregated data for a stream and window
    pub async fn get_aggregated_data(
        &self,
        stream_id: &str,
        window_size_seconds: u64,
        start_timestamp: u64,
    ) -> ArbitrageResult<Option<AggregatedWindow>> {
        let cache_key = format!(
            "{}:{}:{}",
            stream_id,
            window_size_seconds,
            start_timestamp / 1000
        );

        // Check memory cache first
        if let Some(aggregation) = self.aggregation_cache.get(&cache_key) {
            return Ok(Some(aggregation.clone()));
        }

        // Check KV store
        if let Some(kv) = &self.kv_store {
            if let Ok(Some(cached_data)) = kv.get(&cache_key).text().await {
                if let Ok(aggregation) = serde_json::from_str::<AggregatedWindow>(&cached_data) {
                    return Ok(Some(aggregation));
                }
            }
        }

        Ok(None)
    }

    /// Get current stream data
    pub fn get_stream_data(&self, stream_id: &str) -> Option<&VecDeque<TimeSeriesDataPoint>> {
        self.active_streams.get(stream_id)
    }

    /// Clear old data from streams
    pub async fn cleanup_old_data(&mut self, max_age_seconds: u64) -> ArbitrageResult<()> {
        let cutoff_time = worker::Date::now().as_millis() - (max_age_seconds * 1000);

        for stream_buffer in self.active_streams.values_mut() {
            stream_buffer.retain(|point| point.timestamp >= cutoff_time);
        }

        // Clear old aggregation cache
        self.aggregation_cache
            .retain(|_, aggregation| aggregation.end_timestamp >= cutoff_time);

        Ok(())
    }
}

impl Default for DataProcessorMetrics {
    fn default() -> Self {
        Self {
            queries_processed: 0,
            data_points_processed: 0,
            average_processing_time_ms: 0.0,
            processing_rate_per_second: 0.0,
            active_streams: 0,
            total_streams_created: 0,
            stream_errors: 0,
            buffer_overflows: 0,
            aggregations_computed: 0,
            aggregation_cache_hits: 0,
            aggregation_cache_misses: 0,
            anomalies_detected: 0,
            trends_identified: 0,
            statistical_computations: 0,
            cache_hit_rate: 0.0,
            error_rate: 0.0,
            throughput_mbps: 0.0,
            last_updated: worker::Date::now().as_millis(),
        }
    }
}
