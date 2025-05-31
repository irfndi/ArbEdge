// Notification Module - Advanced Multi-Channel Notification System
// Replaces notifications.rs with modular architecture

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::kv::KvStore;

// Module components
pub mod channel_manager;
pub mod delivery_manager;
pub mod notification_coordinator;
pub mod template_engine;

// Re-export main types for easy access
pub use channel_manager::{
    AuthType, ChannelAuth, ChannelDeliveryConfig, ChannelEndpoint, ChannelFeature, ChannelManager,
    ChannelManagerConfig, MessageFormat,
};
pub use delivery_manager::{
    DeliveryAttempt, DeliveryManager, DeliveryManagerConfig, DeliveryRequest, DeliveryStatus,
    NotificationChannel, RetryConfig,
};
pub use notification_coordinator::{
    ChannelResult, NotificationCoordinator, NotificationCoordinatorConfig, NotificationRequest,
    NotificationResult,
};
pub use template_engine::{
    ChannelTemplate, NotificationTemplate, TemplateCategory, TemplateEngine, TemplateEngineConfig,
    TemplateFormat, TemplateVariable, VariableType,
};

/// Notification types for categorization
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    OpportunityAlert,
    BalanceChange,
    PriceAlert,
    ProfitLoss,
    RiskWarning,
    SystemMaintenance,
    SecurityAlert,
    TradingSignal,
    Custom(String),
}

impl NotificationType {
    pub fn as_str(&self) -> &str {
        match self {
            NotificationType::OpportunityAlert => "opportunity_alert",
            NotificationType::BalanceChange => "balance_change",
            NotificationType::PriceAlert => "price_alert",
            NotificationType::ProfitLoss => "profit_loss",
            NotificationType::RiskWarning => "risk_warning",
            NotificationType::SystemMaintenance => "system_maintenance",
            NotificationType::SecurityAlert => "security_alert",
            NotificationType::TradingSignal => "trading_signal",
            NotificationType::Custom(name) => name,
        }
    }

    pub fn default_priority(&self) -> u8 {
        match self {
            NotificationType::SecurityAlert => 10,
            NotificationType::RiskWarning => 9,
            NotificationType::OpportunityAlert => 8,
            NotificationType::TradingSignal => 7,
            NotificationType::ProfitLoss => 6,
            NotificationType::BalanceChange => 6,
            NotificationType::PriceAlert => 5,
            NotificationType::SystemMaintenance => 4,
            NotificationType::Custom(_) => 5,
        }
    }

    pub fn default_channels(&self) -> Vec<NotificationChannel> {
        match self {
            NotificationType::SecurityAlert => vec![
                NotificationChannel::Push,
                NotificationChannel::Email,
                NotificationChannel::Telegram,
            ],
            NotificationType::RiskWarning => {
                vec![NotificationChannel::Push, NotificationChannel::Telegram]
            }
            NotificationType::OpportunityAlert => {
                vec![NotificationChannel::Telegram, NotificationChannel::Push]
            }
            NotificationType::TradingSignal => vec![NotificationChannel::Telegram],
            NotificationType::ProfitLoss => {
                vec![NotificationChannel::Email, NotificationChannel::Telegram]
            }
            NotificationType::BalanceChange => vec![NotificationChannel::Email],
            NotificationType::PriceAlert => vec![NotificationChannel::Push],
            NotificationType::SystemMaintenance => vec![NotificationChannel::Email],
            NotificationType::Custom(_) => vec![NotificationChannel::Email],
        }
    }
}

/// Priority levels for notifications
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPriority {
    Critical,
    High,
    Medium,
    Low,
}

impl NotificationPriority {
    pub fn as_u8(&self) -> u8 {
        match self {
            NotificationPriority::Critical => 10,
            NotificationPriority::High => 8,
            NotificationPriority::Medium => 5,
            NotificationPriority::Low => 3,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            10 => NotificationPriority::Critical,
            8..=9 => NotificationPriority::High,
            5..=7 => NotificationPriority::Medium,
            3..=4 => NotificationPriority::Low,
            _ => NotificationPriority::Low,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            NotificationPriority::Critical => "critical",
            NotificationPriority::High => "high",
            NotificationPriority::Medium => "medium",
            NotificationPriority::Low => "low",
        }
    }
}

/// Notification module health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationModuleHealth {
    pub is_healthy: bool,
    pub template_engine_healthy: bool,
    pub delivery_manager_healthy: bool,
    pub channel_manager_healthy: bool,
    pub coordinator_healthy: bool,
    pub total_notifications_processed: u64,
    pub successful_notifications: u64,
    pub failed_notifications: u64,
    pub avg_processing_time_ms: f64,
    pub active_notifications: u64,
    pub rate_limit_status: HashMap<NotificationChannel, bool>,
    pub kv_store_available: bool,
    pub last_health_check: u64,
    pub last_error: Option<String>,
}

