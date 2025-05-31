// Trace Collector - Distributed Tracing for Request Flow and Performance Analysis
// Part of Monitoring Module replacing monitoring_observability.rs

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Trace span representing a unit of work in a distributed system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub span_id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub operation_name: String,
    pub component: String,
    pub service_name: String,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub duration_ms: Option<u64>,
    pub status: SpanStatus,
    pub tags: HashMap<String, String>,
    pub logs: Vec<SpanLog>,
    pub baggage: HashMap<String, String>,
    pub references: Vec<SpanReference>,
}

impl TraceSpan {
    pub fn new(
        trace_id: String,
        operation_name: String,
        component: String,
        service_name: String,
    ) -> Self {
        Self {
            span_id: uuid::Uuid::new_v4().to_string(),
            trace_id,
            parent_span_id: None,
            operation_name,
            component,
            service_name,
            start_time: chrono::Utc::now().timestamp_millis() as u64,
            end_time: None,
            duration_ms: None,
            status: SpanStatus::Ok,
            tags: HashMap::new(),
            logs: Vec::new(),
            baggage: HashMap::new(),
            references: Vec::new(),
        }
    }

    pub fn with_parent(mut self, parent_span_id: String) -> Self {
        self.parent_span_id = Some(parent_span_id);
        self
    }

    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }

    pub fn with_baggage(mut self, key: String, value: String) -> Self {
        self.baggage.insert(key, value);
        self
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        self.logs.push(SpanLog {
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            level,
            message,
            fields: HashMap::new(),
        });
    }

    pub fn add_log_with_fields(
        &mut self,
        level: LogLevel,
        message: String,
        fields: HashMap<String, String>,
    ) {
        self.logs.push(SpanLog {
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            level,
            message,
            fields,
        });
    }

    pub fn finish(&mut self) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        self.end_time = Some(end_time);
        self.duration_ms = Some(end_time - self.start_time);
    }

    pub fn finish_with_error(&mut self, error: String) {
        self.finish();
        self.status = SpanStatus::Error;
        self.add_log(LogLevel::Error, error);
    }

    pub fn is_finished(&self) -> bool {
        self.end_time.is_some()
    }

    pub fn get_duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }
}

/// Span status indicating success or failure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Cancelled,
    Unknown,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
    Error,
    Timeout,
}

impl SpanStatus {
    pub fn as_str(&self) -> &str {
        match self {
            SpanStatus::Ok => "ok",
            SpanStatus::Cancelled => "cancelled",
            SpanStatus::Unknown => "unknown",
            SpanStatus::InvalidArgument => "invalid_argument",
            SpanStatus::DeadlineExceeded => "deadline_exceeded",
            SpanStatus::NotFound => "not_found",
            SpanStatus::AlreadyExists => "already_exists",
            SpanStatus::PermissionDenied => "permission_denied",
            SpanStatus::ResourceExhausted => "resource_exhausted",
            SpanStatus::FailedPrecondition => "failed_precondition",
            SpanStatus::Aborted => "aborted",
            SpanStatus::OutOfRange => "out_of_range",
            SpanStatus::Unimplemented => "unimplemented",
            SpanStatus::Internal => "internal",
            SpanStatus::Unavailable => "unavailable",
            SpanStatus::DataLoss => "data_loss",
            SpanStatus::Unauthenticated => "unauthenticated",
            SpanStatus::Error => "error",
            SpanStatus::Timeout => "timeout",
        }
    }

    pub fn is_error(&self) -> bool {
        !matches!(self, SpanStatus::Ok)
    }
}

/// Log entry within a span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLog {
    pub timestamp: u64,
    pub level: LogLevel,
    pub message: String,
    pub fields: HashMap<String, String>,
}

/// Log levels for span logs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn as_str(&self) -> &str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warning => "warning",
            LogLevel::Error => "error",
            LogLevel::Fatal => "fatal",
        }
    }
}

/// Reference to another span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanReference {
    pub reference_type: ReferenceType,
    pub trace_id: String,
    pub span_id: String,
}

/// Types of span references
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceType {
    ChildOf,
    FollowsFrom,
}

