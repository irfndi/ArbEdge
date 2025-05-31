// Analytics Repository - Specialized Analytics Data Access Component
// Handles trading analytics, performance data, and user analytics

use super::{utils::*, Repository, RepositoryConfig, RepositoryHealth, RepositoryMetrics};
use crate::types::TradingAnalytics;
use crate::utils::ArbitrageResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::{kv::KvStore, D1Database};

/// Configuration for AnalyticsRepository
#[derive(Debug, Clone)]
pub struct AnalyticsRepositoryConfig {
    pub enable_caching: bool,
    pub cache_ttl_seconds: u64,
    pub enable_batch_operations: bool,
    pub batch_size: usize,
    pub enable_performance_tracking: bool,
    pub enable_health_monitoring: bool,
    pub connection_pool_size: u32,
    pub query_timeout_seconds: u64,
    pub enable_analytics_aggregation: bool,
    pub aggregation_interval_minutes: u64,
}

impl Default for AnalyticsRepositoryConfig {
    fn default() -> Self {
        Self {
            enable_caching: true,
            cache_ttl_seconds: 900, // 15 minutes for analytics data
            enable_batch_operations: true,
            batch_size: 25, // Smaller batches for analytics
            enable_performance_tracking: true,
            enable_health_monitoring: true,
            connection_pool_size: 15, // Medium pool for analytics
            query_timeout_seconds: 30,
            enable_analytics_aggregation: true,
            aggregation_interval_minutes: 60, // Hourly aggregation
        }
    }
}

impl RepositoryConfig for AnalyticsRepositoryConfig {
    fn validate(&self) -> ArbitrageResult<()> {
        if self.cache_ttl_seconds == 0 {
            return Err(validation_error(
                "cache_ttl_seconds",
                "must be greater than 0",
            ));
        }
        if self.batch_size == 0 {
            return Err(validation_error("batch_size", "must be greater than 0"));
        }
        if self.connection_pool_size == 0 {
            return Err(validation_error(
                "connection_pool_size",
                "must be greater than 0",
            ));
        }
        if self.query_timeout_seconds == 0 {
            return Err(validation_error(
                "query_timeout_seconds",
                "must be greater than 0",
            ));
        }
        if self.aggregation_interval_minutes == 0 {
            return Err(validation_error(
                "aggregation_interval_minutes",
                "must be greater than 0",
            ));
        }
        Ok(())
    }

    fn connection_pool_size(&self) -> u32 {
        self.connection_pool_size
    }

    fn batch_size(&self) -> usize {
        self.batch_size
    }

    fn cache_ttl_seconds(&self) -> u64 {
        self.cache_ttl_seconds
    }
}

/// Analytics aggregation data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsAggregation {
    pub user_id: String,
    pub period: String, // daily, weekly, monthly
    pub total_trades: u32,
    pub successful_trades: u32,
    pub total_pnl: f64,
    pub average_pnl: f64,
    pub win_rate: f64,
    pub total_volume: f64,
    pub average_trade_size: f64,
    pub risk_score: f64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub last_updated: u64,
}

/// User analytics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAnalyticsSummary {
    pub user_id: String,
    pub total_trades: u32,
    pub total_pnl: f64,
    pub average_pnl: f64,
    pub win_rate: f64,
    pub average_trade_size: f64,
    pub best_trade_pnl: f64,
    pub worst_trade_pnl: f64,
    pub total_volume_traded: f64,
    pub risk_score: f64,
    pub last_trade_timestamp: Option<u64>,
    pub account_age_days: u32,
    pub last_updated: u64,
}

/// System analytics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemAnalyticsSummary {
    pub total_users: u32,
    pub active_users: u32,
    pub premium_users: u32,
    pub total_system_pnl: f64,
    pub total_system_trades: u32,
    pub average_user_pnl: f64,
    pub system_win_rate: f64,
    pub total_volume_traded: f64,
    pub last_updated: u64,
}

/// Analytics repository for trading analytics and performance data
pub struct AnalyticsRepository {
    db: Arc<D1Database>,
    config: AnalyticsRepositoryConfig,
    cache: Option<KvStore>,
    metrics: Arc<std::sync::Mutex<RepositoryMetrics>>,
    startup_time: u64,
}