impl Default for NotificationModuleHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            template_engine_healthy: false,
            delivery_manager_healthy: false,
            channel_manager_healthy: false,
            coordinator_healthy: false,
            total_notifications_processed: 0,
            successful_notifications: 0,
            failed_notifications: 0,
            avg_processing_time_ms: 0.0,
            active_notifications: 0,
            rate_limit_status: HashMap::new(),
            kv_store_available: false,
            last_health_check: 0,
            last_error: None,
        }
    }
}

/// Notification module performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationModuleMetrics {
    pub total_notifications: u64,
    pub notifications_per_second: f64,
    pub successful_notifications: u64,
    pub failed_notifications: u64,
    pub avg_processing_time_ms: f64,
    pub min_processing_time_ms: f64,
    pub max_processing_time_ms: f64,
    pub notifications_by_type: HashMap<NotificationType, u64>,
    pub notifications_by_channel: HashMap<NotificationChannel, u64>,
    pub notifications_by_priority: HashMap<NotificationPriority, u64>,
    pub template_usage: HashMap<String, u64>,
    pub channel_success_rates: HashMap<NotificationChannel, f32>,
    pub error_counts: HashMap<String, u64>,
    pub last_updated: u64,
}

impl Default for NotificationModuleMetrics {
    fn default() -> Self {
        Self {
            total_notifications: 0,
            notifications_per_second: 0.0,
            successful_notifications: 0,
            failed_notifications: 0,
            avg_processing_time_ms: 0.0,
            min_processing_time_ms: f64::MAX,
            max_processing_time_ms: 0.0,
            notifications_by_type: HashMap::new(),
            notifications_by_channel: HashMap::new(),
            notifications_by_priority: HashMap::new(),
            template_usage: HashMap::new(),
            channel_success_rates: HashMap::new(),
            error_counts: HashMap::new(),
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for the entire Notification Module
#[derive(Debug, Clone)]
pub struct NotificationModuleConfig {
    pub enable_notification_module: bool,
    pub enable_high_performance_mode: bool,
    pub enable_high_reliability_mode: bool,
    pub max_notifications_per_minute: u32,
    pub max_notifications_per_hour: u32,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_metrics: bool,
    pub enable_health_monitoring: bool,
    pub health_check_interval_seconds: u64,
    pub coordinator_config: NotificationCoordinatorConfig,
}

impl Default for NotificationModuleConfig {
    fn default() -> Self {
        Self {
            enable_notification_module: true,
            enable_high_performance_mode: false,
            enable_high_reliability_mode: true,
            max_notifications_per_minute: 200,
            max_notifications_per_hour: 5000,
            enable_kv_storage: true,
            kv_key_prefix: "notification_module:".to_string(),
            enable_metrics: true,
            enable_health_monitoring: true,
            health_check_interval_seconds: 60,
            coordinator_config: NotificationCoordinatorConfig::default(),
        }
    }
}

impl NotificationModuleConfig {
    pub fn high_performance() -> Self {
        Self {
            enable_high_performance_mode: true,
            enable_high_reliability_mode: false,
            max_notifications_per_minute: 500,
            max_notifications_per_hour: 15000,
            coordinator_config: NotificationCoordinatorConfig::high_performance(),
            ..Default::default()
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            enable_high_performance_mode: false,
            enable_high_reliability_mode: true,
            max_notifications_per_minute: 100,
            max_notifications_per_hour: 2500,
            coordinator_config: NotificationCoordinatorConfig::high_reliability(),
            ..Default::default()
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.max_notifications_per_minute == 0 {
            return Err(ArbitrageError::validation_error(
                "max_notifications_per_minute must be greater than 0",
            ));
        }
        if self.max_notifications_per_hour == 0 {
            return Err(ArbitrageError::validation_error(
                "max_notifications_per_hour must be greater than 0",
            ));
        }
        if self.health_check_interval_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "health_check_interval_seconds must be greater than 0",
            ));
        }

        self.coordinator_config.validate()?;

        Ok(())
    }
}

/// Main Notification Module - Unified interface for all notification functionality
pub struct NotificationModule {
    config: NotificationModuleConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Main coordinator
    coordinator: NotificationCoordinator,

    // Health and performance tracking
    health: std::sync::Arc<std::sync::Mutex<NotificationModuleHealth>>,
    metrics: std::sync::Arc<std::sync::Mutex<NotificationModuleMetrics>>,

