use crate::services::core::infrastructure::DatabaseManager;
use crate::services::core::opportunities::opportunity_core::OpportunityType;
use crate::services::UserProfileService;
use crate::types::{
    ChatContext, ExchangeIdEnum, UserAccessLevel, UserOpportunityLimits, /* UserProfile, */
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::kv::KvStore;

/// Service for managing user access levels and opportunity distribution limits
#[allow(dead_code)]
pub struct UserAccessService {
    database_manager: DatabaseManager,
    user_profile_service: UserProfileService,
    kv_store: KvStore,
    cache_ttl_seconds: u64,
}

impl UserAccessService {
    pub fn new(
        database_manager: DatabaseManager,
        user_profile_service: UserProfileService,
        kv_store: KvStore,
    ) -> Self {
        Self::with_cache_ttl(database_manager, user_profile_service, kv_store, 3600)
    }

    pub fn with_cache_ttl(
        database_manager: DatabaseManager,
        user_profile_service: UserProfileService,
        kv_store: KvStore,
        cache_ttl_seconds: u64,
    ) -> Self {
        Self {
            database_manager,
            user_profile_service,
            kv_store,
            cache_ttl_seconds,
        }
    }

    /// Get user's current access level
    pub async fn get_user_access_level(&self, user_id: &str) -> ArbitrageResult<UserAccessLevel> {
        // Try to get from cache first
        let cache_key = format!("user_access_level:{}", user_id);
        if let Ok(Some(cached)) = self.get_cached_access_level(&cache_key).await {
            return Ok(cached);
        }

        // Get user profile to determine access level
        let user_profile = self
            .user_profile_service
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found(format!("User not found: {}", user_id)))?;

        let access_level = user_profile.get_access_level();

        // Cache the result
        self.cache_access_level(&cache_key, access_level.clone())
            .await?;

        Ok(access_level.clone())
    }

    /// Get user opportunity limits based on access level and context
    pub async fn get_user_opportunity_limits(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<UserOpportunityLimits> {
        let user_profile = self.user_profile_service.get_user_profile(user_id).await?;
        let user_profile = user_profile.ok_or("User profile not found")?;
        let access_level = user_profile.get_access_level();
        let _context_id = chat_context.get_context_id();
        let is_group_context = chat_context.is_group_context();

        // Check if we need to reset daily counters
        let mut limits = self.get_cached_limits(user_id).await.unwrap_or_else(|| {
            UserOpportunityLimits::new(user_id.to_string(), &access_level, is_group_context)
        });

        if limits.needs_daily_reset() {
            limits.reset_daily_counters();
        }

        // Cache the updated limits
        self.cache_limits(user_id, &limits).await?;

        Ok(limits)
    }

    /// Check if user can receive a specific type of opportunity
    pub async fn can_receive_opportunity(
        &self,
        user_id: &str,
        opportunity_type: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<bool> {
        let mut limits = self
            .get_user_opportunity_limits(user_id, chat_context)
            .await?;

        let can_receive = match opportunity_type {
            "arbitrage" | "global" => limits.can_receive_arbitrage(),
            "technical" => limits.can_receive_technical(),
            _ => false,
        };

        if can_receive {
            // Record the opportunity reception
            match opportunity_type {
                "arbitrage" | "global" => {
                    limits.record_arbitrage_received();
                }
                "technical" => {
                    limits.record_technical_received();
                }
                _ => {}
            }

            // Update cached limits
            self.cache_limits(user_id, &limits).await?;
        }

        Ok(can_receive)
    }

    /// Get user access level for a specific feature
    pub async fn get_feature_access_level(
        &self,
        user_id: &str,
        feature: &str,
    ) -> ArbitrageResult<bool> {
        let user_profile = self.user_profile_service.get_user_profile(user_id).await?;
        let user_profile = user_profile.ok_or_else(|| {
            ArbitrageError::not_found(format!("User profile not found: {}", user_id))
        })?;
        let access_level = user_profile.get_access_level();

        let has_access = match (access_level, feature) {
            (UserAccessLevel::Guest, "view_opportunities") => true,
            (UserAccessLevel::Guest, _) => false,
            (UserAccessLevel::Registered, "basic_trading") => true,
            (UserAccessLevel::Registered, "view_opportunities") => true,
            (UserAccessLevel::Verified, _) => true,
            (UserAccessLevel::Premium, _) => true,
            (UserAccessLevel::Admin, _) => true,
            (UserAccessLevel::SuperAdmin, _) => true,
            (UserAccessLevel::FreeWithoutAPI, "view_opportunities") => true,
            (UserAccessLevel::FreeWithoutAPI, _) => false,
            (UserAccessLevel::FreeWithAPI, "basic_trading") => true,
            (UserAccessLevel::FreeWithAPI, "view_opportunities") => true,
            (UserAccessLevel::SubscriptionWithAPI, _) => true,
            _ => false,
        };

        Ok(has_access)
    }

    /// Get user's remaining opportunities for today
    pub async fn get_remaining_opportunities(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<(u32, u32)> {
        let limits = self
            .get_user_opportunity_limits(user_id, chat_context)
            .await?;
        Ok(limits.get_remaining_opportunities())
    }

    /// Check if user can enable trading (has compatible API keys)
    pub async fn can_user_enable_trading(&self, user_id: &str) -> ArbitrageResult<bool> {
        let user_profile = self
            .user_profile_service
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found(format!("User not found: {}", user_id)))?;

        Ok(user_profile.has_trading_api_keys())
    }

    /// Get user's opportunity delivery delay based on access level
    pub async fn get_opportunity_delay(&self, user_id: &str) -> ArbitrageResult<u64> {
        let access_level = self.get_user_access_level(user_id).await?;
        Ok(access_level.get_opportunity_delay_seconds())
    }

    /// Validate user access for specific opportunity type
    pub async fn validate_opportunity_access(
        &self,
        user_id: &str,
        opportunity_type: &str,
        chat_context: &ChatContext,
        required_exchanges: &[ExchangeIdEnum],
    ) -> ArbitrageResult<OpportunityAccessResult> {
        let access_level = self.get_user_access_level(user_id).await?;
        let limits = self
            .get_user_opportunity_limits(user_id, chat_context)
            .await?;

        let can_access = match opportunity_type {
            "arbitrage" => {
                self.can_user_receive_arbitrage(user_id, chat_context, required_exchanges)
                    .await?
            }
            "technical" => {
                self.can_user_receive_technical(user_id, chat_context, required_exchanges)
                    .await?
            }
            _ => false,
        };

        let delay_seconds = access_level.get_opportunity_delay_seconds();
        let (remaining_arbitrage, remaining_technical) = limits.get_remaining_opportunities();

        Ok(OpportunityAccessResult {
            can_access,
            access_level: access_level.clone(),
            delay_seconds,
            remaining_arbitrage,
            remaining_technical,
            reason: if !can_access {
                Some(
                    self.get_access_denial_reason(&access_level, &limits, required_exchanges)
                        .await?,
                )
            } else {
                None
            },
        })
    }

    /// Record that user received an arbitrage opportunity
    pub async fn record_arbitrage_opportunity_received(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<()> {
        let mut limits = self
            .get_user_opportunity_limits(user_id, chat_context)
            .await?;

        limits.record_arbitrage_received();
        self.cache_limits(user_id, &limits).await?;

        Ok(())
    }

    /// Record that user received a technical opportunity
    pub async fn record_technical_opportunity_received(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<()> {
        let mut limits = self
            .get_user_opportunity_limits(user_id, chat_context)
            .await?;

        limits.record_technical_received();
        self.cache_limits(user_id, &limits).await?;

        Ok(())
    }

    /// Check if user can receive arbitrage opportunities
    pub async fn can_user_receive_arbitrage(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        required_exchanges: &[ExchangeIdEnum],
    ) -> ArbitrageResult<bool> {
        self.can_user_receive_opportunity_type(
            user_id,
            chat_context,
            required_exchanges,
            OpportunityType::Arbitrage,
        )
        .await
    }

    /// Check if user can receive technical opportunities
    pub async fn can_user_receive_technical(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        required_exchanges: &[ExchangeIdEnum],
    ) -> ArbitrageResult<bool> {
        self.can_user_receive_opportunity_type(
            user_id,
            chat_context,
            required_exchanges,
            OpportunityType::Technical,
        )
        .await
    }

    // Private helper methods

    /// Generic method for checking if user can receive a specific opportunity type
    async fn can_user_receive_opportunity_type(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        required_exchanges: &[ExchangeIdEnum],
        opportunity_type: OpportunityType,
    ) -> ArbitrageResult<bool> {
        // Check access level
        let access_level = self.get_user_access_level(user_id).await?;
        if !access_level.can_trade() {
            return Ok(false);
        }

        // Check daily limits
        let limits = self
            .get_user_opportunity_limits(user_id, chat_context)
            .await?;

        let can_receive = match opportunity_type {
            OpportunityType::Arbitrage => limits.can_receive_arbitrage(),
            OpportunityType::Technical => limits.can_receive_technical(),
            _ => false, // Other types not supported yet
        };

        if !can_receive {
            return Ok(false);
        }

        // Check exchange compatibility
        let user_profile = self
            .user_profile_service
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found(format!("User not found: {}", user_id)))?;

        if !user_profile.has_compatible_exchanges(required_exchanges) {
            return Ok(false);
        }

        Ok(true)
    }

    /// Get cached user opportunity limits
    async fn get_cached_limits(&self, user_id: &str) -> Option<UserOpportunityLimits> {
        let cache_key = format!("user_limits:{}", user_id);
        match self.kv_store.get(&cache_key).text().await {
            Ok(Some(value)) => serde_json::from_str(&value).ok(),
            _ => None,
        }
    }

    /// Cache user opportunity limits
    async fn cache_limits(
        &self,
        user_id: &str,
        limits: &UserOpportunityLimits,
    ) -> ArbitrageResult<()> {
        let cache_key = format!("user_limits:{}", user_id);
        let value = serde_json::to_string(limits).map_err(|e| {
            ArbitrageError::serialization_error(format!("Failed to serialize limits: {}", e))
        })?;

        self.kv_store
            .put(&cache_key, value)
            .map_err(|e| ArbitrageError::database_error(format!("Failed to cache limits: {}", e)))?
            .expiration_ttl(self.cache_ttl_seconds)
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute cache: {}", e))
            })?;

        Ok(())
    }

    async fn cache_access_level(
        &self,
        cache_key: &str,
        access_level: UserAccessLevel,
    ) -> ArbitrageResult<()> {
        let value = serde_json::to_string(&access_level).map_err(|e| {
            ArbitrageError::serialization_error(format!("Failed to serialize access level: {}", e))
        })?;

        self.kv_store
            .put(cache_key, value)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to cache access level: {}", e))
            })?
            .expiration_ttl(self.cache_ttl_seconds)
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute cache: {}", e))
            })?;

        Ok(())
    }

    async fn get_cached_access_level(
        &self,
        cache_key: &str,
    ) -> ArbitrageResult<Option<UserAccessLevel>> {
        match self.kv_store.get(cache_key).text().await {
            Ok(Some(value)) => {
                let access_level: UserAccessLevel = serde_json::from_str(&value).map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to deserialize access level: {}",
                        e
                    ))
                })?;
                Ok(Some(access_level))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(ArbitrageError::database_error(format!(
                "Failed to get cached access level: {}",
                e
            ))),
        }
    }

    #[allow(dead_code)]
    async fn invalidate_cache(&self, cache_key: &str) -> ArbitrageResult<()> {
        self.kv_store.delete(cache_key).await.map_err(|e| {
            ArbitrageError::database_error(format!("Failed to invalidate cache: {}", e))
        })?;
        Ok(())
    }

    async fn get_access_denial_reason(
        &self,
        access_level: &UserAccessLevel,
        limits: &UserOpportunityLimits,
        _required_exchanges: &[ExchangeIdEnum],
    ) -> ArbitrageResult<String> {
        let reason = match access_level {
            UserAccessLevel::Guest => {
                if !access_level.can_access_feature("basic_trading") {
                    "Guest users cannot access trading features. Please register to continue."
                } else {
                    "Access granted for guest user."
                }
            }
            UserAccessLevel::Free => {
                if !access_level.can_access_feature("ai_analysis_byok") {
                    "Free users need to provide their own AI API keys for enhanced features."
                } else {
                    "Access granted for free user with BYOK."
                }
            }
            UserAccessLevel::Registered => {
                if limits.can_receive_arbitrage() {
                    "Access granted for registered user."
                } else {
                    "Daily opportunity limit reached for registered user."
                }
            }
            UserAccessLevel::Paid => {
                if !access_level.can_access_feature("ai_analysis_enhanced") {
                    "Paid user access to enhanced AI features."
                } else {
                    "Full access granted for paid user."
                }
            }
            _ => "Access level evaluation completed.",
        };

        Ok(reason.to_string())
    }
}

