// Ingestion Coordinator - Main Orchestrator for Data Ingestion Operations
// Coordinates between PipelineManager, QueueManager, and DataTransformer

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::Mutex;
use worker::kv::KvStore;

use super::{
    data_transformer::{TransformationRequest, TransformationResponse},
    pipeline_manager::{PipelineEvent, PipelineManager, PipelineType},
    queue_manager::{QueueManager, QueueMessage},
    DataFormat, DataTransformer, IngestionEvent, IngestionEventType, MessagePriority,
};

/// Ingestion request for the coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionRequest {
    pub request_id: String,
    pub event: IngestionEvent,
    pub processing_options: ProcessingOptions,
    pub metadata: HashMap<String, String>,
    pub timestamp: u64,
}

impl IngestionRequest {
    pub fn new(event: IngestionEvent) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            event,
            processing_options: ProcessingOptions::default(),
            metadata: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn with_options(mut self, options: ProcessingOptions) -> Self {
        self.processing_options = options;
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Processing options for ingestion requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingOptions {
    pub use_pipeline: bool,
    pub use_queue: bool,
    pub use_transformer: bool,
    pub enable_fallback: bool,
    pub priority: MessagePriority,
    pub timeout_seconds: u64,
    pub retry_attempts: u32,
    pub enable_compression: bool,
    pub target_format: Option<DataFormat>,
}

impl Default for ProcessingOptions {
    fn default() -> Self {
        Self {
            use_pipeline: true,
            use_queue: true,
            use_transformer: true,
            enable_fallback: true,
            priority: MessagePriority::Normal,
            timeout_seconds: 30,
            retry_attempts: 3,
            enable_compression: true,
            target_format: None,
        }
    }
}

impl ProcessingOptions {
    pub fn pipeline_only() -> Self {
        Self {
            use_pipeline: true,
            use_queue: false,
            use_transformer: false,
            ..Default::default()
        }
    }

    pub fn queue_only() -> Self {
        Self {
            use_pipeline: false,
            use_queue: true,
            use_transformer: false,
            ..Default::default()
        }
    }

    pub fn high_priority() -> Self {
        Self {
            priority: MessagePriority::High,
            timeout_seconds: 15,
            retry_attempts: 5,
            ..Default::default()
        }
    }

    pub fn reliable() -> Self {
        Self {
            enable_fallback: true,
            retry_attempts: 5,
            timeout_seconds: 60,
            ..Default::default()
        }
    }
}

/// Ingestion response from the coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionResponse {
    pub request_id: String,
    pub success: bool,
    pub processing_path: Vec<String>,
    pub pipeline_result: Option<String>,
    pub queue_result: Option<String>,
    pub transformation_result: Option<String>,
    pub fallback_used: bool,
    pub processing_time_ms: u64,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub timestamp: u64,
}

/// Ingestion metrics for performance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub pipeline_requests: u64,
    pub queue_requests: u64,
    pub transformation_requests: u64,
    pub fallback_requests: u64,
    pub average_processing_time_ms: f64,
    pub min_processing_time_ms: f64,
    pub max_processing_time_ms: f64,
    pub requests_by_event_type: HashMap<IngestionEventType, u64>,
    pub requests_by_priority: HashMap<MessagePriority, u64>,
    pub error_rate_percent: f32,
    pub throughput_per_second: f64,
    pub last_updated: u64,
}

impl Default for IngestionMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            pipeline_requests: 0,
            queue_requests: 0,
            transformation_requests: 0,
            fallback_requests: 0,
            average_processing_time_ms: 0.0,
            min_processing_time_ms: f64::MAX,
            max_processing_time_ms: 0.0,
            requests_by_event_type: HashMap::new(),
            requests_by_priority: HashMap::new(),
            error_rate_percent: 0.0,
            throughput_per_second: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for IngestionCoordinator
