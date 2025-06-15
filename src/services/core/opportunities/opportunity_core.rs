// src/services/core/opportunities/opportunity_core.rs

use crate::types::{
    ArbitrageOpportunity, ChatContext, ExchangeCredentials, ExchangeIdEnum, FundingRateInfo,
    TechnicalOpportunity, Ticker,
};
use crate::utils::feature_flags;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::console_log;

/// Context for opportunity processing (Personal, Group, or Global)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpportunityContext {
    Personal {
        user_id: String,
    },
    Group {
        admin_id: String,
        chat_context: ChatContext,
    },
    Global {
        system_level: bool,
    },
}

/// Source of opportunity generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpportunitySource {
    UserAPIs,
    AdminAPIs,
    SystemAPIs,
    Hybrid,
}

/// Configuration for opportunity detection and processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityConfig {
    pub symbols: Vec<String>,
    pub max_opportunities: u32,
    pub enable_ai: bool,
    pub enable_caching: bool,
    pub cache_ttl_seconds: u64,
    pub min_confidence_threshold: f64,
    pub max_risk_level: f64,
    // Additional fields needed by the modular architecture
    pub default_pairs: Vec<String>,
    pub min_rate_difference: f64,
    pub monitored_exchanges: Vec<ExchangeIdEnum>,
    pub opportunity_ttl_minutes: u32,
    pub max_participants_per_opportunity: u32,
}

impl Default for OpportunityConfig {
    fn default() -> Self {
        // Use feature flags for dynamic configuration with production-ready fallbacks
        let min_rate_threshold = feature_flags::get_numeric_feature_value(
            "opportunity_engine.min_rate_threshold",
            0.05, // Lower default threshold: 0.05%
        );

        console_log!(
            "🔧 OPPORTUNITY_CONFIG - Min rate threshold: {:.4}% (from feature flags)",
            min_rate_threshold
        );

        Self {
            symbols: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            max_opportunities: 100,
            enable_ai: true,
            enable_caching: true,
            cache_ttl_seconds: 300,
            min_confidence_threshold: 0.7,
            max_risk_level: 0.8,
            default_pairs: vec![
                "BTC/USDT".to_string(),
                "ETH/USDT".to_string(),
                "BNB/USDT".to_string(),
                "SOL/USDT".to_string(),
                "ADA/USDT".to_string(),
            ],
            min_rate_difference: min_rate_threshold,
            monitored_exchanges: vec![
                ExchangeIdEnum::Coinbase, // Prioritize Coinbase first per user request
                ExchangeIdEnum::OKX,      // OKX second
                ExchangeIdEnum::Binance,  // Binance third
                ExchangeIdEnum::Bybit,
                ExchangeIdEnum::Bitget,
            ],
            opportunity_ttl_minutes: 15,
            max_participants_per_opportunity: 10,
        }
    }
}

impl OpportunityConfig {
    /// Create config with feature flag integration
    pub fn with_feature_flags() -> Self {
        Self::default()
    }

    /// Update minimum rate difference from feature flags
    pub fn update_min_rate_difference(&mut self) {
        self.min_rate_difference = feature_flags::get_numeric_feature_value(
            "opportunity_engine.min_rate_threshold",
            self.min_rate_difference,
        );
        console_log!(
            "🔄 OPPORTUNITY_CONFIG - Updated min rate threshold: {:.4}%",
            self.min_rate_difference
        );
    }

    /// Get fault-tolerant configuration for production
    pub fn production_config() -> Self {
        let config = Self {
            cache_ttl_seconds: 60,         // Longer cache for production
            max_opportunities: 50,         // Reasonable limit for production
            min_confidence_threshold: 0.6, // Lower threshold for more opportunities
            monitored_exchanges: vec![
                ExchangeIdEnum::Coinbase,
                ExchangeIdEnum::OKX,
                ExchangeIdEnum::Binance,
                ExchangeIdEnum::Bybit,
                ExchangeIdEnum::Bitget,
            ],
            ..Default::default()
        };

        console_log!("🏭 OPPORTUNITY_CONFIG - Production configuration enabled");
        config
    }

    /// Validate configuration for production readiness
    pub fn validate(&self) -> Result<(), String> {
        if self.min_rate_difference < 0.01 {
            return Err("Minimum rate difference too low for production".to_string());
        }
        if self.monitored_exchanges.is_empty() {
            return Err("No monitored exchanges configured".to_string());
        }
        if self.max_opportunities == 0 {
            return Err("Max opportunities must be greater than 0".to_string());
        }
        Ok(())
    }
}

/// Result of opportunity generation
#[derive(Debug, Clone)]
pub struct OpportunityResult {
    pub arbitrage_opportunities: Vec<ArbitrageOpportunity>,
    pub technical_opportunities: Vec<TechnicalOpportunity>,
    pub source: OpportunitySource,
    pub context: OpportunityContext,
    pub ai_enhanced: bool,
    pub cached: bool,
    pub generation_time_ms: u64,
}

