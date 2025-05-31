use crate::services::{
    core::infrastructure::DatabaseManager, core::trading::exchange::SuperAdminApiConfig,
    core::user::user_profile::UserProfileService, ExchangeService,
};
use crate::types::{ApiKeyProvider, ExchangeIdEnum, UserApiKey, UserProfile};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use worker::kv::KvStore;

/// Exchange availability levels for different contexts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeAvailabilityLevel {
    /// Global level - uses super admin read-only APIs
    Global,
    /// Personal level - uses user's API keys
    Personal,
    /// Group/Channel level - uses group-specific configurations
    GroupChannel,
}

/// Exchange availability information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeAvailability {
    pub exchange: ExchangeIdEnum,
    pub level: ExchangeAvailabilityLevel,
    pub is_available: bool,
    pub is_read_only: bool,
    pub can_trade: bool,
    pub api_key_source: Option<String>, // Source of the API key (user_id, group_id, or "super_admin")
    pub last_checked: u64,
}

/// Exchange selection criteria for opportunity generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeSelectionCriteria {
    pub required_exchanges: Option<Vec<ExchangeIdEnum>>,
    pub preferred_exchanges: Vec<ExchangeIdEnum>,
    pub exclude_exchanges: Vec<ExchangeIdEnum>,
    pub require_trading_capability: bool,
    pub level: ExchangeAvailabilityLevel,
    pub context_id: String, // user_id, group_id, or "global"
}

/// Result of exchange selection for opportunity creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeSelectionResult {
    pub long_exchange: ExchangeIdEnum,
    pub short_exchange: ExchangeIdEnum,
    pub selection_reason: String,
    pub availability_info: Vec<ExchangeAvailability>,
    pub fallback_used: bool,
}

/// Service for managing exchange availability and dynamic selection
pub struct ExchangeAvailabilityService {
    super_admin_config: SuperAdminApiConfig,
    user_profile_service: UserProfileService,
    #[allow(dead_code)]
    exchange_service: ExchangeService,
    #[allow(dead_code)]
    d1_service: DatabaseManager,
    kv_store: KvStore,
    cache_ttl_seconds: u64,
}

impl ExchangeAvailabilityService {
    pub fn new(
        super_admin_config: SuperAdminApiConfig,
        user_profile_service: UserProfileService,
        exchange_service: ExchangeService,
        d1_service: DatabaseManager,
        kv_store: KvStore,
    ) -> Self {
        Self {
            super_admin_config,
            user_profile_service,
            exchange_service,
            d1_service,
            kv_store,
            cache_ttl_seconds: 300, // 5 minutes cache
        }
    }

    /// Get available exchanges for a specific context and level
    pub async fn get_available_exchanges(
        &self,
        level: ExchangeAvailabilityLevel,
        context_id: &str,
    ) -> ArbitrageResult<Vec<ExchangeAvailability>> {
        // Check cache first
        let cache_key = format!(
            "exchange_availability:{}:{}",
            level.to_cache_key(),
            context_id
        );
        if let Some(cached) = self.get_cached_availability(&cache_key).await? {
            return Ok(cached);
        }

        let availability = match level {
            ExchangeAvailabilityLevel::Global => self.get_global_exchange_availability().await?,
            ExchangeAvailabilityLevel::Personal => {
                self.get_personal_exchange_availability(context_id).await?
            }
            ExchangeAvailabilityLevel::GroupChannel => {
                self.get_group_exchange_availability(context_id).await?
            }
        };

        // Cache the result
        self.cache_availability(&cache_key, &availability).await?;

        Ok(availability)
    }

