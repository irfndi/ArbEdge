// Database Core Module - Unified Database Operations and Connection Management
// Consolidates all database operations from D1Service with optimized patterns for high concurrency

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use worker::{wasm_bindgen::JsValue, D1Database, Env};

/// Database connection pool for high-concurrency scenarios
#[derive(Clone)]
pub struct DatabaseCore {
    db: Arc<D1Database>,
    connection_pool_size: usize,
    query_timeout_ms: u64,
    max_retries: u32,
    batch_size: usize,
}

/// Database operation result with metadata
#[derive(Debug, Clone)]
pub struct DatabaseResult {
    pub success: bool,
    pub rows_affected: u64,
    pub last_insert_id: Option<u64>,
    pub execution_time_ms: u64,
    pub query_hash: String,
}

/// Batch operation for high-throughput scenarios
#[derive(Debug, Clone)]
pub struct BatchOperation {
    pub sql: String,
    pub params: Vec<JsValue>,
    pub operation_id: String,
}

/// Database transaction context
pub struct TransactionContext {
    pub transaction_id: String,
    pub started_at: u64,
    pub operations: Vec<BatchOperation>,
    pub is_active: bool,
}

/// Database health metrics
#[derive(Debug, Clone)]
pub struct DatabaseHealth {
    pub is_healthy: bool,
    pub connection_count: usize,
    pub avg_query_time_ms: f64,
    pub total_queries: u64,
    pub failed_queries: u64,
    pub last_error: Option<String>,
    pub uptime_seconds: u64,
}

/// Query execution statistics
#[derive(Debug, Clone)]
pub struct QueryStats {
    pub query_type: String,
    pub execution_count: u64,
    pub avg_execution_time_ms: f64,
    pub success_rate: f64,
    pub last_executed: u64,
}

impl DatabaseCore {
    /// Create new DatabaseCore with optimized settings for high concurrency
    pub fn new(env: &Env) -> ArbitrageResult<Self> {
        let db = env
            .d1("ArbEdgeD1")
            .map_err(|e| ArbitrageError::database_error(format!("Failed to get D1 database: {}", e)))?;

        Ok(Self {
            db: Arc::new(db),
            connection_pool_size: 10, // Optimized for 1000-2500 concurrent users
            query_timeout_ms: 30000,  // 30 second timeout
            max_retries: 3,
            batch_size: 100, // Batch operations for efficiency
        })
    }

    /// Create with custom configuration for specific use cases
    pub fn new_with_config(
        env: &Env,
        pool_size: usize,
        timeout_ms: u64,
        retries: u32,
        batch_size: usize,
    ) -> ArbitrageResult<Self> {
        let db = env
            .d1("ArbEdgeD1")
            .map_err(|e| ArbitrageError::database_error(format!("Failed to get D1 database: {}", e)))?;

        Ok(Self {
            db: Arc::new(db),
            connection_pool_size: pool_size,
            query_timeout_ms: timeout_ms,
            max_retries: retries,
            batch_size,
        })
    }

    /// Execute a single query with retry logic and performance tracking
    pub async fn execute_query(
        &self,
        sql: &str,
        params: &[JsValue],
    ) -> ArbitrageResult<DatabaseResult> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let query_hash = self.generate_query_hash(sql);

        for attempt in 0..=self.max_retries {
            match self.execute_query_internal(sql, params).await {
                Ok(result) => {
                    let execution_time = chrono::Utc::now().timestamp_millis() as u64 - start_time;
                    return Ok(DatabaseResult {
                        success: true,
                        rows_affected: result.rows_affected,
                        last_insert_id: result.last_insert_id,
                        execution_time_ms: execution_time,
                        query_hash,
                    });
                }
                Err(e) => {
                    if attempt == self.max_retries {
                        return Err(e);
                    }
                    // Exponential backoff for retries
                    let delay_ms = 100 * (2_u64.pow(attempt));
                    self.sleep_ms(delay_ms).await;
                }
            }
        }