/// Exchange API information
#[derive(Debug, Clone)]
pub struct ExchangeAPI {
    pub exchange_id: ExchangeIdEnum,
    pub credentials: ExchangeCredentials,
    pub is_active: bool,
    pub can_trade: bool,
}

/// Market data for analysis
#[derive(Debug, Clone)]
pub struct MarketData {
    pub symbol: String,
    pub exchange_tickers: HashMap<ExchangeIdEnum, Ticker>,
    pub funding_rates: HashMap<ExchangeIdEnum, Option<FundingRateInfo>>,
    pub timestamp: u64,
}

/// Technical analysis result
#[derive(Debug, Clone)]
pub struct TechnicalAnalysis {
    pub signal: String,
    pub confidence: f64,
    pub target_price: f64,
    pub stop_loss: f64,
    pub expected_return: f64,
    pub risk_level: String,
    pub market_conditions: String,
}

/// Arbitrage analysis result
#[derive(Debug, Clone)]
pub struct ArbitrageAnalysis {
    pub buy_exchange: ExchangeIdEnum,
    pub sell_exchange: ExchangeIdEnum,
    pub price_difference: f64,
    pub price_difference_percent: f64,
    pub confidence: f64,
    pub risk_factors: Vec<String>,
    pub liquidity_score: f64,
}

/// Common constants used across opportunity services
pub struct OpportunityConstants;

impl OpportunityConstants {
    pub const DEFAULT_SYMBOLS: &'static [&'static str] =
        &["BTC/USDT", "ETH/USDT", "BNB/USDT", "ADA/USDT", "SOL/USDT"];

    pub const MIN_ARBITRAGE_THRESHOLD: f64 = 0.05; // Lowered to 0.05% for production
    pub const MIN_VOLUME_THRESHOLD: f64 = 100000.0;
    pub const HIGH_VOLUME_THRESHOLD: f64 = 1000000.0;
    pub const FUNDING_RATE_THRESHOLD: f64 = 0.01; // 1%
    pub const PRICE_MOMENTUM_THRESHOLD: f64 = 2.0; // 2%

    pub const CACHE_TTL_SECONDS: u64 = 300; // 5 minutes
    pub const GROUP_CACHE_TTL_SECONDS: u64 = 600; // 10 minutes

    pub const MAX_PERSONAL_OPPORTUNITIES: usize = 10;
    pub const MAX_GROUP_OPPORTUNITIES: usize = 20;
    pub const MAX_GLOBAL_OPPORTUNITIES: usize = 50;
}

/// Utility functions used across opportunity services
pub struct OpportunityUtils;

