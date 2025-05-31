// Delivery Manager - Reliable Message Delivery with Multi-Channel Support and Analytics
// Part of Notification Module replacing notifications.rs

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Notification channels for delivery
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    Telegram,
    Email,
    Push,
    Webhook,
    Sms,
    Slack,
    Discord,
    Custom(String),
}

impl NotificationChannel {
    pub fn as_str(&self) -> &str {
        match self {
            NotificationChannel::Telegram => "telegram",
            NotificationChannel::Email => "email",
            NotificationChannel::Push => "push",
            NotificationChannel::Webhook => "webhook",
            NotificationChannel::Sms => "sms",
            NotificationChannel::Slack => "slack",
            NotificationChannel::Discord => "discord",
            NotificationChannel::Custom(name) => name,
        }
    }

    pub fn default_priority(&self) -> u8 {
        match self {
            NotificationChannel::Telegram => 9,
            NotificationChannel::Push => 8,
            NotificationChannel::Email => 7,
            NotificationChannel::Sms => 6,
            NotificationChannel::Slack => 5,
            NotificationChannel::Discord => 4,
            NotificationChannel::Webhook => 3,
            NotificationChannel::Custom(_) => 5,
        }
    }

    pub fn supports_rich_content(&self) -> bool {
        matches!(
            self,
            NotificationChannel::Telegram
                | NotificationChannel::Email
                | NotificationChannel::Push
                | NotificationChannel::Slack
                | NotificationChannel::Discord
        )
    }

    pub fn max_message_length(&self) -> usize {
        match self {
            NotificationChannel::Telegram => 4096,
            NotificationChannel::Email => 1048576, // 1MB
            NotificationChannel::Push => 256,
            NotificationChannel::Webhook => 1048576,
            NotificationChannel::Sms => 160,
            NotificationChannel::Slack => 40000,
            NotificationChannel::Discord => 2000,
            NotificationChannel::Custom(_) => 4096,
        }
    }
}

/// Delivery status for tracking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Processing,
    Sent,
    Delivered,
    Failed,
    Cancelled,
    Expired,
    Retrying,
}

impl DeliveryStatus {
    pub fn as_str(&self) -> &str {
        match self {
            DeliveryStatus::Pending => "pending",
            DeliveryStatus::Processing => "processing",
            DeliveryStatus::Sent => "sent",
            DeliveryStatus::Delivered => "delivered",
            DeliveryStatus::Failed => "failed",
            DeliveryStatus::Cancelled => "cancelled",
            DeliveryStatus::Expired => "expired",
            DeliveryStatus::Retrying => "retrying",
        }
    }

    pub fn is_final(&self) -> bool {
        matches!(
            self,
            DeliveryStatus::Delivered
                | DeliveryStatus::Failed
                | DeliveryStatus::Cancelled
                | DeliveryStatus::Expired
        )
    }

    pub fn is_successful(&self) -> bool {
        matches!(self, DeliveryStatus::Delivered | DeliveryStatus::Sent)
    }
}

/// Delivery attempt record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    pub attempt_id: String,
    pub notification_id: String,
    pub channel: NotificationChannel,
    pub attempt_number: u32,
    pub status: DeliveryStatus,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub duration_ms: Option<u64>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub response_data: Option<String>,
    pub retry_after_seconds: Option<u64>,
    pub metadata: HashMap<String, String>,
}

