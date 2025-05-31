// src/services/core/opportunities/opportunity_core.rs

use crate::types::{
    ArbitrageOpportunity, ChatContext, ExchangeCredentials, ExchangeIdEnum, FundingRateInfo,
    TechnicalOpportunity, Ticker,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Type of opportunity to generate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpportunityType {
    Arbitrage,
    Technical,
    Both,
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
        Self {
            symbols: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
            max_opportunities: 100,
            enable_ai: true,
            enable_caching: true,
            cache_ttl_seconds: 300,
            min_confidence_threshold: 0.7,
            max_risk_level: 0.8,
            default_pairs: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
            min_rate_difference: 0.001, // 0.1%
            monitored_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::Bybit],
            opportunity_ttl_minutes: 15,
            max_participants_per_opportunity: 10,
        }
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
        &["BTCUSDT", "ETHUSDT", "BNBUSDT", "ADAUSDT", "SOLUSDT"];

    pub const MIN_ARBITRAGE_THRESHOLD: f64 = 0.001; // 0.1%
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

    /// Calculate price difference percentage
    pub fn calculate_price_difference_percent(price_a: f64, price_b: f64) -> f64 {
        if price_a > 0.0 {
            ((price_b - price_a).abs() / price_a) * 100.0
        } else {
            0.0
        }
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

    /// Determine if price difference is significant for arbitrage
    pub fn is_arbitrage_significant(price_diff_percent: f64) -> bool {
        // value already expressed in percent
        const MIN_ARBITRAGE_THRESHOLD_PERCENT: f64 = 0.1; // 0.1%
        price_diff_percent >= MIN_ARBITRAGE_THRESHOLD_PERCENT
    }

    /// Determine if volume is sufficient for trading
    pub fn is_volume_sufficient(volume: f64) -> bool {
        volume >= OpportunityConstants::MIN_VOLUME_THRESHOLD
    }

    /// Calculate confidence score based on multiple factors
    pub fn calculate_base_confidence(
        volume: f64,
        price_change_percent: f64,
        funding_rate: Option<f64>,
    ) -> f64 {
        let mut score: f64 = 0.5; // Base score

        // Volume factor
        if volume > OpportunityConstants::HIGH_VOLUME_THRESHOLD {
            score += 0.2;
        } else if volume > OpportunityConstants::MIN_VOLUME_THRESHOLD {
            score += 0.1;
        }

        // Funding rate factor
        if let Some(fr) = funding_rate {
            if fr.abs() > OpportunityConstants::FUNDING_RATE_THRESHOLD {
                score += 0.2;
            }
        }

        // Price momentum factor
        if price_change_percent.abs() > OpportunityConstants::PRICE_MOMENTUM_THRESHOLD {
            score += 0.1;
        }

        score.min(1.0_f64)
    }

    /// Generate unique opportunity ID
    pub fn generate_opportunity_id(prefix: &str, user_id: &str, symbol: &str) -> String {
        format!("{}_{}_{}_{}", prefix, user_id, symbol, uuid::Uuid::new_v4())
    }

    /// Apply delay to opportunity timestamps
    pub fn apply_delay_to_arbitrage(
        opportunities: &mut Vec<ArbitrageOpportunity>,
        delay_seconds: u64,
    ) {
        let delay_ms = delay_seconds * 1000;
        for opportunity in opportunities {
            opportunity.timestamp += delay_ms;
        }
    }

    /// Apply delay to technical opportunity timestamps
    pub fn apply_delay_to_technical(
        opportunities: &mut Vec<TechnicalOpportunity>,
        delay_seconds: u64,
    ) {
        let delay_ms = delay_seconds * 1000;
        for opportunity in opportunities {
            opportunity.timestamp += delay_ms;
        }
    }

    #[allow(clippy::ptr_arg)]
    pub fn sort_arbitrage_by_profit(opportunities: &mut Vec<ArbitrageOpportunity>) {
        opportunities.sort_by(|a, b| {
            let a_profit = a.potential_profit_value.unwrap_or(0.0);
            let b_profit = b.potential_profit_value.unwrap_or(0.0);
            b_profit
                .partial_cmp(&a_profit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Sort technical opportunities by confidence
    #[allow(clippy::ptr_arg)]
    pub fn sort_technical_by_confidence(opportunities: &mut Vec<TechnicalOpportunity>) {
        opportunities.sort_by(|a, b| {
            b.confidence_score
                .partial_cmp(&a.confidence_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Merge and deduplicate arbitrage opportunities
    pub fn merge_arbitrage_opportunities(
        primary: Vec<ArbitrageOpportunity>,
        secondary: Vec<ArbitrageOpportunity>,
        max_count: usize,
    ) -> Vec<ArbitrageOpportunity> {
        let mut merged = primary;
        merged.extend(secondary);

        // Remove duplicates based on symbol and exchange pair
        merged.sort_by(|a, b| {
            let a_key = format!("{}_{:?}_{:?}", a.pair, a.long_exchange, a.short_exchange);
            let b_key = format!("{}_{:?}_{:?}", b.pair, b.long_exchange, b.short_exchange);
            a_key.cmp(&b_key)
        });
        merged.dedup_by(|a, b| {
            a.pair == b.pair
                && a.long_exchange == b.long_exchange
                && a.short_exchange == b.short_exchange
        });

        // Sort by profit and limit
        Self::sort_arbitrage_by_profit(&mut merged);
        merged.truncate(max_count);

        merged
    }

    /// Merge and deduplicate technical opportunities
    pub fn merge_technical_opportunities(
        primary: Vec<TechnicalOpportunity>,
        secondary: Vec<TechnicalOpportunity>,
        max_count: usize,
    ) -> Vec<TechnicalOpportunity> {
        let mut merged = primary;
        merged.extend(secondary);

        // Remove duplicates based on symbol and exchange
        merged.sort_by(|a, b| {
            let a_key = format!("{}_{:?}", a.pair, a.exchange);
            let b_key = format!("{}_{:?}", b.pair, b.exchange);
            a_key.cmp(&b_key)
        });
        merged.dedup_by(|a, b| a.pair == b.pair && a.exchange == b.exchange);

        // Sort by confidence and limit
        Self::sort_technical_by_confidence(&mut merged);
        merged.truncate(max_count);

        merged
    }
}