impl OpportunityUtils {
    /// Get default symbols for opportunity generation
    pub fn get_default_symbols() -> Vec<String> {
        OpportunityConstants::DEFAULT_SYMBOLS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Calculate price difference percentage with enhanced logging
    pub fn calculate_price_difference_percent(price_a: f64, price_b: f64) -> f64 {
        let result = if price_a > 0.0 {
            ((price_b - price_a).abs() / price_a) * 100.0
        } else {
            0.0
        };

        // Enhanced logging for production debugging
        if feature_flags::is_feature_enabled("opportunity_engine.enhanced_logging").unwrap_or(false)
        {
            console_log!(
                "🧮 PRICE_CALC - Price A: ${:.4}, Price B: ${:.4}, Diff: ${:.4}, Result: {:.6}%",
                price_a,
                price_b,
                (price_b - price_a).abs(),
                result
            );
        }

        result
    }

    /// Calculate price change percentage from ticker data
    pub fn calculate_price_change_percent(ticker: &Ticker) -> f64 {
        let last_price = ticker.last.unwrap_or(0.0);
        let low_24h = ticker.low.unwrap_or(last_price);

        if low_24h > 0.0 {
            ((last_price - low_24h) / low_24h) * 100.0
        } else {
            0.0
        }
    }

    /// Determine if price difference is significant for arbitrage with feature flag support
    pub fn is_arbitrage_significant(price_diff_percent: f64) -> bool {
        let min_threshold = feature_flags::get_numeric_feature_value(
            "opportunity_engine.min_rate_threshold",
            OpportunityConstants::MIN_ARBITRAGE_THRESHOLD,
        );

        let is_significant = price_diff_percent >= min_threshold;

        if feature_flags::is_feature_enabled("opportunity_engine.enhanced_logging").unwrap_or(false)
        {
            console_log!(
                "🎯 ARBITRAGE_CHECK - Diff: {:.6}%, Threshold: {:.6}%, Significant: {}",
                price_diff_percent,
                min_threshold,
                is_significant
            );
        }

        is_significant
    }

    /// Enhanced volume check with feature flag support
    pub fn is_volume_sufficient(volume: f64) -> bool {
        let min_volume = feature_flags::get_numeric_feature_value(
            "opportunity_engine.min_volume_threshold",
            OpportunityConstants::MIN_VOLUME_THRESHOLD,
        );

        volume >= min_volume
    }

    /// Calculate base confidence with enhanced metrics
    pub fn calculate_base_confidence(
        volume: f64,
        price_change_percent: f64,
        funding_rate: Option<f64>,
    ) -> f64 {
        let mut confidence: f64 = 0.5; // Base confidence

        // Volume component
        if volume > OpportunityConstants::HIGH_VOLUME_THRESHOLD {
            confidence += 0.2;
        } else if volume > OpportunityConstants::MIN_VOLUME_THRESHOLD {
            confidence += 0.1;
        }

        // Price momentum component
        if price_change_percent.abs() > OpportunityConstants::PRICE_MOMENTUM_THRESHOLD {
            confidence += 0.15;
        }

        // Funding rate component
        if let Some(rate) = funding_rate {
            if rate.abs() > OpportunityConstants::FUNDING_RATE_THRESHOLD {
                confidence += 0.1;
            }
        }

        // Cap at 0.95 to maintain realistic confidence
        confidence.min(0.95)
    }

    /// Generate opportunity ID with enhanced entropy
    pub fn generate_opportunity_id(prefix: &str, user_id: &str, symbol: &str) -> String {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let hash = md5::compute(format!("{}{}{}{}", prefix, user_id, symbol, timestamp));
        format!("{}_{:x}", prefix, hash)
    }

    /// Apply delay to arbitrage opportunities for production safety
    pub fn apply_delay_to_arbitrage(
        opportunities: &mut [ArbitrageOpportunity],
        delay_seconds: u64,
    ) {
        let delay_millis = delay_seconds * 1000;
        for opportunity in opportunities.iter_mut() {
            if let Some(expires_at) = opportunity.expires_at {
                opportunity.expires_at = Some(expires_at + delay_millis);
            }
        }
    }

    /// Apply delay to technical opportunities for production safety
    pub fn apply_delay_to_technical(
        opportunities: &mut [TechnicalOpportunity],
        delay_seconds: u64,
    ) {
        let delay_millis = delay_seconds * 1000;
        for opportunity in opportunities.iter_mut() {
            opportunity.expires_at = Some(opportunity.expires_at.unwrap_or(0) + delay_millis);
        }
    }

    /// Sort arbitrage opportunities by profit with enhanced prioritization
    pub fn sort_arbitrage_by_profit(opportunities: &mut [ArbitrageOpportunity]) {
        opportunities.sort_by(|a, b| {
            // Primary: profit percentage (descending)
            let profit_cmp = b
                .profit_percentage
                .partial_cmp(&a.profit_percentage)
                .unwrap_or(std::cmp::Ordering::Equal);

            if profit_cmp != std::cmp::Ordering::Equal {
                return profit_cmp;
            }

            // Secondary: confidence score (descending)
            b.confidence_score
                .partial_cmp(&a.confidence_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Sort technical opportunities by confidence score (descending)
    pub fn sort_technical_by_confidence(opportunities: &mut [TechnicalOpportunity]) {
        opportunities.sort_by(|a, b| {
            // Primary: confidence score (descending)
            let confidence_cmp = b
                .confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal);

            if confidence_cmp != std::cmp::Ordering::Equal {
                return confidence_cmp;
            }

            // Secondary: expected return (descending)
            b.expected_return_percentage
                .partial_cmp(&a.expected_return_percentage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Merge arbitrage opportunities with deduplication
    pub fn merge_arbitrage_opportunities(
        primary: Vec<ArbitrageOpportunity>,
        secondary: Vec<ArbitrageOpportunity>,
        max_count: usize,
    ) -> Vec<ArbitrageOpportunity> {
        let mut merged = primary;

        // Add secondary opportunities that don't duplicate primary ones
        for secondary_opp in secondary {
            let is_duplicate = merged.iter().any(|primary_opp| {
                primary_opp.pair == secondary_opp.pair
                    && primary_opp.long_exchange == secondary_opp.long_exchange
                    && primary_opp.short_exchange == secondary_opp.short_exchange
            });

            if !is_duplicate {
                merged.push(secondary_opp);
            }
        }

        // Sort and limit
        Self::sort_arbitrage_by_profit(&mut merged);
        merged.truncate(max_count);
        merged
    }

    /// Merge technical opportunities with deduplication
    pub fn merge_technical_opportunities(
        primary: Vec<TechnicalOpportunity>,
        secondary: Vec<TechnicalOpportunity>,
        max_count: usize,
    ) -> Vec<TechnicalOpportunity> {
        let mut merged = primary;

        // Add secondary opportunities that don't duplicate primary ones
        for secondary_opp in secondary {
            let is_duplicate = merged.iter().any(|primary_opp| {
                primary_opp.pair == secondary_opp.pair
                    && primary_opp.exchanges == secondary_opp.exchanges
                    && primary_opp.signal_type == secondary_opp.signal_type
            });

            if !is_duplicate {
                merged.push(secondary_opp);
            }
        }

        // Sort and limit
        Self::sort_technical_by_confidence(&mut merged);
        merged.truncate(max_count);
        merged
    }
}
