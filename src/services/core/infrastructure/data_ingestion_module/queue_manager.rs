// Queue Manager - Cloudflare Queues Integration with Priority Processing
// Provides reliable message queuing with dead letter queues and retry mechanisms

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use worker::Env;

/// Queue types for different message categories
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueType {
    HighPriority,
    Standard,
    LowPriority,
    DeadLetter,
    Retry,
    Batch,
    Streaming,
    Custom(String),
}

impl QueueType {
    pub fn as_str(&self) -> &str {
        match self {
            QueueType::HighPriority => "high_priority",
            QueueType::Standard => "standard",
            QueueType::LowPriority => "low_priority",
            QueueType::DeadLetter => "dead_letter",
            QueueType::Retry => "retry",
            QueueType::Batch => "batch",
            QueueType::Streaming => "streaming",
            QueueType::Custom(name) => name,
        }
    }

    pub fn default_queue_name(&self) -> &str {
        match self {
            QueueType::HighPriority => "high-priority-queue",
            QueueType::Standard => "standard-queue",
            QueueType::LowPriority => "low-priority-queue",
            QueueType::DeadLetter => "dead-letter-queue",
            QueueType::Retry => "retry-queue",
            QueueType::Batch => "batch-queue",
            QueueType::Streaming => "streaming-queue",
            QueueType::Custom(_) => "custom-queue",
        }
    }

    pub fn max_retry_attempts(&self) -> u32 {
        match self {
            QueueType::HighPriority => 5,
            QueueType::Standard => 3,
            QueueType::LowPriority => 2,
            QueueType::DeadLetter => 0, // No retries for dead letter
            QueueType::Retry => 1,
            QueueType::Batch => 2,
            QueueType::Streaming => 1,
            QueueType::Custom(_) => 3,
        }
    }

    pub fn visibility_timeout_seconds(&self) -> u64 {
        match self {
            QueueType::HighPriority => 30,
            QueueType::Standard => 60,
            QueueType::LowPriority => 120,
            QueueType::DeadLetter => 300,
            QueueType::Retry => 60,
            QueueType::Batch => 300,
            QueueType::Streaming => 15,
            QueueType::Custom(_) => 60,
        }
    }
}

/// Message priority levels
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessagePriority {
    Low,
    Normal,
    High,
    Critical,
}

impl MessagePriority {
    pub fn as_u8(&self) -> u8 {
        match self {
            MessagePriority::Critical => 1,
            MessagePriority::High => 2,
            MessagePriority::Normal => 3,
            MessagePriority::Low => 4,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => MessagePriority::Critical,
            2 => MessagePriority::High,
            3 => MessagePriority::Normal,
            4 => MessagePriority::Low,
            _ => MessagePriority::Low,
        }
    }

    pub fn queue_type(&self) -> QueueType {
        match self {
            MessagePriority::Critical | MessagePriority::High => QueueType::HighPriority,
            MessagePriority::Normal => QueueType::Standard,
            MessagePriority::Low => QueueType::LowPriority,
        }
    }
}

/// Queue health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueHealth {
    pub is_healthy: bool,
    pub queues_available: bool,
    pub total_queues: u32,
    pub active_queues: u32,
    pub queue_depths: HashMap<QueueType, u64>,
    pub processing_rates: HashMap<QueueType, f64>,
    pub error_rates: HashMap<QueueType, f32>,
    pub last_success_timestamp: u64,
    pub last_error: Option<String>,
    pub average_processing_time_ms: f64,
    pub last_health_check: u64,
}

impl Default for QueueHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            queues_available: false,
            total_queues: 0,
            active_queues: 0,
            queue_depths: HashMap::new(),
            processing_rates: HashMap::new(),
            error_rates: HashMap::new(),
            last_success_timestamp: 0,
            last_error: None,
            average_processing_time_ms: 0.0,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Queue performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMetrics {
    pub total_messages_processed: u64,
    pub messages_per_second: f64,
    pub successful_processing: u64,
    pub failed_processing: u64,
    pub retried_messages: u64,
    pub dead_letter_messages: u64,
    pub average_processing_time_ms: f64,
    pub min_processing_time_ms: f64,
    pub max_processing_time_ms: f64,
    pub queue_utilization_percent: f32,
    pub messages_by_priority: HashMap<MessagePriority, u64>,
    pub messages_by_queue_type: HashMap<QueueType, u64>,
    pub batch_operations: u64,
    pub last_updated: u64,
}

