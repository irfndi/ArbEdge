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
pub struct HistoricalOpportunity {
    pub id: String,
    pub symbol: String,
    pub exchange_from: String,
    pub exchange_to: String,
    pub profit_percentage: f64,
    pub rate_difference: f64,
    pub volume_24h: Option<f64>,
    pub price_from: f64,
    pub price_to: f64,
    pub confidence_score: i32,
    pub market_conditions: Option<String>, // JSON string
    pub execution_attempted: bool,
    pub execution_success: bool,
    pub execution_profit_actual: Option<f64>,
    pub created_at: i64,
    pub archived_at: i64,
    pub expires_at: Option<i64>,
    pub funding_rate_from: Option<f64>,
    pub funding_rate_to: Option<f64>,
    pub next_funding_time: Option<i64>,
    pub opportunity_type: String, // 'spot', 'perpetual', 'funding_rate'
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityAnalytics {
    pub id: String,
    pub date_key: String, // YYYY-MM-DD
    pub symbol: String,
    pub total_opportunities: i32,
    pub avg_profit_percentage: f64,
    pub max_profit_percentage: f64,
    pub execution_success_rate: f64,
    pub total_volume: f64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_opportunities: i32,
    pub successful_executions: i32,
    pub avg_profit_percentage: f64,
    pub best_performing_symbols: Vec<String>,
    pub best_performing_exchanges: Vec<String>,
    pub success_rate_by_symbol: HashMap<String, f64>,
    pub profit_trends: Vec<(String, f64)>, // (date, avg_profit)
}

pub struct HistoricalDataManager {
    database: Option<Arc<D1Database>>,
}

impl HistoricalDataManager {
    pub fn new(database: Option<Arc<D1Database>>) -> Self {
        Self { database }
    }

    /// Archive expired opportunities to historical storage
    pub async fn archive_expired_opportunities(
        &self,
        max_age_hours: i64,
    ) -> Result<i32, ArbitrageError> {
        let db = self.database.as_ref().ok_or_else(|| {
            ArbitrageError::database_error("Database not available for archival".to_string())
        })?;

        let current_time = get_current_timestamp();
        let cutoff_time = (current_time as i64) - (max_age_hours * 3600);

        // First, move opportunities to history with NULL handling
        let archive_query = format!(
            "INSERT INTO opportunity_history (id, symbol, exchange_from, exchange_to, profit_percentage, rate_difference, volume_24h, price_from, price_to, confidence_score, created_at, archived_at, expires_at, opportunity_type) SELECT id, COALESCE(pair, 'unknown'), COALESCE(long_exchange, 'unknown'), COALESCE(short_exchange, 'unknown'), COALESCE(rate_difference, 0.0), COALESCE(rate_difference, 0.0), COALESCE(potential_profit_value, 0.0), COALESCE(long_rate, 0.0), COALESCE(short_rate, 0.0), COALESCE(priority_score, 50), created_at, {}, COALESCE(expiry_timestamp, created_at + 3600000), COALESCE(type, 'spot') FROM opportunities WHERE created_at < {}",
            current_time as i64, cutoff_time
        );

        let archive_stmt = db.prepare(&archive_query);
        let archive_result = archive_stmt.run().await;

        match archive_result {
            Ok(_) => {
                // Delete archived opportunities from main table
                let delete_query = format!(
                    "DELETE FROM opportunities WHERE created_at < {}",
                    cutoff_time
                );
                let delete_stmt = db.prepare(&delete_query);
                let delete_result = delete_stmt.run().await;

                match delete_result {
                    Ok(result) => {
                        let changes = result
                            .meta()
                            .map_or(0, |m| m.unwrap().rows_written.unwrap_or(0))
                            as i32;

                        console_log!(
                            "📚 Archived {} opportunities to historical storage",
                            changes
                        );
                        Ok(changes)
                    }
                    Err(e) => {
                        console_log!("❌ Failed to delete archived opportunities: {:?}", e);
                        Err(ArbitrageError::database_error(format!(
                            "Failed to delete archived opportunities: {:?}",
                            e
                        )))
                    }
                }
            }
            Err(e) => {
                console_log!("❌ Failed to archive opportunities: {:?}", e);
                Err(ArbitrageError::database_error(format!(
                    "Failed to archive opportunities: {:?}",
                    e
                )))
            }
        }
    }

