// Notification Coordinator - Main Orchestrator for Notification Module
// Part of Notification Module replacing notifications.rs

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

use super::{
    channel_manager::{ChannelManager, ChannelManagerConfig},
    delivery_manager::{
        DeliveryManager, DeliveryManagerConfig, DeliveryRequest, NotificationChannel,
    },
    template_engine::{TemplateEngine, TemplateEngineConfig},
};

/// Notification request for processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRequest {
    pub notification_id: String,
    pub user_id: String,
    pub notification_type: String,
    pub priority: u8,
    pub channels: Vec<NotificationChannel>,
    pub template_id: Option<String>,
    pub template_variables: HashMap<String, String>,
    pub custom_content: Option<String>,
    pub recipients: HashMap<NotificationChannel, String>, // channel -> recipient
    pub metadata: HashMap<String, String>,
    pub expires_at: Option<u64>,
    pub created_at: u64,
}

impl NotificationRequest {
    pub fn new(
        user_id: String,
        notification_type: String,
        channels: Vec<NotificationChannel>,
    ) -> Self {
        Self {
            notification_id: uuid::Uuid::new_v4().to_string(),
            user_id,
            notification_type,
            priority: 5,
            channels,
            template_id: None,
            template_variables: HashMap::new(),
            custom_content: None,
            recipients: HashMap::new(),
            metadata: HashMap::new(),
            expires_at: None,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_template(
        mut self,
        template_id: String,
        variables: HashMap<String, String>,
    ) -> Self {
        self.template_id = Some(template_id);
        self.template_variables = variables;
        self
    }

    pub fn with_custom_content(mut self, content: String) -> Self {
        self.custom_content = Some(content);
        self
    }

    pub fn with_recipient(mut self, channel: NotificationChannel, recipient: String) -> Self {
        self.recipients.insert(channel, recipient);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
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
        if self.channels.is_empty() {
            return Err(ArbitrageError::validation_error(
                "At least one channel must be specified",
            ));
        }

        if self.template_id.is_none() && self.custom_content.is_none() {
            return Err(ArbitrageError::validation_error(
                "Either template_id or custom_content must be provided",
            ));
        }

        if self.is_expired() {
            return Err(ArbitrageError::validation_error(
                "Notification request has expired",
            ));
        }

        // Check that recipients are provided for all channels
        for channel in &self.channels {
            if !self.recipients.contains_key(channel) {
                return Err(ArbitrageError::validation_error(&format!(
                    "Recipient not provided for channel: {}",
                    channel.as_str()
                )));
            }
        }

        Ok(())
    }
}

/// Notification processing result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResult {
    pub notification_id: String,
    pub success: bool,
    pub channel_results: HashMap<NotificationChannel, ChannelResult>,
    pub total_channels: u32,
    pub successful_channels: u32,
    pub failed_channels: u32,
    pub processing_time_ms: u64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelResult {
    pub channel: NotificationChannel,
    pub success: bool,
    pub message_id: Option<String>,
    pub error_message: Option<String>,
    pub delivery_time_ms: u64,
}

/// Notification coordinator health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationCoordinatorHealth {
    pub is_healthy: bool,
    pub template_engine_healthy: bool,
    pub delivery_manager_healthy: bool,
    pub channel_manager_healthy: bool,
    pub total_notifications: u64,
    pub successful_notifications: u64,
    pub failed_notifications: u64,
    pub avg_processing_time_ms: f64,
    pub active_notifications: u64,
    pub last_health_check: u64,
    pub last_error: Option<String>,
}

impl Default for NotificationCoordinatorHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            template_engine_healthy: false,
            delivery_manager_healthy: false,
            channel_manager_healthy: false,
            total_notifications: 0,
            successful_notifications: 0,
            failed_notifications: 0,
            avg_processing_time_ms: 0.0,
            active_notifications: 0,
            last_health_check: 0,
            last_error: None,
        }
    }
}

/// Configuration for NotificationCoordinator
#[derive(Debug, Clone)]
pub struct NotificationCoordinatorConfig {
    pub enable_coordinator: bool,
    pub enable_parallel_processing: bool,
    pub max_concurrent_notifications: usize,
    pub enable_fallback_processing: bool,
    pub enable_metrics: bool,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub template_engine_config: TemplateEngineConfig,
    pub delivery_manager_config: DeliveryManagerConfig,
    pub channel_manager_config: ChannelManagerConfig,
}