#[derive(Debug, Clone)]
pub struct IngestionCoordinatorConfig {
    pub enable_flow_control: bool,
    pub enable_performance_monitoring: bool,
    pub enable_resource_management: bool,
    pub enable_error_recovery: bool,
    pub max_concurrent_requests: u32,
    pub request_timeout_seconds: u64,
    pub max_retry_attempts: u32,
    pub retry_delay_seconds: u64,
    pub enable_circuit_breaker: bool,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_seconds: u64,
    pub enable_rate_limiting: bool,
    pub rate_limit_per_second: u32,
    pub enable_kv_fallback: bool,
    pub kv_fallback_ttl_seconds: u64,
    pub enable_metrics: bool,
    pub metrics_collection_interval_seconds: u64,
}

impl Default for IngestionCoordinatorConfig {
    fn default() -> Self {
        Self {
            enable_flow_control: true,
            enable_performance_monitoring: true,
            enable_resource_management: true,
            enable_error_recovery: true,
            max_concurrent_requests: 100,
            request_timeout_seconds: 30,
            max_retry_attempts: 3,
            retry_delay_seconds: 1,
            enable_circuit_breaker: true,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_seconds: 60,
            enable_rate_limiting: true,
            rate_limit_per_second: 1000,
            enable_kv_fallback: true,
            kv_fallback_ttl_seconds: 300, // 5 minutes
            enable_metrics: true,
            metrics_collection_interval_seconds: 30,
        }
    }
}

impl IngestionCoordinatorConfig {
    /// Create configuration optimized for high throughput
    pub fn high_throughput() -> Self {
        Self {
            max_concurrent_requests: 500,
            request_timeout_seconds: 15,
            rate_limit_per_second: 5000,
            circuit_breaker_threshold: 20,
            enable_performance_monitoring: true,
            enable_flow_control: true,
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            max_concurrent_requests: 50,
            request_timeout_seconds: 60,
            max_retry_attempts: 5,
            retry_delay_seconds: 2,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_seconds: 120,
            enable_error_recovery: true,
            enable_kv_fallback: true,
            kv_fallback_ttl_seconds: 600, // 10 minutes
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.max_concurrent_requests == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_requests must be greater than 0",
            ));
        }
        if self.request_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "request_timeout_seconds must be greater than 0",
            ));
        }
        if self.max_retry_attempts == 0 {
            return Err(ArbitrageError::validation_error(
                "max_retry_attempts must be greater than 0",
            ));
        }
        if self.circuit_breaker_threshold == 0 {
            return Err(ArbitrageError::validation_error(
                "circuit_breaker_threshold must be greater than 0",
            ));
        }
        if self.rate_limit_per_second == 0 {
            return Err(ArbitrageError::validation_error(
                "rate_limit_per_second must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Circuit breaker state for error recovery
#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker for managing service failures
#[derive(Debug)]
struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    threshold: u32,
    timeout_seconds: u64,
    last_failure_time: u64,
    success_count_in_half_open: u32,
}

impl CircuitBreaker {
    fn new(threshold: u32, timeout_seconds: u64) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            threshold,
            timeout_seconds,
            last_failure_time: 0,
            success_count_in_half_open: 0,
        }
    }

    fn can_execute(&mut self) -> bool {
        let current_time = chrono::Utc::now().timestamp_millis() as u64;

        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if current_time - self.last_failure_time > self.timeout_seconds * 1000 {
                    self.state = CircuitBreakerState::HalfOpen;
                    self.success_count_in_half_open = 0;
                    true
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.success_count_in_half_open += 1;
                if self.success_count_in_half_open >= 3 {
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                }
            }
            CircuitBreakerState::Open => {}
        }
    }

    fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = chrono::Utc::now().timestamp_millis() as u64;

        if self.failure_count >= self.threshold {
            self.state = CircuitBreakerState::Open;
        }
    }
}

/// Rate limiter for controlling request flow
#[derive(Debug)]
struct RateLimiter {
    requests_per_second: u32,
    window_start: u64,
    request_count: u32,
}

impl RateLimiter {
    fn new(requests_per_second: u32) -> Self {
        Self {
            requests_per_second,
            window_start: chrono::Utc::now().timestamp_millis() as u64,
            request_count: 0,
        }
    }

    fn can_proceed(&mut self) -> bool {
        let current_time = chrono::Utc::now().timestamp_millis() as u64;

        // Reset window if a second has passed
        if current_time - self.window_start >= 1000 {
            self.window_start = current_time;
            self.request_count = 0;
        }

        if self.request_count < self.requests_per_second {
            self.request_count += 1;
            true
        } else {
            false
        }
    }
}

