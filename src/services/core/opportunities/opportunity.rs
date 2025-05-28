// src/services/opportunity.rs

use crate::log_error;
use crate::services::core::trading::exchange::ExchangeInterface;
use crate::services::core::trading::exchange::ExchangeService;
use crate::services::core::user::user_profile::UserProfileService;
use crate::services::interfaces::telegram::telegram::TelegramService;
use crate::types::{
    ArbitrageOpportunity, ArbitrageType, CommandPermission, ExchangeIdEnum, FundingRateInfo,
    StructuredTradingPair, UserProfile,
};
use crate::utils::ArbitrageResult;
use std::sync::Arc;

use futures::future::join_all;
use std::collections::HashMap;

#[derive(Clone)]
pub struct OpportunityServiceConfig {
    pub exchanges: Vec<ExchangeIdEnum>,
    pub monitored_pairs: Vec<StructuredTradingPair>,
    pub threshold: f64,
}

pub struct OpportunityService {
    config: OpportunityServiceConfig,
    exchange_service: Arc<ExchangeService>,
    telegram_service: Option<Arc<TelegramService>>,
    user_profile_service: Option<UserProfileService>,
}

impl OpportunityService {
    pub fn new(
        config: OpportunityServiceConfig,
        exchange_service: Arc<ExchangeService>,
        telegram_service: Option<Arc<TelegramService>>,
    ) -> Self {
        Self {
            config,
            exchange_service,
            telegram_service,
            user_profile_service: None,
        }
    }

    /// Set the UserProfile service for database-based RBAC
    pub fn set_user_profile_service(&mut self, user_profile_service: UserProfileService) {
        self.user_profile_service = Some(user_profile_service);
    }

    /// Helper method to safely parse user ID and get user profile
    async fn get_user_profile(&self, user_id: &str) -> Option<UserProfile> {
        let user_profile_service = self.user_profile_service.as_ref()?;

        // Safely parse user ID - return None for invalid IDs
        let telegram_id = match user_id.parse::<i64>() {
            Ok(id) if id > 0 => id, // Telegram user IDs start from 1
            Ok(_) => {
                log_error!(
                    "Opportunity access denied: Invalid user ID format (non-positive)",
                    serde_json::json!({
                        "error": "User ID must be greater than 0"
                    })
                );
                return None;
            }
            Err(e) => {
                log_error!(
                    "Opportunity access denied: Invalid user ID format (parse error)",
                    serde_json::json!({
                        "error": e.to_string()
                    })
                );
                return None;
            }
        };

        // Get user profile from database
        match user_profile_service
            .get_user_by_telegram_id(telegram_id)
            .await
        {
            Ok(Some(profile)) => Some(profile),
            Ok(None) => {
                log_error!(
                    "Opportunity access denied: User not found in database",
                    serde_json::json!({
                        "error": "User authentication failed"
                    })
                );
                None
            }
            Err(e) => {
                log_error!(
                    "Opportunity access denied: Database error during user lookup",
                    serde_json::json!({
                        "error": e.to_string()
                    })
                );
                None
            }
        }
    }

    /// Check if user has required permission using database-based RBAC
    async fn check_user_permission(&self, user_id: &str, permission: &CommandPermission) -> bool {
        // If UserProfile service is not available, allow basic access (for backward compatibility)
        if self.user_profile_service.is_none() {
            // Allow basic opportunity viewing without RBAC (legacy behavior)
            return true;
        }

        // Get user profile using the safe helper method
        let user_profile = match self.get_user_profile(user_id).await {
            Some(profile) => profile,
            None => {
                // If user profile retrieval failed, deny access for security
                return false;
            }
        };

        // Use the existing UserProfile permission checking method
        user_profile.has_permission(permission.clone())
    }