impl DeliveryAttempt {
    pub fn new(notification_id: String, channel: NotificationChannel, attempt_number: u32) -> Self {
        Self {
            attempt_id: uuid::Uuid::new_v4().to_string(),
            notification_id,
            channel,
            attempt_number,
            status: DeliveryStatus::Pending,
            started_at: chrono::Utc::now().timestamp_millis() as u64,
            completed_at: None,
            duration_ms: None,
            error_message: None,
            error_code: None,
            response_data: None,
            retry_after_seconds: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn start_processing(&mut self) {
        self.status = DeliveryStatus::Processing;
        self.started_at = chrono::Utc::now().timestamp_millis() as u64;
    }

    pub fn complete_success(&mut self, response_data: Option<String>) {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        self.status = DeliveryStatus::Delivered;
        self.completed_at = Some(now);
        self.duration_ms = Some(now - self.started_at);
        self.response_data = response_data;
    }

    pub fn complete_failure(
        &mut self,
        error_message: String,
        error_code: Option<String>,
        retry_after: Option<u64>,
    ) {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        self.status = if retry_after.is_some() {
            DeliveryStatus::Retrying
        } else {
            DeliveryStatus::Failed
        };
        self.completed_at = Some(now);
        self.duration_ms = Some(now - self.started_at);
        self.error_message = Some(error_message);
        self.error_code = error_code;
        self.retry_after_seconds = retry_after;
    }

    pub fn get_duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }
}

/// Delivery request for processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryRequest {
    pub request_id: String,
    pub notification_id: String,
    pub user_id: String,
    pub channel: NotificationChannel,
    pub priority: u8,
    pub subject: Option<String>,
    pub title: Option<String>,
    pub content: String,
    pub format: String,    // "plain_text", "html", "markdown", "json"
    pub recipient: String, // email, phone, user_id, webhook_url, etc.
    pub attachments: Vec<DeliveryAttachment>,
    pub metadata: HashMap<String, String>,
    pub retry_config: RetryConfig,
    pub expires_at: Option<u64>,
    pub created_at: u64,
}

impl DeliveryRequest {
    pub fn new(
        notification_id: String,
        user_id: String,
        channel: NotificationChannel,
        content: String,
        recipient: String,
    ) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            notification_id,
            user_id,
            channel,
            priority: 5,
            subject: None,
            title: None,
            content,
            format: "plain_text".to_string(),
            recipient,
            attachments: Vec::new(),
            metadata: HashMap::new(),
            retry_config: RetryConfig::default(),
            expires_at: None,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_subject(mut self, subject: String) -> Self {
        self.subject = Some(subject);
        self
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_format(mut self, format: String) -> Self {
        self.format = format;
        self
    }

    pub fn with_attachment(mut self, attachment: DeliveryAttachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    pub fn with_expiry(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            chrono::Utc::now().timestamp_millis() as u64 > expires_at
        } else {
            false
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.content.is_empty() {
            return Err(ArbitrageError::validation_error("Content cannot be empty"));
        }

        if self.recipient.is_empty() {
            return Err(ArbitrageError::validation_error(
                "Recipient cannot be empty",
            ));
        }

        let max_length = self.channel.max_message_length();
        if self.content.len() > max_length {
            return Err(ArbitrageError::validation_error(&format!(
                "Content too long for {}: {} > {} characters",
                self.channel.as_str(),
                self.content.len(),
                max_length
            )));
        }

        if self.is_expired() {
            return Err(ArbitrageError::validation_error(
                "Delivery request has expired",
            ));
        }

        Ok(())
    }
}

/// Delivery attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttachment {
    pub attachment_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub url: Option<String>,
    pub content: Option<String>, // Base64 encoded for small attachments
    pub metadata: HashMap<String, String>,
}

impl DeliveryAttachment {
    pub fn new(filename: String, content_type: String, size_bytes: u64) -> Self {
        Self {
            attachment_id: uuid::Uuid::new_v4().to_string(),
            filename,
            content_type,
            size_bytes,
            url: None,
            content: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_url(mut self, url: String) -> Self {
        self.url = Some(url);
        self
    }

    pub fn with_content(mut self, content: String) -> Self {
        self.content = Some(content);
        self
    }
}

/// Retry configuration for failed deliveries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_seconds: u64,
    pub max_delay_seconds: u64,
    pub backoff_multiplier: f64,
    pub retry_on_errors: Vec<String>, // Error codes to retry on
    pub stop_on_errors: Vec<String>,  // Error codes to stop retrying on
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_seconds: 30,
            max_delay_seconds: 3600, // 1 hour
            backoff_multiplier: 2.0,
            retry_on_errors: vec![
                "timeout".to_string(),
                "rate_limit".to_string(),
                "server_error".to_string(),
                "network_error".to_string(),
            ],
            stop_on_errors: vec![
                "invalid_recipient".to_string(),
                "permission_denied".to_string(),
                "content_rejected".to_string(),
            ],
        }
    }
}