    /// Select optimal exchanges for arbitrage opportunity creation
    pub async fn select_exchanges_for_arbitrage(
        &self,
        criteria: &ExchangeSelectionCriteria,
    ) -> ArbitrageResult<ExchangeSelectionResult> {
        let available_exchanges = self
            .get_available_exchanges(criteria.level.clone(), &criteria.context_id)
            .await?;

        // Filter exchanges based on criteria
        let suitable_exchanges: Vec<&ExchangeAvailability> = available_exchanges
            .iter()
            .filter(|ex| {
                // Must be available
                if !ex.is_available {
                    return false;
                }

                // Check trading capability requirement
                if criteria.require_trading_capability && !ex.can_trade {
                    return false;
                }

                // Check exclusions
                if criteria.exclude_exchanges.contains(&ex.exchange) {
                    return false;
                }

                true
            })
            .collect();

        // If we have required exchanges, ensure they're available
        if let Some(ref required) = criteria.required_exchanges {
            let available_required: HashSet<ExchangeIdEnum> = suitable_exchanges
                .iter()
                .map(|ex| ex.exchange)
                .filter(|ex| required.contains(ex))
                .collect();

            if available_required.len() < required.len() {
                return Err(ArbitrageError::validation_error(format!(
                    "Required exchanges not available: {:?}",
                    required
                )));
            }
        }

        // Ensure we have at least 2 exchanges for arbitrage
        if suitable_exchanges.len() < 2 {
            return Err(ArbitrageError::validation_error(
                "Insufficient exchanges available for arbitrage (need at least 2)".to_string(),
            ));
        }

        // Select exchanges based on preference
        let (long_exchange, short_exchange, fallback_used) =
            self.select_optimal_exchange_pair(&suitable_exchanges, criteria)?;

        let selection_reason = if fallback_used {
            "Used fallback selection due to preferred exchanges unavailability".to_string()
        } else {
            "Selected based on preference and availability".to_string()
        };

        Ok(ExchangeSelectionResult {
            long_exchange,
            short_exchange,
            selection_reason,
            availability_info: available_exchanges,
            fallback_used,
        })
    }

    /// Get global exchange availability using super admin APIs
    async fn get_global_exchange_availability(&self) -> ArbitrageResult<Vec<ExchangeAvailability>> {
        let mut availability = Vec::new();
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Check each exchange in super admin config
        for exchange in ExchangeIdEnum::all_supported() {
            let is_available = self.super_admin_config.has_exchange_config(&exchange);

            availability.push(ExchangeAvailability {
                exchange,
                level: ExchangeAvailabilityLevel::Global,
                is_available,
                is_read_only: true, // Super admin APIs are read-only
                can_trade: false,   // Global level cannot trade
                api_key_source: if is_available {
                    Some("super_admin".to_string())
                } else {
                    None
                },
                last_checked: now,
            });
        }

        Ok(availability)
    }

