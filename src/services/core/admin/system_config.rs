use crate::utils::{ArbitrageResult, ArbitrageError};
use worker::{Env, kv::KvStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// System configuration service for super admin operations
#[derive(Debug, Clone)]
pub struct SystemConfigService {
    kv_store: KvStore,
    env: Env,
}

impl SystemConfigService {
    pub fn new(env: Env, kv_store: KvStore) -> Self {
        Self { kv_store, env }
    }

    /// Get system configuration
    pub async fn get_system_config(&self) -> ArbitrageResult<SystemConfig> {
        let config_key = "system_config";
        
        if let Some(config_data) = self.kv_store.get(config_key).text().await? {
            let config = serde_json::from_str::<SystemConfig>(&config_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse system config: {}", e)))?;
            Ok(config)
        } else {
            // Return default configuration if none exists
            Ok(SystemConfig::default())
        }
    }

    /// Update system configuration (super admin only)
    pub async fn update_system_config(&self, config: SystemConfig) -> ArbitrageResult<()> {
        let config_key = "system_config";
        let config_data = serde_json::to_string(&config)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize system config: {}", e)))?;

        self.kv_store.put(config_key, &config_data)?
            .execute()
            .await?;

        Ok(())
    }

    /// Get feature flags configuration
    pub async fn get_feature_flags(&self) -> ArbitrageResult<FeatureFlagsConfig> {
        let flags_key = "feature_flags";
        
        if let Some(flags_data) = self.kv_store.get(flags_key).text().await? {
            let flags = serde_json::from_str::<FeatureFlagsConfig>(&flags_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse feature flags: {}", e)))?;
            Ok(flags)
        } else {
            Ok(FeatureFlagsConfig::default())
        }
    }

    /// Update feature flags (super admin only)
    pub async fn update_feature_flags(&self, flags: FeatureFlagsConfig) -> ArbitrageResult<()> {
        let flags_key = "feature_flags";
        let flags_data = serde_json::to_string(&flags)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize feature flags: {}", e)))?;

        self.kv_store.put(flags_key, &flags_data)?
            .execute()
            .await?;

        Ok(())
    }

    /// Get rate limiting configuration
    pub async fn get_rate_limits(&self) -> ArbitrageResult<RateLimitConfig> {
        let limits_key = "rate_limits";
        
        if let Some(limits_data) = self.kv_store.get(limits_key).text().await? {
            let limits = serde_json::from_str::<RateLimitConfig>(&limits_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse rate limits: {}", e)))?;
            Ok(limits)
        } else {
            Ok(RateLimitConfig::default())
        }
    }

    /// Update rate limiting configuration (super admin only)
    pub async fn update_rate_limits(&self, limits: RateLimitConfig) -> ArbitrageResult<()> {
        let limits_key = "rate_limits";
        let limits_data = serde_json::to_string(&limits)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize rate limits: {}", e)))?;

        self.kv_store.put(limits_key, &limits_data)?
            .execute()
            .await?;

        Ok(())
    }

    /// Get API configuration
    pub async fn get_api_config(&self) -> ArbitrageResult<ApiConfig> {
        let api_key = "api_config";
        
        if let Some(api_data) = self.kv_store.get(api_key).text().await? {
            let api_config = serde_json::from_str::<ApiConfig>(&api_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse API config: {}", e)))?;
            Ok(api_config)
        } else {
            Ok(ApiConfig::default())
        }
    }

    /// Update API configuration (super admin only)
    pub async fn update_api_config(&self, config: ApiConfig) -> ArbitrageResult<()> {
        let api_key = "api_config";
        let api_data = serde_json::to_string(&config)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize API config: {}", e)))?;

        self.kv_store.put(api_key, &api_data)?
            .execute()
            .await?;

        Ok(())
    }

    /// Get maintenance mode status
    pub async fn get_maintenance_mode(&self) -> ArbitrageResult<MaintenanceMode> {
        let maintenance_key = "maintenance_mode";
        
        if let Some(maintenance_data) = self.kv_store.get(maintenance_key).text().await? {
            let maintenance = serde_json::from_str::<MaintenanceMode>(&maintenance_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse maintenance mode: {}", e)))?;
            Ok(maintenance)
        } else {
            Ok(MaintenanceMode::default())
        }
    }

    /// Set maintenance mode (super admin only)
    pub async fn set_maintenance_mode(&self, maintenance: MaintenanceMode) -> ArbitrageResult<()> {
        let maintenance_key = "maintenance_mode";
        let maintenance_data = serde_json::to_string(&maintenance)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize maintenance mode: {}", e)))?;

        self.kv_store.put(maintenance_key, &maintenance_data)?
            .execute()
            .await?;

        Ok(())
    }

    /// Get all configuration as a summary
    pub async fn get_config_summary(&self) -> ArbitrageResult<ConfigSummary> {
        let system_config = self.get_system_config().await?;
        let feature_flags = self.get_feature_flags().await?;
        let rate_limits = self.get_rate_limits().await?;
        let api_config = self.get_api_config().await?;
        let maintenance_mode = self.get_maintenance_mode().await?;

        Ok(ConfigSummary {
            system_config,
            feature_flags,
            rate_limits,
            api_config,
            maintenance_mode,
        })
    }
}

/// System configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemConfig {
    pub app_name: String,
    pub app_version: String,
    pub environment: String, // "development", "staging", "production"
    pub max_concurrent_users: u32,
    pub session_timeout_minutes: u32,
    pub default_opportunity_ttl_seconds: u32,
    pub max_opportunities_per_user_per_hour: u32,
    pub enable_analytics: bool,
    pub enable_logging: bool,
    pub log_level: String, // "debug", "info", "warn", "error"
    pub updated_at: u64,
    pub updated_by: String,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            app_name: "ArbEdge".to_string(),
            app_version: "0.1.0".to_string(),
            environment: "development".to_string(),
            max_concurrent_users: 10000,
            session_timeout_minutes: 30,
            default_opportunity_ttl_seconds: 300, // 5 minutes
            max_opportunities_per_user_per_hour: 100,
            enable_analytics: true,
            enable_logging: true,
            log_level: "info".to_string(),
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_by: "system".to_string(),
        }
    }
}

/// Feature flags configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlagsConfig {
    pub modular_auth: bool,
    pub modular_telegram: bool,
    pub modular_trading: bool,
    pub modular_ai: bool,
    pub modular_analytics: bool,
    pub super_admin_priority: bool,
    pub beta_features_enabled: bool,
    pub ai_enhancement_enabled: bool,
    pub group_management_enabled: bool,
    pub advanced_analytics_enabled: bool,
    pub updated_at: u64,
    pub updated_by: String,
}

impl Default for FeatureFlagsConfig {
    fn default() -> Self {
        Self {
            modular_auth: false,
            modular_telegram: false,
            modular_trading: false,
            modular_ai: false,
            modular_analytics: false,
            super_admin_priority: true,
            beta_features_enabled: true,
            ai_enhancement_enabled: true,
            group_management_enabled: true,
            advanced_analytics_enabled: false,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_by: "system".to_string(),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitConfig {
    pub api_requests_per_minute: u32,
    pub telegram_messages_per_minute: u32,
    pub opportunities_per_hour: u32,
    pub login_attempts_per_hour: u32,
    pub enable_rate_limiting: bool,
    pub rate_limit_window_minutes: u32,
    pub updated_at: u64,
    pub updated_by: String,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            api_requests_per_minute: 60,
            telegram_messages_per_minute: 20,
            opportunities_per_hour: 100,
            login_attempts_per_hour: 10,
            enable_rate_limiting: true,
            rate_limit_window_minutes: 60,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_by: "system".to_string(),
        }
    }
}

/// API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiConfig {
    pub enable_cors: bool,
    pub allowed_origins: Vec<String>,
    pub api_timeout_seconds: u32,
    pub max_request_size_mb: u32,
    pub enable_compression: bool,
    pub enable_caching: bool,
    pub cache_ttl_seconds: u32,
    pub updated_at: u64,
    pub updated_by: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enable_cors: true,
            allowed_origins: vec!["*".to_string()],
            api_timeout_seconds: 30,
            max_request_size_mb: 10,
            enable_compression: true,
            enable_caching: true,
            cache_ttl_seconds: 300, // 5 minutes
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_by: "system".to_string(),
        }
    }
}

/// Maintenance mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceMode {
    pub enabled: bool,
    pub message: String,
    pub allowed_user_ids: Vec<String>, // Users who can still access during maintenance
    pub estimated_duration_minutes: Option<u32>,
    pub started_at: Option<u64>,
    pub updated_at: u64,
    pub updated_by: String,
}

impl Default for MaintenanceMode {
    fn default() -> Self {
        Self {
            enabled: false,
            message: "System is under maintenance. Please try again later.".to_string(),
            allowed_user_ids: Vec::new(),
            estimated_duration_minutes: None,
            started_at: None,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_by: "system".to_string(),
        }
    }
}

/// Configuration summary for admin dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSummary {
    pub system_config: SystemConfig,
    pub feature_flags: FeatureFlagsConfig,
    pub rate_limits: RateLimitConfig,
    pub api_config: ApiConfig,
    pub maintenance_mode: MaintenanceMode,
} 