impl AnalyticsRepository {
    /// Create new AnalyticsRepository
    pub fn new(db: Arc<D1Database>, config: AnalyticsRepositoryConfig) -> Self {
        let metrics = RepositoryMetrics {
            repository_name: "analytics_repository".to_string(),
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            avg_response_time_ms: 0.0,
            operations_per_second: 0.0,
            cache_hit_rate: 0.0,
            last_updated: current_timestamp_ms(),
        };

        Self {
            db,
            config,
            cache: None,
            metrics: Arc::new(std::sync::Mutex::new(metrics)),
            startup_time: current_timestamp_ms(),
        }
    }

    /// Set cache store
    pub fn with_cache(mut self, cache: KvStore) -> Self {
        self.cache = Some(cache);
        self
    }

    // ============= TRADING ANALYTICS OPERATIONS =============

    /// Store trading analytics data
    pub async fn store_trading_analytics(
        &self,
        analytics: &TradingAnalytics,
    ) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate analytics data
        self.validate_trading_analytics(analytics)?;

        let metric_data_json = serde_json::to_string(&analytics.metric_data)
            .map_err(|e| validation_error("metric_data", &format!("Failed to serialize: {}", e)))?;

        let analytics_metadata_json = serde_json::to_string(&analytics.analytics_metadata)
            .map_err(|e| {
                validation_error("analytics_metadata", &format!("Failed to serialize: {}", e))
            })?;

        let stmt = self.db.prepare(
            "INSERT INTO trading_analytics (
                analytics_id, user_id, metric_type, metric_value,
                metric_data, exchange_id, trading_pair, opportunity_type,
                timestamp, session_id, analytics_metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        let timestamp = analytics.timestamp.timestamp_millis();

        let result = stmt
            .bind(&[
                analytics.analytics_id.clone().into(),
                analytics.user_id.clone().into(),
                analytics.metric_type.clone().into(),
                analytics.metric_value.into(),
                metric_data_json.into(),
                analytics.exchange_id.clone().unwrap_or_default().into(),
                analytics.trading_pair.clone().unwrap_or_default().into(),
                analytics
                    .opportunity_type
                    .clone()
                    .unwrap_or_default()
                    .into(),
                timestamp.into(),
                analytics.session_id.clone().unwrap_or_default().into(),
                analytics_metadata_json.into(),
            ])
            .map_err(|e| {
                database_error(
                    "bind parameters",
                    format!("Failed to bind parameters: {}", e),
                )
            })?
            .run()
            .await
            .map_err(|e| {
                database_error("execute query", format!("Failed to execute query: {}", e))
            });

        // Invalidate cache for user analytics
        if let Some(ref cache) = self.cache {
            let cache_key = format!("analytics:user:{}", analytics.user_id);
            let _ = cache.delete(&cache_key).await; // Ignore cache errors
        }

        self.update_metrics(start_time, result.is_ok()).await;
        result.map(|_| ())
    }