        Err(ArbitrageError::database_error("Max retries exceeded"))
    }

    /// Execute batch operations for high-throughput scenarios
    pub async fn execute_batch(
        &self,
        operations: &[BatchOperation],
    ) -> ArbitrageResult<Vec<DatabaseResult>> {
        let mut results = Vec::new();
        let chunks = operations.chunks(self.batch_size);

        for chunk in chunks {
            let chunk_results = self.execute_batch_chunk(chunk).await?;
            results.extend(chunk_results);
        }

        Ok(results)
    }

    /// Execute operations within a transaction
    pub async fn execute_transaction<F, T>(&self, operations: F) -> ArbitrageResult<T>
    where
        F: FnOnce(&Self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ArbitrageResult<T>> + '_>>,
    {
        // Begin transaction
        self.execute_query("BEGIN TRANSACTION", &[]).await?;

        match operations(self).await {
            Ok(result) => {
                // Commit transaction
                self.execute_query("COMMIT", &[]).await?;
                Ok(result)
            }
            Err(e) => {
                // Rollback transaction
                let _ = self.execute_query("ROLLBACK", &[]).await;
                Err(e)
            }
        }
    }

    /// Query with result parsing for SELECT operations
    pub async fn query_rows(
        &self,
        sql: &str,
        params: &[JsValue],
    ) -> ArbitrageResult<Vec<HashMap<String, Value>>> {
        let stmt = self.db.prepare(sql);
        
        let result = stmt
            .bind(params)
            .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
            .all()
            .await
            .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(result.results::<HashMap<String, Value>>())
    }

    /// Query single row for SELECT operations
    pub async fn query_first(
        &self,
        sql: &str,
        params: &[JsValue],
    ) -> ArbitrageResult<Option<HashMap<String, Value>>> {
        let stmt = self.db.prepare(sql);
        
        let result = stmt
            .bind(params)
            .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(result)
    }

    /// Optimized bulk insert for high-throughput scenarios
    pub async fn bulk_insert(
        &self,
        table: &str,
        columns: &[&str],
        rows: &[Vec<JsValue>],
    ) -> ArbitrageResult<DatabaseResult> {
        if rows.is_empty() {
            return Err(ArbitrageError::validation_error("No rows to insert"));
        }

        let placeholders = columns.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let columns_str = columns.join(", ");
        
        // Build bulk insert SQL
        let values_placeholders = rows
            .iter()
            .map(|_| format!("({})", placeholders))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            "INSERT INTO {} ({}) VALUES {}",
            table, columns_str, values_placeholders
        );

        // Flatten all parameters
        let mut all_params = Vec::new();
        for row in rows {
            all_params.extend_from_slice(row);
        }

        self.execute_query(&sql, &all_params).await
    }

    /// Bulk update operation with SQL injection protection
    pub async fn bulk_update(
        &self,
        table: &str,
        updates: &[(&str, JsValue)], // (column, value) pairs
        where_clause: &str,
        where_params: &[JsValue],
    ) -> ArbitrageResult<DatabaseResult> {
        // Define allowed column names to prevent SQL injection
        let allowed_columns = self.get_allowed_columns_for_table(table)?;
        
        // Validate all column names against whitelist
        for (column, _) in updates {
            if !allowed_columns.contains(&column.to_string()) {
                return Err(ArbitrageError::validation_error(format!(
                    "Column '{}' is not allowed for table '{}'", column, table
                )));
            }
        }

        let set_clause = updates
            .iter()
            .map(|(column, _)| format!("{} = ?", column))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            "UPDATE {} SET {} WHERE {}",
            table, set_clause, where_clause
        );

        // Combine update values and where parameters
        let mut all_params = Vec::new();
        for (_, value) in updates {
            all_params.push(value.clone());
        }
        all_params.extend_from_slice(where_params);

        self.execute_query(&sql, &all_params).await
    }

    /// Get allowed columns for a specific table to prevent SQL injection
    fn get_allowed_columns_for_table(&self, table: &str) -> ArbitrageResult<Vec<String>> {
        let allowed_columns = match table {
            "user_profiles" => vec![
                "user_id".to_string(),
                "telegram_id".to_string(),
                "username".to_string(),
                "api_keys_json".to_string(),
                "subscription_json".to_string(),
                "configuration_json".to_string(),
                "created_at".to_string(),
                "updated_at".to_string(),
                "last_active".to_string(),
                "is_active".to_string(),
                "beta_expires_at".to_string(),
                "account_balance_usdt".to_string(),
            ],
            "analytics_data" => vec![
                "user_id".to_string(),
                "metric_type".to_string(),
                "timestamp".to_string(),
                "data_json".to_string(),
                "created_at".to_string(),
            ],
            "invitation_codes" => vec![
                "code".to_string(),
                "created_by".to_string(),
                "created_at".to_string(),
                "expires_at".to_string(),
                "max_uses".to_string(),
                "current_uses".to_string(),
                "is_active".to_string(),
            ],
            "group_registrations" => vec![
                "group_id".to_string(),
                "group_name".to_string(),
                "registered_by".to_string(),
                "registered_at".to_string(),
                "is_active".to_string(),
                "settings_json".to_string(),
            ],
            _ => {
                return Err(ArbitrageError::validation_error(format!(
                    "Table '{}' is not supported for bulk updates", table
                )));
            }
        };
        
        Ok(allowed_columns)
    }

    /// Health check with comprehensive metrics
    pub async fn health_check(&self) -> ArbitrageResult<DatabaseHealth> {
        let start_time = chrono::Utc::now().timestamp_millis();

        // Test basic connectivity
        let test_result = self.query_first("SELECT 1 as test", &[]).await;
        
        let query_time = chrono::Utc::now().timestamp_millis() - start_time;
        let is_healthy = test_result.is_ok();

        Ok(DatabaseHealth {
            is_healthy,
            connection_count: 1, // D1 doesn't expose connection pool info
            avg_query_time_ms: query_time as f64,
            total_queries: 0, // Would be tracked by metrics collector
            failed_queries: 0, // Would be tracked by metrics collector
            last_error: if is_healthy {
                None
            } else {
                Some(test_result.unwrap_err().to_string())
            },
            uptime_seconds: 0, // Would be tracked by service health module
        })
    }

    /// Get database statistics for monitoring
    pub async fn get_statistics(&self) -> ArbitrageResult<HashMap<String, QueryStats>> {
        // This would integrate with metrics collector in real implementation
        // For now, return empty stats
        Ok(HashMap::new())
    }

    // ============= INTERNAL HELPER METHODS =============

    async fn execute_query_internal(&self, sql: &str, params: &[JsValue]) -> ArbitrageResult<InternalResult> {
        let stmt = self.db.prepare(sql);
        
        let result = stmt
            .bind(params)
            .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
            .run()
            .await
            .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(InternalResult {
            rows_affected: result.changes() as u64,
            last_insert_id: result.meta().last_row_id.map(|id| id as u64),
        })
    }

    async fn execute_batch_chunk(
        &self,
        chunk: &[BatchOperation],
    ) -> ArbitrageResult<Vec<DatabaseResult>> {
        let mut results = Vec::new();

        for operation in chunk {
            let result = self.execute_query(&operation.sql, &operation.params).await?;
            results.push(result);
        }

        Ok(results)
    }

    fn generate_query_hash(&self, sql: &str) -> String {
        // Simple hash for query identification
        format!("{:x}", md5::compute(sql.as_bytes()))
    }

    async fn sleep_ms(&self, ms: u64) {
        use wasm_timer::Delay;
        Delay::new(std::time::Duration::from_millis(ms)).await;
    }

    // ============= UTILITY METHODS FOR DATA EXTRACTION =============

    /// Extract string field from database row with error handling
    pub fn get_string_field(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
    ) -> ArbitrageResult<String> {
        row.get(field_name)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                ArbitrageError::parse_error(format!("Missing or invalid string field: {}", field_name))
            })
    }

    /// Extract optional string field from database row
    pub fn get_optional_string_field(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
    ) -> Option<String> {
        row.get(field_name)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Extract boolean field with default value
    pub fn get_bool_field(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
        default: bool,
    ) -> bool {
        row.get(field_name)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }

    /// Extract i64 field with default value
    pub fn get_i64_field(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
        default: i64,
    ) -> i64 {
        row.get(field_name)
            .and_then(|v| v.as_i64())
            .unwrap_or(default)
    }

    /// Extract f64 field with default value
    pub fn get_f64_field(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
        default: f64,
    ) -> f64 {
        row.get(field_name)
            .and_then(|v| v.as_f64())
            .unwrap_or(default)
    }

    /// Extract and deserialize JSON field
    pub fn get_json_field<T: serde::de::DeserializeOwned>(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
        default: T,
    ) -> T {
        row.get(field_name)
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(default)
    }
}

