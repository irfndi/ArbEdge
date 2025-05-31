// src/services/core/opportunities/opportunity_engine.rs

use crate::log_info;
use crate::services::core::ai::ai_beta_integration::AiBetaIntegrationService;
use crate::services::core::opportunities::{
    access_manager::AccessManager,
    ai_enhancer::AIEnhancer,
    cache_manager::CacheManager,
    market_analyzer::MarketAnalyzer,
    opportunity_builders::OpportunityBuilder,
    opportunity_core::{OpportunityConfig, OpportunityContext},
};
use crate::services::core::user::user_access::UserAccessService;
use crate::services::core::user::UserProfileService;
use crate::types::{
    ArbitrageOpportunity, ChatContext, DistributionStrategy, GlobalOpportunity, OpportunitySource,
    TechnicalOpportunity,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::Utc;
use serde_json;
use std::sync::Arc;
use worker::kv::KvStore;

/// Unified opportunity engine that orchestrates all opportunity services
/// Eliminates redundancy by consolidating logic from personal, group, global, and legacy services
pub struct OpportunityEngine {
    // Core components
    market_analyzer: Arc<MarketAnalyzer>,
    access_manager: Arc<AccessManager>,
    ai_enhancer: Arc<AIEnhancer>,
    cache_manager: Arc<CacheManager>,
    opportunity_builder: Arc<OpportunityBuilder>,

    // Configuration
    config: OpportunityConfig,

    // Services
    user_profile_service: Arc<UserProfileService>,
    #[allow(dead_code)]
    kv_store: KvStore,
}

impl OpportunityEngine {
    pub fn new(
        user_profile_service: Arc<UserProfileService>,
        user_access_service: Arc<UserAccessService>,
        ai_service: Arc<AiBetaIntegrationService>,
        kv_store: KvStore,
        config: OpportunityConfig,
    ) -> ArbitrageResult<Self> {
        let access_manager = Arc::new(AccessManager::new(
            user_profile_service.clone(),
            user_access_service,
            kv_store.clone(),
        ));

        // Create market analyzer without exchange service for now
        // TODO: Properly inject ExchangeService when available
        let market_analyzer = Arc::new(MarketAnalyzer::new_without_exchange());
        let ai_enhancer = Arc::new(AIEnhancer::new(ai_service, access_manager.clone()));
        let cache_manager = Arc::new(CacheManager::new(kv_store.clone()));
        let opportunity_builder = Arc::new(OpportunityBuilder::new(config.clone()));

        Ok(Self {
            market_analyzer,
            access_manager,
            ai_enhancer,
            cache_manager,
            opportunity_builder,
            config,
            user_profile_service,
            kv_store,
        })
    }

    // Personal Opportunity Generation (replaces PersonalOpportunityService)

    /// Generate personal arbitrage opportunities for a user
    pub async fn generate_personal_arbitrage_opportunities(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        pairs: Option<Vec<String>>,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        // Validate user access
        let access_result = self
            .access_manager
            .validate_user_access(user_id, "arbitrage", chat_context)
            .await?;

        if !access_result.can_access {
            return Err(ArbitrageError::access_denied(
                access_result.reason.unwrap_or("Access denied".to_string()),
            ));
        }

        // Check cache first
        if let Ok(Some(cached_opportunities)) = self
            .cache_manager
            .get_cached_user_arbitrage_opportunities(user_id)
            .await
        {
            log_info!(
                "Retrieved cached personal arbitrage opportunities",
                serde_json::json!({
                    "user_id": user_id,
                    "count": cached_opportunities.len(),
                    "cache_hit": true
                })
            );
            return Ok(cached_opportunities);
        }

        // Get user's exchange APIs
        let user_exchanges = self.access_manager.get_user_exchange_apis(user_id).await?;
        if user_exchanges.len() < 2 {
            return Err(ArbitrageError::validation_error(
                "At least 2 exchange APIs required for arbitrage opportunities".to_string(),
            ));
        }

        // Use default pairs if none provided
        let trading_pairs = pairs.unwrap_or_else(|| self.config.default_pairs.clone());

        // Analyze market data and detect opportunities
        let mut opportunities = Vec::new();
        for pair in &trading_pairs {
            let pair_opportunities = self
                .market_analyzer
                .detect_arbitrage_opportunities(
                    pair,
                    &user_exchanges.iter().map(|(ex, _)| *ex).collect::<Vec<_>>(),
                    &self.config,
                )
                .await?;

            for market_opp in pair_opportunities {
                let opportunity = self.opportunity_builder.build_funding_rate_arbitrage(
                    market_opp.pair,
                    market_opp.long_exchange,
                    market_opp.short_exchange,
                    market_opp.long_rate.unwrap_or(0.0),
                    market_opp.short_rate.unwrap_or(0.0),
                    &OpportunityContext::Personal {
                        user_id: user_id.to_string(),
                    },
                )?;
                opportunities.push(opportunity);
            }
        }

        // Apply subscription-based filtering
        opportunities = self
            .access_manager
            .filter_opportunities_by_subscription(user_id, opportunities)
            .await?;

        // Enhance with AI if available
        opportunities = self
            .ai_enhancer
            .enhance_arbitrage_opportunities(user_id, opportunities, "personal_arbitrage")
            .await?;

        // Cache the results
        let _ = self
            .cache_manager
            .cache_user_arbitrage_opportunities(user_id, &opportunities, None)
            .await;

        // Record opportunity generation for rate limiting
        let _ = self
            .access_manager
            .record_opportunity_received(user_id, "arbitrage", chat_context)
            .await;

        log_info!(
            "Generated personal arbitrage opportunities",
            serde_json::json!({
                "user_id": user_id,
                "count": opportunities.len(),
                "pairs": trading_pairs,
                "ai_enhanced": self.ai_enhancer.is_ai_available_for_user(user_id).await.unwrap_or(false)
            })
        );

        Ok(opportunities)
    }

    /// Generate personal technical opportunities for a user
    pub async fn generate_personal_technical_opportunities(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        pairs: Option<Vec<String>>,
    ) -> ArbitrageResult<Vec<TechnicalOpportunity>> {
        // Validate user access
        let access_result = self
            .access_manager
            .validate_user_access(user_id, "technical", chat_context)
            .await?;

        if !access_result.can_access {
            return Err(ArbitrageError::access_denied(
                access_result.reason.unwrap_or("Access denied".to_string()),
            ));
        }

        // Check cache first
        if let Ok(Some(cached_opportunities)) = self
            .cache_manager
            .get_cached_user_technical_opportunities(user_id)
            .await
        {
            return Ok(cached_opportunities);
        }

        // Get user's exchange APIs
        let user_exchanges = self.access_manager.get_user_exchange_apis(user_id).await?;
        if user_exchanges.is_empty() {
            return Err(ArbitrageError::validation_error(
                "At least 1 exchange API required for technical opportunities".to_string(),
            ));
        }

        let trading_pairs = pairs.unwrap_or_else(|| self.config.default_pairs.clone());

        // Analyze technical signals
        let mut opportunities = Vec::new();
        for pair in &trading_pairs {
            for (exchange, _) in &user_exchanges {
                if let Ok(technical_signals) = self
                    .market_analyzer
                    .analyze_technical_signals(pair, *exchange, &self.config)
                    .await
                {
                    for signal in technical_signals {
                        let opportunity = self.opportunity_builder.build_technical_opportunity(
                            signal.pair,
                            signal.exchange,
                            signal.signal_type,
                            signal.signal_strength,
                            signal.confidence_score,
                            signal.entry_price,
                            signal.target_price,
                            signal.stop_loss,
                            signal.technical_indicators,
                            signal.timeframe,
                            signal.expected_return_percentage,
                            signal.market_conditions,
                            &OpportunityContext::Personal {
                                user_id: user_id.to_string(),
                            },
                        )?;
                        opportunities.push(opportunity);
                    }
                }
            }
        }

        // Apply subscription-based filtering
        opportunities = self
            .access_manager
            .filter_opportunities_by_subscription(user_id, opportunities)
            .await?;

        // Enhance with AI if available
        opportunities = self
            .ai_enhancer
            .enhance_technical_opportunities(user_id, opportunities, "personal_technical")
            .await?;

        // Cache the results
        let _ = self
            .cache_manager
            .cache_user_technical_opportunities(user_id, &opportunities, None)
            .await;

        // Record opportunity generation
        let _ = self
            .access_manager
            .record_opportunity_received(user_id, "technical", chat_context)
            .await;

        log_info!(
            "Generated personal technical opportunities",
            serde_json::json!({
                "user_id": user_id,
                "count": opportunities.len(),
                "pairs": trading_pairs
            })
        );

        Ok(opportunities)
    }

    // Group Opportunity Generation (replaces GroupOpportunityService)

    /// Generate group arbitrage opportunities for group members
    pub async fn generate_group_arbitrage_opportunities(
        &self,
        group_admin_id: &str,
        chat_context: &ChatContext,
        pairs: Option<Vec<String>>,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        // Validate group admin access
        let access_result = self
            .access_manager
            .validate_group_admin_access(group_admin_id, "arbitrage", chat_context)
            .await?;

        if !access_result.can_access {
            return Err(ArbitrageError::access_denied(
                access_result
                    .reason
                    .unwrap_or("Group admin access denied".to_string()),
            ));
        }

        // Check cache first
        let group_id = chat_context
            .get_group_id()
            .unwrap_or("unknown_group".to_string());
        if let Ok(Some(cached_opportunities)) = self
            .cache_manager
            .get_cached_group_arbitrage_opportunities(&group_id)
            .await
        {
            return Ok(cached_opportunities);
        }

        // Get group admin's exchange APIs
        let admin_exchanges = self
            .access_manager
            .get_group_admin_exchange_apis(group_admin_id)
            .await?;

        if admin_exchanges.len() < 2 {
            return Err(ArbitrageError::validation_error(
                "Group admin needs at least 2 exchange APIs for group arbitrage".to_string(),
            ));
        }

        let trading_pairs = pairs.unwrap_or_else(|| self.config.default_pairs.clone());

        // Generate opportunities using admin's APIs
        let mut opportunities = Vec::new();
        for pair in &trading_pairs {
            let pair_opportunities = self
                .market_analyzer
                .detect_arbitrage_opportunities(
                    pair,
                    &admin_exchanges
                        .iter()
                        .map(|(ex, _)| *ex)
                        .collect::<Vec<_>>(),
                    &self.config,
                )
                .await?;

            for market_opp in pair_opportunities {
                let mut opportunity = self.opportunity_builder.build_funding_rate_arbitrage(
                    market_opp.pair,
                    market_opp.long_exchange,
                    market_opp.short_exchange,
                    market_opp.long_rate.unwrap_or(0.0),
                    market_opp.short_rate.unwrap_or(0.0),
                    &OpportunityContext::Group {
                        admin_id: group_admin_id.to_string(),
                        chat_context: chat_context.clone(),
                    },
                )?;

                // Apply group multiplier (2x opportunities)
                if let Some(profit) = opportunity.potential_profit_value {
                    opportunity.potential_profit_value = Some(profit * 2.0);
                }

                opportunities.push(opportunity);
            }
        }

        // Enhance with AI using admin's access level
        opportunities = self
            .ai_enhancer
            .enhance_arbitrage_opportunities(group_admin_id, opportunities, "group_arbitrage")
            .await?;

        // Cache the results
        let _ = self
            .cache_manager
            .cache_group_arbitrage_opportunities(&group_id, &opportunities, None)
            .await;

        log_info!(
            "Generated group arbitrage opportunities",
            serde_json::json!({
                "group_admin_id": group_admin_id,
                "group_id": group_id,
                "count": opportunities.len(),
                "multiplier_applied": true
            })
        );

        Ok(opportunities)
    }

    // Global Opportunity Generation (replaces GlobalOpportunityService)

    /// Generate global opportunities for system-wide distribution
    pub async fn generate_global_opportunities(
        &self,
        pairs: Option<Vec<String>>,
    ) -> ArbitrageResult<Vec<GlobalOpportunity>> {
        // Check cache first
        if let Ok(Some(cached_opportunities)) =
            self.cache_manager.get_cached_global_opportunities().await
        {
            return Ok(cached_opportunities);
        }

        let trading_pairs = pairs.unwrap_or_else(|| self.config.default_pairs.clone());
        let monitored_exchanges = self.config.monitored_exchanges.clone();

        // Generate arbitrage opportunities across all monitored exchanges
        let mut global_opportunities = Vec::new();
        for pair in &trading_pairs {
            let arbitrage_opportunities = self
                .market_analyzer
                .detect_arbitrage_opportunities(pair, &monitored_exchanges, &self.config)
                .await?;

            for arb_opp in arbitrage_opportunities {
                let opportunity = self.opportunity_builder.build_funding_rate_arbitrage(
                    arb_opp.pair,
                    arb_opp.long_exchange,
                    arb_opp.short_exchange,
                    arb_opp.long_rate.unwrap_or(0.0),
                    arb_opp.short_rate.unwrap_or(0.0),
                    &OpportunityContext::Global { system_level: true },
                )?;

                // Convert to global opportunity
                let expires_at = Utc::now().timestamp_millis() as u64
                    + (self.config.opportunity_ttl_minutes as u64 * 60 * 1000);
                let global_opp = self
                    .opportunity_builder
                    .build_global_opportunity_from_arbitrage(
                        opportunity,
                        OpportunitySource::SystemGenerated,
                        expires_at,
                        Some(self.config.max_participants_per_opportunity),
                        DistributionStrategy::RoundRobin,
                    )?;

                global_opportunities.push(global_opp);
            }
        }

        // Enhance with system-level AI
        let arbitrage_opportunities: Vec<ArbitrageOpportunity> = global_opportunities
            .iter()
            .filter_map(|global_opp| {
                if let crate::types::OpportunityData::Arbitrage(arb_opp) =
                    &global_opp.opportunity_data
                {
                    Some(arb_opp.clone())
                } else {
                    None
                }
            })
            .collect();

        let enhanced_arbitrage = self
            .ai_enhancer
            .enhance_system_opportunities(arbitrage_opportunities, "global_system")
            .await?;

        // Update global opportunities with AI enhancements
        for (global_opp, enhanced_arb) in global_opportunities
            .iter_mut()
            .zip(enhanced_arbitrage.iter())
        {
            global_opp.opportunity_data =
                crate::types::OpportunityData::Arbitrage(enhanced_arb.clone());
            global_opp.ai_enhanced = true;
            global_opp.ai_confidence_score =
                enhanced_arb.potential_profit_value.map(|p| p / 1000.0);
        }

        // Cache the results
        let _ = self
            .cache_manager
            .cache_global_opportunities(&global_opportunities, None)
            .await;

        log_info!(
            "Generated global opportunities",
            serde_json::json!({
                "count": global_opportunities.len(),
                "pairs": trading_pairs,
                "exchanges": monitored_exchanges.len(),
                "ai_enhanced": true
            })
        );

        Ok(global_opportunities)
    }

    // Legacy Compatibility (replaces OpportunityService)

    /// Generate opportunities with legacy compatibility
    pub async fn generate_legacy_opportunities(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        opportunity_type: Option<String>,
    ) -> ArbitrageResult<(Vec<ArbitrageOpportunity>, Vec<TechnicalOpportunity>)> {
        let opp_type = opportunity_type.unwrap_or_else(|| "both".to_string());

        let (arbitrage_opportunities, technical_opportunities) = match opp_type.as_str() {
            "arbitrage" => {
                let arb_opps = self
                    .generate_personal_arbitrage_opportunities(user_id, chat_context, None)
                    .await?;
                (arb_opps, Vec::new())
            }
            "technical" => {
                let tech_opps = self
                    .generate_personal_technical_opportunities(user_id, chat_context, None)
                    .await?;
                (Vec::new(), tech_opps)
            }
            _ => {
                // Generate both types
                let arb_opps = self
                    .generate_personal_arbitrage_opportunities(user_id, chat_context, None)
                    .await
                    .unwrap_or_default();
                let tech_opps = self
                    .generate_personal_technical_opportunities(user_id, chat_context, None)
                    .await
                    .unwrap_or_default();
                (arb_opps, tech_opps)
            }
        };

        log_info!(
            "Generated legacy opportunities",
            serde_json::json!({
                "user_id": user_id,
                "type": opp_type,
                "arbitrage_count": arbitrage_opportunities.len(),
                "technical_count": technical_opportunities.len()
            })
        );

        Ok((arbitrage_opportunities, technical_opportunities))
    }

    // Utility Methods

    /// Get user's opportunity statistics
    pub async fn get_user_opportunity_stats(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<serde_json::Value> {
        let user_profile = self.user_profile_service.get_user_profile(user_id).await?;
        let ai_access_level = self
            .access_manager
            .get_user_ai_access_level(user_id)
            .await?;
        let user_exchanges = self.access_manager.get_user_exchange_apis(user_id).await?;

        Ok(serde_json::json!({
            "user_id": user_id,
            "subscription_tier": user_profile.map(|p| p.subscription.tier),
            "ai_access_level": format!("{:?}", ai_access_level),
            "exchange_count": user_exchanges.len(),
            "can_generate_arbitrage": user_exchanges.len() >= 2,
            "can_generate_technical": !user_exchanges.is_empty(),
            "ai_available": self.ai_enhancer.is_ai_available_for_user(user_id).await.unwrap_or(false)
        }))
    }

    /// Invalidate user caches
    pub async fn invalidate_user_caches(&self, user_id: &str) -> ArbitrageResult<()> {
        self.cache_manager.invalidate_user_cache(user_id).await
    }

    /// Invalidate group caches
    pub async fn invalidate_group_caches(&self, group_id: &str) -> ArbitrageResult<()> {
        self.cache_manager.invalidate_group_cache(group_id).await
    }

    /// Get engine configuration
    pub fn get_config(&self) -> &OpportunityConfig {
        &self.config
    }

    /// Update engine configuration
    pub fn update_config(&mut self, new_config: OpportunityConfig) {
        self.config = new_config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SubscriptionTier, UserAccessLevel, UserConfiguration, UserProfile};
    use chrono::Utc;

    fn create_test_user_profile(user_id: &str) -> UserProfile {
        UserProfile {
            user_id: "test_user".to_string(),
            telegram_user_id: Some(123456789),
            username: Some("testuser".to_string()),
            email: Some("test@example.com".to_string()),
            subscription_tier: SubscriptionTier::Free,
            access_level: UserAccessLevel::Registered,
            is_active: true,
            created_at: Utc::now().timestamp_millis() as u64,
            last_login: None,
            preferences: crate::types::UserPreferences::default(),
            risk_profile: crate::types::RiskProfile::default(),
            configuration: crate::types::UserConfiguration::default(),
            api_keys: Vec::new(),
            invitation_code: None,
            beta_expires_at: None,
            updated_at: Utc::now().timestamp_millis() as u64,
            last_active: Some(Utc::now().timestamp_millis() as u64),
            total_trades: 0,
            total_pnl_usdt: 0.0,
            account_balance_usdt: 0.0,
            profile_metadata: None,
            telegram_username: Some("testuser".to_string()),
            subscription: crate::types::UserSubscription::default(),
            group_admin_roles: Vec::new(),
        }
    }

    #[test]
    fn test_opportunity_context_mapping() {
        // Test that different contexts are properly handled
        let personal_context = OpportunityContext::Personal {
            user_id: "test_user".to_string(),
        };
        let group_context = OpportunityContext::Group {
            admin_id: "admin_user".to_string(),
            chat_context: crate::types::ChatContext {
                chat_id: -123456789,
                chat_type: "group".to_string(),
                user_id: Some("test_user".to_string()),
                username: Some("testuser".to_string()),
                is_group: true,
                group_title: Some("Test Group".to_string()),
                message_id: Some(1),
                reply_to_message_id: None,
            },
        };
        let global_context = OpportunityContext::Global { system_level: true };

        assert!(matches!(
            personal_context,
            OpportunityContext::Personal { .. }
        ));
        assert!(matches!(group_context, OpportunityContext::Group { .. }));
        assert!(matches!(global_context, OpportunityContext::Global { .. }));
    }

    #[test]
    fn test_user_profile_structure() {
        let user_profile = create_test_user_profile("test_user");

        assert_eq!(user_profile.user_id, "test_user");
        assert_eq!(user_profile.subscription.tier, SubscriptionTier::Free);
        assert!(user_profile.is_active);
        assert_eq!(user_profile.account_balance_usdt, 0.0);
    }

    #[test]
    fn test_opportunity_config_defaults() {
        let config = OpportunityConfig::default();

        assert!(config.min_rate_difference > 0.0);
        assert!(config.min_rate_difference > 0.0);
        assert!(!config.default_pairs.is_empty());
        assert!(!config.monitored_exchanges.is_empty());
        assert!(config.opportunity_ttl_minutes > 0);
        assert!(config.max_participants_per_opportunity > 0);
    }
}