/// Trace context for propagating trace information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub baggage: HashMap<String, String>,
    pub sampling_decision: SamplingDecision,
}

impl TraceContext {
    pub fn new() -> Self {
        Self {
            trace_id: uuid::Uuid::new_v4().to_string(),
            span_id: uuid::Uuid::new_v4().to_string(),
            baggage: HashMap::new(),
            sampling_decision: SamplingDecision::Sample,
        }
    }

    pub fn with_baggage(mut self, key: String, value: String) -> Self {
        self.baggage.insert(key, value);
        self
    }

    pub fn with_sampling_decision(mut self, decision: SamplingDecision) -> Self {
        self.sampling_decision = decision;
        self
    }

    pub fn should_sample(&self) -> bool {
        matches!(self.sampling_decision, SamplingDecision::Sample)
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Sampling decision for traces
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplingDecision {
    Sample,
    Drop,
    Defer,
}

/// Tracing health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingHealth {
    pub is_healthy: bool,
    pub active_traces_count: u64,
    pub active_spans_count: u64,
    pub trace_processing_rate_per_second: f64,
    pub span_processing_rate_per_second: f64,
    pub error_rate_percent: f32,
    pub avg_trace_duration_ms: f64,
    pub avg_span_duration_ms: f64,
    pub sampling_rate_percent: f32,
    pub storage_usage_percent: f32,
    pub kv_store_available: bool,
    pub last_trace_timestamp: u64,
    pub last_error: Option<String>,
}

impl Default for TracingHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            active_traces_count: 0,
            active_spans_count: 0,
            trace_processing_rate_per_second: 0.0,
            span_processing_rate_per_second: 0.0,
            error_rate_percent: 0.0,
            avg_trace_duration_ms: 0.0,
            avg_span_duration_ms: 0.0,
            sampling_rate_percent: 100.0,
            storage_usage_percent: 0.0,
            kv_store_available: false,
            last_trace_timestamp: 0,
            last_error: None,
        }
    }
}

/// Tracing performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingMetrics {
    pub total_traces_collected: u64,
    pub total_spans_collected: u64,
    pub traces_per_second: f64,
    pub spans_per_second: f64,
    pub successful_traces: u64,
    pub failed_traces: u64,
    pub successful_spans: u64,
    pub failed_spans: u64,
    pub avg_trace_duration_ms: f64,
    pub min_trace_duration_ms: f64,
    pub max_trace_duration_ms: f64,
    pub avg_span_duration_ms: f64,
    pub min_span_duration_ms: f64,
    pub max_span_duration_ms: f64,
    pub traces_by_service: HashMap<String, u64>,
    pub spans_by_operation: HashMap<String, u64>,
    pub spans_by_status: HashMap<SpanStatus, u64>,
    pub sampling_stats: SamplingStats,
    pub storage_used_mb: f64,
    pub last_updated: u64,
}

