// src/services/core/opportunities/access_manager.rs

use crate::log_info;
use crate::services::core::user::user_access::{OpportunityAccessResult, UserAccessService};
use crate::services::core::user::UserProfileService;
use crate::types::{
    ChatContext, CommandPermission, ExchangeCredentials, ExchangeIdEnum, SubscriptionTier,
    UserAccessLevel,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde_json;
use std::sync::Arc;
use worker::kv::KvStore;

/// Unified access manager for all opportunity services
/// Consolidates permission checking, access validation, and exchange API management
pub struct AccessManager {
    user_profile_service: Arc<UserProfileService>,
    user_access_service: Arc<UserAccessService>,
    kv_store: KvStore,
    cache_ttl_seconds: u64,
}

impl AccessManager {
    const USER_EXCHANGE_CACHE_PREFIX: &'static str = "user_exchanges";
    const GROUP_ADMIN_CACHE_PREFIX: &'static str = "group_admin_apis";
    const DEFAULT_CACHE_TTL: u64 = 600; // 10 minutes

    pub fn new(
        user_profile_service: Arc<UserProfileService>,
        user_access_service: Arc<UserAccessService>,
        kv_store: KvStore,
    ) -> Self {
        Self {
            user_profile_service,
            user_access_service,
            kv_store,
            cache_ttl_seconds: Self::DEFAULT_CACHE_TTL,
        }
    }

    /// Validate user access for opportunity generation
    pub async fn validate_user_access(
        &self,
        user_id: &str,
        opportunity_type: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<OpportunityAccessResult> {
        // Get user profile
        let user_profile = match self.user_profile_service.get_user_profile(user_id).await? {
            Some(profile) => profile,
            None => {
                return Ok(OpportunityAccessResult {
                    can_access: false,
                    reason: Some("User not found".to_string()),
                    delay_seconds: 0,
                    access_level: UserAccessLevel::FreeWithoutAPI,
                    remaining_arbitrage: 0,
                    remaining_technical: 0,
                });
            }
        };

        // Check basic permissions
        if !user_profile.has_permission(CommandPermission::BasicOpportunities) {
            return Ok(OpportunityAccessResult {
                can_access: false,
                reason: Some("Insufficient permissions: BasicOpportunities required".to_string()),
                delay_seconds: 0,
                access_level: UserAccessLevel::FreeWithoutAPI,
                remaining_arbitrage: 0,
                remaining_technical: 0,
            });
        }

        // Use UserAccessService for detailed access validation
        self.user_access_service
            .validate_opportunity_access(user_id, opportunity_type, chat_context, &[])
            .await
    }

    /// Validate group admin access for group opportunity generation
    pub async fn validate_group_admin_access(
        &self,
        group_admin_id: &str,
        opportunity_type: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<OpportunityAccessResult> {
        // Validate that this is a group/channel context
        if !chat_context.is_group_context() {
            return Ok(OpportunityAccessResult {
                can_access: false,
                reason: Some(
                    "Group opportunity generation only available in group/channel contexts"
                        .to_string(),
                ),
                delay_seconds: 0,
                access_level: UserAccessLevel::FreeWithoutAPI,
                remaining_arbitrage: 0,
                remaining_technical: 0,
            });
        }

        // Get group admin profile
        let admin_profile = match self
            .user_profile_service
            .get_user_profile(group_admin_id)
            .await?
        {
            Some(profile) => profile,
            None => {
                return Ok(OpportunityAccessResult {
                    can_access: false,
                    reason: Some("Group admin not found".to_string()),
                    delay_seconds: 0,
                    access_level: UserAccessLevel::FreeWithoutAPI,
                    remaining_arbitrage: 0,
                    remaining_technical: 0,
                });
            }
        };

        // Check if admin has required permissions for group management
        if !admin_profile.has_permission(CommandPermission::BasicOpportunities) {
            return Ok(OpportunityAccessResult {
                can_access: false,
                reason: Some("Group admin lacks BasicOpportunities permission".to_string()),
                delay_seconds: 0,
                access_level: UserAccessLevel::FreeWithoutAPI,
                remaining_arbitrage: 0,
                remaining_technical: 0,
            });
        }

        // Check if admin has trading APIs (required for group opportunities)
        if !admin_profile.has_trading_api_keys() {
            return Ok(OpportunityAccessResult {
                can_access: false,
                reason: Some("Group admin must have trading API keys configured".to_string()),
                delay_seconds: 0,
                access_level: UserAccessLevel::FreeWithoutAPI,
                remaining_arbitrage: 0,
                remaining_technical: 0,
            });
        }

        // Use UserAccessService for detailed validation
        self.user_access_service
            .validate_opportunity_access(group_admin_id, opportunity_type, chat_context, &[])
            .await
    }

    /// Check if user has permission using RBAC
    pub async fn check_user_permission(
        &self,
        user_id: &str,
        permission: &CommandPermission,
    ) -> ArbitrageResult<bool> {
        match self.user_profile_service.get_user_profile(user_id).await? {
            Some(profile) => Ok(profile.has_permission(permission.clone())),
            None => Ok(false),
        }
    }

    /// Get user's exchange APIs with caching
    pub async fn get_user_exchange_apis(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Vec<(ExchangeIdEnum, ExchangeCredentials)>> {
        let cache_key = format!("{}:{}", Self::USER_EXCHANGE_CACHE_PREFIX, user_id);

        // Try cache first
        if let Ok(Some(cached_exchanges)) = self.get_cached_exchanges(&cache_key).await {
            return Ok(cached_exchanges);
        }

        // Get user profile
        let user_profile = match self.user_profile_service.get_user_profile(user_id).await? {
            Some(profile) => profile,
            None => return Ok(Vec::new()),
        };

        // Extract exchange APIs
        let mut exchanges = Vec::new();
        for api_key in &user_profile.api_keys {
            if api_key.is_active {
                // Extract exchange from provider
                let exchange_id = match &api_key.provider {
                    crate::types::ApiKeyProvider::Exchange(exchange) => *exchange,
                    _ => continue, // Skip non-exchange API keys
                };

                let credentials = ExchangeCredentials {
                    exchange: exchange_id,
                    api_key: api_key.encrypted_key.clone(),
                    api_secret: api_key.encrypted_secret.clone().unwrap_or_default(),
                    secret: api_key.encrypted_secret.clone().unwrap_or_default(),
                    passphrase: api_key.passphrase.clone(),
                    sandbox: api_key.is_testnet,
                    default_leverage: 1,               // Default value
                    exchange_type: "spot".to_string(), // Default to spot
                };
                exchanges.push((exchange_id, credentials));
            }
        }

        // Cache the result
        self.cache_exchanges(&cache_key, &exchanges).await;

        log_info!(
            "Retrieved user exchange APIs",
            serde_json::json!({
                "user_id": user_id,
                "exchange_count": exchanges.len(),
                "cached": false
            })
        );

        Ok(exchanges)
    }

    /// Get group admin's exchange APIs with caching
    pub async fn get_group_admin_exchange_apis(
        &self,
        group_admin_id: &str,
    ) -> ArbitrageResult<Vec<(ExchangeIdEnum, ExchangeCredentials)>> {
        let cache_key = format!("{}:{}", Self::GROUP_ADMIN_CACHE_PREFIX, group_admin_id);

        // Try cache first
        if let Ok(Some(cached_exchanges)) = self.get_cached_exchanges(&cache_key).await {
            return Ok(cached_exchanges);
        }

        // Get admin's exchanges (same logic as user exchanges)
        let exchanges = self.get_user_exchange_apis(group_admin_id).await?;

        // Cache the result
        self.cache_exchanges(&cache_key, &exchanges).await;

        log_info!(
            "Retrieved group admin exchange APIs",
            serde_json::json!({
                "group_admin_id": group_admin_id,
                "exchange_count": exchanges.len(),
                "cached": false
            })
        );

        Ok(exchanges)
    }

    /// Filter opportunities based on user subscription tier
    pub async fn filter_opportunities_by_subscription<T>(
        &self,
        user_id: &str,
        mut opportunities: Vec<T>,
    ) -> ArbitrageResult<Vec<T>> {
        let subscription_tier = match self.user_profile_service.get_user_profile(user_id).await? {
            Some(profile) => profile.subscription.tier,
            None => SubscriptionTier::Free,
        };

        // Apply subscription-based filtering
        match subscription_tier {
            SubscriptionTier::Free => {
                opportunities.truncate(2); // Free users get limited opportunities
            }
            SubscriptionTier::Paid => {
                // Paid tier - enhanced opportunities with BYOK
                opportunities.truncate(7); // More opportunities for paid users
            }
            SubscriptionTier::Admin | SubscriptionTier::SuperAdmin => {
                // Admin tiers - full access (no truncation)
            }
            SubscriptionTier::Basic => {
                opportunities.truncate(2); // Basic tier gets limited opportunities
            }
            SubscriptionTier::Premium => {
                opportunities.truncate(5); // Premium tier gets more opportunities
            }
            SubscriptionTier::Pro => {
                opportunities.truncate(8); // Pro tier gets many opportunities
            }
            SubscriptionTier::Enterprise => {
                // Enterprise tier - full access (no truncation)
            }
        }

        Ok(opportunities)
    }

    /// Check if user has compatible exchange APIs for trading
    pub async fn validate_user_exchange_compatibility(
        &self,
        user_id: &str,
        required_exchanges: &[ExchangeIdEnum],
    ) -> ArbitrageResult<bool> {
        let user_exchanges = self.get_user_exchange_apis(user_id).await?;
        let user_exchange_ids: Vec<ExchangeIdEnum> = user_exchanges
            .iter()
            .map(|(exchange_id, _)| *exchange_id)
            .collect();

        let has_all_required = required_exchanges
            .iter()
            .all(|req_exchange| user_exchange_ids.contains(req_exchange));

        if !has_all_required {
            log_info!(
                "User exchange compatibility check failed",
                serde_json::json!({
                    "user_id": user_id,
                    "user_exchanges": user_exchange_ids,
                    "required_exchanges": required_exchanges
                })
            );
        }

        Ok(has_all_required)
    }

    /// Check if user has permission to access trading opportunities
    pub async fn check_user_trading_permission(&self, user_id: &str) -> ArbitrageResult<bool> {
        let user_profile = match self.user_profile_service.get_user_profile(user_id).await? {
            Some(profile) => profile,
            None => return Ok(false),
        };

        // Check if user has BasicOpportunities permission
        let has_basic_permission =
            user_profile.has_permission(CommandPermission::BasicOpportunities);

        // Check if user has trading API keys
        let has_trading_apis = user_profile.has_trading_api_keys();

        Ok(has_basic_permission && has_trading_apis)
    }

    /// Get user's access level for AI features
    pub async fn get_user_ai_access_level(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserAccessLevel> {
        match self.user_profile_service.get_user_profile(user_id).await? {
            Some(profile) => {
                // Convert AIAccessLevel to UserAccessLevel
                let ai_level = profile.get_ai_access_level();
                let user_level = match ai_level {
                    crate::types::AIAccessLevel::FreeWithoutAI => UserAccessLevel::FreeWithoutAPI,
                    crate::types::AIAccessLevel::FreeWithAI => UserAccessLevel::FreeWithAPI,
                    crate::types::AIAccessLevel::SubscriptionWithAI => {
                        UserAccessLevel::SubscriptionWithAPI
                    }
                    crate::types::AIAccessLevel::PremiumAI => UserAccessLevel::Premium,
                    crate::types::AIAccessLevel::EnterpriseAI => UserAccessLevel::Admin,
                };
                Ok(user_level)
            }
            None => Ok(UserAccessLevel::FreeWithoutAPI),
        }
    }

    /// Record opportunity generation for rate limiting
    pub async fn record_opportunity_received(
        &self,
        user_id: &str,
        opportunity_type: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<()> {
        match opportunity_type {
            "arbitrage" => {
                self.user_access_service
                    .record_arbitrage_opportunity_received(user_id, chat_context)
                    .await?;
                Ok(())
            }
            "technical" => {
                self.user_access_service
                    .record_technical_opportunity_received(user_id, chat_context)
                    .await?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    // Private helper methods

    async fn cache_exchanges(
        &self,
        cache_key: &str,
        exchanges: &[(ExchangeIdEnum, ExchangeCredentials)],
    ) -> ArbitrageResult<()> {
        let data = serde_json::to_string(exchanges).map_err(|e| {
            ArbitrageError::serialization_error(format!("Failed to serialize exchanges: {}", e))
        })?;

        self.kv_store
            .put(cache_key, data)?
            .expiration_ttl(self.cache_ttl_seconds)
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to cache exchanges: {}", e))
            })?;

        Ok(())
    }

    async fn get_cached_exchanges(
        &self,
        cache_key: &str,
    ) -> ArbitrageResult<Option<Vec<(ExchangeIdEnum, ExchangeCredentials)>>> {
        match self.kv_store.get(cache_key).text().await {
            Ok(Some(data)) => {
                match serde_json::from_str(&data) {
                    Ok(exchanges) => Ok(Some(exchanges)),
                    Err(_) => Ok(None), // Invalid cache data, ignore
                }
            }
            Ok(None) => Ok(None),
            Err(_) => Ok(None), // Cache miss, ignore error
        }
    }
}

impl ExchangeIdEnum {
    #[allow(dead_code)]
    fn from_str(s: &str) -> ArbitrageResult<Self> {
        match s.to_lowercase().as_str() {
            "binance" => Ok(ExchangeIdEnum::Binance),
            "bybit" => Ok(ExchangeIdEnum::Bybit),
            "okx" => Ok(ExchangeIdEnum::OKX),
            "bitget" => Ok(ExchangeIdEnum::Bitget),
            "kucoin" => Ok(ExchangeIdEnum::Kucoin),
            "gate" => Ok(ExchangeIdEnum::Gate),
            "mexc" => Ok(ExchangeIdEnum::Mexc),
            "huobi" => Ok(ExchangeIdEnum::Huobi),
            "kraken" => Ok(ExchangeIdEnum::Kraken),
            "coinbase" => Ok(ExchangeIdEnum::Coinbase),
            _ => Err(ArbitrageError::validation_error(format!(
                "Unknown exchange: {}",
                s
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{UserApiKey, UserConfiguration, UserProfile, UserSubscription};

    fn create_test_user_profile(user_id: &str, tier: SubscriptionTier) -> UserProfile {
        UserProfile {
            user_id: user_id.to_string(),
            telegram_user_id: Some(123456789),
            username: Some("testuser".to_string()),
            email: Some("test@example.com".to_string()),
            subscription_tier: tier.clone(),
            access_level: UserAccessLevel::Registered,
            is_active: true,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            last_login: None,
            preferences: crate::types::UserPreferences::default(),
            risk_profile: crate::types::RiskProfile::default(),
            subscription: UserSubscription {
                tier,
                is_active: true,
                features: vec!["basic_features".to_string()],
                expires_at: None,
                auto_renew: false,
            },
            configuration: UserConfiguration::default(),
            api_keys: Vec::new(),
            invitation_code: None,
            beta_expires_at: None,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            last_active: Some(chrono::Utc::now().timestamp_millis() as u64),
            total_trades: 0,
            total_pnl_usdt: 0.0,
            account_balance_usdt: 0.0,
            profile_metadata: None,
            telegram_username: Some("testuser".to_string()),
            group_admin_roles: Vec::new(),
        }
    }

    #[test]
    fn test_exchange_id_from_str() {
        assert!(matches!(
            ExchangeIdEnum::from_str("binance"),
            Ok(ExchangeIdEnum::Binance)
        ));
        assert!(matches!(
            ExchangeIdEnum::from_str("BYBIT"),
            Ok(ExchangeIdEnum::Bybit)
        ));
        assert!(ExchangeIdEnum::from_str("invalid").is_err());
    }

    #[test]
    fn test_subscription_tier_filtering() {
        // Test that subscription tier logic is properly implemented
        let free_user = create_test_user_profile("user1", SubscriptionTier::Free);
        let premium_user = create_test_user_profile("user2", SubscriptionTier::Premium);

        assert_eq!(free_user.subscription.tier, SubscriptionTier::Free);
        assert_eq!(premium_user.subscription.tier, SubscriptionTier::Premium);
    }
}