/// Result of opportunity access validation
#[derive(Debug, Clone)]
pub struct OpportunityAccessResult {
    pub can_access: bool,
    pub access_level: UserAccessLevel,
    pub delay_seconds: u64,
    pub remaining_arbitrage: u32,
    pub remaining_technical: u32,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_access_level_limits() {
        let free_without_api = UserAccessLevel::FreeWithoutAPI;
        let free_with_api = UserAccessLevel::FreeWithAPI;
        let subscription_with_api = UserAccessLevel::SubscriptionWithAPI;

        // Test daily limits
        assert_eq!(free_without_api.get_daily_opportunity_limits(), (0, 0));
        assert_eq!(free_with_api.get_daily_opportunity_limits(), (10, 10));
        assert_eq!(
            subscription_with_api.get_daily_opportunity_limits(),
            (u32::MAX, u32::MAX)
        );

        // Test trading capability
        assert!(!free_without_api.can_trade());
        assert!(free_with_api.can_trade());
        assert!(subscription_with_api.can_trade());

        // Test real-time opportunities
        assert!(!free_without_api.gets_realtime_opportunities());
        assert!(!free_with_api.gets_realtime_opportunities());
        assert!(subscription_with_api.gets_realtime_opportunities());

        // Test delay
        assert_eq!(free_without_api.get_opportunity_delay_seconds(), 0);
        assert_eq!(free_with_api.get_opportunity_delay_seconds(), 300);
        assert_eq!(subscription_with_api.get_opportunity_delay_seconds(), 0);
    }