    /// Get trading analytics for a user
    pub async fn get_trading_analytics(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> ArbitrageResult<Vec<TradingAnalytics>> {
        let start_time = current_timestamp_ms();

        // Validate input
        if user_id.is_empty() {
            return Err(validation_error("user_id", "cannot be empty"));
        }

        let limit = limit.unwrap_or(100).min(1000); // Cap at 1000 for performance

        // Try cache first
        if let Some(ref cache) = self.cache {
            let cache_key = format!("analytics:user:{}:limit:{}", user_id, limit);
            if let Ok(Some(cached_data)) = cache.get(&cache_key).text().await {
                if let Ok(analytics) = serde_json::from_str::<Vec<TradingAnalytics>>(&cached_data) {
                    self.update_metrics(start_time, true).await;
                    self.update_cache_hit_rate(true).await;
                    return Ok(analytics);
                }
            }
            self.update_cache_hit_rate(false).await;
        }

        let stmt = self.db.prepare(
            "SELECT * FROM trading_analytics 
            WHERE user_id = ? 
            ORDER BY timestamp DESC 
            LIMIT ?",
        );

        let result = stmt
            .bind(&[user_id.into(), limit.into()])
            .map_err(|e| {
                database_error(
                    "bind parameters",
                    format!("Failed to bind parameters: {}", e),
                )
            })?
            .all()
            .await
            .map_err(|e| {
                database_error("execute query", format!("Failed to execute query: {}", e))
            })?;

        let mut analytics = Vec::new();
        let results = result
            .results::<HashMap<String, serde_json::Value>>()
            .map_err(|e| {
                database_error("parse results", format!("Failed to parse results: {}", e))
            })?;

        for row in results {
            analytics.push(self.row_to_trading_analytics(row)?);
        }

        // Cache the results
        if let Some(ref cache) = self.cache {
            let cache_key = format!("analytics:user:{}:limit:{}", user_id, limit);
            let cache_data = serde_json::to_string(&analytics).unwrap_or_default();
            // persist with TTL, swallow failure intentionally
            let _ = cache
                .put(&cache_key, cache_data)?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await;
        }

        self.update_metrics(start_time, true).await;
        Ok(analytics)
    }

    /// Store multiple analytics in batch
    pub async fn store_analytics_batch(
        &self,
        analytics_list: &[TradingAnalytics],
    ) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        if analytics_list.is_empty() {
            return Err(validation_error("analytics_list", "cannot be empty"));
        }

        if analytics_list.len() > self.config.batch_size {
            return Err(validation_error(
                "analytics_list",
                &format!("exceeds batch size limit of {}", self.config.batch_size),
            ));
        }

        // Validate all analytics
        for analytics in analytics_list {
            self.validate_trading_analytics(analytics)?;
        }

        // Process in batches
        for chunk in analytics_list.chunks(self.config.batch_size) {
            for analytics in chunk {
                self.store_trading_analytics(analytics).await?;
            }
        }

        self.update_metrics(start_time, true).await;
        Ok(())
    }

    // ============= USER ANALYTICS OPERATIONS =============