/// Ingestion Coordinator for orchestrating data ingestion operations
pub struct IngestionCoordinator {
    config: IngestionCoordinatorConfig,
    logger: crate::utils::logger::Logger,

    // Component references
    pipeline_manager: Arc<PipelineManager>,
    queue_manager: Arc<QueueManager>,
    data_transformer: Arc<DataTransformer>,
    kv_store: KvStore,

    // Flow control
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    rate_limiter: Arc<Mutex<RateLimiter>>,

    // Metrics tracking
    metrics: Arc<Mutex<IngestionMetrics>>,

    // Active request tracking
    active_requests: Arc<Mutex<HashMap<String, u64>>>, // request_id -> start_time

    // Performance tracking
    startup_time: u64,
}

impl IngestionCoordinator {
    /// Create new IngestionCoordinator instance
    pub async fn new(
        config: IngestionCoordinatorConfig,
        kv_store: KvStore,
        pipeline_manager: Arc<PipelineManager>,
        queue_manager: Arc<QueueManager>,
        data_transformer: Arc<DataTransformer>,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize circuit breaker and rate limiter
        let circuit_breaker = Arc::new(Mutex::new(CircuitBreaker::new(
            config.circuit_breaker_threshold,
            config.circuit_breaker_timeout_seconds,
        )));

        let rate_limiter = Arc::new(Mutex::new(RateLimiter::new(config.rate_limit_per_second)));

        logger.info(&format!(
            "IngestionCoordinator initialized: max_concurrent={}, rate_limit={}/s, circuit_breaker_threshold={}",
            config.max_concurrent_requests, config.rate_limit_per_second, config.circuit_breaker_threshold
        ));

        Ok(Self {
            config,
            logger,
            pipeline_manager,
            queue_manager,
            data_transformer,
            kv_store,
            circuit_breaker,
            rate_limiter,
            metrics: Arc::new(Mutex::new(IngestionMetrics::default())),
            active_requests: Arc::new(Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Ingest a single event
    pub async fn ingest_event(&self, event: IngestionEvent) -> ArbitrageResult<()> {
        let request = IngestionRequest::new(event);
        let response = self.process_request(request).await?;

        if response.success {
            Ok(())
        } else {
            Err(ArbitrageError::internal_error(format!(
                "Ingestion failed: {:?}",
                response.errors
            )))
        }
    }

    /// Ingest multiple events in batch
    pub async fn ingest_batch(
        &self,
        events: Vec<IngestionEvent>,
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        let mut results = Vec::with_capacity(events.len());

        for event in events {
            let result = self.ingest_event(event).await;
            results.push(result);
        }

        Ok(results)
    }

    /// Process an ingestion request
    pub async fn process_request(
        &self,
        request: IngestionRequest,
    ) -> ArbitrageResult<IngestionResponse> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check rate limiting
        let should_rate_limit = {
            let mut limiter = self.rate_limiter.lock().unwrap();
            !limiter.can_proceed()
        };

        if should_rate_limit {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            return Err(ArbitrageError::rate_limit_error("Rate limit exceeded"));
        }

        // Check circuit breaker
        let should_circuit_break = {
            let mut breaker = self.circuit_breaker.lock().unwrap();
            !breaker.can_execute()
        };

        if should_circuit_break {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            return Err(ArbitrageError::service_unavailable(
                "Circuit breaker is open",
            ));
        }

        // Track active request
        {
            let mut active = self.active_requests.lock().unwrap();
            active.insert(request.request_id.clone(), start_time);
        }

        // Process the request
        let result = self.execute_ingestion(&request, start_time).await;

        // Remove from active requests
        {
            let mut active = self.active_requests.lock().unwrap();
            active.remove(&request.request_id);
        }

        // Update circuit breaker
        if self.config.enable_circuit_breaker {
            let mut breaker = self.circuit_breaker.lock().unwrap();
            match &result {
                Ok(_) => breaker.record_success(),
                Err(_) => breaker.record_failure(),
            }
        }

        result
    }

    /// Execute the ingestion process
    async fn execute_ingestion(
        &self,
        request: &IngestionRequest,
        start_time: u64,
    ) -> ArbitrageResult<IngestionResponse> {
        let mut processing_path = Vec::new();
        let mut pipeline_result = None;
        let mut queue_result = None;
        let mut transformation_result = None;
        let mut fallback_used = false;
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Step 1: Data Transformation (if enabled)
        if request.processing_options.use_transformer {
            processing_path.push("transformer".to_string());

            match self.transform_data(&request.event).await {
                Ok(result) => {
                    transformation_result =
                        Some(format!("Transformed to {}", result.target_format.as_str()));
                }
                Err(e) => {
                    errors.push(format!("Transformation failed: {}", e));
                    if !request.processing_options.enable_fallback {
                        return self
                            .create_error_response(request, e.to_string(), start_time)
                            .await;
                    }
                    warnings.push("Continuing without transformation".to_string());
                }
            }
        }

        // Step 2: Pipeline Processing (if enabled)
        if request.processing_options.use_pipeline {
            processing_path.push("pipeline".to_string());

            match self.process_pipeline(&request.event).await {
                Ok(_) => {
                    pipeline_result = Some("Successfully sent to pipeline".to_string());
                }
                Err(e) => {
                    errors.push(format!("Pipeline processing failed: {}", e));
                    if !request.processing_options.enable_fallback {
                        return self
                            .create_error_response(request, e.to_string(), start_time)
                            .await;
                    }
                    fallback_used = true;
                    warnings.push("Pipeline failed, trying queue fallback".to_string());
                }
            }
        }

        // Step 3: Queue Processing (if enabled or as fallback)
        if request.processing_options.use_queue
            || (fallback_used && request.processing_options.enable_fallback)
        {
            processing_path.push("queue".to_string());

            match self
                .process_queue(&request.event, &request.processing_options)
                .await
            {
                Ok(_) => {
                    queue_result = Some("Successfully sent to queue".to_string());
                }
                Err(e) => {
                    errors.push(format!("Queue processing failed: {}", e));
                    if !request.processing_options.enable_fallback {
                        return self
                            .create_error_response(request, e.to_string(), start_time)
                            .await;
                    }
                    fallback_used = true;
                    warnings.push("Queue failed, trying KV fallback".to_string());
                }
            }
        }

        // Step 4: KV Fallback (if all else fails)
        if fallback_used && self.config.enable_kv_fallback {
            processing_path.push("kv_fallback".to_string());

            match self.store_to_kv_fallback(&request.event).await {
                Ok(_) => {
                    warnings.push("Data stored to KV fallback".to_string());
                }
                Err(e) => {
                    errors.push(format!("KV fallback failed: {}", e));
                    return self
                        .create_error_response(
                            request,
                            "All processing methods failed".to_string(),
                            start_time,
                        )
                        .await;
                }
            }
        }

        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let success = errors.is_empty() || fallback_used;

        // Record metrics
        self.record_request_metrics(request, success, fallback_used, start_time)
            .await;

        Ok(IngestionResponse {
            request_id: request.request_id.clone(),
            success,
            processing_path,
            pipeline_result,
            queue_result,
            transformation_result,
            fallback_used,
            processing_time_ms: end_time - start_time,
            errors,
            warnings,
            metadata: request.metadata.clone(),
            timestamp: end_time,
        })
    }

    /// Transform data using DataTransformer
    async fn transform_data(
        &self,
        event: &IngestionEvent,
    ) -> ArbitrageResult<TransformationResponse> {
        let transformation_request = TransformationRequest::new(
            event.data.clone(),
            DataFormat::Json, // Assume JSON input
            DataFormat::Json, // Default to JSON output
        );

        self.data_transformer
            .transform(transformation_request)
            .await
    }

    /// Process data through pipeline
    async fn process_pipeline(&self, event: &IngestionEvent) -> ArbitrageResult<()> {
        let pipeline_type = match event.event_type {
            IngestionEventType::MarketData => PipelineType::MarketData,
            IngestionEventType::Analytics => PipelineType::Analytics,
            IngestionEventType::Audit => PipelineType::Audit,
            IngestionEventType::UserActivity => PipelineType::UserActivity,
            IngestionEventType::SystemMetrics => PipelineType::SystemMetrics,
            IngestionEventType::TradingSignals => PipelineType::TradingSignals,
            IngestionEventType::AIAnalysis => PipelineType::AIAnalysis,
            IngestionEventType::Custom(ref name) => PipelineType::Custom(name.clone()),
        };

        let pipeline_event = PipelineEvent::new(pipeline_type, event.data.clone())
            .with_metadata("source".to_string(), event.source.clone())
            .with_metadata("event_id".to_string(), event.event_id.clone());

        self.pipeline_manager.ingest_event(pipeline_event).await
    }

    /// Process data through queue
    async fn process_queue(
        &self,
        event: &IngestionEvent,
        options: &ProcessingOptions,
    ) -> ArbitrageResult<()> {
        let queue_type = options.priority.queue_type();

        let queue_message =
            QueueMessage::new(queue_type, options.priority.clone(), event.data.clone())
                .with_attribute("source".to_string(), event.source.clone())
                .with_attribute("event_id".to_string(), event.event_id.clone())
                .with_attribute(
                    "event_type".to_string(),
                    event.event_type.as_str().to_string(),
                );

        self.queue_manager.send_message(queue_message).await
    }

    /// Store data to KV as fallback
    async fn store_to_kv_fallback(&self, event: &IngestionEvent) -> ArbitrageResult<()> {
        // Create a safe event ID for the key to avoid exceeding Cloudflare's 2KB key limit
        let safe_event_id = if event.event_id.len() > 128 {
            // Hash the event_id if it's too long
            let mut hasher = DefaultHasher::new();
            event.event_id.hash(&mut hasher);
            format!("hash_{:x}", hasher.finish())
        } else {
            event.event_id.clone()
        };

        let key = format!("fallback:{}:{}", event.event_type.as_str(), safe_event_id);
        let value = serde_json::to_string(event)?;

        self.kv_store
            .put(&key, value)?
            .expiration_ttl(self.config.kv_fallback_ttl_seconds)
            .execute()
            .await
            .map_err(|e| ArbitrageError::internal_error(format!("KV store failed: {:?}", e)))?;

        Ok(())
    }

    /// Get ingestion metrics
    pub async fn get_metrics(&self) -> IngestionMetrics {
        let metrics = self.metrics.lock().unwrap();
        metrics.clone()
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Check if all components are healthy
        let pipeline_healthy = self.pipeline_manager.health_check().await.unwrap_or(false);
        let queue_healthy = self.queue_manager.health_check().await.unwrap_or(false);
        let transformer_healthy = self.data_transformer.health_check().await.unwrap_or(false);

        // At least one component must be healthy, or KV fallback must be available
        let is_healthy = pipeline_healthy
            || queue_healthy
            || transformer_healthy
            || self.config.enable_kv_fallback;

        Ok(is_healthy)
    }

    /// Create error response
    async fn create_error_response(
        &self,
        request: &IngestionRequest,
        error: String,
        start_time: u64,
    ) -> ArbitrageResult<IngestionResponse> {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        // Record failed request metrics
        self.record_request_metrics(request, false, false, start_time)
            .await;

        Ok(IngestionResponse {
            request_id: request.request_id.clone(),
            success: false,
            processing_path: vec!["error".to_string()],
            pipeline_result: None,
            queue_result: None,
            transformation_result: None,
            fallback_used: false,
            processing_time_ms: end_time - start_time,
            errors: vec![error],
            warnings: Vec::new(),
            metadata: request.metadata.clone(),
            timestamp: end_time,
        })
    }

    /// Record request metrics
    async fn record_request_metrics(
        &self,
        request: &IngestionRequest,
        success: bool,
        fallback_used: bool,
        start_time: u64,
    ) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let processing_time = end_time - start_time;

        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_requests += 1;

        if success {
            metrics.successful_requests += 1;
        } else {
            metrics.failed_requests += 1;
        }

        if request.processing_options.use_pipeline {
            metrics.pipeline_requests += 1;
        }
        if request.processing_options.use_queue {
            metrics.queue_requests += 1;
        }
        if request.processing_options.use_transformer {
            metrics.transformation_requests += 1;
        }
        if fallback_used {
            metrics.fallback_requests += 1;
        }

        // Update timing metrics
        metrics.average_processing_time_ms = (metrics.average_processing_time_ms
            * (metrics.total_requests - 1) as f64
            + processing_time as f64)
            / metrics.total_requests as f64;
        metrics.min_processing_time_ms = metrics.min_processing_time_ms.min(processing_time as f64);
        metrics.max_processing_time_ms = metrics.max_processing_time_ms.max(processing_time as f64);

        // Update categorical metrics
        *metrics
            .requests_by_event_type
            .entry(request.event.event_type.clone())
            .or_insert(0) += 1;
        *metrics
            .requests_by_priority
            .entry(request.processing_options.priority.clone())
            .or_insert(0) += 1;

        // Update error rate
        metrics.error_rate_percent =
            (metrics.failed_requests as f32 / metrics.total_requests as f32) * 100.0;

        // Update throughput (simplified calculation)
        let elapsed_seconds = (end_time - self.startup_time) as f64 / 1000.0;
        metrics.throughput_per_second = metrics.total_requests as f64 / elapsed_seconds.max(1.0);

        metrics.last_updated = end_time;
    }

    /// Get active request count
    pub async fn get_active_request_count(&self) -> u32 {
        let active = self.active_requests.lock().unwrap();
        active.len() as u32
    }

    /// Get circuit breaker status
    pub async fn get_circuit_breaker_status(&self) -> String {
        let breaker = self.circuit_breaker.lock().unwrap();
        format!("{:?}", breaker.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_options_defaults() {
        let options = ProcessingOptions::default();
        assert!(options.use_pipeline);
        assert!(options.use_queue);
        assert!(options.use_transformer);
        assert!(options.enable_fallback);
        assert_eq!(options.priority, MessagePriority::Normal);
    }

    #[test]
    fn test_processing_options_presets() {
        let pipeline_only = ProcessingOptions::pipeline_only();
        assert!(pipeline_only.use_pipeline);
        assert!(!pipeline_only.use_queue);
        assert!(!pipeline_only.use_transformer);

        let queue_only = ProcessingOptions::queue_only();
        assert!(!queue_only.use_pipeline);
        assert!(queue_only.use_queue);
        assert!(!queue_only.use_transformer);

        let high_priority = ProcessingOptions::high_priority();
        assert_eq!(high_priority.priority, MessagePriority::High);
        assert_eq!(high_priority.timeout_seconds, 15);
        assert_eq!(high_priority.retry_attempts, 5);
    }

    #[test]
    fn test_ingestion_request_creation() {
        let event = IngestionEvent::new(
            IngestionEventType::MarketData,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        let request =
            IngestionRequest::new(event.clone()).with_options(ProcessingOptions::high_priority());

        assert_eq!(request.event.event_type, IngestionEventType::MarketData);
        assert_eq!(request.processing_options.priority, MessagePriority::High);
    }

    #[test]
    fn test_ingestion_coordinator_config_validation() {
        let mut config = IngestionCoordinatorConfig::default();
        assert!(config.validate().is_ok());

        config.max_concurrent_requests = 0;
        assert!(config.validate().is_err());

        config.max_concurrent_requests = 100;
        config.rate_limit_per_second = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_throughput_config() {
        let config = IngestionCoordinatorConfig::high_throughput();
        assert_eq!(config.max_concurrent_requests, 500);
        assert_eq!(config.rate_limit_per_second, 5000);
        assert_eq!(config.request_timeout_seconds, 15);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = IngestionCoordinatorConfig::high_reliability();
        assert_eq!(config.max_retry_attempts, 5);
        assert_eq!(config.circuit_breaker_threshold, 5);
        assert!(config.enable_error_recovery);
        assert!(config.enable_kv_fallback);
    }

    #[test]
    fn test_circuit_breaker() {
        let mut breaker = CircuitBreaker::new(3, 60);

        // Initially closed
        assert!(breaker.can_execute());

        // Record failures
        breaker.record_failure();
        breaker.record_failure();
        assert!(breaker.can_execute()); // Still closed

        breaker.record_failure();
        assert!(!breaker.can_execute()); // Now open

        // Record success in half-open state would require time passage simulation
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(2);

        assert!(limiter.can_proceed());
        assert!(limiter.can_proceed());
        assert!(!limiter.can_proceed()); // Rate limit exceeded
    }
}
