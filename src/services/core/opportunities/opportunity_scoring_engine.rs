use crate::types::ArbitrageOpportunity;
use crate::utils::get_current_timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(target_arch = "wasm32")]
use worker::console_log;

#[cfg(not(target_arch = "wasm32"))]
macro_rules! console_log {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityScore {
    pub opportunity_id: String,
    pub symbol: String,
    pub total_score: f64,
    pub profit_score: f64,
    pub freshness_score: f64,
    pub exchange_reliability_score: f64,
    pub volume_score: f64,
    pub confidence_score: f64,
    pub time_decay_factor: f64,
    pub calculated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    pub profit_weight: f64,     // Default: 0.4 (40%)
    pub freshness_weight: f64,  // Default: 0.25 (25%)
    pub exchange_weight: f64,   // Default: 0.15 (15%)
    pub volume_weight: f64,     // Default: 0.1 (10%)
    pub confidence_weight: f64, // Default: 0.1 (10%)
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            profit_weight: 0.4,
            freshness_weight: 0.25,
            exchange_weight: 0.15,
            volume_weight: 0.1,
            confidence_weight: 0.1,
        }
    }
}

pub struct OpportunityScoringEngine {
    weights: ScoringWeights,
    exchange_reliability: HashMap<String, f64>,
}

impl OpportunityScoringEngine {
    pub fn new() -> Self {
        let mut exchange_reliability = HashMap::new();

        // Exchange reliability scores based on uptime, API stability, and liquidity
        exchange_reliability.insert("coinbase".to_string(), 0.95);
        exchange_reliability.insert("binance".to_string(), 0.90);
        exchange_reliability.insert("okx".to_string(), 0.85);
        exchange_reliability.insert("bybit".to_string(), 0.80);
        exchange_reliability.insert("kraken".to_string(), 0.85);
        exchange_reliability.insert("huobi".to_string(), 0.75);
        exchange_reliability.insert("kucoin".to_string(), 0.70);

        Self {
            weights: ScoringWeights::default(),
            exchange_reliability,
        }
    }

    pub fn with_custom_weights(weights: ScoringWeights) -> Self {
        let mut engine = Self::new();
        engine.weights = weights;
        engine
    }

    /// Calculate comprehensive scores for opportunities and sort by highest total score
    pub fn score_and_sort_opportunities(
        &self,
        opportunities: &mut Vec<ArbitrageOpportunity>,
    ) -> Vec<OpportunityScore> {
        let current_time = get_current_timestamp() as i64;
        let mut scores = Vec::new();

        // Calculate scores for each opportunity
        for opportunity in opportunities.iter() {
            let score = self.calculate_opportunity_score(opportunity, current_time);
            scores.push(score);
        }

        // Sort opportunities by total score (highest first)
        let mut indexed_scores: Vec<(usize, &OpportunityScore)> =
            scores.iter().enumerate().collect();
        indexed_scores.sort_by(|a, b| b.1.total_score.partial_cmp(&a.1.total_score).unwrap());

        // Reorder the original opportunities array
        let mut sorted_opportunities = Vec::new();
        for (original_index, _) in &indexed_scores {
            sorted_opportunities.push(opportunities[*original_index].clone());
        }
        *opportunities = sorted_opportunities;

        // Sort scores by total score
        scores.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap());