    /// Get user analytics summary
    pub async fn get_user_analytics_summary(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserAnalyticsSummary> {
        let start_time = current_timestamp_ms();

        // Validate input
        if user_id.is_empty() {
            return Err(validation_error("user_id", "cannot be empty"));
        }

        // Try cache first
        if let Some(ref cache) = self.cache {
            let cache_key = format!("analytics:summary:user:{}", user_id);
            if let Ok(Some(cached_data)) = cache.get(&cache_key).text().await {
                if let Ok(summary) = serde_json::from_str::<UserAnalyticsSummary>(&cached_data) {
                    self.update_metrics(start_time, true).await;
                    self.update_cache_hit_rate(true).await;
                    return Ok(summary);
                }
            }
            self.update_cache_hit_rate(false).await;
        }

        // Query analytics data
        let analytics = self.get_trading_analytics(user_id, Some(1000)).await?;

        if analytics.is_empty() {
            return Ok(UserAnalyticsSummary {
                user_id: user_id.to_string(),
                total_trades: 0,
                total_pnl: 0.0,
                average_pnl: 0.0,
                win_rate: 0.0,
                average_trade_size: 0.0,
                best_trade_pnl: 0.0,
                worst_trade_pnl: 0.0,
                total_volume_traded: 0.0,
                risk_score: 0.0,
                last_trade_timestamp: None,
                account_age_days: 0,
                last_updated: current_timestamp_ms(),
            });
        }

        // Calculate summary metrics
        let total_trades = analytics.len() as u32;
        let profitable_trades = analytics
            .iter()
            .filter(|a| a.metric_type == "trade_executed" && a.metric_value > 0.0)
            .count() as u32;

        let win_rate = if total_trades > 0 {
            (profitable_trades as f64 / total_trades as f64) * 100.0
        } else {
            0.0
        };

        let pnl_values: Vec<f64> = analytics
            .iter()
            .filter(|a| a.metric_type == "profit_loss")
            .map(|a| a.metric_value)
            .collect();

        let total_pnl = pnl_values.iter().sum::<f64>();
        let average_pnl = if !pnl_values.is_empty() {
            total_pnl / pnl_values.len() as f64
        } else {
            0.0
        };

        let best_trade_pnl = pnl_values.iter().fold(0.0_f64, |a, &b| a.max(b));
        let worst_trade_pnl = pnl_values.iter().fold(0.0_f64, |a, &b| a.min(b));

        let trade_sizes: Vec<f64> = analytics
            .iter()
            .filter(|a| a.metric_type == "trade_executed")
            .map(|a| a.metric_value)
            .collect();

        let average_trade_size = if !trade_sizes.is_empty() {
            trade_sizes.iter().sum::<f64>() / trade_sizes.len() as f64
        } else {
            0.0
        };

        let total_volume_traded = trade_sizes.iter().sum::<f64>();

        let risk_score = analytics
            .iter()
            .filter(|a| a.metric_type == "risk_assessment")
            .map(|a| a.metric_value)
            .next_back()
            .unwrap_or(0.0);

        let last_trade_timestamp = analytics
            .iter()
            .filter(|a| a.metric_type == "trade_executed")
            .map(|a| a.timestamp.timestamp_millis() as u64)
            .max();

        // Calculate account age (simplified)
        let account_age_days = if let Some(oldest) = analytics.iter().map(|a| a.timestamp).min() {
            let age_ms = Utc::now().timestamp_millis() - oldest.timestamp_millis();
            (age_ms / (24 * 60 * 60 * 1000)) as u32
        } else {
            0
        };

        let summary = UserAnalyticsSummary {
            user_id: user_id.to_string(),
            total_trades,
            total_pnl,
            average_pnl,
            win_rate,
            average_trade_size,
            best_trade_pnl,
            worst_trade_pnl,
            total_volume_traded,
            risk_score,
            last_trade_timestamp,
            account_age_days,
            last_updated: current_timestamp_ms(),
        };

        // Cache the summary
        if let Some(ref cache) = self.cache {
            let cache_key = format!("analytics:summary:user:{}", user_id);
            let cache_data = serde_json::to_string(&summary).unwrap_or_default();
            let _ = cache.put(&cache_key, &cache_data);
        }

        self.update_metrics(start_time, true).await;
        Ok(summary)
    }

    // ============= SYSTEM ANALYTICS OPERATIONS =============

    /// Get system analytics summary
    pub async fn get_system_analytics_summary(&self) -> ArbitrageResult<SystemAnalyticsSummary> {
        let start_time = current_timestamp_ms();

        // Try cache first
        if let Some(ref cache) = self.cache {
            let cache_key = "analytics:summary:system";
            if let Ok(Some(cached_data)) = cache.get(cache_key).text().await {
                if let Ok(summary) = serde_json::from_str::<SystemAnalyticsSummary>(&cached_data) {
                    self.update_metrics(start_time, true).await;
                    self.update_cache_hit_rate(true).await;
                    return Ok(summary);
                }
            }
            self.update_cache_hit_rate(false).await;
        }

        // Query system analytics (WASM-specific implementation)
        #[cfg(target_arch = "wasm32")]
        {
            let query = "
                SELECT 
                    COUNT(DISTINCT up.user_id) as total_users,
                    COUNT(DISTINCT CASE WHEN up.is_active = 1 THEN up.user_id END) as active_users,
                    COUNT(DISTINCT CASE WHEN up.subscription_tier IN ('Premium', 'Enterprise', 'SuperAdmin') THEN up.user_id END) as premium_users,
                    COALESCE(SUM(ta.metric_value), 0.0) as total_system_pnl,
                    COUNT(DISTINCT ta.analytics_id) as total_system_trades
                FROM user_profiles up 
                LEFT JOIN trading_analytics ta ON up.user_id = ta.user_id AND ta.metric_type = 'profit_loss'
            ";

            let stmt = self.db.prepare(query);
            let result = stmt
                .first::<HashMap<String, serde_json::Value>>(None)
                .await
                .map_err(|e| {
                    database_error(&format!("Failed to execute system analytics query: {}", e))
                })?;

            if let Some(row) = result {
                let total_users =
                    row.get("total_users").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                let active_users = row
                    .get("active_users")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                let premium_users = row
                    .get("premium_users")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                let total_system_pnl = row
                    .get("total_system_pnl")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                let total_system_trades = row
                    .get("total_system_trades")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                let average_user_pnl = if active_users > 0 {
                    total_system_pnl / active_users as f64
                } else {
                    0.0
                };

                // Calculate system win rate (simplified)
                let system_win_rate = if total_system_trades > 0 {
                    // This would need a more complex query in practice
                    65.0 // Placeholder
                } else {
                    0.0
                };

                let summary = SystemAnalyticsSummary {
                    total_users,
                    active_users,
                    premium_users,
                    total_system_pnl,
                    total_system_trades,
                    average_user_pnl,
                    system_win_rate,
                    total_volume_traded: 0.0, // Would need additional query
                    last_updated: current_timestamp_ms(),
                };

                // Cache the summary
                if let Some(ref cache) = self.cache {
                    let cache_key = "analytics:summary:system";
                    let cache_data = serde_json::to_string(&summary).unwrap_or_default();
                    let _ = cache.put(cache_key, &cache_data);
                }

                self.update_metrics(start_time, true).await;
                return Ok(summary);
            }
        }

        // Fallback for non-WASM or if query fails
        let summary = SystemAnalyticsSummary {
            total_users: 0,
            active_users: 0,
            premium_users: 0,
            total_system_pnl: 0.0,
            total_system_trades: 0,
            average_user_pnl: 0.0,
            system_win_rate: 0.0,
            total_volume_traded: 0.0,
            last_updated: current_timestamp_ms(),
        };

        self.update_metrics(start_time, true).await;
        Ok(summary)
    }

    // ============= ANALYTICS AGGREGATION OPERATIONS =============

    /// Create analytics aggregation for a user
    pub async fn create_analytics_aggregation(
        &self,
        user_id: &str,
        period: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ArbitrageResult<AnalyticsAggregation> {
        let start = current_timestamp_ms();

        // Validate inputs
        if user_id.is_empty() {
            return Err(validation_error("user_id", "cannot be empty"));
        }
        if !["daily", "weekly", "monthly"].contains(&period) {
            return Err(validation_error(
                "period",
                "must be daily, weekly, or monthly",
            ));
        }

        // Get analytics for the period
        let analytics = self.get_trading_analytics(user_id, Some(10000)).await?;

        // Filter by time period
        let period_analytics: Vec<_> = analytics
            .into_iter()
            .filter(|a| a.timestamp >= start_time && a.timestamp <= end_time)
            .collect();

        // Calculate aggregation metrics
        let total_trades = period_analytics.len() as u32;
        let successful_trades = period_analytics
            .iter()
            .filter(|a| a.metric_type == "trade_executed" && a.metric_value > 0.0)
            .count() as u32;

        let win_rate = if total_trades > 0 {
            (successful_trades as f64 / total_trades as f64) * 100.0
        } else {
            0.0
        };

        let pnl_values: Vec<f64> = period_analytics
            .iter()
            .filter(|a| a.metric_type == "profit_loss")
            .map(|a| a.metric_value)
            .collect();

        let total_pnl = pnl_values.iter().sum::<f64>();
        let average_pnl = if !pnl_values.is_empty() {
            total_pnl / pnl_values.len() as f64
        } else {
            0.0
        };

        let trade_sizes: Vec<f64> = period_analytics
            .iter()
            .filter(|a| a.metric_type == "trade_executed")
            .map(|a| a.metric_value)
            .collect();

        let total_volume = trade_sizes.iter().sum::<f64>();
        let average_trade_size = if !trade_sizes.is_empty() {
            total_volume / trade_sizes.len() as f64
        } else {
            0.0
        };

        let risk_score = period_analytics
            .iter()
            .filter(|a| a.metric_type == "risk_assessment")
            .map(|a| a.metric_value)
            .next_back()
            .unwrap_or(0.0);

        let aggregation = AnalyticsAggregation {
            user_id: user_id.to_string(),
            period: period.to_string(),
            total_trades,
            successful_trades,
            total_pnl,
            average_pnl,
            win_rate,
            total_volume,
            average_trade_size,
            risk_score,
            period_start: start_time,
            period_end: end_time,
            last_updated: current_timestamp_ms(),
        };

        self.update_metrics(start, true).await;
        Ok(aggregation)
    }

    // ============= HELPER METHODS =============

    /// Validate trading analytics data
    fn validate_trading_analytics(&self, analytics: &TradingAnalytics) -> ArbitrageResult<()> {
        if analytics.analytics_id.is_empty() {
            return Err(validation_error("analytics_id", "cannot be empty"));
        }
        if analytics.user_id.is_empty() {
            return Err(validation_error("user_id", "cannot be empty"));
        }
        if analytics.metric_type.is_empty() {
            return Err(validation_error("metric_type", "cannot be empty"));
        }
        if analytics.analytics_id.len() > 255 {
            return Err(validation_error(
                "analytics_id",
                "exceeds maximum length of 255",
            ));
        }
        if analytics.user_id.len() > 255 {
            return Err(validation_error("user_id", "exceeds maximum length of 255"));
        }
        if analytics.metric_type.len() > 100 {
            return Err(validation_error(
                "metric_type",
                "exceeds maximum length of 100",
            ));
        }
        Ok(())
    }

    /// Convert database row to TradingAnalytics
    fn row_to_trading_analytics(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<TradingAnalytics> {
        let analytics_id = get_string_field(&row, "analytics_id")?;
        let user_id = get_string_field(&row, "user_id")?;
        let metric_type = get_string_field(&row, "metric_type")?;
        let metric_value = get_f64_field(&row, "metric_value", 0.0);

        let metric_data: serde_json::Value = row
            .get("metric_data")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::json!({}));

        let analytics_metadata: serde_json::Value = row
            .get("analytics_metadata")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::json!({}));

        let exchange_id = row
            .get("exchange_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let trading_pair = row
            .get("trading_pair")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let opportunity_type = row
            .get("opportunity_type")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let session_id = row
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let timestamp = row
            .get("timestamp")
            .and_then(|v| v.as_i64())
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        Ok(TradingAnalytics {
            user_id: user_id.clone(),
            total_trades: 0, // These would be calculated from aggregation
            successful_trades: 0,
            total_pnl_usdt: 0.0,
            best_trade_pnl: 0.0,
            worst_trade_pnl: 0.0,
            average_trade_size: 0.0,
            total_volume_traded: 0.0,
            win_rate_percentage: 0.0,
            average_holding_time_hours: 0.0,
            risk_score: 0.0,
            last_updated: current_timestamp_ms(),
            analytics_id,
            metric_type,
            metric_value,
            metric_data,
            exchange_id,
            trading_pair,
            opportunity_type,
            timestamp,
            session_id,
            analytics_metadata,
        })
    }

    async fn update_metrics(&self, start_time: u64, success: bool) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_operations += 1;

            if success {
                metrics.successful_operations += 1;
            } else {
                metrics.failed_operations += 1;
            }

            let response_time = current_timestamp_ms() - start_time;
            let total_time = metrics.avg_response_time_ms * (metrics.total_operations - 1) as f64
                + response_time as f64;
            metrics.avg_response_time_ms = total_time / metrics.total_operations as f64;

            metrics.last_updated = current_timestamp_ms();
        }
    }