    /// RBAC-protected opportunity finding with subscription-based filtering
    pub async fn find_opportunities_with_permission(
        &self,
        user_id: &str,
        exchange_ids: &[ExchangeIdEnum],
        pairs: &[String],
        threshold: f64,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        // Check BasicOpportunities permission for opportunity access
        if !self
            .check_user_permission(user_id, &CommandPermission::BasicOpportunities)
            .await
        {
            return Err(crate::utils::ArbitrageError::validation_error(
                "Insufficient permissions: BasicOpportunities required for opportunity access"
                    .to_string(),
            ));
        }

        // Get user subscription tier to determine filtering level
        let subscription_tier = if let Some(profile) = self.get_user_profile(user_id).await {
            profile.subscription.tier
        } else {
            crate::types::SubscriptionTier::Free // Default for unknown users or when RBAC not configured
        };

        // Call the original find_opportunities method
        let mut opportunities = self
            .find_opportunities(exchange_ids, pairs, threshold)
            .await?;

        // Filter opportunities based on user subscription tier
        match subscription_tier {
            crate::types::SubscriptionTier::Free => {
                // Free users get limited opportunities (first 2)
                opportunities.truncate(2);
            }
            crate::types::SubscriptionTier::Basic => {
                // Basic users get more opportunities (first 5)
                opportunities.truncate(5);
            }
            crate::types::SubscriptionTier::Pro
            | crate::types::SubscriptionTier::Admin
            | crate::types::SubscriptionTier::Premium
            | crate::types::SubscriptionTier::Enterprise
            | crate::types::SubscriptionTier::SuperAdmin => {
                // Premium+ users get all opportunities (no filtering)
            }
        }

        Ok(opportunities)
    }

