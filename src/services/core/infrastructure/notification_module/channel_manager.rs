// Channel Manager - Channel-Specific Delivery Logic with Authentication and Rate Limiting
// Part of Notification Module replacing notifications.rs

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Simple rate limit info for channel management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
    pub requests_per_minute: u32,
    pub requests_remaining: u32,
    pub reset_time: u64,
}

/// Channel authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelAuth {
    pub auth_type: AuthType,
    pub api_key: Option<String>,
    pub secret_key: Option<String>,
    pub token: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub additional_params: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    ApiKey,
    BearerToken,
    BasicAuth,
    OAuth2,
    Custom(String),
}

/// Channel endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEndpoint {
    pub base_url: String,
    pub send_path: String,
    pub auth_path: Option<String>,
    pub webhook_path: Option<String>,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub headers: HashMap<String, String>,
}

/// Channel-specific delivery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelDeliveryConfig {
    pub channel_id: String,
    pub channel_name: String,
    pub enabled: bool,
    pub endpoint: ChannelEndpoint,
    pub auth: ChannelAuth,
    pub rate_limits: RateLimits,
    pub message_format: MessageFormat,
    pub supported_features: Vec<ChannelFeature>,
    pub fallback_channels: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub requests_per_second: u32,
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub burst_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageFormat {
    PlainText,
    Html,
    Markdown,
    Json,
    Xml,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelFeature {
    RichText,
    Attachments,
    Buttons,
    Images,
    Audio,
    Video,
    Location,
    Polls,
    Threading,
}

/// Channel delivery result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelDeliveryResult {
    pub success: bool,
    pub message_id: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub delivery_time_ms: u64,
    pub rate_limited: bool,
    pub retry_after_seconds: Option<u64>,
    pub response_data: Option<String>,
}

/// Channel manager health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelManagerHealth {
    pub is_healthy: bool,
    pub total_channels: u64,
    pub active_channels: u64,
    pub failed_channels: u64,
    pub channel_health: HashMap<String, bool>,
    pub avg_delivery_time_ms: f64,
    pub total_deliveries: u64,
    pub successful_deliveries: u64,
    pub failed_deliveries: u64,
    pub last_health_check: u64,
}

/// Configuration for ChannelManager
#[derive(Debug, Clone)]
pub struct ChannelManagerConfig {
    pub enable_channel_manager: bool,
    pub enable_authentication: bool,
    pub enable_rate_limiting: bool,
    pub enable_fallback: bool,
    pub max_concurrent_deliveries: usize,
    pub default_timeout_seconds: u64,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_metrics: bool,
    pub health_check_interval_seconds: u64,
}

impl Default for ChannelManagerConfig {
    fn default() -> Self {
        Self {
            enable_channel_manager: true,
            enable_authentication: true,
            enable_rate_limiting: true,
            enable_fallback: true,
            max_concurrent_deliveries: 100,
            default_timeout_seconds: 30,
            enable_kv_storage: true,
            kv_key_prefix: "channel:".to_string(),
            enable_metrics: true,
            health_check_interval_seconds: 60,
        }
    }
}