    async fn update_cache_hit_rate(&self, hit: bool) {
        if let Ok(mut metrics) = self.metrics.lock() {
            // Capture current values before updating
            let current_total_operations = metrics.total_operations;
            let current_hits = metrics.cache_hit_rate * current_total_operations as f64;

            // Calculate new hit rate with the captured total operations
            let new_hits = if hit {
                current_hits + 1.0
            } else {
                current_hits
            };

            // Update total operations first
            metrics.total_operations += 1;

            // Then calculate hit rate using the new total
            metrics.cache_hit_rate = new_hits / metrics.total_operations as f64;
        }
    }
}

impl Repository for AnalyticsRepository {
    fn name(&self) -> &str {
        "analytics_repository"
    }

    async fn health_check(&self) -> ArbitrageResult<RepositoryHealth> {
        let start_time = current_timestamp_ms();

        // Test basic database connectivity with analytics table
        let test_result = self
            .db
            .prepare("SELECT COUNT(*) as count FROM trading_analytics LIMIT 1")
            .first::<HashMap<String, serde_json::Value>>(None)
            .await;

        let is_healthy = test_result.is_ok();
        let response_time = current_timestamp_ms() - start_time;

        let metrics = self.metrics.lock().unwrap();
        let success_rate = if metrics.total_operations > 0 {
            metrics.successful_operations as f64 / metrics.total_operations as f64
        } else {
            1.0
        };

        Ok(RepositoryHealth {
            repository_name: self.name().to_string(),
            is_healthy,
            database_healthy: is_healthy,
            cache_healthy: true,
            last_health_check: current_timestamp_ms(),
            response_time_ms: response_time as f64,
            error_rate: if is_healthy {
                (1.0 - success_rate) * 100.0
            } else {
                100.0
            },
        })
    }