    pub async fn find_opportunities(
        &self,
        exchange_ids: &[ExchangeIdEnum],
        pairs: &[String],
        threshold: f64,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        let mut opportunities = Vec::new();

        // Step 1: Fetch funding rates for all pairs and exchanges
        let mut funding_rate_data: HashMap<
            String,
            HashMap<ExchangeIdEnum, Option<FundingRateInfo>>,
        > = HashMap::new();

        // Initialize maps
        for pair in pairs {
            funding_rate_data.insert(pair.clone(), HashMap::new());
        }

        // Collect funding rate fetch operations
        let mut funding_tasks = Vec::new();

        for pair in pairs {
            for exchange_id in exchange_ids {
                let exchange_service = Arc::clone(&self.exchange_service);
                let pair = pair.clone();
                let exchange_id = *exchange_id;

                let task = Box::pin(async move {
                    let result = exchange_service
                        .fetch_funding_rates(&exchange_id.to_string(), Some(&pair))
                        .await;

                    // Parse the result to extract FundingRateInfo
                    let funding_info = match result {
                        Ok(rates) => {
                            if let Some(rate_data) = rates.first() {
                                // Extract funding rate from the response - handle parsing errors properly
                                match rate_data["fundingRate"].as_str() {
                                    Some(rate_str) => match rate_str.parse::<f64>() {
                                        Ok(funding_rate) => Some(FundingRateInfo {
                                            symbol: pair.clone(),
                                            funding_rate,
                                            timestamp: Some(chrono::Utc::now()),
                                            datetime: Some(chrono::Utc::now().to_rfc3339()),
                                            next_funding_time: None,
                                            estimated_rate: None,
                                        }),
                                        Err(parse_err) => {
                                            log_error!(
                                                "Failed to parse funding rate",
                                                serde_json::json!({
                                                    "exchange": exchange_id.to_string(),
                                                    "pair": pair,
                                                    "raw_value": rate_str,
                                                    "error": parse_err.to_string()
                                                })
                                            );
                                            None
                                        }
                                    },
                                    None => {
                                        log_error!(
                                            "Missing fundingRate field in response",
                                            serde_json::json!({
                                                "exchange": exchange_id.to_string(),
                                                "pair": pair,
                                                "response": rate_data
                                            })
                                        );
                                        None
                                    }
                                }
                            } else {
                                log_error!(
                                    "Empty funding rates response",
                                    serde_json::json!({
                                        "exchange": exchange_id.to_string(),
                                        "pair": pair
                                    })
                                );
                                None
                            }
                        }
                        Err(fetch_err) => {
                            log_error!(
                                "Failed to fetch funding rates",
                                serde_json::json!({
                                    "exchange": exchange_id.to_string(),
                                    "pair": pair,
                                    "error": fetch_err.to_string()
                                })
                            );
                            None
                        }
                    };

                    (pair, exchange_id, funding_info)
                });
                funding_tasks.push(task);
            }
        }

        // Execute all funding rate fetch operations concurrently
        let funding_results = join_all(funding_tasks).await;

        // Process funding rate results
        for (pair, exchange_id, funding_info) in funding_results {
            if let Some(pair_map) = funding_rate_data.get_mut(&pair) {
                pair_map.insert(exchange_id, funding_info);
            }
        }

        // Step 2: Identify opportunities
        for pair in pairs {
            if let Some(pair_funding_rates) = funding_rate_data.get(pair) {
                let available_exchanges: Vec<ExchangeIdEnum> = pair_funding_rates
                    .iter()
                    .filter_map(|(exchange_id, rate_info)| {
                        if rate_info.is_some() {
                            Some(*exchange_id)
                        } else {
                            None
                        }
                    })
                    .collect();

                if available_exchanges.len() < 2 {
                    continue;
                }

                // Compare all pairs of exchanges
                for i in 0..available_exchanges.len() {
                    for j in (i + 1)..available_exchanges.len() {
                        let exchange_a = available_exchanges[i];
                        let exchange_b = available_exchanges[j];

                        if let (Some(Some(rate_a)), Some(Some(rate_b))) = (
                            pair_funding_rates.get(&exchange_a),
                            pair_funding_rates.get(&exchange_b),
                        ) {
                            let rate_diff = (rate_a.funding_rate - rate_b.funding_rate).abs();

                            if rate_diff >= threshold {
                                // Determine which exchange to go long/short
                                let (long_exchange, short_exchange, long_rate, short_rate) =
                                    if rate_a.funding_rate > rate_b.funding_rate {
                                        (
                                            exchange_b,
                                            exchange_a,
                                            rate_b.funding_rate,
                                            rate_a.funding_rate,
                                        )
                                    } else {
                                        (
                                            exchange_a,
                                            exchange_b,
                                            rate_a.funding_rate,
                                            rate_b.funding_rate,
                                        )
                                    };

                                // For now, we'll set net difference same as rate difference
                                // In a real implementation, you'd fetch trading fees and subtract them
                                let _net_difference = Some(rate_diff);

                                // Create opportunity using new constructor
                                let mut opportunity = match ArbitrageOpportunity::new(
                                    pair.clone(),
                                    long_exchange,  // **REQUIRED**: No longer optional
                                    short_exchange, // **REQUIRED**: No longer optional
                                    Some(long_rate),
                                    Some(short_rate),
                                    rate_diff,
                                    ArbitrageType::FundingRate,
                                ) {
                                    Ok(opp) => opp,
                                    Err(e) => {
                                        log_error!(
                                            "Failed to create arbitrage opportunity",
                                            serde_json::json!({
                                                "pair": pair,
                                                "error": e,
                                                "long_exchange": long_exchange.as_str(),
                                                "short_exchange": short_exchange.as_str()
                                            })
                                        );
                                        continue;
                                    }
                                };

                                // Set additional fields
                                opportunity.id = uuid::Uuid::new_v4().to_string();
                                opportunity = opportunity
                                    .with_net_difference(rate_diff)
                                    .with_details(format!(
                                        "Funding rate arbitrage: Long {} ({}%) / Short {} ({}%)",
                                        long_exchange.as_str(),
                                        (long_rate * 100.0 * 10000.0).round() / 10000.0,
                                        short_exchange.as_str(),
                                        (short_rate * 100.0 * 10000.0).round() / 10000.0
                                    ));

                                opportunities.push(opportunity);
                            }
                        }
                    }
                }
            }
        }

        Ok(opportunities)
    }

    pub async fn monitor_opportunities(&self) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        let pair_symbols: Vec<String> = self
            .config
            .monitored_pairs
            .iter()
            .map(|p| p.symbol.clone())
            .collect();

        self.find_opportunities(&self.config.exchanges, &pair_symbols, self.config.threshold)
            .await
    }

    pub async fn process_opportunities(
        &self,
        opportunities: &[ArbitrageOpportunity],
    ) -> ArbitrageResult<()> {
        if opportunities.is_empty() {
            return Ok(());
        }

        // Send notifications if Telegram service is available
        if let Some(telegram_service) = &self.telegram_service {
            for opportunity in opportunities {
                if let Err(e) = telegram_service
                    .send_opportunity_notification(opportunity)
                    .await
                {
                    // Log error but don't fail the whole process
                    log_error!(
                        "Failed to send Telegram notification",
                        serde_json::json!({
                            "error": e.to_string(),
                            "opportunity_id": opportunity.id,
                            "pair": opportunity.pair
                        })
                    );
                }
            }
        }

        Ok(())
    }

    pub fn get_config(&self) -> &OpportunityServiceConfig {
        &self.config
    }
}
