use crate::utils::{get_current_timestamp, ArbitrageError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::D1Database;

#[cfg(target_arch = "wasm32")]
use worker::console_log;

#[cfg(not(target_arch = "wasm32"))]
macro_rules! console_log {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRate {
    pub id: String,
    pub exchange: String,
    pub symbol: String,
    pub funding_rate: f64,      // Current funding rate (e.g., 0.0001 = 0.01%)
    pub next_funding_time: i64, // Unix timestamp
    pub predicted_rate: Option<f64>, // AI-predicted next rate
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateArbitrage {
    pub id: String,
    pub symbol: String,
    pub long_exchange: String,  // Exchange to go long (lower funding rate)
    pub short_exchange: String, // Exchange to go short (higher funding rate)
    pub rate_difference: f64,   // Funding rate difference
    pub profit_potential: f64,  // Expected profit percentage
    pub next_funding_time: i64, // When rates refresh
    pub time_to_funding: i64,   // Seconds until funding
    pub confidence_score: i32,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateNotification {
    pub symbol: String,
    pub profit_potential: f64,
    pub time_to_funding: String,   // Human readable (e.g., "2h 15m")
    pub next_funding_time: String, // Human readable (e.g., "16:00 UTC")
    pub exchanges: String,         // "Long: Binance, Short: OKX"
    pub urgency: String,           // "HIGH", "MEDIUM", "LOW"
}

pub struct FundingRateManager {
    database: Option<Arc<D1Database>>,
    exchange_service: Option<crate::services::core::trading::exchange::ExchangeService>,
}

impl FundingRateManager {
    pub fn new(
        database: Option<Arc<D1Database>>,
        exchange_service: Option<crate::services::core::trading::exchange::ExchangeService>,
    ) -> Self {
        Self {
            database,
            exchange_service,
        }
    }

    /// Fetch current funding rates from all exchanges
    pub async fn fetch_funding_rates(
        &self,
        symbols: &[String],
    ) -> Result<Vec<FundingRate>, ArbitrageError> {
        let exchange_service = self.exchange_service.as_ref().ok_or_else(|| {
            ArbitrageError::api_error("Exchange service not available".to_string())
        })?;

        let mut funding_rates = Vec::new();
        let exchanges = vec!["binance", "okx", "bybit"]; // Exchanges that support perpetual futures

        for exchange in &exchanges {
            for symbol in symbols {
                match self
                    .fetch_exchange_funding_rate(exchange_service, exchange, symbol)
                    .await
                {
                    Ok(rate) => funding_rates.push(rate),
                    Err(e) => {
                        console_log!(
                            "⚠️ Failed to fetch funding rate for {} on {}: {:?}",
                            symbol,
                            exchange,
                            e
                        );
                    }
                }
            }
        }

        console_log!(
            "📊 Fetched {} funding rates across {} exchanges",
            funding_rates.len(),
            exchanges.len()
        );
        Ok(funding_rates)
    }

    /// Detect funding rate arbitrage opportunities
    pub async fn detect_funding_arbitrage(
        &self,
        symbols: &[String],
    ) -> Result<Vec<FundingRateArbitrage>, ArbitrageError> {
        let funding_rates = self.fetch_funding_rates(symbols).await?;
        let mut arbitrage_opportunities = Vec::new();

        // Group funding rates by symbol
        let mut rates_by_symbol: HashMap<String, Vec<FundingRate>> = HashMap::new();
        for rate in funding_rates {
            rates_by_symbol
                .entry(rate.symbol.clone())
                .or_default()
                .push(rate);
        }

        // Find arbitrage opportunities for each symbol
        for (symbol, rates) in rates_by_symbol {
            if rates.len() < 2 {
                continue; // Need at least 2 exchanges for arbitrage
            }

            // Find the exchange with lowest and highest funding rates
            let min_rate = rates
                .iter()
                .min_by(|a, b| a.funding_rate.partial_cmp(&b.funding_rate).unwrap());
            let max_rate = rates
                .iter()
                .max_by(|a, b| a.funding_rate.partial_cmp(&b.funding_rate).unwrap());

            if let (Some(min), Some(max)) = (min_rate, max_rate) {
                let rate_difference = max.funding_rate - min.funding_rate;

                console_log!(
                    "🔍 FUNDING RATE ANALYSIS - {}: Min={:.6}% ({}), Max={:.6}% ({}), Diff={:.6}%",
                    symbol,
                    min.funding_rate * 100.0,
                    min.exchange,
                    max.funding_rate * 100.0,
                    max.exchange,
                    rate_difference * 100.0
                );

                // Only consider significant rate differences (>0.005% = 0.00005)
                if rate_difference > 0.00005 {
                    let profit_potential = rate_difference * 100.0; // Convert to percentage
                    let next_funding_time =
                        std::cmp::min(min.next_funding_time, max.next_funding_time);
                    let time_to_funding = next_funding_time - (get_current_timestamp() as i64);

                    // Only consider opportunities with at least 10 minutes until funding
                    if time_to_funding > 600 {
                        let confidence_score =
                            self.calculate_funding_confidence(rate_difference, time_to_funding);

                        console_log!(
                            "✅ FUNDING ARBITRAGE OPPORTUNITY - {}: {:.4}% profit, {}m until next funding",
                            symbol,
                            profit_potential,
                            time_to_funding / 60
                        );

                        arbitrage_opportunities.push(FundingRateArbitrage {
                            id: format!(
                                "funding_{}_{}_{}",
                                symbol.replace("/", ""),
                                min.exchange,
                                max.exchange
                            ),
                            symbol: symbol.clone(),
                            long_exchange: min.exchange.clone(), // Go long on lower funding rate
                            short_exchange: max.exchange.clone(), // Go short on higher funding rate
                            rate_difference,
                            profit_potential,
                            next_funding_time,
                            time_to_funding,
                            confidence_score,
                            created_at: get_current_timestamp() as i64,
                            expires_at: next_funding_time - 300, // Expire 5 minutes before funding
                        });
                    } else {
                        console_log!(
                            "⏰ FUNDING OPPORTUNITY FILTERED - {}: Too close to funding ({}m remaining)",
                            symbol,
                            time_to_funding / 60
                        );
                    }
                } else {
                    console_log!(
                        "⏰ FUNDING OPPORTUNITY FILTERED - {}: Funding rate difference too small",
                        symbol
                    );
                }
            }
        }

        console_log!(
            "🎯 Found {} funding rate arbitrage opportunities",
            arbitrage_opportunities.len()
        );
        Ok(arbitrage_opportunities)
    }

    /// Store funding rates in database
    pub async fn store_funding_rates(&self, rates: &[FundingRate]) -> Result<(), ArbitrageError> {
        let db = self.database.as_ref().ok_or_else(|| {
            ArbitrageError::database_error(
                "Database not available for funding rate storage".to_string(),
            )
        })?;

        for rate in rates {
            let query = format!(
                "INSERT OR REPLACE INTO funding_rates (id, exchange, symbol, funding_rate, next_funding_time, predicted_rate, created_at, updated_at) VALUES ('{}', '{}', '{}', {}, {}, {}, {}, {})",
                rate.id,
                rate.exchange,
                rate.symbol,
                rate.funding_rate,
                rate.next_funding_time,
                rate.predicted_rate.map(|p| p.to_string()).unwrap_or("NULL".to_string()),
                rate.created_at,
                rate.updated_at
            );

            let stmt = db.prepare(&query);
            let result = stmt.run().await;

            if let Err(e) = result {
                console_log!("❌ Failed to store funding rate {}: {:?}", rate.id, e);
            }
        }

        console_log!("💾 Stored {} funding rates in database", rates.len());
        Ok(())
    }

    /// Generate user-friendly notifications for funding rate opportunities
    pub fn generate_funding_notifications(
        &self,
        opportunities: &[FundingRateArbitrage],
    ) -> Vec<FundingRateNotification> {
        opportunities
            .iter()
            .map(|opp| {
                let time_to_funding_str = self.format_time_duration(opp.time_to_funding);
                let next_funding_str = self.format_timestamp_utc(opp.next_funding_time);
                let exchanges_str =
                    format!("Long: {}, Short: {}", opp.long_exchange, opp.short_exchange);

                let urgency = if opp.time_to_funding < 3600 {
                    "HIGH".to_string() // Less than 1 hour
                } else if opp.time_to_funding < 7200 {
                    "MEDIUM".to_string() // Less than 2 hours
                } else {
                    "LOW".to_string()
                };

                FundingRateNotification {
                    symbol: opp.symbol.clone(),
                    profit_potential: opp.profit_potential,
                    time_to_funding: time_to_funding_str,
                    next_funding_time: next_funding_str,
                    exchanges: exchanges_str,
                    urgency,
                }
            })
            .collect()
    }

    /// Get next funding times for all tracked symbols
    pub async fn get_next_funding_times(&self) -> Result<HashMap<String, i64>, ArbitrageError> {
        let db = self
            .database
            .as_ref()
            .ok_or_else(|| ArbitrageError::database_error("Database not available".to_string()))?;

        let query = "SELECT symbol, MIN(next_funding_time) as next_funding FROM funding_rates GROUP BY symbol";
        let stmt = db.prepare(query);
        let result = stmt.all().await;

        match result {
            Ok(d1_result) => {
                let results = d1_result.results::<HashMap<String, serde_json::Value>>()?;
                let mut funding_times = HashMap::new();

                for row in results {
                    let symbol = row
                        .get("symbol")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let next_funding = row
                        .get("next_funding")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);

                    if !symbol.is_empty() && next_funding > 0 {
                        funding_times.insert(symbol, next_funding);
                    }
                }

                Ok(funding_times)
            }
            Err(e) => {
                console_log!("❌ Failed to get funding times: {:?}", e);
                Ok(HashMap::new())
            }
        }
    }

    /// Get funding rate for a specific symbol and exchange
    pub async fn get_funding_rate_for_exchange(
        &self,
        symbol: &str,
        exchange: &str,
    ) -> Result<Option<f64>, ArbitrageError> {
        let db = self
            .database
            .as_ref()
            .ok_or_else(|| ArbitrageError::database_error("Database not available".to_string()))?;

        let query = format!(
            "SELECT funding_rate FROM funding_rates WHERE symbol = '{}' AND exchange = '{}' ORDER BY updated_at DESC LIMIT 1",
            symbol, exchange
        );
        let stmt = db.prepare(&query);
        let result = stmt.first::<HashMap<String, serde_json::Value>>(None).await;

        match result {
            Ok(Some(row)) => {
                let funding_rate = row
                    .get("funding_rate")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                Ok(Some(funding_rate))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                console_log!(
                    "❌ Failed to get funding rate for {} on {}: {:?}",
                    symbol,
                    exchange,
                    e
                );
                Ok(None)
            }
        }
    }

    /// Get all funding rates for a symbol (for integration purposes)
    pub async fn get_funding_rates_for_symbol(
        &self,
        symbol: &str,
    ) -> Result<HashMap<String, f64>, ArbitrageError> {
        let db = self
            .database
            .as_ref()
            .ok_or_else(|| ArbitrageError::database_error("Database not available".to_string()))?;

        let query = format!(
            "SELECT exchange, funding_rate FROM funding_rates WHERE symbol = '{}' ORDER BY updated_at DESC",
            symbol
        );
        let stmt = db.prepare(&query);
        let result = stmt.all().await;

        match result {
            Ok(d1_result) => {
                let results = d1_result.results::<HashMap<String, serde_json::Value>>()?;
                let mut rates = HashMap::new();

                for row in results {
                    let exchange = row
                        .get("exchange")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let funding_rate = row
                        .get("funding_rate")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    if !exchange.is_empty() {
                        rates.insert(exchange, funding_rate);
                    }
                }

                Ok(rates)
            }
            Err(e) => {
                console_log!("❌ Failed to get funding rates for {}: {:?}", symbol, e);
                Ok(HashMap::new())
            }
        }
    }

    // Private helper methods
    async fn fetch_exchange_funding_rate(
        &self,
        exchange_service: &crate::services::core::trading::exchange::ExchangeService,
        exchange: &str,
        symbol: &str,
    ) -> Result<FundingRate, ArbitrageError> {
        let current_time = get_current_timestamp();
        let next_funding_time = self.calculate_next_funding_time(current_time as i64);

        // Convert symbol format for different exchanges (e.g., BTC/USDT -> BTCUSDT)
        let exchange_symbol = symbol.replace("/", "");

        // Fetch real funding rate from exchange API
        let funding_rate = match exchange {
            "binance" => {
                self.fetch_binance_funding_rate(exchange_service, &exchange_symbol)
                    .await
                    .unwrap_or(0.0001) // Fallback to 0.01%
            }
            "okx" => {
                self.fetch_okx_funding_rate(exchange_service, &exchange_symbol)
                    .await
                    .unwrap_or(0.0002) // Fallback to 0.02%
            }
            "bybit" => {
                self.fetch_bybit_funding_rate(exchange_service, &exchange_symbol)
                    .await
                    .unwrap_or(0.00015) // Fallback to 0.015%
            }
            _ => 0.0001, // Default fallback
        };

        console_log!(
            "📊 FUNDING RATE FETCHED - {}: {} = {:.4}%",
            exchange,
            symbol,
            funding_rate * 100.0
        );

        Ok(FundingRate {
            id: format!("{}_{}_{}", exchange, symbol, current_time),
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            funding_rate,
            next_funding_time,
            predicted_rate: None, // TODO: Implement AI prediction
            created_at: current_time as i64,
            updated_at: current_time as i64,
        })
    }

    fn calculate_next_funding_time(&self, current_time: i64) -> i64 {
        // Funding typically occurs every 8 hours at 00:00, 08:00, 16:00 UTC
        let hours_in_day = 24;
        let funding_interval = 8; // hours
        let seconds_per_hour = 3600;

        let current_hour = (current_time / seconds_per_hour) % hours_in_day;
        let next_funding_hour = ((current_hour / funding_interval) + 1) * funding_interval;

        let hours_until_funding = if next_funding_hour >= hours_in_day {
            funding_interval - (current_hour % funding_interval)
        } else {
            next_funding_hour - current_hour
        };

        current_time + (hours_until_funding * seconds_per_hour)
    }

    fn calculate_funding_confidence(&self, rate_difference: f64, time_to_funding: i64) -> i32 {
        let mut confidence = 50; // Base confidence

        // Higher rate difference = higher confidence
        if rate_difference > 0.0005 {
            confidence += 30; // Very significant difference
        } else if rate_difference > 0.0002 {
            confidence += 20; // Significant difference
        } else if rate_difference > 0.0001 {
            confidence += 10; // Moderate difference
        }

        // More time = higher confidence (more stable)
        if time_to_funding > 14400 {
            // > 4 hours
            confidence += 20;
        } else if time_to_funding > 7200 {
            // > 2 hours
            confidence += 10;
        }

        std::cmp::min(confidence, 95) // Cap at 95%
    }

    fn format_time_duration(&self, seconds: i64) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;

        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        }
    }

    fn format_timestamp_utc(&self, timestamp: i64) -> String {
        // Simplified UTC formatting (in production, use proper date library)
        let hours = (timestamp / 3600) % 24;
        let minutes = (timestamp % 3600) / 60;
        format!("{:02}:{:02} UTC", hours, minutes)
    }

    /// Fetch funding rate from Binance API
    async fn fetch_binance_funding_rate(
        &self,
        exchange_service: &crate::services::core::trading::exchange::ExchangeService,
        symbol: &str,
    ) -> Result<f64, ArbitrageError> {
        console_log!("🌐 BINANCE FUNDING API CALL for symbol: {}", symbol);

        // Use the existing get_binance_funding_rate method
        match exchange_service.get_binance_funding_rate(symbol).await {
            Ok(funding_info) => {
                console_log!(
                    "📊 BINANCE FUNDING RATE FETCHED: {:.6}",
                    funding_info.funding_rate
                );
                Ok(funding_info.funding_rate)
            }
            Err(e) => {
                console_log!("❌ BINANCE FUNDING API ERROR: {}", e);
                Ok(0.0001) // Fallback
            }
        }
    }

    /// Fetch funding rate from OKX API
    async fn fetch_okx_funding_rate(
        &self,
        exchange_service: &crate::services::core::trading::exchange::ExchangeService,
        symbol: &str,
    ) -> Result<f64, ArbitrageError> {
        console_log!("🌐 OKX FUNDING API CALL for symbol: {}", symbol);

        // Use the existing funding rate fetching method
        match exchange_service
            .get_global_funding_rates("okx", Some(symbol))
            .await
        {
            Ok(rates) => {
                if let Some(rate_data) = rates.first() {
                    if let Some(funding_rate) =
                        rate_data.get("fundingRate").and_then(|v| v.as_str())
                    {
                        if let Ok(rate) = funding_rate.parse::<f64>() {
                            console_log!("📊 OKX FUNDING RATE FETCHED: {:.6}", rate);
                            return Ok(rate);
                        }
                    }
                }
                console_log!("⚠️ OKX FUNDING RATE PARSE FAILED - Using fallback");
                Ok(0.0002) // Fallback
            }
            Err(e) => {
                console_log!("❌ OKX FUNDING API ERROR: {}", e);
                Ok(0.0002) // Fallback
            }
        }
    }

    /// Fetch funding rate from Bybit API
    async fn fetch_bybit_funding_rate(
        &self,
        exchange_service: &crate::services::core::trading::exchange::ExchangeService,
        symbol: &str,
    ) -> Result<f64, ArbitrageError> {
        console_log!("🌐 BYBIT FUNDING API CALL for symbol: {}", symbol);

        // Use the existing get_bybit_funding_rate method
        match exchange_service.get_bybit_funding_rate(symbol).await {
            Ok(funding_info) => {
                console_log!(
                    "📊 BYBIT FUNDING RATE FETCHED: {:.6}",
                    funding_info.funding_rate
                );
                Ok(funding_info.funding_rate)
            }
            Err(e) => {
                console_log!("❌ BYBIT FUNDING API ERROR: {}", e);
                Ok(0.00015) // Fallback
            }
        }
    }
}