    // Performance tracking
    startup_time: u64,
}

impl NotificationModule {
    /// Create new NotificationModule instance
    pub async fn new(
        config: NotificationModuleConfig,
        kv_store: KvStore,
        env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize coordinator
        let coordinator =
            NotificationCoordinator::new(config.coordinator_config.clone(), kv_store.clone(), env)
                .await?;

        logger.info(&format!(
            "NotificationModule initialized: performance_mode={}, reliability_mode={}, max_per_minute={}",
            config.enable_high_performance_mode,
            config.enable_high_reliability_mode,
            config.max_notifications_per_minute
        ));

        Ok(Self {
            config,
            logger,
            kv_store,
            coordinator,
            health: std::sync::Arc::new(std::sync::Mutex::new(NotificationModuleHealth::default())),
            metrics: std::sync::Arc::new(std::sync::Mutex::new(
                NotificationModuleMetrics::default(),
            )),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Send a notification using the simplified interface
    pub async fn send_notification(
        &self,
        user_id: String,
        notification_type: NotificationType,
        template_variables: HashMap<String, String>,
        recipients: HashMap<NotificationChannel, String>,
        priority: Option<NotificationPriority>,
    ) -> ArbitrageResult<NotificationResult> {
        if !self.config.enable_notification_module {
            return Err(ArbitrageError::service_unavailable(
                "Notification module is disabled",
            ));
        }

        // Create notification request
        let channels = notification_type.default_channels();
        let priority_value = priority.unwrap_or(NotificationPriority::from_u8(
            notification_type.default_priority(),
        ));

        let request =
            NotificationRequest::new(user_id, notification_type.as_str().to_string(), channels)
                .with_priority(priority_value.as_u8())
                .with_template(notification_type.as_str().to_string(), template_variables);

        // Add recipients
        let mut request_with_recipients = request;
        for (channel, recipient) in recipients {
            request_with_recipients = request_with_recipients.with_recipient(channel, recipient);
        }

        // Process through coordinator
        let result = self
            .coordinator
            .process_notification(request_with_recipients)
            .await?;

        // Update module metrics
        self.update_module_metrics(&result, &notification_type, &priority_value)
            .await;

        Ok(result)
    }

    /// Send a custom notification with custom content
    pub async fn send_custom_notification(
        &self,
        user_id: String,
        channels: Vec<NotificationChannel>,
        content: String,
        recipients: HashMap<NotificationChannel, String>,
        priority: Option<NotificationPriority>,
    ) -> ArbitrageResult<NotificationResult> {
        if !self.config.enable_notification_module {
            return Err(ArbitrageError::service_unavailable(
                "Notification module is disabled",
            ));
        }

        let priority_value = priority.unwrap_or(NotificationPriority::Medium);

        let request = NotificationRequest::new(user_id, "custom".to_string(), channels)
            .with_priority(priority_value.as_u8())
            .with_custom_content(content);

        // Add recipients
        let mut request_with_recipients = request;
        for (channel, recipient) in recipients {
            request_with_recipients = request_with_recipients.with_recipient(channel, recipient);
        }

        // Process through coordinator
        let result = self
            .coordinator
            .process_notification(request_with_recipients)
            .await?;

        // Update module metrics
        self.update_module_metrics(
            &result,
            &NotificationType::Custom("custom".to_string()),
            &priority_value,
        )
        .await;

        Ok(result)
    }

    /// Send batch of notifications
    pub async fn send_batch_notifications(
        &self,
        requests: Vec<NotificationRequest>,
    ) -> ArbitrageResult<Vec<NotificationResult>> {
        if !self.config.enable_notification_module {
            return Err(ArbitrageError::service_unavailable(
                "Notification module is disabled",
            ));
        }

        let results = self.coordinator.process_batch(requests).await?;

        // Update metrics for each result
        for result in &results {
            let notification_type = NotificationType::Custom("batch".to_string());
            let priority = NotificationPriority::Medium;
            self.update_module_metrics(result, &notification_type, &priority)
                .await;
        }

        Ok(results)
    }

    /// Update module-level metrics
    async fn update_module_metrics(
        &self,
        result: &NotificationResult,
        notification_type: &NotificationType,
        priority: &NotificationPriority,
    ) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_notifications += 1;

            if result.success {
                metrics.successful_notifications += 1;
            } else {
                metrics.failed_notifications += 1;
            }

            // Update processing time metrics
            let processing_time = result.processing_time_ms as f64;
            metrics.avg_processing_time_ms = (metrics.avg_processing_time_ms
                * (metrics.total_notifications - 1) as f64
                + processing_time)
                / metrics.total_notifications as f64;
            metrics.min_processing_time_ms = metrics.min_processing_time_ms.min(processing_time);
            metrics.max_processing_time_ms = metrics.max_processing_time_ms.max(processing_time);

            // Update type and priority metrics
            *metrics
                .notifications_by_type
                .entry(notification_type.clone())
                .or_insert(0) += 1;
            *metrics
                .notifications_by_priority
                .entry(priority.clone())
                .or_insert(0) += 1;

            // Update channel metrics
            for (channel, channel_result) in &result.channel_results {
                *metrics
                    .notifications_by_channel
                    .entry(channel.clone())
                    .or_insert(0) += 1;

                // Update success rates
                let channel_total = *metrics.notifications_by_channel.get(channel).unwrap_or(&1);
                let channel_success = if channel_result.success { 1.0 } else { 0.0 };
                let current_rate = metrics.channel_success_rates.get(channel).unwrap_or(&0.0);
                let new_rate = (current_rate * (channel_total - 1) as f32 + channel_success)
                    / channel_total as f32;
                metrics
                    .channel_success_rates
                    .insert(channel.clone(), new_rate);
            }

            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Get notification module health
    pub async fn get_health(&self) -> NotificationModuleHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            NotificationModuleHealth::default()
        }
    }

    /// Get notification module metrics
    pub async fn get_metrics(&self) -> NotificationModuleMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            NotificationModuleMetrics::default()
        }
    }