impl RetryConfig {
    pub fn should_retry(&self, attempt_number: u32, error_code: Option<&str>) -> bool {
        if attempt_number >= self.max_attempts {
            return false;
        }

        if let Some(code) = error_code {
            if self.stop_on_errors.contains(&code.to_string()) {
                return false;
            }
            if !self.retry_on_errors.is_empty() && !self.retry_on_errors.contains(&code.to_string())
            {
                return false;
            }
        }

        true
    }

    pub fn calculate_delay(&self, attempt_number: u32) -> u64 {
        let delay = self.initial_delay_seconds as f64
            * self.backoff_multiplier.powi(attempt_number as i32 - 1);
        delay.min(self.max_delay_seconds as f64) as u64
    }
}

/// Delivery manager health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryManagerHealth {
    pub is_healthy: bool,
    pub active_deliveries: u64,
    pub pending_deliveries: u64,
    pub failed_deliveries: u64,
    pub delivery_success_rate_percent: f32,
    pub avg_delivery_time_ms: f64,
    pub channel_health: HashMap<NotificationChannel, bool>,
    pub rate_limit_status: HashMap<NotificationChannel, RateLimitStatus>,
    pub kv_store_available: bool,
    pub last_health_check: u64,
    pub last_error: Option<String>,
}

impl Default for DeliveryManagerHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            active_deliveries: 0,
            pending_deliveries: 0,
            failed_deliveries: 0,
            delivery_success_rate_percent: 0.0,
            avg_delivery_time_ms: 0.0,
            channel_health: HashMap::new(),
            rate_limit_status: HashMap::new(),
            kv_store_available: false,
            last_health_check: 0,
            last_error: None,
        }
    }
}

/// Rate limit status for channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    pub requests_remaining: u32,
    pub reset_time: u64,
    pub is_limited: bool,
}

/// Delivery manager performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryManagerMetrics {
    pub total_deliveries: u64,
    pub deliveries_per_second: f64,
    pub successful_deliveries: u64,
    pub failed_deliveries: u64,
    pub retried_deliveries: u64,
    pub avg_delivery_time_ms: f64,
    pub min_delivery_time_ms: f64,
    pub max_delivery_time_ms: f64,
    pub deliveries_by_channel: HashMap<NotificationChannel, u64>,
    pub success_rate_by_channel: HashMap<NotificationChannel, f32>,
    pub avg_delivery_time_by_channel: HashMap<NotificationChannel, f64>,
    pub deliveries_by_priority: HashMap<u8, u64>,
    pub error_counts_by_code: HashMap<String, u64>,
    pub retry_statistics: RetryStatistics,
    pub last_updated: u64,
}