impl Default for NotificationCoordinatorConfig {
    fn default() -> Self {
        Self {
            enable_coordinator: true,
            enable_parallel_processing: true,
            max_concurrent_notifications: 100,
            enable_fallback_processing: true,
            enable_metrics: true,
            enable_kv_storage: true,
            kv_key_prefix: "notification:".to_string(),
            template_engine_config: TemplateEngineConfig::default(),
            delivery_manager_config: DeliveryManagerConfig::default(),
            channel_manager_config: ChannelManagerConfig::default(),
        }
    }
}

impl NotificationCoordinatorConfig {
    pub fn high_performance() -> Self {
        Self {
            max_concurrent_notifications: 200,
            enable_parallel_processing: true,
            template_engine_config: TemplateEngineConfig::high_performance(),
            delivery_manager_config: DeliveryManagerConfig::high_performance(),
            ..Default::default()
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            max_concurrent_notifications: 50,
            enable_fallback_processing: true,
            template_engine_config: TemplateEngineConfig::high_reliability(),
            delivery_manager_config: DeliveryManagerConfig::high_reliability(),
            ..Default::default()
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.max_concurrent_notifications == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_notifications must be greater than 0",
            ));
        }

        self.template_engine_config.validate()?;
        self.delivery_manager_config.validate()?;
        self.channel_manager_config.validate()?;

        Ok(())
    }
}

/// Notification Coordinator - Main orchestrator for notification processing
pub struct NotificationCoordinator {
    config: NotificationCoordinatorConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Component managers
    template_engine: TemplateEngine,
    delivery_manager: DeliveryManager,
    channel_manager: ChannelManager,

    // Processing tracking
    active_notifications: Arc<Mutex<HashMap<String, NotificationRequest>>>,

    // Health and performance tracking
    health: Arc<Mutex<NotificationCoordinatorHealth>>,

    // Performance tracking
    startup_time: u64,
}

