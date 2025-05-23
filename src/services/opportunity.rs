// src/services/opportunity.rs

use crate::services::exchange::ExchangeService;
use crate::services::telegram::TelegramService;
use crate::types::{ArbitrageOpportunity, ArbitrageType, ExchangeIdEnum, FundingRateInfo, StructuredTradingPair};
use crate::utils::ArbitrageResult;
use crate::services::exchange::ExchangeInterface;
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
        }
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

                let task = async move {
                    let result = exchange_service
                        .fetch_funding_rates(&exchange_id.to_string(), Some(&pair))
                        .await;
                    
                    // Parse the result to extract FundingRateInfo
                    let funding_info = match result {
                        Ok(rates) => {
                            if let Some(rate_data) = rates.first() {
                                // Extract funding rate from the response
                                let funding_rate = rate_data["fundingRate"]
                                    .as_str()
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0);
                                
                                Some(FundingRateInfo {
                                    symbol: pair.clone(),
                                    funding_rate,
                                    timestamp: Some(chrono::Utc::now()),
                                    datetime: Some(chrono::Utc::now().to_rfc3339()),
                                    next_funding_time: None,
                                    estimated_rate: None,
                                })
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    };
                    
                    (pair, exchange_id, funding_info)
                };
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
                                let net_difference = Some(rate_diff);

                                // Create opportunity
                                let opportunity = ArbitrageOpportunity {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    pair: pair.clone(),
                                    r#type: ArbitrageType::FundingRate,
                                    long_exchange: Some(long_exchange),
                                    short_exchange: Some(short_exchange),
                                    long_rate: Some(long_rate),
                                    short_rate: Some(short_rate),
                                    rate_difference: rate_diff,
                                    net_rate_difference: net_difference,
                                    potential_profit_value: None, // Will be calculated if position size is known
                                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                    details: Some(format!(
                                        "Funding rate arbitrage: Long {} ({}%) / Short {} ({}%)",
                                        long_exchange,
                                        (long_rate * 100.0 * 10000.0).round() / 10000.0,
                                        short_exchange,
                                        (short_rate * 100.0 * 10000.0).round() / 10000.0
                                    )),
                                };

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
                    eprintln!("Failed to send Telegram notification: {}", e);
                }
            }
        }

        Ok(())
    }

    pub fn get_config(&self) -> &OpportunityServiceConfig {
        &self.config
    }
}