impl Default for QueueMetrics {
    fn default() -> Self {
        Self {
            total_messages_processed: 0,
            messages_per_second: 0.0,
            successful_processing: 0,
            failed_processing: 0,
            retried_messages: 0,
            dead_letter_messages: 0,
            average_processing_time_ms: 0.0,
            min_processing_time_ms: f64::MAX,
            max_processing_time_ms: 0.0,
            queue_utilization_percent: 0.0,
            messages_by_priority: HashMap::new(),
            messages_by_queue_type: HashMap::new(),
            batch_operations: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for QueueManager
#[derive(Debug, Clone)]
pub struct QueueManagerConfig {
    pub enable_queues: bool,
    pub enable_priority_processing: bool,
    pub enable_dead_letter_queues: bool,
    pub enable_batch_processing: bool,
    pub enable_message_deduplication: bool,
    pub max_concurrent_processors: u32,
    pub batch_size: usize,
    pub batch_timeout_seconds: u64,
    pub message_retention_seconds: u64,
    pub visibility_timeout_seconds: u64,
    pub max_receive_count: u32,
    pub enable_fifo_queues: bool,
    pub enable_content_based_deduplication: bool,
    pub enable_health_monitoring: bool,
    pub health_check_interval_seconds: u64,
    pub account_id: String,
    pub api_token: String,
}

impl Default for QueueManagerConfig {
    fn default() -> Self {
        Self {
            enable_queues: true,
            enable_priority_processing: true,
            enable_dead_letter_queues: true,
            enable_batch_processing: true,
            enable_message_deduplication: true,
            max_concurrent_processors: 10,
            batch_size: 100,
            batch_timeout_seconds: 60,
            message_retention_seconds: 1209600, // 14 days
            visibility_timeout_seconds: 60,
            max_receive_count: 3,
            enable_fifo_queues: false,
            enable_content_based_deduplication: false,
            enable_health_monitoring: true,
            health_check_interval_seconds: 30,
            account_id: String::new(),
            api_token: String::new(),
        }
    }
}

impl QueueManagerConfig {
    /// Create configuration optimized for high throughput
    pub fn high_throughput() -> Self {
        Self {
            max_concurrent_processors: 25,
            batch_size: 250,
            batch_timeout_seconds: 30,
            enable_batch_processing: true,
            enable_priority_processing: true,
            visibility_timeout_seconds: 30,
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            max_concurrent_processors: 5,
            batch_size: 50,
            batch_timeout_seconds: 120,
            max_receive_count: 5,
            enable_dead_letter_queues: true,
            enable_message_deduplication: true,
            message_retention_seconds: 2592000, // 30 days
            health_check_interval_seconds: 15,
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.max_concurrent_processors == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_processors must be greater than 0",
            ));
        }
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
        if self.message_retention_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "message_retention_seconds must be greater than 0",
            ));
        }
        if self.visibility_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "visibility_timeout_seconds must be greater than 0",
            ));
        }
        if self.max_receive_count == 0 {
            return Err(ArbitrageError::validation_error(
                "max_receive_count must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Queue message for processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    pub message_id: String,
    pub queue_type: QueueType,
    pub priority: MessagePriority,
    pub body: serde_json::Value,
    pub attributes: HashMap<String, String>,
    pub timestamp: u64,
    pub receive_count: u32,
    pub max_receive_count: u32,
    pub visibility_timeout_seconds: u64,
    pub deduplication_id: Option<String>,
    pub group_id: Option<String>,
    pub delay_seconds: Option<u64>,
}

impl QueueMessage {
    pub fn new(queue_type: QueueType, priority: MessagePriority, body: serde_json::Value) -> Self {
        Self {
            message_id: uuid::Uuid::new_v4().to_string(),
            queue_type: queue_type.clone(),
            priority,
            body,
            attributes: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            receive_count: 0,
            max_receive_count: queue_type.max_retry_attempts(),
            visibility_timeout_seconds: queue_type.visibility_timeout_seconds(),
            deduplication_id: None,
            group_id: None,
            delay_seconds: None,
        }
    }

    pub fn with_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.insert(key, value);
        self
    }

    pub fn with_deduplication_id(mut self, deduplication_id: String) -> Self {
        self.deduplication_id = Some(deduplication_id);
        self
    }

    pub fn with_group_id(mut self, group_id: String) -> Self {
        self.group_id = Some(group_id);
        self
    }

    pub fn with_delay(mut self, delay_seconds: u64) -> Self {
        self.delay_seconds = Some(delay_seconds);
        self
    }

    pub fn can_retry(&self) -> bool {
        self.receive_count < self.max_receive_count
    }

    pub fn increment_receive_count(&mut self) {
        self.receive_count += 1;
    }

    pub fn should_move_to_dead_letter(&self) -> bool {
        self.receive_count >= self.max_receive_count
    }

    pub fn is_expired(&self, current_time: u64) -> bool {
        let age_seconds = (current_time - self.timestamp) / 1000;
        age_seconds > self.visibility_timeout_seconds
    }
}

/// Queue Manager for Cloudflare Queues integration
pub struct QueueManager {
    config: QueueManagerConfig,
    logger: crate::utils::logger::Logger,

    // Cloudflare credentials
    account_id: String,
    api_token: String,

    // Service availability
    queues_available: Arc<std::sync::Mutex<bool>>,

    // Health and metrics
    health: Arc<std::sync::Mutex<QueueHealth>>,
    metrics: Arc<std::sync::Mutex<QueueMetrics>>,

    // In-memory queue simulation (for fallback when Cloudflare Queues unavailable)
    local_queues: Arc<std::sync::Mutex<HashMap<QueueType, VecDeque<QueueMessage>>>>,
    dead_letter_queue: Arc<std::sync::Mutex<VecDeque<QueueMessage>>>,

    // Message deduplication
    message_hashes: Arc<std::sync::Mutex<HashMap<String, u64>>>, // hash -> timestamp

    // Performance tracking
    startup_time: u64,
}

impl QueueManager {
    /// Create new QueueManager instance
    pub async fn new(mut config: QueueManagerConfig, env: &Env) -> ArbitrageResult<Self> {
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
        let queues_available =
            !config.account_id.is_empty() && !config.api_token.is_empty() && config.enable_queues;

        if !queues_available && config.enable_queues {
            logger.warn(
                "Queues service disabled: missing Cloudflare credentials, using local fallback",
            );
        }

        // Initialize local queues for fallback
        let mut local_queues = HashMap::new();
        local_queues.insert(QueueType::HighPriority, VecDeque::new());
        local_queues.insert(QueueType::Standard, VecDeque::new());
        local_queues.insert(QueueType::LowPriority, VecDeque::new());
        local_queues.insert(QueueType::Retry, VecDeque::new());
        local_queues.insert(QueueType::Batch, VecDeque::new());
        local_queues.insert(QueueType::Streaming, VecDeque::new());

        logger.info(&format!(
            "QueueManager initialized: queues_enabled={}, priority_processing={}, batch_size={}",
            queues_available, config.enable_priority_processing, config.batch_size
        ));

        Ok(Self {
            account_id: config.account_id.clone(),
            api_token: config.api_token.clone(),
            config,
            logger,
            queues_available: Arc::new(std::sync::Mutex::new(queues_available)),
            health: Arc::new(std::sync::Mutex::new(QueueHealth::default())),
            metrics: Arc::new(std::sync::Mutex::new(QueueMetrics::default())),
            local_queues: Arc::new(std::sync::Mutex::new(local_queues)),
            dead_letter_queue: Arc::new(std::sync::Mutex::new(VecDeque::new())),
            message_hashes: Arc::new(std::sync::Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Send a message to queue
    pub async fn send_message(&self, message: QueueMessage) -> ArbitrageResult<()> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check for message deduplication
        if self.config.enable_message_deduplication {
            if let Some(dedup_id) = &message.deduplication_id {
                if self.is_duplicate_message(dedup_id).await {
                    self.logger
                        .debug(&format!("Duplicate message detected: {}", dedup_id));
                    return Ok(()); // Silently ignore duplicates
                }
            }
        }

        // Process the message
        match self.process_queue_message(&message).await {
            Ok(_) => {
                self.record_success(&message.queue_type, &message.priority, start_time)
                    .await;
                Ok(())
            }
            Err(e) => {
                self.record_failure(&message.queue_type, &message.priority, start_time, &e)
                    .await;
                Err(e)
            }
        }
    }

    /// Send multiple messages in batch
    pub async fn send_batch(
        &self,
        messages: Vec<QueueMessage>,
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        if messages.is_empty() {
            return Ok(vec![]);
        }

        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let mut results = Vec::with_capacity(messages.len());

        // Process messages in batches
        for chunk in messages.chunks(self.config.batch_size) {
            for message in chunk {
                let result = self.send_message(message.clone()).await;
                results.push(result);
            }
        }

        self.record_batch_operation(start_time).await;
        Ok(results)
    }

    /// Receive messages from queue
    pub async fn receive_messages(
        &self,
        queue_type: QueueType,
        max_messages: u32,
    ) -> ArbitrageResult<Vec<QueueMessage>> {
        if self.is_queues_available().await {
            // Use Cloudflare Queues
            self.receive_from_cloudflare_queue(&queue_type, max_messages)
                .await
        } else {
            // Use local fallback queue
            self.receive_from_local_queue(&queue_type, max_messages)
                .await
        }
    }

    /// Delete a processed message
    pub async fn delete_message(&self, message: &QueueMessage) -> ArbitrageResult<()> {
        if self.is_queues_available().await {
            // Delete from Cloudflare Queues
            self.delete_from_cloudflare_queue(message).await
        } else {
            // Delete from local queue (already removed when received)
            Ok(())
        }
    }

    /// Move message to dead letter queue
    pub async fn move_to_dead_letter(&self, message: QueueMessage) -> ArbitrageResult<()> {
        let mut dead_letter_message = message;
        dead_letter_message.queue_type = QueueType::DeadLetter;
        dead_letter_message.timestamp = chrono::Utc::now().timestamp_millis() as u64;

        if let Ok(mut dlq) = self.dead_letter_queue.lock() {
            dlq.push_back(dead_letter_message);
        }

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.dead_letter_messages += 1;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        Ok(())
    }

    /// Check if queues service is available
    pub async fn is_queues_available(&self) -> bool {
        if let Ok(available) = self.queues_available.lock() {
            *available
        } else {
            false
        }
    }

    /// Get queue health status
    pub async fn get_health(&self) -> QueueHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            QueueHealth::default()
        }
    }

    /// Get queue metrics
    pub async fn get_metrics(&self) -> QueueMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            QueueMetrics::default()
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Test queue availability
        let queues_healthy = self.test_queue_connection().await;

        // Update availability status
        if let Ok(mut available) = self.queues_available.lock() {
            *available = queues_healthy;
        }

        // Calculate queue depths and processing rates
        let queue_depths = self.calculate_queue_depths().await;
        let processing_rates = self.calculate_processing_rates().await;

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = queues_healthy || !queue_depths.is_empty(); // Healthy if queues available or local queues working
            health.queues_available = queues_healthy;
            health.queue_depths = queue_depths;
            health.processing_rates = processing_rates;
            health.last_health_check = chrono::Utc::now().timestamp_millis() as u64;

            if health.is_healthy {
                health.last_success_timestamp = start_time;
                health.last_error = None;
            } else {
                health.last_error = Some("Queue services unavailable".to_string());
            }
        }

        Ok(queues_healthy || !self.local_queues.lock().unwrap().is_empty())
    }

    /// Process a queue message
    async fn process_queue_message(&self, message: &QueueMessage) -> ArbitrageResult<()> {
        if self.is_queues_available().await {
            // Send to Cloudflare Queues
            self.send_to_cloudflare_queue(message).await
        } else {
            // Send to local fallback queue
            self.send_to_local_queue(message).await
        }
    }

    /// Send message to Cloudflare Queue
    async fn send_to_cloudflare_queue(&self, message: &QueueMessage) -> ArbitrageResult<()> {
        // In a real implementation, this would use the Cloudflare Queues API
        self.logger.info(&format!(
            "Sending message {} to Cloudflare queue {}",
            message.message_id,
            message.queue_type.as_str()
        ));
        Ok(())
    }

    /// Send message to local fallback queue
    async fn send_to_local_queue(&self, message: &QueueMessage) -> ArbitrageResult<()> {
        if let Ok(mut queues) = self.local_queues.lock() {
            if let Some(queue) = queues.get_mut(&message.queue_type) {
                queue.push_back(message.clone());
                self.logger.debug(&format!(
                    "Message {} added to local queue {}",
                    message.message_id,
                    message.queue_type.as_str()
                ));
            } else {
                return Err(ArbitrageError::validation_error(format!(
                    "Queue type {:?} not found",
                    message.queue_type
                )));
            }
        }
        Ok(())
    }

    /// Receive messages from Cloudflare Queue
    async fn receive_from_cloudflare_queue(
        &self,
        queue_type: &QueueType,
        max_messages: u32,
    ) -> ArbitrageResult<Vec<QueueMessage>> {
        // In a real implementation, this would use the Cloudflare Queues API
        self.logger.info(&format!(
            "Receiving {} messages from Cloudflare queue {}",
            max_messages,
            queue_type.as_str()
        ));
        Ok(vec![])
    }

    /// Receive messages from local fallback queue
    async fn receive_from_local_queue(
        &self,
        queue_type: &QueueType,
        max_messages: u32,
    ) -> ArbitrageResult<Vec<QueueMessage>> {
        let mut messages = Vec::new();

        if let Ok(mut queues) = self.local_queues.lock() {
            if let Some(queue) = queues.get_mut(queue_type) {
                for _ in 0..max_messages {
                    if let Some(mut message) = queue.pop_front() {
                        message.increment_receive_count();
                        messages.push(message);
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(messages)
    }

    /// Delete message from Cloudflare Queue
    async fn delete_from_cloudflare_queue(&self, message: &QueueMessage) -> ArbitrageResult<()> {
        // In a real implementation, this would use the Cloudflare Queues API
        self.logger.info(&format!(
            "Deleting message {} from Cloudflare queue",
            message.message_id
        ));
        Ok(())
    }

    /// Test queue connection
    async fn test_queue_connection(&self) -> bool {
        if self.account_id.is_empty() || self.api_token.is_empty() {
            return false;
        }

        // In a real implementation, this would test the Cloudflare Queues API
        // For now, we'll simulate a successful connection if credentials are present
        true
    }

    /// Check if message is duplicate
    async fn is_duplicate_message(&self, deduplication_id: &str) -> bool {
        if let Ok(mut hashes) = self.message_hashes.lock() {
            let current_time = chrono::Utc::now().timestamp_millis() as u64;
            let hash = self.calculate_message_hash(deduplication_id);

            // Clean up old hashes (older than 5 minutes)
            hashes.retain(|_, &mut timestamp| current_time - timestamp < 300000);

            if hashes.contains_key(&hash) {
                return true;
            }

            hashes.insert(hash, current_time);
        }
        false
    }

    /// Calculate message hash for deduplication
    fn calculate_message_hash(&self, content: &str) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(content.as_bytes()))
    }

    /// Calculate queue depths
    async fn calculate_queue_depths(&self) -> HashMap<QueueType, u64> {
        let mut depths = HashMap::new();

        if let Ok(queues) = self.local_queues.lock() {
            for (queue_type, queue) in queues.iter() {
                depths.insert(queue_type.clone(), queue.len() as u64);
            }
        }

        depths
    }

    /// Calculate processing rates
    async fn calculate_processing_rates(&self) -> HashMap<QueueType, f64> {
        let mut rates = HashMap::new();

        // In a real implementation, this would calculate actual processing rates
        // For now, we'll return placeholder values
        rates.insert(QueueType::HighPriority, 100.0);
        rates.insert(QueueType::Standard, 50.0);
        rates.insert(QueueType::LowPriority, 25.0);

        rates
    }

    /// Record successful operation
    async fn record_success(
        &self,
        queue_type: &QueueType,
        priority: &MessagePriority,
        start_time: u64,
    ) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let processing_time = end_time - start_time;

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_messages_processed += 1;
            metrics.successful_processing += 1;
            metrics.average_processing_time_ms = (metrics.average_processing_time_ms
                * (metrics.total_messages_processed - 1) as f64
                + processing_time as f64)
                / metrics.total_messages_processed as f64;
            metrics.min_processing_time_ms =
                metrics.min_processing_time_ms.min(processing_time as f64);
            metrics.max_processing_time_ms =
                metrics.max_processing_time_ms.max(processing_time as f64);

            *metrics
                .messages_by_priority
                .entry(priority.clone())
                .or_insert(0) += 1;
            *metrics
                .messages_by_queue_type
                .entry(queue_type.clone())
                .or_insert(0) += 1;
            metrics.last_updated = end_time;
        }

        if let Ok(mut health) = self.health.lock() {
            health.last_success_timestamp = end_time;
        }
    }

    /// Record failed operation
    async fn record_failure(
        &self,
        _queue_type: &QueueType,
        _priority: &MessagePriority,
        start_time: u64,
        error: &ArbitrageError,
    ) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let processing_time = end_time - start_time;

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_messages_processed += 1;
            metrics.failed_processing += 1;
            metrics.average_processing_time_ms = (metrics.average_processing_time_ms
                * (metrics.total_messages_processed - 1) as f64
                + processing_time as f64)
                / metrics.total_messages_processed as f64;
            metrics.min_processing_time_ms =
                metrics.min_processing_time_ms.min(processing_time as f64);
            metrics.max_processing_time_ms =
                metrics.max_processing_time_ms.max(processing_time as f64);
            metrics.last_updated = end_time;
        }

        if let Ok(mut health) = self.health.lock() {
            health.last_error = Some(error.to_string());
        }
    }

    /// Record batch operation
    async fn record_batch_operation(&self, _start_time: u64) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.batch_operations += 1;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_type_defaults() {
        assert_eq!(QueueType::HighPriority.as_str(), "high_priority");
        assert_eq!(QueueType::Standard.default_queue_name(), "standard-queue");
        assert_eq!(QueueType::HighPriority.max_retry_attempts(), 5);
        assert_eq!(QueueType::LowPriority.visibility_timeout_seconds(), 120);
    }

    #[test]
    fn test_message_priority() {
        assert_eq!(MessagePriority::Critical.as_u8(), 1);
        assert_eq!(MessagePriority::from_u8(3), MessagePriority::Normal);
        assert_eq!(MessagePriority::High.queue_type(), QueueType::HighPriority);
        assert_eq!(MessagePriority::Low.queue_type(), QueueType::LowPriority);
    }

    #[test]
    fn test_queue_message_creation() {
        let body = serde_json::json!({"test": "data"});
        let message = QueueMessage::new(QueueType::Standard, MessagePriority::Normal, body.clone());

        assert_eq!(message.queue_type, QueueType::Standard);
        assert_eq!(message.priority, MessagePriority::Normal);
        assert_eq!(message.body, body);
        assert_eq!(message.receive_count, 0);
        assert!(message.can_retry());
    }

    #[test]
    fn test_queue_message_retry_logic() {
        let mut message = QueueMessage::new(
            QueueType::Standard,
            MessagePriority::Normal,
            serde_json::json!({}),
        );

        assert!(message.can_retry());
        assert!(!message.should_move_to_dead_letter());

        // Simulate multiple receives
        for _ in 0..3 {
            message.increment_receive_count();
        }

        assert!(!message.can_retry());
        assert!(message.should_move_to_dead_letter());
    }

    #[test]
    fn test_queue_manager_config_validation() {
        let mut config = QueueManagerConfig::default();
        assert!(config.validate().is_ok());

        config.max_concurrent_processors = 0;
        assert!(config.validate().is_err());

        config.max_concurrent_processors = 10;
        config.batch_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_throughput_config() {
        let config = QueueManagerConfig::high_throughput();
        assert_eq!(config.max_concurrent_processors, 25);
        assert_eq!(config.batch_size, 250);
        assert_eq!(config.batch_timeout_seconds, 30);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = QueueManagerConfig::high_reliability();
        assert_eq!(config.max_receive_count, 5);
        assert!(config.enable_dead_letter_queues);
        assert_eq!(config.health_check_interval_seconds, 15);
    }
}