impl NotificationCoordinator {
    /// Create new NotificationCoordinator instance
    pub async fn new(
        config: NotificationCoordinatorConfig,
        kv_store: KvStore,
        env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize component managers
        let template_engine =
            TemplateEngine::new(config.template_engine_config.clone(), kv_store.clone(), env)
                .await?;

        let delivery_manager = DeliveryManager::new(
            config.delivery_manager_config.clone(),
            kv_store.clone(),
            env,
        )
        .await?;

        let channel_manager =
            ChannelManager::new(config.channel_manager_config.clone(), kv_store.clone(), env)
                .await?;

        logger.info(&format!(
            "NotificationCoordinator initialized: parallel={}, max_concurrent={}",
            config.enable_parallel_processing, config.max_concurrent_notifications
        ));

        Ok(Self {
            config,
            logger,
            kv_store,
            template_engine,
            delivery_manager,
            channel_manager,
            active_notifications: Arc::new(Mutex::new(HashMap::new())),
            health: Arc::new(Mutex::new(NotificationCoordinatorHealth::default())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Process a notification request
    pub async fn process_notification(
        &self,
        request: NotificationRequest,
    ) -> ArbitrageResult<NotificationResult> {
        if !self.config.enable_coordinator {
            return Err(ArbitrageError::service_unavailable(
                "Notification coordinator is disabled",
            ));
        }

        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Validate request
        request.validate()?;

        let notification_id = request.notification_id.clone();

        // Store active notification
        if let Ok(mut active) = self.active_notifications.lock() {
            active.insert(notification_id.clone(), request.clone());
        }

        // Process notification
        let result = self.process_notification_internal(request.clone()).await;

        // Remove from active notifications
        if let Ok(mut active) = self.active_notifications.lock() {
            active.remove(&notification_id);
        }

        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let processing_time = end_time - start_time;

        // Update metrics
        match &result {
            Ok(notification_result) => {
                self.update_processing_metrics(notification_result, processing_time)
                    .await;
            }
            Err(_) => {
                // Create a default NotificationResult for error cases
                let error_result = NotificationResult {
                    notification_id: request.notification_id.clone(),
                    success: false,
                    channel_results: HashMap::new(),
                    total_channels: request.channels.len() as u32,
                    successful_channels: 0,
                    failed_channels: request.channels.len() as u32,
                    processing_time_ms: processing_time,
                    error_message: Some("Processing failed".to_string()),
                };
                self.update_processing_metrics(&error_result, processing_time)
                    .await;
            }
        }

        result
    }

    /// Internal notification processing logic
    async fn process_notification_internal(
        &self,
        request: NotificationRequest,
    ) -> ArbitrageResult<NotificationResult> {
        let mut channel_results = HashMap::new();
        let mut successful_channels = 0;
        let mut failed_channels = 0;

        // Prepare content for each channel
        for channel in &request.channels {
            let channel_start_time = chrono::Utc::now().timestamp_millis() as u64;

            // Get recipient for this channel
            let recipient = match request.recipients.get(channel) {
                Some(r) => r.clone(),
                None => {
                    let error_msg = format!("No recipient found for channel: {}", channel.as_str());
                    channel_results.insert(
                        channel.clone(),
                        ChannelResult {
                            channel: channel.clone(),
                            success: false,
                            message_id: None,
                            error_message: Some(error_msg),
                            delivery_time_ms: 0,
                        },
                    );
                    failed_channels += 1;
                    continue;
                }
            };

            // Prepare content
            let content = match self.prepare_content(&request, channel).await {
                Ok(c) => c,
                Err(e) => {
                    let error_msg = format!("Failed to prepare content: {}", e);
                    channel_results.insert(
                        channel.clone(),
                        ChannelResult {
                            channel: channel.clone(),
                            success: false,
                            message_id: None,
                            error_message: Some(error_msg),
                            delivery_time_ms: 0,
                        },
                    );
                    failed_channels += 1;
                    continue;
                }
            };

            // Create delivery request
            let delivery_request = DeliveryRequest::new(
                request.notification_id.clone(),
                request.user_id.clone(),
                channel.clone(),
                content,
                recipient,
            )
            .with_priority(request.priority);

            // Deliver through delivery manager
            let delivery_result = self.delivery_manager.deliver(delivery_request).await;

            let channel_end_time = chrono::Utc::now().timestamp_millis() as u64;
            let channel_delivery_time = channel_end_time - channel_start_time;

            match delivery_result {
                Ok(message_id) => {
                    channel_results.insert(
                        channel.clone(),
                        ChannelResult {
                            channel: channel.clone(),
                            success: true,
                            message_id: Some(message_id),
                            error_message: None,
                            delivery_time_ms: channel_delivery_time,
                        },
                    );
                    successful_channels += 1;
                }
                Err(e) => {
                    channel_results.insert(
                        channel.clone(),
                        ChannelResult {
                            channel: channel.clone(),
                            success: false,
                            message_id: None,
                            error_message: Some(e.to_string()),
                            delivery_time_ms: channel_delivery_time,
                        },
                    );
                    failed_channels += 1;
                }
            }
        }

        let total_channels = request.channels.len() as u32;
        let overall_success = successful_channels > 0;

        Ok(NotificationResult {
            notification_id: request.notification_id,
            success: overall_success,
            channel_results,
            total_channels,
            successful_channels,
            failed_channels,
            processing_time_ms: 0, // Will be set by caller
            error_message: if overall_success {
                None
            } else {
                Some("All channels failed".to_string())
            },
        })
    }

    /// Prepare content for a specific channel
    async fn prepare_content(
        &self,
        request: &NotificationRequest,
        channel: &NotificationChannel,
    ) -> ArbitrageResult<String> {
        // If custom content is provided, use it
        if let Some(custom_content) = &request.custom_content {
            return Ok(custom_content.clone());
        }

        // If template is provided, render it
        if let Some(template_id) = &request.template_id {
            let rendered_content = self
                .template_engine
                .render_template(
                    template_id,
                    channel.as_str(),
                    request.template_variables.clone(),
                    None, // Use default language
                )
                .await?;

            return Ok(rendered_content);
        }

        Err(ArbitrageError::validation_error(
            "No content or template provided",
        ))
    }

    /// Process batch of notifications
    pub async fn process_batch(
        &self,
        requests: Vec<NotificationRequest>,
    ) -> ArbitrageResult<Vec<NotificationResult>> {
        let mut results = Vec::new();

        // Process in batches based on max concurrent limit
        for chunk in requests.chunks(self.config.max_concurrent_notifications) {
            for request in chunk {
                let start_time = chrono::Utc::now().timestamp_millis() as u64;

                match self.process_notification_internal(request.clone()).await {
                    Ok(mut notification_result) => {
                        let processing_time =
                            chrono::Utc::now().timestamp_millis() as u64 - start_time;
                        notification_result.processing_time_ms = processing_time;
                        results.push(notification_result);
                    }
                    Err(e) => {
                        results.push(NotificationResult {
                            notification_id: request.notification_id.clone(),
                            success: false,
                            channel_results: HashMap::new(),
                            total_channels: request.channels.len() as u32,
                            successful_channels: 0,
                            failed_channels: request.channels.len() as u32,
                            processing_time_ms: 0,
                            error_message: Some(e.to_string()),
                        });
                    }
                }
            }
        }

        Ok(results)
    }

    /// Update processing metrics
    async fn update_processing_metrics(&self, result: &NotificationResult, processing_time: u64) {
        if let Ok(mut health) = self.health.lock() {
            health.total_notifications += 1;

            if result.success {
                health.successful_notifications += 1;
            } else {
                health.failed_notifications += 1;
            }

            // Update average processing time
            health.avg_processing_time_ms = (health.avg_processing_time_ms
                * (health.total_notifications - 1) as f64
                + processing_time as f64)
                / health.total_notifications as f64;
        }
    }

    /// Get notification coordinator health
    pub async fn get_health(&self) -> NotificationCoordinatorHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            NotificationCoordinatorHealth::default()
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check component health
        let template_engine_healthy = self.template_engine.health_check().await.unwrap_or(false);
        let delivery_manager_healthy = self.delivery_manager.health_check().await.unwrap_or(false);
        let channel_manager_healthy = self.channel_manager.health_check().await.unwrap_or(false);

        let overall_healthy =
            template_engine_healthy && delivery_manager_healthy && channel_manager_healthy;

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = overall_healthy;
            health.template_engine_healthy = template_engine_healthy;
            health.delivery_manager_healthy = delivery_manager_healthy;
            health.channel_manager_healthy = channel_manager_healthy;
            health.last_health_check = start_time;
            health.last_error = if overall_healthy {
                None
            } else {
                Some("One or more components unhealthy".to_string())
            };

            // Update active notification count
            if let Ok(active) = self.active_notifications.lock() {
                health.active_notifications = active.len() as u64;
            }
        }

        Ok(overall_healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_request_creation() {
        let request = NotificationRequest::new(
            "user_123".to_string(),
            "opportunity_alert".to_string(),
            vec![NotificationChannel::Telegram, NotificationChannel::Email],
        )
        .with_priority(8)
        .with_recipient(NotificationChannel::Telegram, "@testuser".to_string())
        .with_recipient(NotificationChannel::Email, "test@example.com".to_string());

        assert_eq!(request.user_id, "user_123");
        assert_eq!(request.notification_type, "opportunity_alert");
        assert_eq!(request.priority, 8);
        assert_eq!(request.channels.len(), 2);
        assert_eq!(request.recipients.len(), 2);
    }

    #[test]
    fn test_notification_request_validation() {
        let mut request = NotificationRequest::new(
            "user_123".to_string(),
            "test".to_string(),
            vec![NotificationChannel::Telegram],
        );

        // Should fail - no content or template
        assert!(request.validate().is_err());

        // Should fail - no recipient
        request.custom_content = Some("Test content".to_string());
        assert!(request.validate().is_err());

        // Should pass
        request
            .recipients
            .insert(NotificationChannel::Telegram, "@testuser".to_string());
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_notification_result_structure() {
        let mut channel_results = HashMap::new();
        channel_results.insert(
            NotificationChannel::Telegram,
            ChannelResult {
                channel: NotificationChannel::Telegram,
                success: true,
                message_id: Some("msg_123".to_string()),
                error_message: None,
                delivery_time_ms: 150,
            },
        );

        let result = NotificationResult {
            notification_id: "notif_123".to_string(),
            success: true,
            channel_results,
            total_channels: 1,
            successful_channels: 1,
            failed_channels: 0,
            processing_time_ms: 200,
            error_message: None,
        };

        assert!(result.success);
        assert_eq!(result.total_channels, 1);
        assert_eq!(result.successful_channels, 1);
        assert_eq!(result.failed_channels, 0);
    }

    #[test]
    fn test_coordinator_config_validation() {
        let mut config = NotificationCoordinatorConfig::default();
        assert!(config.validate().is_ok());

        config.max_concurrent_notifications = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = NotificationCoordinatorConfig::high_performance();
        assert_eq!(config.max_concurrent_notifications, 200);
        assert!(config.enable_parallel_processing);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = NotificationCoordinatorConfig::high_reliability();
        assert_eq!(config.max_concurrent_notifications, 50);
        assert!(config.enable_fallback_processing);
    }
}
