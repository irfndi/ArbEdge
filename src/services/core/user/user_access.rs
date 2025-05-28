use crate::services::core::analysis::market_analysis::OpportunityType;
use crate::services::{D1Service, UserProfileService};
use crate::types::*;
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::kv::KvStore;

/// Service for managing user access levels and opportunity distribution limits
#[allow(dead_code)]
pub struct UserAccessService {
    d1_service: D1Service,
    user_profile_service: UserProfileService,
    kv_store: KvStore,
    cache_ttl_seconds: u64,
}

impl UserAccessService {
    pub fn new(
        d1_service: D1Service,
        user_profile_service: UserProfileService,
        kv_store: KvStore,
    ) -> Self {
        Self::with_cache_ttl(d1_service, user_profile_service, kv_store, 3600)
    }

    pub fn with_cache_ttl(
        d1_service: D1Service,
        user_profile_service: UserProfileService,
        kv_store: KvStore,
        cache_ttl_seconds: u64,
    ) -> Self {
        Self {
            d1_service,
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
        self.cache_access_level(&cache_key, &access_level).await?;

        Ok(access_level)
    }

    /// Get or create user's opportunity limits for today
    pub async fn get_user_opportunity_limits(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<UserOpportunityLimits> {
        let access_level = self.get_user_access_level(user_id).await?;
        let is_group_context = chat_context.is_group_context();

        // Try to get existing limits from database
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let context_id = chat_context.get_context_id();

        if let Ok(Some(mut limits)) = self
            .get_stored_opportunity_limits(user_id, &today, &context_id)
            .await
        {
            // Check if daily reset is needed
            if limits.needs_daily_reset() {
                limits.reset_daily_counters();
                self.update_opportunity_limits(&limits).await?;
            }
            return Ok(limits);
        }

        // Create new limits for today
        let limits =
            UserOpportunityLimits::new(user_id.to_string(), access_level, is_group_context);
        self.store_opportunity_limits(&limits, &context_id).await?;

        Ok(limits)
    }

    /// Check if user can receive an arbitrage opportunity
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

    /// Check if user can receive a technical opportunity
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

    /// Record that user received an arbitrage opportunity
    pub async fn record_arbitrage_opportunity_received(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<bool> {
        let mut limits = self
            .get_user_opportunity_limits(user_id, chat_context)
            .await?;
        let success = limits.record_arbitrage_received();

        if success {
            self.update_opportunity_limits(&limits).await?;
            // Note: No cache invalidation needed - recording opportunities doesn't change access level
        }

        Ok(success)
    }

    /// Record that user received a technical opportunity
    pub async fn record_technical_opportunity_received(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<bool> {
        let mut limits = self
            .get_user_opportunity_limits(user_id, chat_context)
            .await?;
        let success = limits.record_technical_received();

        if success {
            self.update_opportunity_limits(&limits).await?;
            // Note: No cache invalidation needed - recording opportunities doesn't change access level
        }

        Ok(success)
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

    #[cfg(target_arch = "wasm32")]
    async fn get_stored_opportunity_limits(
        &self,
        user_id: &str,
        date: &str,
        context_id: &str,
    ) -> ArbitrageResult<Option<UserOpportunityLimits>> {
        let query = "
            SELECT user_id, access_level, date, arbitrage_opportunities_received, 
                   technical_opportunities_received, arbitrage_limit, technical_limit,
                   last_reset, is_group_context, context_id
            FROM user_opportunity_limits 
            WHERE user_id = ? AND date = ? AND context_id = ?
        ";

        let result = self
            .d1_service
            .query_first(query, &[user_id.into(), date.into(), context_id.into()])
            .await?;

        if let Some(row) = result {
            let access_level_str = row
                .get("access_level")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ArbitrageError::parse_error("Missing access_level".to_string()))?;

            let access_level: UserAccessLevel = access_level_str
                .parse()
                .map_err(|e| ArbitrageError::parse_error(format!("Invalid access_level: {}", e)))?;

            Ok(Some(UserOpportunityLimits {
                user_id: row
                    .get("user_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ArbitrageError::parse_error("Missing user_id".to_string()))?
                    .to_string(),
                access_level,
                is_group_context: row
                    .get("is_group_context")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| {
                        ArbitrageError::parse_error("Missing is_group_context".to_string())
                    })?
                    != 0,
                daily_arbitrage_limit: row
                    .get("arbitrage_limit")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| {
                        ArbitrageError::parse_error("Missing arbitrage_limit".to_string())
                    })? as u32,
                daily_technical_limit: row
                    .get("technical_limit")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| {
                        ArbitrageError::parse_error("Missing technical_limit".to_string())
                    })? as u32,
                current_arbitrage_count: row
                    .get("arbitrage_opportunities_received")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| {
                        ArbitrageError::parse_error(
                            "Missing arbitrage_opportunities_received".to_string(),
                        )
                    })? as u32,
                current_technical_count: row
                    .get("technical_opportunities_received")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| {
                        ArbitrageError::parse_error(
                            "Missing technical_opportunities_received".to_string(),
                        )
                    })? as u32,
                last_reset_date: row
                    .get("date")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ArbitrageError::parse_error("Missing date".to_string()))?
                    .to_string(),
                rate_limit_window_minutes: if row
                    .get("is_group_context")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    != 0
                {
                    60
                } else {
                    15
                },
                opportunities_in_window: 0,
                window_start_time: row
                    .get("last_reset")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| ArbitrageError::parse_error("Missing last_reset".to_string()))?
                    as u64,
            }))
        } else {
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn store_opportunity_limits(
        &self,
        limits: &UserOpportunityLimits,
        context_id: &str,
    ) -> ArbitrageResult<()> {
        let query = "
            INSERT INTO user_opportunity_limits (
                user_id, access_level, date, arbitrage_opportunities_received,
                technical_opportunities_received, arbitrage_limit, technical_limit,
                last_reset, is_group_context, context_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ";

        self.d1_service
            .execute_query(
                query,
                &[
                    limits.user_id.clone().into(),
                    limits.access_level.to_string().into(),
                    limits.last_reset_date.clone().into(),
                    limits.current_arbitrage_count.into(),
                    limits.current_technical_count.into(),
                    limits.daily_arbitrage_limit.into(),
                    limits.daily_technical_limit.into(),
                    (limits.window_start_time as i64).into(),
                    (limits.is_group_context as i32).into(),
                    context_id.to_string().into(),
                ],
            )
            .await?;

        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    async fn update_opportunity_limits(
        &self,
        limits: &UserOpportunityLimits,
    ) -> ArbitrageResult<()> {
        let query = "
            UPDATE user_opportunity_limits 
            SET arbitrage_opportunities_received = ?, technical_opportunities_received = ?,
                last_reset = ?
            WHERE user_id = ? AND date = ?
        ";

        self.d1_service
            .execute_query(
                query,
                &[
                    limits.current_arbitrage_count.into(),
                    limits.current_technical_count.into(),
                    (limits.window_start_time as i64).into(),
                    limits.user_id.clone().into(),
                    limits.last_reset_date.clone().into(),
                ],
            )
            .await?;

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn get_stored_opportunity_limits(
        &self,
        _user_id: &str,
        _date: &str,
        _context_id: &str,
    ) -> ArbitrageResult<Option<UserOpportunityLimits>> {
        // Non-WASM implementation - database operations not supported
        Err(ArbitrageError::not_implemented(
            "Database operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn store_opportunity_limits(
        &self,
        _limits: &UserOpportunityLimits,
        _context_id: &str,
    ) -> ArbitrageResult<()> {
        // Non-WASM implementation - database operations not supported
        Err(ArbitrageError::not_implemented(
            "Database operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn update_opportunity_limits(
        &self,
        _limits: &UserOpportunityLimits,
    ) -> ArbitrageResult<()> {
        // Non-WASM implementation - database operations not supported
        Err(ArbitrageError::not_implemented(
            "Database operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
        ))
    }

    async fn cache_access_level(
        &self,
        cache_key: &str,
        access_level: &UserAccessLevel,
    ) -> ArbitrageResult<()> {
        let value = serde_json::to_string(access_level).map_err(|e| {
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
        required_exchanges: &[ExchangeIdEnum],
    ) -> ArbitrageResult<String> {
        match access_level {
            UserAccessLevel::FreeWithoutAPI => {
                Ok("You need to add exchange API keys to receive trading opportunities. Use /settings to add your API keys.".to_string())
            }
            UserAccessLevel::FreeWithAPI => {
                if !limits.can_receive_arbitrage() && !limits.can_receive_technical() {
                    let (remaining_arb, remaining_tech) = limits.get_remaining_opportunities();
                    Ok(format!(
                        "Daily limit reached. Remaining: {} arbitrage, {} technical opportunities. Resets at midnight UTC.",
                        remaining_arb, remaining_tech
                    ))
                } else {
                    Ok(format!(
                        "Missing required exchanges: {}. Add these exchanges in /settings to receive this opportunity.",
                        required_exchanges.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
                    ))
                }
            }
            UserAccessLevel::SubscriptionWithAPI => {
                Ok(format!(
                    "Missing required exchanges: {}. Add these exchanges in /settings to receive this opportunity.",
                    required_exchanges.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
                ))
            }
        }
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
        let mut limits = UserOpportunityLimits::new(user_id.clone(), access_level.clone(), false);
        assert_eq!(limits.daily_arbitrage_limit, 10);
        assert_eq!(limits.daily_technical_limit, 10);
        assert!(!limits.is_group_context);

        // Test group context (2x multiplier)
        let group_limits = UserOpportunityLimits::new(user_id, access_level, true);
        assert_eq!(group_limits.daily_arbitrage_limit, 20);
        assert_eq!(group_limits.daily_technical_limit, 20);
        assert!(group_limits.is_group_context);

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
        let private = ChatContext::Private;
        let group = ChatContext::Group;
        let channel = ChatContext::Channel;

        assert!(!private.is_group_context());
        assert!(group.is_group_context());
        assert!(channel.is_group_context());

        assert_eq!(private.get_context_id(), "private");
        assert_eq!(group.get_context_id(), "group");
        assert_eq!(channel.get_context_id(), "channel");
    }
}