    /// Generate daily analytics summary
    pub async fn generate_daily_analytics(
        &self,
        date: &str,
    ) -> Result<Vec<OpportunityAnalytics>, ArbitrageError> {
        let db = self.database.as_ref().ok_or_else(|| {
            ArbitrageError::database_error("Database not available for analytics".to_string())
        })?;

        let start_timestamp = self.date_to_timestamp(date)?;
        let end_timestamp = start_timestamp + 86400; // 24 hours

        let query = format!(
            "SELECT symbol, COUNT(*) as total_opportunities, AVG(profit_percentage) as avg_profit, MAX(profit_percentage) as max_profit, AVG(CASE WHEN execution_success = 1 THEN 1.0 ELSE 0.0 END) as success_rate, SUM(COALESCE(volume_24h, 0)) as total_volume FROM opportunity_history WHERE created_at >= {} AND created_at < {} GROUP BY symbol",
            start_timestamp, end_timestamp
        );

        let stmt = db.prepare(&query);
        let result = stmt.all().await;

        match result {
            Ok(d1_result) => {
                let results = d1_result.results::<HashMap<String, serde_json::Value>>()?;
                let mut analytics = Vec::new();

                for row in results {
                    let symbol_str = row
                        .get("symbol")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let total_opportunities = row
                        .get("total_opportunities")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    let avg_profit = row
                        .get("avg_profit")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let max_profit = row
                        .get("max_profit")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let success_rate = row
                        .get("success_rate")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let total_volume = row
                        .get("total_volume")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    if !symbol_str.is_empty() {
                        analytics.push(OpportunityAnalytics {
                            id: format!("{}_{}", date, symbol_str),
                            date_key: date.to_string(),
                            symbol: symbol_str,
                            total_opportunities,
                            avg_profit_percentage: avg_profit,
                            max_profit_percentage: max_profit,
                            execution_success_rate: success_rate,
                            total_volume,
                            created_at: get_current_timestamp() as i64,
                        });
                    }
                }

                console_log!(
                    "📊 Generated analytics for {} symbols on {}",
                    analytics.len(),
                    date
                );
                Ok(analytics)
            }
            Err(e) => {
                console_log!("❌ Failed to generate analytics: {:?}", e);
                Err(ArbitrageError::database_error(format!(
                    "Failed to generate analytics: {:?}",
                    e
                )))
            }
        }
    }

    /// Record opportunity execution result for ML training
    pub async fn record_execution_result(
        &self,
        opportunity_id: &str,
        success: bool,
        actual_profit: Option<f64>,
    ) -> Result<(), ArbitrageError> {
        let db = self.database.as_ref().ok_or_else(|| {
            ArbitrageError::database_error(
                "Database not available for execution recording".to_string(),
            )
        })?;

        let query = format!(
            "UPDATE opportunity_history SET execution_attempted = 1, execution_success = {}, execution_profit_actual = {} WHERE id = '{}'",
            if success { 1 } else { 0 },
            actual_profit.map(|p| p.to_string()).unwrap_or("NULL".to_string()),
            opportunity_id
        );

        let stmt = db.prepare(&query);
        let result = stmt.run().await;

        match result {
            Ok(_) => {
                console_log!(
                    "📝 Recorded execution result for opportunity {}: success={}, profit={:?}",
                    opportunity_id,
                    success,
                    actual_profit
                );
                Ok(())
            }
            Err(e) => {
                console_log!("❌ Failed to record execution result: {:?}", e);
                Err(ArbitrageError::database_error(format!(
                    "Failed to record execution result: {:?}",
                    e
                )))
            }
        }
    }

    // Helper methods
    fn date_to_timestamp(&self, date: &str) -> Result<i64, ArbitrageError> {
        // Simple date parsing for YYYY-MM-DD format
        let parts: Vec<&str> = date.split('-').collect();
        if parts.len() != 3 {
            return Err(ArbitrageError::validation_error(
                "Invalid date format, expected YYYY-MM-DD".to_string(),
            ));
        }

        // Simplified timestamp calculation (days since epoch)
        let year: i32 = parts[0].parse().unwrap_or(2024);
        let month: i32 = parts[1].parse().unwrap_or(1);
        let day: i32 = parts[2].parse().unwrap_or(1);

        // Approximate timestamp (this is simplified)
        let days_since_epoch = (year - 1970) * 365 + (month - 1) * 30 + day;
        Ok(days_since_epoch as i64 * 86400)
    }
}