impl Default for DeliveryManagerMetrics {
    fn default() -> Self {
        Self {
            total_deliveries: 0,
            deliveries_per_second: 0.0,
            successful_deliveries: 0,
            failed_deliveries: 0,
            retried_deliveries: 0,
            avg_delivery_time_ms: 0.0,
            min_delivery_time_ms: f64::MAX,
            max_delivery_time_ms: 0.0,
            deliveries_by_channel: HashMap::new(),
            success_rate_by_channel: HashMap::new(),
            avg_delivery_time_by_channel: HashMap::new(),
            deliveries_by_priority: HashMap::new(),
            error_counts_by_code: HashMap::new(),
            retry_statistics: RetryStatistics::default(),
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Retry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStatistics {
    pub total_retries: u64,
    pub successful_retries: u64,
    pub failed_retries: u64,
    pub avg_retries_per_delivery: f64,
    pub max_retries_used: u32,
}

impl Default for RetryStatistics {
    fn default() -> Self {
        Self {
            total_retries: 0,
            successful_retries: 0,
            failed_retries: 0,
            avg_retries_per_delivery: 0.0,
            max_retries_used: 0,
        }
    }
}

/// Configuration for DeliveryManager
#[derive(Debug, Clone)]
pub struct DeliveryManagerConfig {
    pub enable_delivery: bool,
    pub enable_retry_logic: bool,
    pub enable_rate_limiting: bool,
    pub enable_batch_delivery: bool,
    pub batch_size: usize,
    pub max_concurrent_deliveries: usize,
    pub delivery_timeout_seconds: u64,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_delivery_analytics: bool,
    pub analytics_retention_days: u32,
    pub enable_fallback_channels: bool,
    pub fallback_channel_map: HashMap<NotificationChannel, Vec<NotificationChannel>>,
    pub channel_configs: HashMap<NotificationChannel, ChannelConfig>,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
}

impl Default for DeliveryManagerConfig {
    fn default() -> Self {
        let mut fallback_map = HashMap::new();
        fallback_map.insert(
            NotificationChannel::Push,
            vec![NotificationChannel::Email, NotificationChannel::Telegram],
        );
        fallback_map.insert(
            NotificationChannel::Email,
            vec![NotificationChannel::Telegram],
        );

        let mut channel_configs = HashMap::new();
        channel_configs.insert(NotificationChannel::Telegram, ChannelConfig::telegram());
        channel_configs.insert(NotificationChannel::Email, ChannelConfig::email());
        channel_configs.insert(NotificationChannel::Push, ChannelConfig::push());

        Self {
            enable_delivery: true,
            enable_retry_logic: true,
            enable_rate_limiting: true,
            enable_batch_delivery: true,
            batch_size: 50,
            max_concurrent_deliveries: 100,
            delivery_timeout_seconds: 30,
            enable_kv_storage: true,
            kv_key_prefix: "delivery:".to_string(),
            enable_delivery_analytics: true,
            analytics_retention_days: 30,
            enable_fallback_channels: true,
            fallback_channel_map: fallback_map,
            channel_configs,
            enable_compression: true,
            compression_threshold_bytes: 1024,
        }
    }
}

impl DeliveryManagerConfig {
    pub fn high_performance() -> Self {
        Self {
            batch_size: 100,
            max_concurrent_deliveries: 200,
            delivery_timeout_seconds: 15,
            enable_batch_delivery: true,
            enable_compression: true,
            ..Default::default()
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            batch_size: 25,
            max_concurrent_deliveries: 50,
            delivery_timeout_seconds: 60,
            enable_retry_logic: true,
            enable_fallback_channels: true,
            analytics_retention_days: 90,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "batch_size must be greater than 0",
            ));
        }
        if self.max_concurrent_deliveries == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_deliveries must be greater than 0",
            ));
        }
        if self.delivery_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "delivery_timeout_seconds must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Channel-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub enabled: bool,
    pub rate_limit_per_minute: u32,
    pub rate_limit_per_hour: u32,
    pub rate_limit_per_day: u32,
    pub timeout_seconds: u64,
    pub retry_config: RetryConfig,
    pub endpoint_url: Option<String>,
    pub api_key: Option<String>,
    pub additional_headers: HashMap<String, String>,
    pub metadata: HashMap<String, String>,
}