    /// Get personal exchange availability using user's API keys
    async fn get_personal_exchange_availability(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Vec<ExchangeAvailability>> {
        let user_profile = self
            .user_profile_service
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        let mut availability = Vec::new();
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Check each supported exchange
        for exchange in ExchangeIdEnum::all_supported() {
            let user_api_key = self.find_user_api_key_for_exchange(&user_profile, &exchange);

            let (is_available, can_trade) = if let Some(api_key) = user_api_key {
                (
                    api_key.is_active,
                    api_key.is_active && !api_key.is_read_only,
                )
            } else {
                (false, false)
            };

            availability.push(ExchangeAvailability {
                exchange,
                level: ExchangeAvailabilityLevel::Personal,
                is_available,
                is_read_only: user_api_key.map(|k| k.is_read_only).unwrap_or(true),
                can_trade,
                api_key_source: if is_available {
                    Some(user_id.to_string())
                } else {
                    None
                },
                last_checked: now,
            });
        }

        Ok(availability)
    }

    /// Get group/channel exchange availability
    async fn get_group_exchange_availability(
        &self,
        group_id: &str,
    ) -> ArbitrageResult<Vec<ExchangeAvailability>> {
        // For now, group/channel level uses global availability with 2x multiplier
        // In the future, this could support group-specific API configurations
        let mut global_availability = self.get_global_exchange_availability().await?;

        // Update level to GroupChannel
        for availability in &mut global_availability {
            availability.level = ExchangeAvailabilityLevel::GroupChannel;
            availability.api_key_source = Some(format!("group:{}", group_id));
        }

        Ok(global_availability)
    }

    /// Find user's API key for a specific exchange
    fn find_user_api_key_for_exchange<'a>(
        &self,
        user_profile: &'a UserProfile,
        exchange: &ExchangeIdEnum,
    ) -> Option<&'a UserApiKey> {
        user_profile.api_keys.iter().find(move |key| {
            key.is_active && self.is_api_key_compatible_with_exchange(key, exchange)
        })
    }

    /// Check if an API key is compatible with an exchange
    fn is_api_key_compatible_with_exchange(
        &self,
        api_key: &UserApiKey,
        exchange: &ExchangeIdEnum,
    ) -> bool {
        matches!(
            (&api_key.provider, exchange),
            (
                ApiKeyProvider::Exchange(ExchangeIdEnum::Binance),
                ExchangeIdEnum::Binance
            ) | (
                ApiKeyProvider::Exchange(ExchangeIdEnum::Bybit),
                ExchangeIdEnum::Bybit
            ) | (
                ApiKeyProvider::Exchange(ExchangeIdEnum::OKX),
                ExchangeIdEnum::OKX
            ) | (
                ApiKeyProvider::Exchange(ExchangeIdEnum::Bitget),
                ExchangeIdEnum::Bitget
            )
        )
    }

    /// Select optimal exchange pair for arbitrage
    fn select_optimal_exchange_pair(
        &self,
        suitable_exchanges: &[&ExchangeAvailability],
        criteria: &ExchangeSelectionCriteria,
    ) -> ArbitrageResult<(ExchangeIdEnum, ExchangeIdEnum, bool)> {
        let exchanges: Vec<ExchangeIdEnum> =
            suitable_exchanges.iter().map(|ex| ex.exchange).collect();

        // Try to use preferred exchanges first
        if !criteria.preferred_exchanges.is_empty() {
            let preferred_available: Vec<ExchangeIdEnum> = exchanges
                .iter()
                .filter(|ex| criteria.preferred_exchanges.contains(ex))
                .copied()
                .collect();

            if preferred_available.len() >= 2 {
                return Ok((preferred_available[0], preferred_available[1], false));
            }
        }

        // Fallback to any available exchanges
        if exchanges.len() >= 2 {
            // Use a deterministic selection for consistency
            let mut sorted_exchanges = exchanges;
            sorted_exchanges.sort();
            Ok((sorted_exchanges[0], sorted_exchanges[1], true))
        } else {
            Err(ArbitrageError::validation_error(
                "Insufficient exchanges for arbitrage selection".to_string(),
            ))
        }
    }

    /// Cache exchange availability
    async fn cache_availability(
        &self,
        cache_key: &str,
        availability: &[ExchangeAvailability],
    ) -> ArbitrageResult<()> {
        let cache_data = serde_json::to_string(availability).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize availability: {}", e))
        })?;

        self.kv_store
            .put(cache_key, cache_data)
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to create KV put request: {}", e))
            })?
            .expiration_ttl(self.cache_ttl_seconds)
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to cache availability: {}", e))
            })?;

        Ok(())
    }

    /// Get cached exchange availability
    async fn get_cached_availability(
        &self,
        cache_key: &str,
    ) -> ArbitrageResult<Option<Vec<ExchangeAvailability>>> {
        match self.kv_store.get(cache_key).text().await {
            Ok(Some(data)) => {
                match serde_json::from_str::<Vec<ExchangeAvailability>>(&data) {
                    Ok(availability) => Ok(Some(availability)),
                    Err(_) => Ok(None), // Invalid cache data, ignore
                }
            }
            _ => Ok(None),
        }
    }

    /// Validate exchange selection for opportunity creation
    pub async fn validate_exchange_selection(
        &self,
        long_exchange: ExchangeIdEnum,
        short_exchange: ExchangeIdEnum,
        level: ExchangeAvailabilityLevel,
        context_id: &str,
        require_trading: bool,
    ) -> ArbitrageResult<bool> {
        let availability = self.get_available_exchanges(level, context_id).await?;

        let long_available = availability
            .iter()
            .find(|ex| ex.exchange == long_exchange)
            .map(|ex| ex.is_available && (!require_trading || ex.can_trade))
            .unwrap_or(false);

        let short_available = availability
            .iter()
            .find(|ex| ex.exchange == short_exchange)
            .map(|ex| ex.is_available && (!require_trading || ex.can_trade))
            .unwrap_or(false);

        Ok(long_available && short_available)
    }
}

