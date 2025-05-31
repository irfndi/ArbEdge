use crate::services::core::infrastructure::DatabaseManager;
use crate::types::{
    GroupAdminRole, GroupAISettings, GroupChannelConfig, GroupRegistration, 
    GroupSubscriptionSettings, SubscriptionTier, UserProfile, ChatResponseMode,
    AIEnhancementMode, ApiKeyProvider
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use std::sync::Arc;
use worker::console_log;

/// Service for managing group/channel configurations and admin roles
#[derive(Clone)]
pub struct GroupManagementService {
    d1_service: Arc<DatabaseManager>,
    kv_service: Arc<worker::kv::KvStore>,
}

impl GroupManagementService {
    pub fn new(d1_service: DatabaseManager, kv_service: worker::kv::KvStore) -> Self {
        Self {
            d1_service: Arc::new(d1_service),
            kv_service: Arc::new(kv_service),
        }
    }

    /// Register a new group/channel
    pub async fn register_group(
        &self,
        group_id: &str,
        group_name: &str,
        group_type: &str, // "group", "supergroup", "channel"
        admin_user_id: &str,
    ) -> ArbitrageResult<GroupRegistration> {
        console_log!("📊 Registering group: {} ({})", group_name, group_id);

        let now = chrono::Utc::now().timestamp_millis() as u64;
        
        // Create group registration
        let registration = GroupRegistration {
            group_id: group_id.to_string(),
            group_name: group_name.to_string(),
            registered_by: admin_user_id.to_string(),
            registration_date: now,
            is_active: true,
            subscription_tier: SubscriptionTier::Free,
        };

        // Store in D1
        let query = r#"
            INSERT OR REPLACE INTO group_registrations (
                group_id, group_name, registered_by, registration_date, 
                is_active, subscription_tier
            ) VALUES (?, ?, ?, ?, ?, ?)
        "#;

        self.d1_service.execute(query, &[
            serde_json::Value::String(registration.group_id.clone()),
            serde_json::Value::String(registration.group_name.clone()),
            serde_json::Value::String(registration.registered_by.clone()),
            serde_json::Value::Number(serde_json::Number::from(registration.registration_date)),
            serde_json::Value::Bool(registration.is_active),
            serde_json::Value::String(registration.subscription_tier.to_string()),
        ]).await?;

        // Create group configuration
        let config = match group_type {
            "channel" => GroupChannelConfig::new_channel(group_id.to_string(), admin_user_id.to_string()),
            _ => GroupChannelConfig::new_group(group_id.to_string(), admin_user_id.to_string()),
        };

        self.store_group_config(&config).await?;

        // Create AI settings
        let ai_settings = GroupAISettings::new(group_id.to_string(), admin_user_id.to_string());
        self.store_group_ai_settings(&ai_settings).await?;

        // Create subscription settings
        let subscription_settings = GroupSubscriptionSettings::new(group_id.to_string(), admin_user_id.to_string());
        self.store_group_subscription_settings(&subscription_settings).await?;

        console_log!("✅ Group registered successfully: {}", group_id);
        Ok(registration)
    }

    /// Get group configuration
    pub async fn get_group_config(&self, group_id: &str) -> ArbitrageResult<Option<GroupChannelConfig>> {
        let cache_key = format!("group_config:{}", group_id);
        
        // Try cache first
        if let Some(cached) = self.kv_service.get(&cache_key).text().await? {
            if let Ok(config) = serde_json::from_str::<GroupChannelConfig>(&cached) {
                return Ok(Some(config));
            }
        }

        // Query from D1
        let query = r#"
            SELECT * FROM group_configurations WHERE group_id = ?
        "#;

        let result = self.d1_service.query(query, &[
            serde_json::Value::String(group_id.to_string()),
        ]).await?;

        let rows = result.results::<std::collections::HashMap<String, serde_json::Value>>()?;
        
        if let Some(row) = rows.first() {
            let config = self.parse_group_config_row(row)?;
            
            // Cache the result
            let config_json = serde_json::to_string(&config)?;
            self.kv_service
                .put(&cache_key, &config_json)?
                .expiration_ttl(3600) // 1 hour
                .execute()
                .await?;
            
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Store group configuration
    pub async fn store_group_config(&self, config: &GroupChannelConfig) -> ArbitrageResult<()> {
        let query = r#"
            INSERT OR REPLACE INTO group_configurations (
                group_id, group_type, opportunities_enabled, manual_requests_enabled,
                trading_enabled, ai_enhancement_enabled, take_action_buttons,
                managed_by_admins
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let admins_json = serde_json::to_string(&config.managed_by_admins)?;

        self.d1_service.execute(query, &[
            serde_json::Value::String(config.group_id.clone()),
            serde_json::Value::String(config.group_type.clone()),
            serde_json::Value::Bool(config.opportunities_enabled),
            serde_json::Value::Bool(config.manual_requests_enabled),
            serde_json::Value::Bool(config.trading_enabled),
            serde_json::Value::Bool(config.ai_enhancement_enabled),
            serde_json::Value::Bool(config.take_action_buttons),
            serde_json::Value::String(admins_json),
        ]).await?;

        // Invalidate cache
        let cache_key = format!("group_config:{}", config.group_id);
        self.kv_service.delete(&cache_key).await?;

        Ok(())
    }

    /// Get group AI settings
    pub async fn get_group_ai_settings(&self, group_id: &str) -> ArbitrageResult<Option<GroupAISettings>> {
        let cache_key = format!("group_ai_settings:{}", group_id);
        
        // Try cache first
        if let Some(cached) = self.kv_service.get(&cache_key).text().await? {
            if let Ok(settings) = serde_json::from_str::<GroupAISettings>(&cached) {
                return Ok(Some(settings));
            }
        }

        // Query from D1
        let query = r#"
            SELECT * FROM group_ai_settings WHERE group_id = ?
        "#;

        let result = self.d1_service.query(query, &[
            serde_json::Value::String(group_id.to_string()),
        ]).await?;

        let rows = result.results::<std::collections::HashMap<String, serde_json::Value>>()?;
        
        if let Some(row) = rows.first() {
            let settings = self.parse_group_ai_settings_row(row)?;
            
            // Cache the result
            let settings_json = serde_json::to_string(&settings)?;
            self.kv_service
                .put(&cache_key, &settings_json)?
                .expiration_ttl(3600) // 1 hour
                .execute()
                .await?;
            
            Ok(Some(settings))
        } else {
            Ok(None)
        }
    }

    /// Store group AI settings
    pub async fn store_group_ai_settings(&self, settings: &GroupAISettings) -> ArbitrageResult<()> {
        let query = r#"
            INSERT OR REPLACE INTO group_ai_settings (
                group_id, ai_enabled, ai_provider, ai_model, managed_by_user_id,
                byok_enabled, group_ai_key_id, created_at, updated_at, settings_metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let provider_str = settings.ai_provider.as_ref().map(|p| p.to_string());

        self.d1_service.execute(query, &[
            serde_json::Value::String(settings.group_id.clone()),
            serde_json::Value::Bool(settings.ai_enabled),
            serde_json::Value::String(provider_str.unwrap_or_default()),
            serde_json::Value::String(settings.ai_model.clone().unwrap_or_default()),
            serde_json::Value::String(settings.managed_by_user_id.clone()),
            serde_json::Value::Bool(settings.byok_enabled),
            serde_json::Value::String(settings.group_ai_key_id.clone().unwrap_or_default()),
            serde_json::Value::Number(serde_json::Number::from(settings.created_at)),
            serde_json::Value::Number(serde_json::Number::from(settings.updated_at)),
            settings.settings_metadata.clone(),
        ]).await?;

        // Invalidate cache
        let cache_key = format!("group_ai_settings:{}", settings.group_id);
        self.kv_service.delete(&cache_key).await?;

        Ok(())
    }

    /// Check if user is admin of a group
    pub async fn is_group_admin(&self, group_id: &str, user_id: &str) -> ArbitrageResult<bool> {
        if let Some(config) = self.get_group_config(group_id).await? {
            return Ok(config.is_admin(user_id));
        }
        Ok(false)
    }

    /// Get response mode for a chat context
    pub async fn get_chat_response_mode(&self, group_id: &str, chat_type: &str) -> ArbitrageResult<ChatResponseMode> {
        match chat_type {
            "private" => Ok(ChatResponseMode::FullInteractive),
            "group" | "supergroup" => {
                if let Some(config) = self.get_group_config(group_id).await? {
                    if config.opportunities_enabled {
                        Ok(ChatResponseMode::OpportunitiesOnly)
                    } else {
                        Ok(ChatResponseMode::BroadcastOnly)
                    }
                } else {
                    Ok(ChatResponseMode::OpportunitiesOnly)
                }
            }
            "channel" => Ok(ChatResponseMode::BroadcastOnly),
            _ => Ok(ChatResponseMode::OpportunitiesOnly),
        }
    }

    /// Get AI enhancement mode for a group
    pub async fn get_ai_enhancement_mode(&self, group_id: &str) -> ArbitrageResult<AIEnhancementMode> {
        if let Some(settings) = self.get_group_ai_settings(group_id).await? {
            Ok(settings.get_ai_enhancement_mode())
        } else {
            Ok(AIEnhancementMode::Disabled)
        }
    }

    /// Store group subscription settings
    async fn store_group_subscription_settings(&self, settings: &GroupSubscriptionSettings) -> ArbitrageResult<()> {
        let query = r#"
            INSERT OR REPLACE INTO group_subscription_settings (
                group_id, subscription_tier, managed_by_user_id, expires_at,
                auto_renew, payment_method, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        self.d1_service.execute(query, &[
            serde_json::Value::String(settings.group_id.clone()),
            serde_json::Value::String(settings.subscription_tier.to_string()),
            serde_json::Value::String(settings.managed_by_user_id.clone()),
            serde_json::Value::Number(serde_json::Number::from(settings.expires_at.unwrap_or(0))),
            serde_json::Value::Bool(settings.auto_renew),
            serde_json::Value::String(settings.payment_method.clone().unwrap_or_default()),
            serde_json::Value::Number(serde_json::Number::from(settings.created_at)),
            serde_json::Value::Number(serde_json::Number::from(settings.updated_at)),
        ]).await?;

        Ok(())
    }

    /// Parse group config from database row
    fn parse_group_config_row(&self, row: &std::collections::HashMap<String, serde_json::Value>) -> ArbitrageResult<GroupChannelConfig> {
        let admins_str = row.get("managed_by_admins")
            .and_then(|v| v.as_str())
            .unwrap_or("[]");
        
        let managed_by_admins: Vec<String> = serde_json::from_str(admins_str)
            .unwrap_or_default();

        Ok(GroupChannelConfig {
            group_id: row.get("group_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            group_type: row.get("group_type")
                .and_then(|v| v.as_str())
                .unwrap_or("group")
                .to_string(),
            opportunities_enabled: row.get("opportunities_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            manual_requests_enabled: row.get("manual_requests_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            trading_enabled: row.get("trading_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            ai_enhancement_enabled: row.get("ai_enhancement_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            take_action_buttons: row.get("take_action_buttons")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            managed_by_admins,
        })
    }

    /// Parse group AI settings from database row
    fn parse_group_ai_settings_row(&self, row: &std::collections::HashMap<String, serde_json::Value>) -> ArbitrageResult<GroupAISettings> {
        let ai_provider = row.get("ai_provider")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "openai" => Some(ApiKeyProvider::OpenAI),
                "anthropic" => Some(ApiKeyProvider::Anthropic),
                "ai" => Some(ApiKeyProvider::AI),
                _ => None,
            });

        Ok(GroupAISettings {
            group_id: row.get("group_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            ai_enabled: row.get("ai_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            ai_provider,
            ai_model: row.get("ai_model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            managed_by_user_id: row.get("managed_by_user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            byok_enabled: row.get("byok_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            group_ai_key_id: row.get("group_ai_key_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            created_at: row.get("created_at")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            updated_at: row.get("updated_at")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            settings_metadata: row.get("settings_metadata")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_group_registration() {
        let group_id = "test_group_123";
        let group_name = "Test Group";
        let admin_user_id = "admin_123";

        // Test group registration data structure
        let registration = GroupRegistration {
            group_id: group_id.to_string(),
            group_name: group_name.to_string(),
            registered_by: admin_user_id.to_string(),
            registration_date: chrono::Utc::now().timestamp_millis() as u64,
            is_active: true,
            subscription_tier: SubscriptionTier::Free,
        };

        assert_eq!(registration.group_id, group_id);
        assert_eq!(registration.subscription_tier, SubscriptionTier::Free);
        assert!(registration.is_active);
    }

    #[tokio::test]
    async fn test_group_config() {
        let group_id = "test_group_123";
        let admin_user_id = "admin_123";

        let config = GroupChannelConfig::new_group(group_id.to_string(), admin_user_id.to_string());

        assert_eq!(config.group_id, group_id);
        assert_eq!(config.group_type, "group");
        assert!(config.opportunities_enabled);
        assert!(!config.manual_requests_enabled);
        assert!(!config.trading_enabled);
        assert!(config.take_action_buttons);
        assert!(config.is_admin(admin_user_id));
    }

    #[tokio::test]
    async fn test_ai_settings() {
        let group_id = "test_group_123";
        let admin_user_id = "admin_123";

        let mut settings = GroupAISettings::new(group_id.to_string(), admin_user_id.to_string());
        
        assert_eq!(settings.group_id, group_id);
        assert!(!settings.ai_enabled);
        assert!(settings.byok_enabled);
        assert_eq!(settings.get_ai_enhancement_mode(), AIEnhancementMode::Disabled);

        // Test enabling AI
        settings.enable_ai(ApiKeyProvider::OpenAI, Some("gpt-4".to_string()));
        assert!(settings.ai_enabled);
        assert_eq!(settings.get_ai_enhancement_mode(), AIEnhancementMode::BYOKOnly);
    }
} 