    /// Perform comprehensive health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check coordinator health
        let coordinator_healthy = self.coordinator.health_check().await.unwrap_or(false);
        let coordinator_health = self.coordinator.get_health().await;

        // Update module health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = coordinator_healthy;
            health.template_engine_healthy = coordinator_health.template_engine_healthy;
            health.delivery_manager_healthy = coordinator_health.delivery_manager_healthy;
            health.channel_manager_healthy = coordinator_health.channel_manager_healthy;
            health.coordinator_healthy = coordinator_healthy;
            health.total_notifications_processed = coordinator_health.total_notifications;
            health.successful_notifications = coordinator_health.successful_notifications;
            health.failed_notifications = coordinator_health.failed_notifications;
            health.avg_processing_time_ms = coordinator_health.avg_processing_time_ms;
            health.active_notifications = coordinator_health.active_notifications;
            health.kv_store_available = self.config.enable_kv_storage;
            health.last_health_check = start_time;
            health.last_error = coordinator_health.last_error;
        }

        Ok(coordinator_healthy)
    }

    /// Get uptime in seconds
    pub fn get_uptime_seconds(&self) -> u64 {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        (now - self.startup_time) / 1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_type_properties() {
        assert_eq!(
            NotificationType::OpportunityAlert.as_str(),
            "opportunity_alert"
        );
        assert_eq!(NotificationType::SecurityAlert.default_priority(), 10);
        assert_eq!(
            NotificationType::OpportunityAlert.default_channels().len(),
            2
        );
    }

    #[test]
    fn test_notification_priority_conversion() {
        assert_eq!(NotificationPriority::Critical.as_u8(), 10);
        assert_eq!(
            NotificationPriority::from_u8(10),
            NotificationPriority::Critical
        );
        assert_eq!(
            NotificationPriority::from_u8(5),
            NotificationPriority::Medium
        );
        assert_eq!(NotificationPriority::from_u8(1), NotificationPriority::Low);
    }

    #[test]
    fn test_notification_module_config_validation() {
        let mut config = NotificationModuleConfig::default();
        assert!(config.validate().is_ok());

        config.max_notifications_per_minute = 0;
        assert!(config.validate().is_err());

        config.max_notifications_per_minute = 100;
        config.health_check_interval_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = NotificationModuleConfig::high_performance();
        assert!(config.enable_high_performance_mode);
        assert!(!config.enable_high_reliability_mode);
        assert_eq!(config.max_notifications_per_minute, 500);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = NotificationModuleConfig::high_reliability();
        assert!(!config.enable_high_performance_mode);
        assert!(config.enable_high_reliability_mode);
        assert_eq!(config.max_notifications_per_minute, 100);
    }

    #[test]
    fn test_notification_module_health_default() {
        let health = NotificationModuleHealth::default();
        assert!(!health.is_healthy);
        assert_eq!(health.total_notifications_processed, 0);
        assert_eq!(health.successful_notifications, 0);
        assert_eq!(health.failed_notifications, 0);
    }

    #[test]
    fn test_notification_module_metrics_default() {
        let metrics = NotificationModuleMetrics::default();
        assert_eq!(metrics.total_notifications, 0);
        assert_eq!(metrics.successful_notifications, 0);
        assert_eq!(metrics.failed_notifications, 0);
        assert_eq!(metrics.avg_processing_time_ms, 0.0);
    }
}