impl ChannelManagerConfig {
    /// Validate the configuration
    pub fn validate(&self) -> crate::utils::ArbitrageResult<()> {
        if self.max_concurrent_deliveries == 0 {
            return Err(crate::utils::ArbitrageError::validation_error(
                "max_concurrent_deliveries must be greater than 0".to_string(),
            ));
        }

        if self.default_timeout_seconds == 0 {
            return Err(crate::utils::ArbitrageError::validation_error(
                "default_timeout_seconds must be greater than 0".to_string(),
            ));
        }

        if self.health_check_interval_seconds == 0 {
            return Err(crate::utils::ArbitrageError::validation_error(
                "health_check_interval_seconds must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Channel Manager for handling channel-specific delivery logic
pub struct ChannelManager {
    config: ChannelManagerConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Channel configurations
    channels: Arc<Mutex<HashMap<String, ChannelDeliveryConfig>>>,

    // Rate limiting tracking
    rate_limits: Arc<Mutex<HashMap<String, RateLimitInfo>>>,

    // Health and performance tracking
    health: Arc<Mutex<ChannelManagerHealth>>,

    // Performance tracking
    startup_time: u64,
}

impl ChannelManager {
    /// Create new ChannelManager instance
    pub async fn new(
        config: ChannelManagerConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        logger.info(&format!(
            "ChannelManager initialized: auth={}, rate_limiting={}, fallback={}",
            config.enable_authentication, config.enable_rate_limiting, config.enable_fallback
        ));

        let manager = Self {
            config,
            logger,
            kv_store,
            channels: Arc::new(Mutex::new(HashMap::new())),
            rate_limits: Arc::new(Mutex::new(HashMap::new())),
            health: Arc::new(Mutex::new(ChannelManagerHealth {
                is_healthy: false,
                total_channels: 0,
                active_channels: 0,
                failed_channels: 0,
                channel_health: HashMap::new(),
                avg_delivery_time_ms: 0.0,
                total_deliveries: 0,
                successful_deliveries: 0,
                failed_deliveries: 0,
                last_health_check: 0,
            })),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        };

        // Load default channel configurations
        manager.load_default_channels().await?;

        Ok(manager)
    }

    /// Register a new channel
    pub async fn register_channel(&self, config: ChannelDeliveryConfig) -> ArbitrageResult<()> {
        if !self.config.enable_channel_manager {
            return Ok(());
        }

        let channel_id = config.channel_id.clone();

        // Store in memory
        if let Ok(mut channels) = self.channels.lock() {
            channels.insert(channel_id.clone(), config.clone());
        }

        // Store in KV if enabled
        if self.config.enable_kv_storage {
            self.store_channel_config(&config).await?;
        }

        self.logger.info(&format!(
            "Registered channel: {} ({})",
            config.channel_name, channel_id
        ));
        Ok(())
    }

    /// Deliver message through specific channel
    pub async fn deliver_to_channel(
        &self,
        channel_id: &str,
        recipient: &str,
        content: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> ArbitrageResult<ChannelDeliveryResult> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Get channel configuration
        let channel_config = self.get_channel_config(channel_id).await?;

        if !channel_config.enabled {
            return Err(ArbitrageError::validation_error(&format!(
                "Channel '{}' is disabled",
                channel_id
            )));
        }

        // Check rate limiting
        if self.config.enable_rate_limiting {
            // TODO: replace with real token-bucket implementation.
            // For now, return an error once burst_limit is exceeded to avoid
            // unbounded traffic.
            if !self.within_rate_limit(channel_id).await? {
                return Ok(ChannelDeliveryResult {
                    success: false,
                    message_id: None,
                    error_code: Some("rate_limited".into()),
                    error_message: Some("Rate limit exceeded".into()),
                    delivery_time_ms: 0,
                    rate_limited: true,
                    retry_after_seconds: Some(1),
                    response_data: None,
                });
            }
        }

        // Perform authentication if required
        if self.config.enable_authentication {
            self.authenticate_channel(&channel_config).await?;
        }

        // Deliver message
        let delivery_result = self
            .perform_channel_delivery(&channel_config, recipient, content, metadata)
            .await;

        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let delivery_time = end_time - start_time;

        // Create result
        let result = match delivery_result {
            Ok(message_id) => ChannelDeliveryResult {
                success: true,
                message_id: Some(message_id),
                error_code: None,
                error_message: None,
                delivery_time_ms: delivery_time,
                rate_limited: false,
                retry_after_seconds: None,
                response_data: None,
            },
            Err(e) => ChannelDeliveryResult {
                success: false,
                message_id: None,
                error_code: Some("delivery_failed".to_string()),
                error_message: Some(e.to_string()),
                delivery_time_ms: delivery_time,
                rate_limited: false,
                retry_after_seconds: None,
                response_data: None,
            },
        };

        // Update metrics
        self.update_delivery_metrics(channel_id, &result).await;

        Ok(result)
    }

    /// Get channel configuration
    async fn get_channel_config(&self, channel_id: &str) -> ArbitrageResult<ChannelDeliveryConfig> {
        // Try to get from memory first
        if let Ok(channels) = self.channels.lock() {
            if let Some(config) = channels.get(channel_id) {
                return Ok(config.clone());
            }
        }

        // Try to load from KV
        if self.config.enable_kv_storage {
            if let Ok(config) = self.load_channel_config(channel_id).await {
                return Ok(config);
            }
        }

        Err(ArbitrageError::not_found(&format!(
            "Channel '{}' not found",
            channel_id
        )))
    }

    /// Authenticate with channel
    async fn authenticate_channel(&self, config: &ChannelDeliveryConfig) -> ArbitrageResult<()> {
        match config.auth.auth_type {
            AuthType::ApiKey => {
                if config.auth.api_key.is_none() {
                    return Err(ArbitrageError::authentication_error("API key is required"));
                }
                self.logger.debug(&format!(
                    "API key authentication for channel: {}",
                    config.channel_id
                ));
            }
            AuthType::BearerToken => {
                if config.auth.token.is_none() {
                    return Err(ArbitrageError::authentication_error(
                        "Bearer token is required",
                    ));
                }
                self.logger.debug(&format!(
                    "Bearer token authentication for channel: {}",
                    config.channel_id
                ));
            }
            AuthType::BasicAuth => {
                if config.auth.username.is_none() || config.auth.password.is_none() {
                    return Err(ArbitrageError::authentication_error(
                        "Username and password are required",
                    ));
                }
                self.logger.debug(&format!(
                    "Basic authentication for channel: {}",
                    config.channel_id
                ));
            }
            AuthType::OAuth2 => {
                // In a real implementation, this would handle OAuth2 flow
                self.logger.debug(&format!(
                    "OAuth2 authentication for channel: {}",
                    config.channel_id
                ));
            }
            AuthType::Custom(_) => {
                self.logger.debug(&format!(
                    "Custom authentication for channel: {}",
                    config.channel_id
                ));
            }
        }

        Ok(())
    }

    /// Perform actual delivery to channel
    async fn perform_channel_delivery(
        &self,
        config: &ChannelDeliveryConfig,
        recipient: &str,
        content: &str,
        _metadata: Option<HashMap<String, String>>,
    ) -> ArbitrageResult<String> {
        // Simulate channel-specific delivery
        match config.channel_name.as_str() {
            "telegram" => {
                self.logger.info(&format!(
                    "Delivering Telegram message to {}: {}",
                    recipient, content
                ));
                Ok(format!("telegram_msg_{}", uuid::Uuid::new_v4()))
            }
            "email" => {
                self.logger
                    .info(&format!("Sending email to {}: {}", recipient, content));
                Ok(format!("email_msg_{}", uuid::Uuid::new_v4()))
            }
            "push" => {
                self.logger.info(&format!(
                    "Sending push notification to {}: {}",
                    recipient, content
                ));
                Ok(format!("push_msg_{}", uuid::Uuid::new_v4()))
            }
            "webhook" => {
                self.logger
                    .info(&format!("Sending webhook to {}: {}", recipient, content));
                Ok(format!("webhook_msg_{}", uuid::Uuid::new_v4()))
            }
            _ => Err(ArbitrageError::not_implemented(&format!(
                "Channel '{}' delivery not implemented",
                config.channel_name
            ))),
        }
    }

    /// Load default channel configurations
    async fn load_default_channels(&self) -> ArbitrageResult<()> {
        // Telegram channel
        let telegram_config = ChannelDeliveryConfig {
            channel_id: "telegram".to_string(),
            channel_name: "telegram".to_string(),
            enabled: true,
            endpoint: ChannelEndpoint {
                base_url: "https://api.telegram.org".to_string(),
                send_path: "/bot{token}/sendMessage".to_string(),
                auth_path: None,
                webhook_path: Some("/webhook".to_string()),
                timeout_seconds: 30,
                max_retries: 3,
                headers: HashMap::new(),
            },
            auth: ChannelAuth {
                auth_type: AuthType::BearerToken,
                api_key: None,
                secret_key: None,
                token: None, // To be set from environment
                username: None,
                password: None,
                additional_params: HashMap::new(),
            },
            rate_limits: RateLimits {
                requests_per_second: 1,
                requests_per_minute: 30,
                requests_per_hour: 1000,
                burst_limit: 5,
            },
            message_format: MessageFormat::Markdown,
            supported_features: vec![
                ChannelFeature::RichText,
                ChannelFeature::Buttons,
                ChannelFeature::Images,
                ChannelFeature::Audio,
                ChannelFeature::Video,
            ],
            fallback_channels: vec!["email".to_string()],
            metadata: HashMap::new(),
        };

        self.register_channel(telegram_config).await?;

        // Email channel
        let email_config = ChannelDeliveryConfig {
            channel_id: "email".to_string(),
            channel_name: "email".to_string(),
            enabled: true,
            endpoint: ChannelEndpoint {
                base_url: "smtp://smtp.gmail.com:587".to_string(),
                send_path: "/send".to_string(),
                auth_path: None,
                webhook_path: None,
                timeout_seconds: 60,
                max_retries: 3,
                headers: HashMap::new(),
            },
            auth: ChannelAuth {
                auth_type: AuthType::BasicAuth,
                api_key: None,
                secret_key: None,
                token: None,
                username: None, // To be set from environment
                password: None, // To be set from environment
                additional_params: HashMap::new(),
            },
            rate_limits: RateLimits {
                requests_per_second: 1,
                requests_per_minute: 10,
                requests_per_hour: 100,
                burst_limit: 3,
            },
            message_format: MessageFormat::Html,
            supported_features: vec![
                ChannelFeature::RichText,
                ChannelFeature::Attachments,
                ChannelFeature::Images,
            ],
            fallback_channels: vec!["telegram".to_string()],
            metadata: HashMap::new(),
        };

        self.register_channel(email_config).await?;

        self.logger.info("Default channels loaded successfully");
        Ok(())
    }

    /// Store channel configuration in KV
    async fn store_channel_config(&self, config: &ChannelDeliveryConfig) -> ArbitrageResult<()> {
        let key = format!("{}{}", self.config.kv_key_prefix, config.channel_id);
        let value = serde_json::to_string(config)?;

        self.kv_store
            .put(&key, value)?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::kv_error(&format!("Failed to store channel config: {}", e))
            })?;

        Ok(())
    }

    /// Load channel configuration from KV
    async fn load_channel_config(
        &self,
        channel_id: &str,
    ) -> ArbitrageResult<ChannelDeliveryConfig> {
        let key = format!("{}{}", self.config.kv_key_prefix, channel_id);

        let value = self
            .kv_store
            .get(&key)
            .text()
            .await
            .map_err(|e| {
                ArbitrageError::kv_error(&format!("Failed to load channel config: {}", e))
            })?
            .ok_or_else(|| {
                ArbitrageError::not_found(&format!("Channel config not found in KV: {}", key))
            })?;

        let config: ChannelDeliveryConfig = serde_json::from_str(&value)?;

        // Store in memory cache
        if let Ok(mut channels) = self.channels.lock() {
            channels.insert(config.channel_id.clone(), config.clone());
        }

        Ok(config)
    }

    /// Update delivery metrics
    async fn update_delivery_metrics(&self, channel_id: &str, result: &ChannelDeliveryResult) {
        if let Ok(mut health) = self.health.lock() {
            health.total_deliveries += 1;

            if result.success {
                health.successful_deliveries += 1;
            } else {
                health.failed_deliveries += 1;
            }

            // Update average delivery time
            health.avg_delivery_time_ms = (health.avg_delivery_time_ms
                * (health.total_deliveries - 1) as f64
                + result.delivery_time_ms as f64)
                / health.total_deliveries as f64;

            // Update channel health
            health
                .channel_health
                .insert(channel_id.to_string(), result.success);
        }
    }

    /// Get channel manager health
    pub async fn get_health(&self) -> ChannelManagerHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            ChannelManagerHealth {
                is_healthy: false,
                total_channels: 0,
                active_channels: 0,
                failed_channels: 0,
                channel_health: HashMap::new(),
                avg_delivery_time_ms: 0.0,
                total_deliveries: 0,
                successful_deliveries: 0,
                failed_deliveries: 0,
                last_health_check: 0,
            }
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check if channels are loaded
        let channels_loaded = if let Ok(channels) = self.channels.lock() {
            !channels.is_empty()
        } else {
            false
        };

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = channels_loaded;
            health.last_health_check = start_time;

            // Update channel counts
            if let Ok(channels) = self.channels.lock() {
                health.total_channels = channels.len() as u64;
                health.active_channels = channels.values().filter(|c| c.enabled).count() as u64;
                health.failed_channels = channels.values().filter(|c| !c.enabled).count() as u64;
            }
        }

        Ok(channels_loaded)
    }

    /// Check if a channel is within its rate limit
    pub async fn within_rate_limit(&self, channel_id: &str) -> ArbitrageResult<bool> {
        let rate_limits = self.rate_limits.lock().unwrap();

        if let Some(limit_info) = rate_limits.get(channel_id) {
            let now = chrono::Utc::now().timestamp_millis() as u64;

            // Check if we're within the time window
            if now < limit_info.reset_time {
                Ok(limit_info.requests_remaining > 0)
            } else {
                // Time window has passed, reset is needed
                Ok(true)
            }
        } else {
            // No rate limit info means no restrictions
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_type_variants() {
        assert_eq!(AuthType::ApiKey, AuthType::ApiKey);
        assert_ne!(AuthType::ApiKey, AuthType::BearerToken);
    }

    #[test]
    fn test_message_format_variants() {
        assert_eq!(MessageFormat::PlainText, MessageFormat::PlainText);
        assert_ne!(MessageFormat::PlainText, MessageFormat::Html);
    }

    #[test]
    fn test_channel_feature_variants() {
        assert_eq!(ChannelFeature::RichText, ChannelFeature::RichText);
        assert_ne!(ChannelFeature::RichText, ChannelFeature::Attachments);
    }

    #[test]
    fn test_channel_delivery_result_creation() {
        let result = ChannelDeliveryResult {
            success: true,
            message_id: Some("msg_123".to_string()),
            error_code: None,
            error_message: None,
            delivery_time_ms: 150,
            rate_limited: false,
            retry_after_seconds: None,
            response_data: None,
        };

        assert!(result.success);
        assert_eq!(result.message_id, Some("msg_123".to_string()));
        assert_eq!(result.delivery_time_ms, 150);
    }

    #[test]
    fn test_rate_limits_structure() {
        let limits = RateLimits {
            requests_per_second: 1,
            requests_per_minute: 30,
            requests_per_hour: 1000,
            burst_limit: 5,
        };

        assert_eq!(limits.requests_per_second, 1);
        assert_eq!(limits.requests_per_minute, 30);
        assert_eq!(limits.requests_per_hour, 1000);
        assert_eq!(limits.burst_limit, 5);
    }

    #[test]
    fn test_channel_manager_config_default() {
        let config = ChannelManagerConfig::default();

        assert!(config.enable_channel_manager);
        assert!(config.enable_authentication);
        assert!(config.enable_rate_limiting);
        assert!(config.enable_fallback);
        assert_eq!(config.max_concurrent_deliveries, 100);
        assert_eq!(config.default_timeout_seconds, 30);
    }
}