    async fn get_metrics(&self) -> RepositoryMetrics {
        self.metrics.lock().unwrap().clone()
    }

    async fn initialize(&self) -> ArbitrageResult<()> {
        // Validate configuration
        self.config.validate()?;

        // Test database connectivity
        self.health_check().await?;

        Ok(())
    }

    async fn shutdown(&self) -> ArbitrageResult<()> {
        // Analytics repository doesn't need special shutdown procedures
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_analytics_repository_config_validation() {
        let mut config = AnalyticsRepositoryConfig::default();
        assert!(config.validate().is_ok());

        config.cache_ttl_seconds = 0;
        assert!(config.validate().is_err());

        config.cache_ttl_seconds = 900;
        config.batch_size = 0;
        assert!(config.validate().is_err());

        config.batch_size = 25;
        config.connection_pool_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_trading_analytics_validation() {
        let config = AnalyticsRepositoryConfig::default();
        let db = Arc::new(unsafe { std::mem::zeroed() }); // Mock for testing
        let repo = AnalyticsRepository::new(db, config);

        let mut analytics = TradingAnalytics {
            analytics_id: "test_id".to_string(),
            user_id: "test_user".to_string(),
            metric_type: "test_metric".to_string(),
            metric_value: 100.0,
            metric_data: serde_json::json!({}),
            exchange_id: None,
            trading_pair: None,
            opportunity_type: None,
            timestamp: Utc::now(),
            session_id: None,
            analytics_metadata: serde_json::json!({}),
            total_trades: 0,
            successful_trades: 0,
            total_pnl_usdt: 0.0,
            best_trade_pnl: 0.0,
            worst_trade_pnl: 0.0,
            average_trade_size: 0.0,
            total_volume_traded: 0.0,
            win_rate_percentage: 0.0,
            average_holding_time_hours: 0.0,
            risk_score: 0.0,
            last_updated: 0,
        };

        assert!(repo.validate_trading_analytics(&analytics).is_ok());

        analytics.analytics_id = "".to_string();
        assert!(repo.validate_trading_analytics(&analytics).is_err());

        analytics.analytics_id = "test_id".to_string();
        analytics.user_id = "".to_string();
        assert!(repo.validate_trading_analytics(&analytics).is_err());

        analytics.user_id = "test_user".to_string();
        analytics.metric_type = "".to_string();
        assert!(repo.validate_trading_analytics(&analytics).is_err());
    }

    #[test]
    fn test_user_analytics_summary_creation() {
        let summary = UserAnalyticsSummary {
            user_id: "test_user".to_string(),
            total_trades: 100,
            total_pnl: 1500.0,
            average_pnl: 15.0,
            win_rate: 65.0,
            average_trade_size: 1000.0,
            best_trade_pnl: 250.0,
            worst_trade_pnl: -50.0,
            total_volume_traded: 100000.0,
            risk_score: 0.3,
            last_trade_timestamp: Some(current_timestamp_ms()),
            account_age_days: 30,
            last_updated: current_timestamp_ms(),
        };

        assert_eq!(summary.user_id, "test_user");
        assert_eq!(summary.total_trades, 100);
        assert_eq!(summary.win_rate, 65.0);
    }

    #[test]
    fn test_analytics_aggregation_creation() {
        let start_time = Utc::now() - chrono::Duration::days(7);
        let end_time = Utc::now();

        let aggregation = AnalyticsAggregation {
            user_id: "test_user".to_string(),
            period: "weekly".to_string(),
            total_trades: 50,
            successful_trades: 35,
            total_pnl: 750.0,
            average_pnl: 15.0,
            win_rate: 70.0,
            total_volume: 50000.0,
            average_trade_size: 1000.0,
            risk_score: 0.25,
            period_start: start_time,
            period_end: end_time,
            last_updated: current_timestamp_ms(),
        };

        assert_eq!(aggregation.period, "weekly");
        assert_eq!(aggregation.total_trades, 50);
        assert_eq!(aggregation.win_rate, 70.0);
    }
}