impl ChannelConfig {
    pub fn telegram() -> Self {
        Self {
            enabled: true,
            rate_limit_per_minute: 30,
            rate_limit_per_hour: 1000,
            rate_limit_per_day: 10000,
            timeout_seconds: 30,
            retry_config: RetryConfig::default(),
            endpoint_url: Some("https://api.telegram.org".to_string()),
            api_key: None,
            additional_headers: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn email() -> Self {
        Self {
            enabled: true,
            rate_limit_per_minute: 10,
            rate_limit_per_hour: 100,
            rate_limit_per_day: 1000,
            timeout_seconds: 60,
            retry_config: RetryConfig::default(),
            endpoint_url: None,
            api_key: None,
            additional_headers: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn push() -> Self {
        Self {
            enabled: true,
            rate_limit_per_minute: 100,
            rate_limit_per_hour: 5000,
            rate_limit_per_day: 50000,
            timeout_seconds: 15,
            retry_config: RetryConfig::default(),
            endpoint_url: None,
            api_key: None,
            additional_headers: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Delivery Manager for reliable message delivery
pub struct DeliveryManager {
    config: DeliveryManagerConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Delivery tracking
    active_deliveries: Arc<Mutex<HashMap<String, DeliveryAttempt>>>,
    delivery_queue: Arc<Mutex<Vec<DeliveryRequest>>>,

    // Rate limiting
    rate_limiters: Arc<Mutex<HashMap<NotificationChannel, RateLimiter>>>,

    // Health and performance tracking
    health: Arc<Mutex<DeliveryManagerHealth>>,
    metrics: Arc<Mutex<DeliveryManagerMetrics>>,

    // Performance tracking
    startup_time: u64,
}

/// Simple rate limiter implementation
#[derive(Debug, Clone)]
pub struct RateLimiter {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub requests_per_day: u32,
    pub minute_count: u32,
    pub hour_count: u32,
    pub day_count: u32,
    pub last_minute_reset: u64,
    pub last_hour_reset: u64,
    pub last_day_reset: u64,
}

impl RateLimiter {
    pub fn new(per_minute: u32, per_hour: u32, per_day: u32) -> Self {
        let now = chrono::Utc::now().timestamp() as u64;
        Self {
            requests_per_minute: per_minute,
            requests_per_hour: per_hour,
            requests_per_day: per_day,
            minute_count: 0,
            hour_count: 0,
            day_count: 0,
            last_minute_reset: now / 60,
            last_hour_reset: now / 3600,
            last_day_reset: now / 86400,
        }
    }

    pub fn can_proceed(&mut self) -> bool {
        let now = chrono::Utc::now().timestamp() as u64;

        // Reset counters if time windows have passed
        let current_minute = now / 60;
        let current_hour = now / 3600;
        let current_day = now / 86400;

        if current_minute > self.last_minute_reset {
            self.minute_count = 0;
            self.last_minute_reset = current_minute;
        }

        if current_hour > self.last_hour_reset {
            self.hour_count = 0;
            self.last_hour_reset = current_hour;
        }

        if current_day > self.last_day_reset {
            self.day_count = 0;
            self.last_day_reset = current_day;
        }

        // Check limits
        self.minute_count < self.requests_per_minute
            && self.hour_count < self.requests_per_hour
            && self.day_count < self.requests_per_day
    }

    pub fn consume(&mut self) {
        self.minute_count += 1;
        self.hour_count += 1;
        self.day_count += 1;
    }

    pub fn get_status(&self) -> RateLimitStatus {
        RateLimitStatus {
            requests_remaining: self.requests_per_minute.saturating_sub(self.minute_count),
            reset_time: (self.last_minute_reset + 1) * 60,
            is_limited: self.minute_count >= self.requests_per_minute,
        }
    }
}

impl DeliveryManager {
    /// Create new DeliveryManager instance
    pub async fn new(
        config: DeliveryManagerConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize rate limiters
        let mut rate_limiters = HashMap::new();
        for (channel, channel_config) in &config.channel_configs {
            if channel_config.enabled {
                rate_limiters.insert(
                    channel.clone(),
                    RateLimiter::new(
                        channel_config.rate_limit_per_minute,
                        channel_config.rate_limit_per_hour,
                        channel_config.rate_limit_per_day,
                    ),
                );
            }
        }

        logger.info(&format!(
            "DeliveryManager initialized: delivery={}, retry={}, rate_limiting={}",
            config.enable_delivery, config.enable_retry_logic, config.enable_rate_limiting
        ));

        Ok(Self {
            config,
            logger,
            kv_store,
            active_deliveries: Arc::new(Mutex::new(HashMap::new())),
            delivery_queue: Arc::new(Mutex::new(Vec::new())),
            rate_limiters: Arc::new(Mutex::new(rate_limiters)),
            health: Arc::new(Mutex::new(DeliveryManagerHealth::default())),
            metrics: Arc::new(Mutex::new(DeliveryManagerMetrics::default())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Deliver a notification
    pub async fn deliver(&self, request: DeliveryRequest) -> ArbitrageResult<String> {
        if !self.config.enable_delivery {
            return Ok("Delivery disabled".to_string());
        }

        // Validate request
        request.validate()?;

        // Rate limiting check
        if self.config.enable_rate_limiting && !self.check_rate_limit(&request.channel).await {
            return Err(ArbitrageError::rate_limit_error(&format!(
                "Rate limit exceeded for channel: {}",
                request.channel.as_str()
            )));
        }

        // Create delivery attempt
        let mut attempt =
            DeliveryAttempt::new(request.notification_id.clone(), request.channel.clone(), 1);

        attempt.start_processing();
        let attempt_id = attempt.attempt_id.clone();

        // Store active delivery
        if let Ok(mut active) = self.active_deliveries.lock() {
            active.insert(attempt_id.clone(), attempt.clone());
        }

        // Perform delivery
        let delivery_result = self.perform_delivery(&request, &mut attempt).await;

        // Update attempt based on result
        match delivery_result {
            Ok(response) => {
                attempt.complete_success(Some(response.clone()));
                self.record_delivery_metrics(&request, &attempt, true).await;

                // Remove from active deliveries
                if let Ok(mut active) = self.active_deliveries.lock() {
                    active.remove(&attempt_id);
                }

                Ok(response)
            }
            Err(e) => {
                let error_code = self.extract_error_code(&e);
                let should_retry = request.retry_config.should_retry(1, error_code.as_deref());

                if should_retry && self.config.enable_retry_logic {
                    let retry_delay = request.retry_config.calculate_delay(1);
                    attempt.complete_failure(e.to_string(), error_code, Some(retry_delay));

                    // Schedule retry (in a real implementation, this would use a job queue)
                    self.schedule_retry(request.clone(), attempt.clone())
                        .await?;
                } else {
                    attempt.complete_failure(e.to_string(), error_code, None);

                    // Try fallback channels if enabled
                    if self.config.enable_fallback_channels {
                        if let Some(fallback_channels) =
                            self.config.fallback_channel_map.get(&request.channel)
                        {
                            for fallback_channel in fallback_channels {
                                let mut fallback_request = request.clone();
                                fallback_request.channel = fallback_channel.clone();
                                fallback_request.request_id = uuid::Uuid::new_v4().to_string();

                                match Box::pin(self.deliver(fallback_request)).await {
                                    Ok(response) => {
                                        self.logger.info(&format!(
                                            "Fallback delivery successful via {}",
                                            fallback_channel.as_str()
                                        ));
                                        return Ok(response);
                                    }
                                    Err(fallback_error) => {
                                        self.logger.warn(&format!(
                                            "Fallback delivery failed via {}: {}",
                                            fallback_channel.as_str(),
                                            fallback_error
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }

                self.record_delivery_metrics(&request, &attempt, false)
                    .await;

                // Remove from active deliveries
                if let Ok(mut active) = self.active_deliveries.lock() {
                    active.remove(&attempt_id);
                }

                Err(e)
            }
        }
    }

    /// Deliver batch of notifications
    pub async fn deliver_batch(
        &self,
        requests: Vec<DeliveryRequest>,
    ) -> ArbitrageResult<Vec<ArbitrageResult<String>>> {
        let mut results = Vec::new();

        // Process in batches
        for chunk in requests.chunks(self.config.batch_size) {
            for request in chunk {
                let result = self.deliver(request.clone()).await;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Perform actual delivery to channel
    async fn perform_delivery(
        &self,
        request: &DeliveryRequest,
        _attempt: &mut DeliveryAttempt,
    ) -> ArbitrageResult<String> {
        // Simulate delivery based on channel
        match request.channel {
            NotificationChannel::Telegram => {
                // In a real implementation, this would call Telegram Bot API
                self.logger.info(&format!(
                    "Delivering Telegram message to {}: {}",
                    request.recipient, request.content
                ));
                Ok("telegram_message_id_123".to_string())
            }
            NotificationChannel::Email => {
                // In a real implementation, this would send email via SMTP or email service
                self.logger.info(&format!(
                    "Sending email to {}: {}",
                    request.recipient,
                    request.subject.as_deref().unwrap_or("No Subject")
                ));
                Ok("email_id_456".to_string())
            }
            NotificationChannel::Push => {
                // In a real implementation, this would send push notification
                self.logger.info(&format!(
                    "Sending push notification to {}: {}",
                    request.recipient,
                    request.title.as_deref().unwrap_or("No Title")
                ));
                Ok("push_id_789".to_string())
            }
            NotificationChannel::Webhook => {
                // In a real implementation, this would make HTTP POST request
                self.logger
                    .info(&format!("Sending webhook to {}", request.recipient));
                Ok("webhook_response_ok".to_string())
            }
            _ => Err(ArbitrageError::not_implemented(&format!(
                "Channel {} not implemented",
                request.channel.as_str()
            ))),
        }
    }

    /// Check rate limit for channel
    async fn check_rate_limit(&self, channel: &NotificationChannel) -> bool {
        if let Ok(mut limiters) = self.rate_limiters.lock() {
            if let Some(limiter) = limiters.get_mut(channel) {
                if limiter.can_proceed() {
                    limiter.consume();
                    return true;
                }
            }
        }
        false
    }

    /// Schedule retry for failed delivery
    async fn schedule_retry(
        &self,
        request: DeliveryRequest,
        _attempt: DeliveryAttempt,
    ) -> ArbitrageResult<()> {
        // In a real implementation, this would schedule the retry using a job queue
        // For now, we'll just add it back to the delivery queue
        if let Ok(mut queue) = self.delivery_queue.lock() {
            queue.push(request);
        }
        Ok(())
    }

    /// Extract error code from error
    fn extract_error_code(&self, error: &ArbitrageError) -> Option<String> {
        // Simple error code extraction based on error message
        let error_msg = error.to_string().to_lowercase();
        if error_msg.contains("timeout") {
            Some("timeout".to_string())
        } else if error_msg.contains("rate limit") {
            Some("rate_limit".to_string())
        } else if error_msg.contains("invalid") {
            Some("invalid_recipient".to_string())
        } else if error_msg.contains("permission") {
            Some("permission_denied".to_string())
        } else {
            Some("unknown_error".to_string())
        }
    }

    /// Record delivery metrics
    async fn record_delivery_metrics(
        &self,
        request: &DeliveryRequest,
        attempt: &DeliveryAttempt,
        success: bool,
    ) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_deliveries += 1;

            if success {
                metrics.successful_deliveries += 1;
            } else {
                metrics.failed_deliveries += 1;
            }

            // Update delivery time metrics
            if let Some(duration) = attempt.get_duration_ms() {
                let duration_f64 = duration as f64;
                metrics.avg_delivery_time_ms = (metrics.avg_delivery_time_ms
                    * (metrics.total_deliveries - 1) as f64
                    + duration_f64)
                    / metrics.total_deliveries as f64;
                metrics.min_delivery_time_ms = metrics.min_delivery_time_ms.min(duration_f64);
                metrics.max_delivery_time_ms = metrics.max_delivery_time_ms.max(duration_f64);

                // Update channel-specific metrics
                let deliveries_for_channel = metrics
                    .deliveries_by_channel
                    .get(&request.channel)
                    .copied()
                    .unwrap_or(1);
                let channel_duration = metrics
                    .avg_delivery_time_by_channel
                    .entry(request.channel.clone())
                    .or_insert(0.0);
                *channel_duration = (*channel_duration * (deliveries_for_channel as f64 - 1.0)
                    + duration_f64)
                    / deliveries_for_channel as f64;
            }

            // Update channel metrics
            *metrics
                .deliveries_by_channel
                .entry(request.channel.clone())
                .or_insert(0) += 1;

            // Update success rate by channel
            let channel_total = *metrics
                .deliveries_by_channel
                .get(&request.channel)
                .unwrap_or(&1);
            let channel_success = if success { 1.0 } else { 0.0 };
            let current_rate = metrics
                .success_rate_by_channel
                .get(&request.channel)
                .unwrap_or(&0.0);
            let new_rate = (current_rate * (channel_total - 1) as f32 + channel_success)
                / channel_total as f32;
            metrics
                .success_rate_by_channel
                .insert(request.channel.clone(), new_rate);

            // Update priority metrics
            *metrics
                .deliveries_by_priority
                .entry(request.priority)
                .or_insert(0) += 1;

            // Update error metrics
            if !success {
                if let Some(error_code) = &attempt.error_code {
                    *metrics
                        .error_counts_by_code
                        .entry(error_code.clone())
                        .or_insert(0) += 1;
                }
            }

            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Get delivery manager health
    pub async fn get_health(&self) -> DeliveryManagerHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            DeliveryManagerHealth::default()
        }
    }

    /// Get delivery manager metrics
    pub async fn get_metrics(&self) -> DeliveryManagerMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            DeliveryManagerMetrics::default()
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Test basic delivery functionality
        let test_request = DeliveryRequest::new(
            "health_check".to_string(),
            "system".to_string(),
            NotificationChannel::Telegram,
            "Health check message".to_string(),
            "test_recipient".to_string(),
        );

        // Validate request (don't actually deliver)
        let validation_result = test_request.validate();

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = validation_result.is_ok();
            health.kv_store_available = self.config.enable_kv_storage;
            health.last_health_check = start_time;
            health.last_error = validation_result.clone().err().map(|e| e.to_string());

            // Update active delivery counts
            if let Ok(active) = self.active_deliveries.lock() {
                health.active_deliveries = active.len() as u64;
            }

            if let Ok(queue) = self.delivery_queue.lock() {
                health.pending_deliveries = queue.len() as u64;
            }

            // Update rate limit status
            if let Ok(limiters) = self.rate_limiters.lock() {
                for (channel, limiter) in limiters.iter() {
                    health
                        .rate_limit_status
                        .insert(channel.clone(), limiter.get_status());
                }
            }
        }

        Ok(validation_result.is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_channel_properties() {
        assert_eq!(NotificationChannel::Telegram.as_str(), "telegram");
        assert_eq!(NotificationChannel::Telegram.default_priority(), 9);
        assert!(NotificationChannel::Telegram.supports_rich_content());
        assert_eq!(NotificationChannel::Telegram.max_message_length(), 4096);
    }

    #[test]
    fn test_delivery_status_properties() {
        assert_eq!(DeliveryStatus::Delivered.as_str(), "delivered");
        assert!(DeliveryStatus::Delivered.is_final());
        assert!(DeliveryStatus::Delivered.is_successful());
        assert!(!DeliveryStatus::Processing.is_final());
    }

    #[test]
    fn test_delivery_request_creation() {
        let request = DeliveryRequest::new(
            "notif_123".to_string(),
            "user_456".to_string(),
            NotificationChannel::Telegram,
            "Test message".to_string(),
            "@testuser".to_string(),
        )
        .with_priority(8)
        .with_subject("Test Subject".to_string());

        assert_eq!(request.notification_id, "notif_123");
        assert_eq!(request.user_id, "user_456");
        assert_eq!(request.channel, NotificationChannel::Telegram);
        assert_eq!(request.priority, 8);
        assert_eq!(request.subject, Some("Test Subject".to_string()));
    }

    #[test]
    fn test_delivery_request_validation() {
        let valid_request = DeliveryRequest::new(
            "notif_123".to_string(),
            "user_456".to_string(),
            NotificationChannel::Telegram,
            "Test message".to_string(),
            "@testuser".to_string(),
        );
        assert!(valid_request.validate().is_ok());

        let empty_content_request = DeliveryRequest::new(
            "notif_123".to_string(),
            "user_456".to_string(),
            NotificationChannel::Telegram,
            "".to_string(),
            "@testuser".to_string(),
        );
        assert!(empty_content_request.validate().is_err());

        let too_long_content = "x".repeat(5000);
        let long_content_request = DeliveryRequest::new(
            "notif_123".to_string(),
            "user_456".to_string(),
            NotificationChannel::Telegram,
            too_long_content,
            "@testuser".to_string(),
        );
        assert!(long_content_request.validate().is_err());
    }

    #[test]
    fn test_retry_config_logic() {
        let config = RetryConfig::default();

        assert!(config.should_retry(1, Some("timeout")));
        assert!(config.should_retry(2, Some("rate_limit")));
        assert!(!config.should_retry(3, Some("timeout"))); // Max attempts reached
        assert!(!config.should_retry(1, Some("invalid_recipient"))); // Stop on error

        assert_eq!(config.calculate_delay(1), 30);
        assert_eq!(config.calculate_delay(2), 60);
        assert_eq!(config.calculate_delay(3), 120);
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(2, 10, 100);

        assert!(limiter.can_proceed());
        limiter.consume();

        assert!(limiter.can_proceed());
        limiter.consume();

        assert!(!limiter.can_proceed()); // Limit reached

        let status = limiter.get_status();
        assert_eq!(status.requests_remaining, 0);
        assert!(status.is_limited);
    }

    #[test]
    fn test_delivery_manager_config_validation() {
        let mut config = DeliveryManagerConfig::default();
        assert!(config.validate().is_ok());

        config.batch_size = 0;
        assert!(config.validate().is_err());

        config.batch_size = 50;
        config.max_concurrent_deliveries = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = DeliveryManagerConfig::high_performance();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_concurrent_deliveries, 200);
        assert_eq!(config.delivery_timeout_seconds, 15);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = DeliveryManagerConfig::high_reliability();
        assert_eq!(config.batch_size, 25);
        assert!(config.enable_retry_logic);
        assert!(config.enable_fallback_channels);
        assert_eq!(config.analytics_retention_days, 90);
    }
}