    #[test]
    fn test_user_opportunity_limits() {
        let user_id = "test_user".to_string();
        let access_level = UserAccessLevel::FreeWithAPI;

        // Test private context
        let mut limits = UserOpportunityLimits::new(user_id.clone(), &access_level, false);
        assert_eq!(limits.daily_global_opportunities, 10);
        assert_eq!(limits.daily_technical_opportunities, 5);
        assert!(!limits.can_receive_realtime);

        // Test group context (reduced limits for groups)
        let group_limits = UserOpportunityLimits::new(user_id, &access_level, true);
        assert_eq!(group_limits.daily_global_opportunities, 5); // Reduced for groups
        assert_eq!(group_limits.daily_technical_opportunities, 2); // Reduced for groups
        assert!(!group_limits.can_receive_realtime);

        // Test receiving opportunities
        assert!(limits.can_receive_arbitrage());
        assert!(limits.record_arbitrage_received());
        assert_eq!(limits.current_arbitrage_count, 1);

        assert!(limits.can_receive_technical());
        assert!(limits.record_technical_received());
        assert_eq!(limits.current_technical_count, 1);

        // Test remaining opportunities
        let (remaining_arb, remaining_tech) = limits.get_remaining_opportunities();
        assert_eq!(remaining_arb, 9);
        assert_eq!(remaining_tech, 9);
    }

    #[test]
    fn test_chat_context() {
        let private = ChatContext {
            chat_id: 123,
            chat_type: ChatContext::PRIVATE.to_string(),
            user_id: Some("user123".to_string()),
            username: None,
            is_group: false,
            group_title: None,
            message_id: None,
            reply_to_message_id: None,
        };
        let group = ChatContext {
            chat_id: 456,
            chat_type: ChatContext::GROUP.to_string(),
            user_id: Some("user123".to_string()),
            username: None,
            is_group: true,
            group_title: Some("Test Group".to_string()),
            message_id: None,
            reply_to_message_id: None,
        };
        let channel = ChatContext {
            chat_id: 789,
            chat_type: ChatContext::CHANNEL.to_string(),
            user_id: Some("user123".to_string()),
            username: None,
            is_group: false,
            group_title: Some("Test Channel".to_string()),
            message_id: None,
            reply_to_message_id: None,
        };

        assert!(!private.is_group_context());
        assert!(group.is_group_context());
        assert!(channel.is_group_context());

        assert_eq!(private.get_context_id(), "private");
        assert_eq!(group.get_context_id(), "group");
        assert_eq!(channel.get_context_id(), "channel");
    }
}