impl Default for TracingMetrics {
    fn default() -> Self {
        Self {
            total_traces_collected: 0,
            total_spans_collected: 0,
            traces_per_second: 0.0,
            spans_per_second: 0.0,
            successful_traces: 0,
            failed_traces: 0,
            successful_spans: 0,
            failed_spans: 0,
            avg_trace_duration_ms: 0.0,
            min_trace_duration_ms: f64::MAX,
            max_trace_duration_ms: 0.0,
            avg_span_duration_ms: 0.0,
            min_span_duration_ms: f64::MAX,
            max_span_duration_ms: 0.0,
            traces_by_service: HashMap::new(),
            spans_by_operation: HashMap::new(),
            spans_by_status: HashMap::new(),
            sampling_stats: SamplingStats::default(),
            storage_used_mb: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Sampling statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingStats {
    pub total_decisions: u64,
    pub sampled_count: u64,
    pub dropped_count: u64,
    pub deferred_count: u64,
    pub sampling_rate_percent: f32,
}

impl Default for SamplingStats {
    fn default() -> Self {
        Self {
            total_decisions: 0,
            sampled_count: 0,
            dropped_count: 0,
            deferred_count: 0,
            sampling_rate_percent: 100.0,
        }
    }
}

/// Configuration for TraceCollector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCollectorConfig {
    pub enable_tracing: bool,
    pub enable_sampling: bool,
    pub default_sampling_rate: f32,
    pub max_traces_in_memory: usize,
    pub max_spans_per_trace: usize,
    pub trace_retention_seconds: u64,
    pub span_retention_seconds: u64,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
    pub enable_export: bool,
    pub export_format: String, // "jaeger", "zipkin", "otlp"
    pub export_endpoint: String,
    pub export_batch_size: usize,
    pub export_timeout_seconds: u64,
    pub enable_performance_analysis: bool,
    pub enable_error_correlation: bool,
    pub max_baggage_items: usize,
    pub max_baggage_size_bytes: usize,
}

impl Default for TraceCollectorConfig {
    fn default() -> Self {
        Self {
            enable_tracing: true,
            enable_sampling: true,
            default_sampling_rate: 0.1, // 10% sampling
            max_traces_in_memory: 10000,
            max_spans_per_trace: 1000,
            trace_retention_seconds: 86400, // 24 hours
            span_retention_seconds: 86400,
            enable_kv_storage: true,
            kv_key_prefix: "trace:".to_string(),
            enable_compression: true,
            compression_threshold_bytes: 1024,
            enable_export: false,
            export_format: "jaeger".to_string(),
            export_endpoint: String::new(),
            export_batch_size: 100,
            export_timeout_seconds: 30,
            enable_performance_analysis: true,
            enable_error_correlation: true,
            max_baggage_items: 64,
            max_baggage_size_bytes: 8192,
        }
    }
}

impl TraceCollectorConfig {
    pub fn high_performance() -> Self {
        Self {
            default_sampling_rate: 0.05, // 5% sampling for high performance
            max_traces_in_memory: 5000,
            max_spans_per_trace: 500,
            export_batch_size: 200,
            enable_compression: true,
            ..Default::default()
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            default_sampling_rate: 0.2, // 20% sampling for better observability
            max_traces_in_memory: 20000,
            max_spans_per_trace: 2000,
            trace_retention_seconds: 259200, // 3 days
            span_retention_seconds: 259200,
            export_batch_size: 50,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.default_sampling_rate < 0.0 || self.default_sampling_rate > 1.0 {
            return Err(ArbitrageError::validation_error(
                "default_sampling_rate must be between 0.0 and 1.0",
            ));
        }
        if self.max_traces_in_memory == 0 {
            return Err(ArbitrageError::validation_error(
                "max_traces_in_memory must be greater than 0",
            ));
        }
        if self.max_spans_per_trace == 0 {
            return Err(ArbitrageError::validation_error(
                "max_spans_per_trace must be greater than 0",
            ));
        }
        if self.trace_retention_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "trace_retention_seconds must be greater than 0",
            ));
        }
        if self.export_batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "export_batch_size must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Trace Collector for distributed tracing
pub struct TraceCollector {
    config: TraceCollectorConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Trace storage
    active_traces: Arc<Mutex<HashMap<String, Vec<TraceSpan>>>>,
    finished_traces: Arc<Mutex<Vec<String>>>, // trace_ids

    // Health and performance tracking
    health: Arc<Mutex<TracingHealth>>,
    metrics: Arc<Mutex<TracingMetrics>>,

    // Sampling
    sampling_decisions: Arc<Mutex<HashMap<String, SamplingDecision>>>,

    // Performance metrics
    startup_time: u64,
}

impl TraceCollector {
    /// Create new TraceCollector instance
    pub async fn new(
        config: TraceCollectorConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        logger.info(&format!(
            "TraceCollector initialized: tracing={}, sampling_rate={:.1}%, max_traces={}",
            config.enable_tracing,
            config.default_sampling_rate * 100.0,
            config.max_traces_in_memory
        ));

        Ok(Self {
            config,
            logger,
            kv_store,
            active_traces: Arc::new(Mutex::new(HashMap::new())),
            finished_traces: Arc::new(Mutex::new(Vec::new())),
            health: Arc::new(Mutex::new(TracingHealth::default())),
            metrics: Arc::new(Mutex::new(TracingMetrics::default())),
            sampling_decisions: Arc::new(Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Start a new trace
    pub async fn start_trace(
        &self,
        operation_name: String,
        component: String,
        service_name: String,
    ) -> ArbitrageResult<TraceContext> {
        if !self.config.enable_tracing {
            return Ok(TraceContext::new());
        }

        let trace_context = TraceContext::new();
        let sampling_decision = self.make_sampling_decision(&trace_context.trace_id).await;

        if sampling_decision == SamplingDecision::Sample {
            let span = TraceSpan::new(
                trace_context.trace_id.clone(),
                operation_name,
                component,
                service_name,
            );

            if let Ok(mut traces) = self.active_traces.lock() {
                traces.insert(trace_context.trace_id.clone(), vec![span]);
            }
        }

        Ok(trace_context.with_sampling_decision(sampling_decision))
    }

    /// Start a new span within a trace
    pub async fn start_span(
        &self,
        trace_context: &TraceContext,
        operation_name: String,
        component: String,
        service_name: String,
    ) -> ArbitrageResult<TraceSpan> {
        if !self.config.enable_tracing || !trace_context.should_sample() {
            return Ok(TraceSpan::new(
                trace_context.trace_id.clone(),
                operation_name,
                component,
                service_name,
            ));
        }

        let span = TraceSpan::new(
            trace_context.trace_id.clone(),
            operation_name,
            component,
            service_name,
        )
        .with_parent(trace_context.span_id.clone());

        // Add span to active trace
        if let Ok(mut traces) = self.active_traces.lock() {
            if let Some(trace_spans) = traces.get_mut(&trace_context.trace_id) {
                if trace_spans.len() < self.config.max_spans_per_trace {
                    trace_spans.push(span.clone());
                } else {
                    self.logger.warn(&format!(
                        "Trace {} exceeded max spans limit",
                        trace_context.trace_id
                    ));
                }
            }
        }

        Ok(span)
    }

    /// Finish a span
    pub async fn finish_span(&self, mut span: TraceSpan) -> ArbitrageResult<()> {
        if !self.config.enable_tracing {
            return Ok(());
        }

        span.finish();
        self.record_span_metrics(&span).await;

        // Store span if KV storage is enabled
        if self.config.enable_kv_storage {
            self.store_span_in_kv(&span).await?;
        }

        Ok(())
    }

    /// Finish a span with error
    pub async fn finish_span_with_error(
        &self,
        mut span: TraceSpan,
        error: String,
    ) -> ArbitrageResult<()> {
        if !self.config.enable_tracing {
            return Ok(());
        }

        span.finish_with_error(error);
        self.record_span_metrics(&span).await;

        // Store span if KV storage is enabled
        if self.config.enable_kv_storage {
            self.store_span_in_kv(&span).await?;
        }

        Ok(())
    }

    /// Finish a trace
    pub async fn finish_trace(&self, trace_id: &str) -> ArbitrageResult<()> {
        if !self.config.enable_tracing {
            return Ok(());
        }

        let spans_to_process = {
            let mut traces = self.active_traces.lock().unwrap();
            traces.remove(trace_id)
        };

        if let Some(spans) = spans_to_process {
            self.record_trace_metrics(trace_id, &spans).await;

            // Store in KV if enabled
            if self.config.enable_kv_storage {
                self.store_trace_in_kv(trace_id, &spans).await?;
            }
        }

        Ok(())
    }

    /// Get trace by ID
    pub async fn get_trace(&self, trace_id: &str) -> Option<Vec<TraceSpan>> {
        if let Ok(traces) = self.active_traces.lock() {
            traces.get(trace_id).cloned()
        } else {
            None
        }
    }

    /// Get traces by service
    pub async fn get_traces_by_service(&self, service_name: &str) -> Vec<(String, Vec<TraceSpan>)> {
        let mut result = Vec::new();

        if let Ok(traces) = self.active_traces.lock() {
            for (trace_id, spans) in traces.iter() {
                if spans.iter().any(|span| span.service_name == service_name) {
                    result.push((trace_id.clone(), spans.clone()));
                }
            }
        }

        result
    }

    /// Make sampling decision for a trace
    async fn make_sampling_decision(&self, trace_id: &str) -> SamplingDecision {
        if !self.config.enable_sampling {
            return SamplingDecision::Sample;
        }

        // Simple random sampling based on trace ID hash
        let hash = trace_id.chars().map(|c| c as u32).sum::<u32>();
        let sample_threshold = (self.config.default_sampling_rate * u32::MAX as f32) as u32;

        let decision = if hash < sample_threshold {
            SamplingDecision::Sample
        } else {
            SamplingDecision::Drop
        };

        // Record sampling decision
        if let Ok(mut decisions) = self.sampling_decisions.lock() {
            decisions.insert(trace_id.to_string(), decision.clone());
        }

        decision
    }

    /// Store span in KV store
    async fn store_span_in_kv(&self, span: &TraceSpan) -> ArbitrageResult<()> {
        let key = format!("{}span:{}", self.config.kv_key_prefix, span.span_id);
        let value = serde_json::to_string(span)?;

        self.kv_store
            .put(&key, value)?
            .expiration_ttl(self.config.span_retention_seconds)
            .execute()
            .await
            .map_err(|e| ArbitrageError::kv_error(format!("Failed to store span: {}", e)))?;

        Ok(())
    }

    /// Store trace in KV store
    async fn store_trace_in_kv(&self, trace_id: &str, spans: &[TraceSpan]) -> ArbitrageResult<()> {
        let key = format!("{}trace:{}", self.config.kv_key_prefix, trace_id);
        let value = serde_json::to_string(spans)?;

        self.kv_store
            .put(&key, value)?
            .expiration_ttl(self.config.trace_retention_seconds)
            .execute()
            .await
            .map_err(|e| ArbitrageError::kv_error(format!("Failed to store trace: {}", e)))?;

        Ok(())
    }

    /// Record span metrics
    async fn record_span_metrics(&self, span: &TraceSpan) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_spans_collected += 1;

            if span.status.is_error() {
                metrics.failed_spans += 1;
            } else {
                metrics.successful_spans += 1;
            }

            if let Some(duration) = span.get_duration_ms() {
                let duration_f64 = duration as f64;
                metrics.avg_span_duration_ms = (metrics.avg_span_duration_ms
                    * (metrics.total_spans_collected - 1) as f64
                    + duration_f64)
                    / metrics.total_spans_collected as f64;
                metrics.min_span_duration_ms = metrics.min_span_duration_ms.min(duration_f64);
                metrics.max_span_duration_ms = metrics.max_span_duration_ms.max(duration_f64);
            }

            *metrics
                .spans_by_operation
                .entry(span.operation_name.clone())
                .or_insert(0) += 1;
            *metrics
                .spans_by_status
                .entry(span.status.clone())
                .or_insert(0) += 1;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Record trace metrics
    async fn record_trace_metrics(&self, _trace_id: &str, spans: &[TraceSpan]) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_traces_collected += 1;

            let has_errors = spans.iter().any(|span| span.status.is_error());
            if has_errors {
                metrics.failed_traces += 1;
            } else {
                metrics.successful_traces += 1;
            }

            // Calculate trace duration (from first span start to last span end)
            if let (Some(first_span), Some(last_span)) = (spans.first(), spans.last()) {
                if let Some(last_end) = last_span.end_time {
                    let trace_duration = last_end - first_span.start_time;
                    let duration_f64 = trace_duration as f64;
                    metrics.avg_trace_duration_ms = (metrics.avg_trace_duration_ms
                        * (metrics.total_traces_collected - 1) as f64
                        + duration_f64)
                        / metrics.total_traces_collected as f64;
                    metrics.min_trace_duration_ms = metrics.min_trace_duration_ms.min(duration_f64);
                    metrics.max_trace_duration_ms = metrics.max_trace_duration_ms.max(duration_f64);
                }
            }

            // Record service usage
            for span in spans {
                *metrics
                    .traces_by_service
                    .entry(span.service_name.clone())
                    .or_insert(0) += 1;
            }

            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Get tracing health
    pub async fn get_health(&self) -> TracingHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            TracingHealth::default()
        }
    }

    /// Get tracing metrics
    pub async fn get_metrics(&self) -> TracingMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            TracingMetrics::default()
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Test basic tracing functionality
        let trace_context = self
            .start_trace(
                "health_check".to_string(),
                "trace_collector".to_string(),
                "monitoring".to_string(),
            )
            .await?;

        let span = self
            .start_span(
                &trace_context,
                "health_check_span".to_string(),
                "trace_collector".to_string(),
                "monitoring".to_string(),
            )
            .await?;

        self.finish_span(span).await?;
        self.finish_trace(&trace_context.trace_id).await?;

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = true;
            health.kv_store_available = self.config.enable_kv_storage;
            health.last_trace_timestamp = start_time;
            health.last_error = None;
        }

        Ok(true)
    }

    /// Cleanup old traces and spans
    pub async fn cleanup_old_traces(&self) -> ArbitrageResult<u64> {
        let mut cleaned_count = 0;
        let current_time = chrono::Utc::now().timestamp_millis() as u64;
        let retention_ms = self.config.trace_retention_seconds * 1000;

        // Clean up finished traces
        if let Ok(mut finished) = self.finished_traces.lock() {
            finished.retain(|_| {
                // In a real implementation, we'd check the actual trace timestamp
                // For now, we'll keep all traces
                true
            });
        }

        // Clean up active traces that are too old
        if let Ok(mut traces) = self.active_traces.lock() {
            let old_trace_ids: Vec<String> = traces
                .iter()
                .filter_map(|(trace_id, spans)| {
                    if let Some(first_span) = spans.first() {
                        if current_time - first_span.start_time > retention_ms {
                            return Some(trace_id.clone());
                        }
                    }
                    None
                })
                .collect();

            for trace_id in old_trace_ids {
                traces.remove(&trace_id);
                cleaned_count += 1;
            }
        }

        Ok(cleaned_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_status_properties() {
        assert_eq!(SpanStatus::Ok.as_str(), "ok");
        assert!(!SpanStatus::Ok.is_error());
        assert!(SpanStatus::Error.is_error());
        assert!(SpanStatus::Internal.is_error());
    }

    #[test]
    fn test_trace_span_creation() {
        let span = TraceSpan::new(
            "trace-123".to_string(),
            "test_operation".to_string(),
            "test_component".to_string(),
            "test_service".to_string(),
        )
        .with_tag("key".to_string(), "value".to_string());

        assert_eq!(span.trace_id, "trace-123");
        assert_eq!(span.operation_name, "test_operation");
        assert_eq!(span.component, "test_component");
        assert_eq!(span.service_name, "test_service");
        assert_eq!(span.tags.get("key"), Some(&"value".to_string()));
        assert!(!span.is_finished());
    }

    #[test]
    fn test_trace_span_finish() {
        let mut span = TraceSpan::new(
            "trace-123".to_string(),
            "test_operation".to_string(),
            "test_component".to_string(),
            "test_service".to_string(),
        );

        assert!(!span.is_finished());
        span.finish();
        assert!(span.is_finished());
        assert!(span.get_duration_ms().is_some());
    }

    #[test]
    fn test_trace_context_creation() {
        let context = TraceContext::new()
            .with_baggage("key".to_string(), "value".to_string())
            .with_sampling_decision(SamplingDecision::Sample);

        assert!(context.should_sample());
        assert_eq!(context.baggage.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_trace_collector_config_validation() {
        let mut config = TraceCollectorConfig::default();
        assert!(config.validate().is_ok());

        config.default_sampling_rate = 1.5;
        assert!(config.validate().is_err());

        config.default_sampling_rate = 0.5;
        config.max_traces_in_memory = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = TraceCollectorConfig::high_performance();
        assert_eq!(config.default_sampling_rate, 0.05);
        assert_eq!(config.max_traces_in_memory, 5000);
        assert_eq!(config.export_batch_size, 200);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = TraceCollectorConfig::high_reliability();
        assert_eq!(config.default_sampling_rate, 0.2);
        assert_eq!(config.trace_retention_seconds, 259200);
        assert_eq!(config.export_batch_size, 50);
    }
}