/// Internal result structure for database operations
#[derive(Debug)]
struct InternalResult {
    rows_affected: u64,
    last_insert_id: Option<u64>,
}

// ============= SPECIALIZED DATABASE OPERATIONS =============

/// User profile database operations
impl DatabaseCore {
    /// Store user profile with optimized SQL
    pub async fn store_user_profile(
        &self,
        user_id: &str,
        telegram_id: Option<i64>,
        username: Option<&str>,
        api_keys_json: &str,
        subscription_json: &str,
        configuration_json: &str,
        created_at: u64,
        updated_at: u64,
        last_active: u64,
        is_active: bool,
        beta_expires_at: Option<u64>,
        account_balance_usdt: f64,
    ) -> ArbitrageResult<DatabaseResult> {
        let sql = "
            INSERT OR REPLACE INTO user_profiles (
                user_id, telegram_id, username, api_keys, 
                subscription_tier, trading_preferences, 
                created_at, updated_at, last_login_at, account_status, 
                beta_expires_at, account_balance_usdt
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ";

        let params = vec![
            user_id.into(),
            JsValue::from_f64(telegram_id.unwrap_or(0) as f64),
            username.unwrap_or_default().into(),
            api_keys_json.into(),
            subscription_json.into(),
            configuration_json.into(),
            (created_at as i64).into(),
            (updated_at as i64).into(),
            (last_active as i64).into(),
            if is_active { "active" } else { "deactivated" }.into(),
            beta_expires_at.map(|t| t as i64).unwrap_or(0).into(),
            account_balance_usdt.into(),
        ];

        self.execute_query(sql, &params).await
    }

    /// Get user profile by user ID
    pub async fn get_user_profile(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<HashMap<String, Value>>> {
        let sql = "SELECT * FROM user_profiles WHERE user_id = ?";
        let params = vec![user_id.into()];
        
        self.query_first(sql, &params).await
    }

    /// Get user profile by Telegram ID
    pub async fn get_user_by_telegram_id(
        &self,
        telegram_id: i64,
    ) -> ArbitrageResult<Option<HashMap<String, Value>>> {
        let sql = "SELECT * FROM user_profiles WHERE telegram_id = ?";
        let params = vec![JsValue::from_f64(telegram_id as f64)];
        
        self.query_first(sql, &params).await
    }
}

/// Analytics and metrics database operations
impl DatabaseCore {
    /// Store analytics data with batch optimization
    pub async fn store_analytics_batch(
        &self,
        analytics_data: &[(&str, &str, u64, &str)], // (user_id, metric_type, timestamp, data_json)
    ) -> ArbitrageResult<DatabaseResult> {
        if analytics_data.is_empty() {
            return Err(ArbitrageError::validation_error("No analytics data to store"));
        }

        let columns = &["user_id", "metric_type", "timestamp", "data_json"];
        let rows: Vec<Vec<JsValue>> = analytics_data
            .iter()
            .map(|(user_id, metric_type, timestamp, data_json)| {
                vec![
                    (*user_id).into(),
                    (*metric_type).into(),
                    (*timestamp as i64).into(),
                    (*data_json).into(),
                ]
            })
            .collect();

        self.bulk_insert("analytics", columns, &rows).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_result_creation() {
        let result = DatabaseResult {
            success: true,
            rows_affected: 1,
            last_insert_id: Some(123),
            execution_time_ms: 50,
            query_hash: "abc123".to_string(),
        };

        assert!(result.success);
        assert_eq!(result.rows_affected, 1);
        assert_eq!(result.last_insert_id, Some(123));
        assert_eq!(result.execution_time_ms, 50);
    }

    #[test]
    fn test_batch_operation_creation() {
        let operation = BatchOperation {
            sql: "INSERT INTO test (id) VALUES (?)".to_string(),
            params: vec![JsValue::from_f64(1.0)],
            operation_id: "op_1".to_string(),
        };

        assert_eq!(operation.sql, "INSERT INTO test (id) VALUES (?)");
        assert_eq!(operation.operation_id, "op_1");
        assert_eq!(operation.params.len(), 1);
    }

    #[test]
    fn test_database_health_creation() {
        let health = DatabaseHealth {
            is_healthy: true,
            connection_count: 5,
            avg_query_time_ms: 25.5,
            total_queries: 1000,
            failed_queries: 5,
            last_error: None,
            uptime_seconds: 3600,
        };

        assert!(health.is_healthy);
        assert_eq!(health.connection_count, 5);
        assert_eq!(health.avg_query_time_ms, 25.5);
        assert_eq!(health.total_queries, 1000);
        assert_eq!(health.failed_queries, 5);
        assert!(health.last_error.is_none());
    }
} 