        console_log!(
            "🎯 Scored and sorted {} opportunities by profit + freshness + reliability",
            scores.len()
        );
        scores
    }

    /// Calculate individual opportunity score
    pub fn calculate_opportunity_score(
        &self,
        opportunity: &ArbitrageOpportunity,
        current_time: i64,
    ) -> OpportunityScore {
        let profit_score = self.calculate_profit_score(opportunity.rate_difference);
        let freshness_score =
            self.calculate_freshness_score(opportunity.created_at as i64, current_time);
        let exchange_reliability_score = self.calculate_exchange_reliability_score(
            opportunity.long_exchange.as_str(),
            opportunity.short_exchange.as_str(),
        );
        let volume_score = self.calculate_volume_score(Some(opportunity.volume));
        let confidence_score = self.calculate_confidence_score(opportunity.confidence_score as i32);
        let time_decay_factor =
            self.calculate_time_decay_factor(opportunity.created_at as i64, current_time);

        // Calculate weighted total score
        let total_score = (profit_score * self.weights.profit_weight)
            + (freshness_score * self.weights.freshness_weight)
            + (exchange_reliability_score * self.weights.exchange_weight)
            + (volume_score * self.weights.volume_weight)
            + (confidence_score * self.weights.confidence_weight);

        // Apply time decay factor
        let final_score = total_score * time_decay_factor;

        OpportunityScore {
            opportunity_id: opportunity.id.clone(),
            symbol: opportunity.pair.clone(),
            total_score: final_score,
            profit_score,
            freshness_score,
            exchange_reliability_score,
            volume_score,
            confidence_score,
            time_decay_factor,
            calculated_at: current_time,
        }
    }

    /// Group opportunities by symbol and return best opportunity per symbol
    pub fn get_best_opportunities_by_symbol(
        &self,
        opportunities: &[ArbitrageOpportunity],
    ) -> Vec<ArbitrageOpportunity> {
        let mut best_by_symbol: HashMap<String, (ArbitrageOpportunity, f64)> = HashMap::new();
        let current_time = get_current_timestamp() as i64;

        for opportunity in opportunities {
            let score = self.calculate_opportunity_score(opportunity, current_time);

            match best_by_symbol.get(&opportunity.pair) {
                Some((_, existing_score)) => {
                    if score.total_score > *existing_score {
                        best_by_symbol.insert(
                            opportunity.pair.clone(),
                            (opportunity.clone(), score.total_score),
                        );
                    }
                }
                None => {
                    best_by_symbol.insert(
                        opportunity.pair.clone(),
                        (opportunity.clone(), score.total_score),
                    );
                }
            }
        }

        // Extract opportunities and sort by score
        let mut best_opportunities: Vec<(ArbitrageOpportunity, f64)> =
            best_by_symbol.into_values().collect();
        best_opportunities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let result: Vec<ArbitrageOpportunity> =
            best_opportunities.into_iter().map(|(opp, _)| opp).collect();

        console_log!(
            "🎯 Selected {} best opportunities (one per symbol) from {} total",
            result.len(),
            opportunities.len()
        );
        result
    }

    /// Get opportunities summary with grouping information
    pub fn get_opportunities_summary(
        &self,
        opportunities: &[ArbitrageOpportunity],
    ) -> OpportunitiesSummary {
        let mut symbol_groups: HashMap<String, Vec<ArbitrageOpportunity>> = HashMap::new();
        let mut exchange_counts: HashMap<String, i32> = HashMap::new();
        let current_time = get_current_timestamp() as i64;

        // Group by symbol and count exchanges
        for opportunity in opportunities {
            symbol_groups
                .entry(opportunity.pair.clone())
                .or_default()
                .push(opportunity.clone());
            *exchange_counts
                .entry(opportunity.long_exchange.as_str().to_string())
                .or_insert(0) += 1;
            *exchange_counts
                .entry(opportunity.short_exchange.as_str().to_string())
                .or_insert(0) += 1;
        }

        // Calculate summary for each symbol
        let mut symbol_summaries = Vec::new();
        for (symbol, group_opportunities) in symbol_groups {
            let best_opportunities = self.get_best_opportunities_by_symbol(&group_opportunities);
            if let Some(best) = best_opportunities.first() {
                let exchanges: Vec<String> = group_opportunities
                    .iter()
                    .flat_map(|opp| {
                        vec![
                            opp.long_exchange.as_str().to_string(),
                            opp.short_exchange.as_str().to_string(),
                        ]
                    })
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                let earliest_expiry = group_opportunities
                    .iter()
                    .filter_map(|opp| opp.expires_at)
                    .min()
                    .unwrap_or((current_time + 3600) as u64); // Default 1 hour if no expiry

                symbol_summaries.push(SymbolSummary {
                    symbol: symbol.clone(),
                    best_profit_percentage: best.rate_difference,
                    total_opportunities: group_opportunities.len() as i32,
                    available_exchanges: exchanges,
                    earliest_expiry: earliest_expiry as i64,
                    time_to_expiry: (earliest_expiry as i64) - current_time,
                    best_opportunity: best.clone(),
                });
            }
        }

        // Sort by best profit percentage
        symbol_summaries.sort_by(|a, b| {
            b.best_profit_percentage
                .partial_cmp(&a.best_profit_percentage)
                .unwrap()
        });

        OpportunitiesSummary {
            total_opportunities: opportunities.len() as i32,
            unique_symbols: symbol_summaries.len() as i32,
            unique_exchanges: exchange_counts.len() as i32,
            symbol_summaries,
            exchange_distribution: exchange_counts,
            generated_at: current_time,
        }
    }

    // Private scoring methods
    fn calculate_profit_score(&self, rate_difference: f64) -> f64 {
        // Normalize profit percentage to 0-100 scale
        // Assume max expected profit is 5% for normalization
        let max_expected_profit = 5.0;
        let normalized_profit = (rate_difference / max_expected_profit * 100.0).min(100.0);

        // Apply logarithmic scaling to emphasize higher profits
        if normalized_profit > 0.0 {
            (normalized_profit.ln() + 1.0) * 20.0 // Scale to 0-100
        } else {
            0.0
        }
    }

    fn calculate_freshness_score(&self, created_at: i64, current_time: i64) -> f64 {
        let age_seconds = current_time - created_at;
        let max_age = 300.0; // 5 minutes for full freshness score

        if age_seconds <= 0 {
            return 100.0; // Brand new
        }

        let age_factor = (max_age - age_seconds as f64) / max_age;
        (age_factor.max(0.0) * 100.0).min(100.0)
    }

    fn calculate_exchange_reliability_score(&self, exchange_from: &str, exchange_to: &str) -> f64 {
        let from_score = self.exchange_reliability.get(exchange_from).unwrap_or(&0.5);
        let to_score = self.exchange_reliability.get(exchange_to).unwrap_or(&0.5);

        // Average reliability of both exchanges
        ((from_score + to_score) / 2.0) * 100.0
    }

    fn calculate_volume_score(&self, volume_24h: Option<f64>) -> f64 {
        match volume_24h {
            Some(volume) => {
                // Normalize volume (assume $10M is high volume)
                let max_volume = 10_000_000.0;
                let normalized_volume = (volume / max_volume * 100.0).min(100.0);

                // Apply square root scaling to reduce volume impact
                normalized_volume.sqrt() * 10.0
            }
            None => 50.0, // Default score when volume is unknown
        }
    }

    fn calculate_confidence_score(&self, confidence: i32) -> f64 {
        confidence as f64 // Already 0-100 scale
    }

    fn calculate_time_decay_factor(&self, created_at: i64, current_time: i64) -> f64 {
        let age_seconds = current_time - created_at;
        let half_life = 600.0; // 10 minutes half-life

        // Exponential decay: score = initial * (0.5)^(age/half_life)
        0.5_f64.powf(age_seconds as f64 / half_life).max(0.1) // Minimum 10% of original score
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSummary {
    pub symbol: String,
    pub best_profit_percentage: f64,
    pub total_opportunities: i32,
    pub available_exchanges: Vec<String>,
    pub earliest_expiry: i64,
    pub time_to_expiry: i64,
    pub best_opportunity: ArbitrageOpportunity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunitiesSummary {
    pub total_opportunities: i32,
    pub unique_symbols: i32,
    pub unique_exchanges: i32,
    pub symbol_summaries: Vec<SymbolSummary>,
    pub exchange_distribution: HashMap<String, i32>,
    pub generated_at: i64,
}

impl Default for OpportunityScoringEngine {
    fn default() -> Self {
        Self::new()
    }
}