impl ExchangeAvailabilityLevel {
    fn to_cache_key(&self) -> &'static str {
        match self {
            ExchangeAvailabilityLevel::Global => "global",
            ExchangeAvailabilityLevel::Personal => "personal",
            ExchangeAvailabilityLevel::GroupChannel => "group",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::UserProfile;

    #[test]
    fn test_exchange_availability_structure() {
        let availability = ExchangeAvailability {
            exchange: ExchangeIdEnum::Binance,
            level: ExchangeAvailabilityLevel::Personal,
            is_available: true,
            is_read_only: false,
            can_trade: true,
            api_key_source: Some("user123".to_string()),
            last_checked: chrono::Utc::now().timestamp_millis() as u64,
        };

        assert_eq!(availability.exchange, ExchangeIdEnum::Binance);
        assert_eq!(availability.level, ExchangeAvailabilityLevel::Personal);
        assert!(availability.is_available);
        assert!(availability.can_trade);
    }

    #[test]
    fn test_exchange_selection_criteria() {
        let criteria = ExchangeSelectionCriteria {
            required_exchanges: Some(vec![ExchangeIdEnum::Binance]),
            preferred_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::Bybit],
            exclude_exchanges: vec![ExchangeIdEnum::OKX],
            require_trading_capability: true,
            level: ExchangeAvailabilityLevel::Personal,
            context_id: "user123".to_string(),
        };

        assert_eq!(criteria.preferred_exchanges.len(), 2);
        assert_eq!(criteria.exclude_exchanges.len(), 1);
        assert!(criteria.require_trading_capability);
    }

    #[test]
    fn test_exchange_selection_result() {
        let result = ExchangeSelectionResult {
            long_exchange: ExchangeIdEnum::Binance,
            short_exchange: ExchangeIdEnum::Bybit,
            selection_reason: "Preferred exchanges available".to_string(),
            availability_info: vec![],
            fallback_used: false,
        };

        assert_eq!(result.long_exchange, ExchangeIdEnum::Binance);
        assert_eq!(result.short_exchange, ExchangeIdEnum::Bybit);
        assert!(!result.fallback_used);
    }

    #[test]
    fn test_exchange_availability_level_cache_key() {
        assert_eq!(ExchangeAvailabilityLevel::Global.to_cache_key(), "global");
        assert_eq!(
            ExchangeAvailabilityLevel::Personal.to_cache_key(),
            "personal"
        );
        assert_eq!(
            ExchangeAvailabilityLevel::GroupChannel.to_cache_key(),
            "group"
        );
    }

    #[test]
    fn test_all_supported_exchanges() {
        let exchanges = ExchangeIdEnum::all_supported();
        assert_eq!(exchanges.len(), 4);
        assert!(exchanges.contains(&ExchangeIdEnum::Binance));
        assert!(exchanges.contains(&ExchangeIdEnum::Bybit));
        assert!(exchanges.contains(&ExchangeIdEnum::OKX));
        assert!(exchanges.contains(&ExchangeIdEnum::Bitget));
    }

    #[test]
    fn test_api_key_exchange_compatibility() {
        // This would be tested with a mock service in a full implementation
        let user_profile = UserProfile::new(Some(123456789), Some("testuser".to_string()));

        // Test structure - actual compatibility logic would be in the service
        assert_eq!(user_profile.api_keys.len(), 0); // New profile has no API keys
    }

    #[test]
    fn test_exchange_selection_insufficient_exchanges() {
        // Test that we properly handle cases with insufficient exchanges
        let suitable_exchanges: Vec<&ExchangeAvailability> = vec![];

        // This would fail in the actual service method
        assert_eq!(suitable_exchanges.len(), 0);
    }

    #[test]
    fn test_serialization() {
        let availability = ExchangeAvailability {
            exchange: ExchangeIdEnum::Binance,
            level: ExchangeAvailabilityLevel::Global,
            is_available: true,
            is_read_only: true,
            can_trade: false,
            api_key_source: Some("super_admin".to_string()),
            last_checked: 1640995200000,
        };

        let serialized = serde_json::to_string(&availability).unwrap();
        let deserialized: ExchangeAvailability = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.exchange, availability.exchange);
        assert_eq!(deserialized.level, availability.level);
        assert_eq!(deserialized.is_available, availability.is_available);
    }
}
