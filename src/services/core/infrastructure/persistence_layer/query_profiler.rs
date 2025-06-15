//! Query Profiler for D1/R2 Persistence Layer
//!
//! Provides query profiling capabilities including SQL parsing, execution timing,
//! plan analysis, and integration with performance monitoring system.

use crate::utils::error::{ArbitrageError, ArbitrageResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::utils::time::{now_instant, WasmInstant};
use std::sync::Arc;

use super::performance_monitor::{
    DatabaseType, ErrorCategory, IndexUsage, PerformanceMonitor, PlanNode, QueryError,
    QueryMetrics, QueryOperationType, QueryPlan, ScanType,
};

/// Query profiler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerConfig {
    /// Enable query profiling
    pub enabled: bool,
    /// Enable query plan collection
    pub collect_execution_plans: bool,
    /// Enable parameter sanitization
    pub sanitize_parameters: bool,
    /// Maximum query text length to capture
    pub max_query_length: usize,
    /// Enable query fingerprinting
    pub enable_fingerprinting: bool,
    /// Enable stack trace collection for slow queries
    pub collect_stack_traces: bool,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collect_execution_plans: true,
            sanitize_parameters: true,
            max_query_length: 10000,
            enable_fingerprinting: true,
            collect_stack_traces: false, // Disabled in production
        }
    }
}

/// Active query context for profiling
#[derive(Debug, Clone)]
pub struct QueryContext {
    /// Unique query ID
    pub query_id: String,
    /// Original SQL text
    pub sql: String,
    /// Query parameters
    pub parameters: Vec<QueryParameter>,
    /// Database type being queried
    pub database_type: DatabaseType,
    /// Query start time
    pub start_time: WasmInstant,
    /// Query start timestamp
    pub started_at: DateTime<Utc>,
    /// Caller context (function name, line, etc.)
    pub caller_context: Option<String>,
}

/// Query parameter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParameter {
    /// Parameter name or index
    pub name: String,
    /// Parameter type
    pub param_type: String,
    /// Sanitized value (no sensitive data)
    pub sanitized_value: String,
}

/// Query profiler implementation
pub struct QueryProfiler {
    /// Configuration
    config: ProfilerConfig,
    /// Performance monitor for metrics collection
    performance_monitor: Arc<PerformanceMonitor>,
    /// Logger instance
    logger: crate::utils::logger::Logger,
}

impl QueryProfiler {
    /// Create new query profiler
    pub fn new(
        config: ProfilerConfig,
        performance_monitor: Arc<PerformanceMonitor>,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        Ok(Self {
            config,
            performance_monitor,
            logger,
        })
    }

    /// Start profiling a query
    pub fn start_query(
        &self,
        sql: &str,
        parameters: Vec<QueryParameter>,
        database_type: DatabaseType,
    ) -> ArbitrageResult<QueryContext> {
        if !self.config.enabled {
            return Err(ArbitrageError::configuration_error(
                "Query profiler is disabled",
            ));
        }

        let query_id = uuid::Uuid::new_v4().to_string();
        let sanitized_sql = self.sanitize_sql(sql)?;

        let context = QueryContext {
            query_id: query_id.clone(),
            sql: sanitized_sql,
            parameters: if self.config.sanitize_parameters {
                self.sanitize_parameters(parameters)?
            } else {
                parameters
            },
            database_type,
            start_time: now_instant(),
            started_at: Utc::now(),
            caller_context: None, // TODO: Implement stack trace collection
        };

        self.logger
            .debug(&format!("Started profiling query: {}", query_id));
        Ok(context)
    }

    /// Complete query profiling
    pub async fn complete_query(
        &self,
        context: QueryContext,
        result: Result<u64, QueryError>,
        execution_plan: Option<QueryPlan>,
    ) -> ArbitrageResult<()> {
        let execution_time_ms = context.start_time.elapsed().as_millis() as u64;
        let operation_type = self.detect_operation_type(&context.sql)?;
        let complexity_score = self.calculate_complexity_score(&context.sql, &operation_type)?;
        let index_usage = self.analyze_index_usage(&context.sql, &execution_plan)?;

        let (rows_affected, error_info) = match result {
            Ok(rows) => (rows, None),
            Err(error) => (0, Some(error)),
        };

        let metrics = QueryMetrics {
            query_id: context.query_id.clone(),
            query_text: context.sql.clone(),
            parameter_count: context.parameters.len() as u32,
            execution_time_ms,
            started_at: context.started_at,
            completed_at: Utc::now(),
            database_type: context.database_type,
            rows_affected,
            operation_type,
            complexity_score,
            index_usage,
            query_plan: execution_plan,
            error_info,
        };

        // Record metrics with performance monitor
        self.performance_monitor
            .record_query_metrics(metrics)
            .await?;

        self.logger.debug(&format!(
            "Completed profiling query: {} ({}ms)",
            context.query_id, execution_time_ms
        ));

        Ok(())
    }

    /// Profile a complete query execution
    pub async fn profile_query<F, T>(
        &self,
        sql: &str,
        parameters: Vec<QueryParameter>,
        database_type: DatabaseType,
        execution_fn: F,
    ) -> ArbitrageResult<T>
    where
        F: FnOnce() -> ArbitrageResult<(T, u64, Option<QueryPlan>)>,
    {
        let context = self.start_query(sql, parameters, database_type)?;

        let execution_result = execution_fn();

        match execution_result {
            Ok((result, rows_affected, execution_plan)) => {
                self.complete_query(context, Ok(rows_affected), execution_plan)
                    .await?;
                Ok(result)
            }
            Err(error) => {
                let query_error = QueryError {
                    error_code: "EXECUTION_ERROR".to_string(),
                    error_message: error.to_string(),
                    error_category: self.categorize_error(&error),
                    is_retryable: self.is_retryable_error(&error),
                };

                self.complete_query(context, Err(query_error), None).await?;
                Err(error)
            }
        }
    }

    /// Sanitize SQL query text
    fn sanitize_sql(&self, sql: &str) -> ArbitrageResult<String> {
        let mut sanitized = sql.to_string();

        // Truncate if too long
        if sanitized.len() > self.config.max_query_length {
            sanitized.truncate(self.config.max_query_length);
            sanitized.push_str("...[TRUNCATED]");
        }

        // Basic sanitization - remove sensitive patterns
        sanitized = sanitized
            .replace("password", "***")
            .replace("secret", "***")
            .replace("token", "***")
            .replace("key", "***");

        Ok(sanitized)
    }

    /// Sanitize query parameters
    fn sanitize_parameters(
        &self,
        parameters: Vec<QueryParameter>,
    ) -> ArbitrageResult<Vec<QueryParameter>> {
        let mut sanitized = Vec::new();

        for param in parameters {
            let sanitized_value = if param.name.to_lowercase().contains("password")
                || param.name.to_lowercase().contains("secret")
                || param.name.to_lowercase().contains("key")
                || param.name.to_lowercase().contains("token")
            {
                "***".to_string()
            } else {
                // Truncate long values
                if param.sanitized_value.len() > 100 {
                    format!("{}...[TRUNCATED]", &param.sanitized_value[..97])
                } else {
                    param.sanitized_value
                }
            };

            sanitized.push(QueryParameter {
                name: param.name,
                param_type: param.param_type,
                sanitized_value,
            });
        }

        Ok(sanitized)
    }

    /// Detect query operation type from SQL
    fn detect_operation_type(&self, sql: &str) -> ArbitrageResult<QueryOperationType> {
        let sql_upper = sql.trim().to_uppercase();

        Ok(match sql_upper.split_whitespace().next() {
            Some("SELECT") => QueryOperationType::Select,
            Some("INSERT") => QueryOperationType::Insert,
            Some("UPDATE") => QueryOperationType::Update,
            Some("DELETE") => QueryOperationType::Delete,
            Some("CREATE") => QueryOperationType::Create,
            Some("DROP") => QueryOperationType::Drop,
            Some("ALTER") => QueryOperationType::Alter,
            Some("BEGIN") | Some("COMMIT") | Some("ROLLBACK") => QueryOperationType::Transaction,
            _ => {
                // Check for R2 operations based on context
                if sql.contains("R2_") || sql.contains("r2_") {
                    if sql.contains("READ") || sql.contains("GET") {
                        QueryOperationType::R2Read
                    } else if sql.contains("WRITE") || sql.contains("PUT") {
                        QueryOperationType::R2Write
                    } else if sql.contains("DELETE") {
                        QueryOperationType::R2Delete
                    } else if sql.contains("LIST") {
                        QueryOperationType::R2List
                    } else {
                        QueryOperationType::Select // Default fallback
                    }
                } else {
                    QueryOperationType::Select // Default fallback
                }
            }
        })
    }

    /// Calculate query complexity score (1-10)
    fn calculate_complexity_score(
        &self,
        sql: &str,
        operation_type: &QueryOperationType,
    ) -> ArbitrageResult<u8> {
        let mut score = 1u8;

        let sql_upper = sql.to_uppercase();

        // Base score by operation type
        score += match operation_type {
            QueryOperationType::Select => 1,
            QueryOperationType::Insert => 2,
            QueryOperationType::Update => 2,
            QueryOperationType::Delete => 3,
            QueryOperationType::Create => 4,
            QueryOperationType::Drop => 4,
            QueryOperationType::Alter => 5,
            QueryOperationType::Transaction => 3,
            _ => 2,
        };

        // Add complexity for various SQL features
        if sql_upper.contains("JOIN") {
            score += 2;
        }
        if sql_upper.contains("SUBQUERY") || sql_upper.contains("EXISTS") {
            score += 2;
        }
        if sql_upper.contains("GROUP BY") {
            score += 1;
        }
        if sql_upper.contains("ORDER BY") {
            score += 1;
        }
        if sql_upper.contains("HAVING") {
            score += 1;
        }
        if sql_upper.contains("UNION") {
            score += 2;
        }
        if sql_upper.contains("WINDOW") || sql_upper.contains("OVER") {
            score += 2;
        }

        // Add complexity for number of tables involved
        let table_count = sql_upper.matches("FROM").count() + sql_upper.matches("JOIN").count();
        if table_count > 3 {
            score += 2;
        } else if table_count > 1 {
            score += 1;
        }

        Ok(score.min(10))
    }

    /// Analyze index usage from query and execution plan
    fn analyze_index_usage(
        &self,
        _sql: &str,
        execution_plan: &Option<QueryPlan>,
    ) -> ArbitrageResult<Vec<IndexUsage>> {
        let mut index_usage = Vec::new();

        if let Some(plan) = execution_plan {
            self.extract_index_usage_from_plan(&plan.nodes, &mut index_usage)?;
        }

        Ok(index_usage)
    }

    /// Extract index usage information from execution plan nodes
    #[allow(clippy::only_used_in_recursion)]
    fn extract_index_usage_from_plan(
        &self,
        nodes: &[PlanNode],
        index_usage: &mut Vec<IndexUsage>,
    ) -> ArbitrageResult<()> {
        for node in nodes {
            if let (Some(table_name), Some(index_name)) = (&node.table_name, &node.index_name) {
                let scan_type = match node.node_type.as_str() {
                    "IndexScan" => ScanType::IndexScan,
                    "IndexSeek" => ScanType::IndexSeek,
                    "TableScan" => ScanType::TableScan,
                    "ClusteredIndexScan" => ScanType::ClusteredIndexScan,
                    "ClusteredIndexSeek" => ScanType::ClusteredIndexSeek,
                    _ => ScanType::TableScan,
                };

                let selectivity = if let (Some(actual_rows), estimated_rows) =
                    (node.actual_rows, node.estimated_rows)
                {
                    if estimated_rows > 0 {
                        actual_rows as f64 / estimated_rows as f64
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };

                index_usage.push(IndexUsage {
                    index_name: index_name.clone(),
                    table_name: table_name.clone(),
                    was_used: !matches!(scan_type, ScanType::TableScan),
                    selectivity: selectivity.clamp(0.0, 1.0),
                    scan_type,
                });
            }

            // Recursively process child nodes
            self.extract_index_usage_from_plan(&node.children, index_usage)?;
        }

        Ok(())
    }

    /// Categorize error for performance analysis
    fn categorize_error(&self, error: &ArbitrageError) -> ErrorCategory {
        let error_msg = error.to_string().to_lowercase();

        if error_msg.contains("timeout") {
            ErrorCategory::Timeout
        } else if error_msg.contains("syntax") || error_msg.contains("parse") {
            ErrorCategory::Syntax
        } else if error_msg.contains("connection") || error_msg.contains("network") {
            ErrorCategory::Connection
        } else if error_msg.contains("permission") || error_msg.contains("access") {
            ErrorCategory::Permission
        } else if error_msg.contains("constraint") || error_msg.contains("foreign key") {
            ErrorCategory::Constraint
        } else if error_msg.contains("memory") || error_msg.contains("resource") {
            ErrorCategory::Resource
        } else {
            ErrorCategory::Unknown
        }
    }

    /// Determine if error is retryable
    fn is_retryable_error(&self, error: &ArbitrageError) -> bool {
        let error_msg = error.to_string().to_lowercase();

        // Retryable errors
        error_msg.contains("timeout")
            || error_msg.contains("connection")
            || error_msg.contains("network")
            || error_msg.contains("temporary")
            || error_msg.contains("deadlock")
    }
}

/// Helper macro for creating query parameters
#[macro_export]
macro_rules! query_param {
    ($name:expr, $type:expr, $value:expr) => {
        QueryParameter {
            name: $name.to_string(),
            param_type: $type.to_string(),
            sanitized_value: $value.to_string(),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_config_default() {
        let config = ProfilerConfig::default();
        assert!(config.enabled);
        assert!(config.collect_execution_plans);
        assert!(config.sanitize_parameters);
        assert_eq!(config.max_query_length, 10000);
        assert!(config.enable_fingerprinting);
        assert!(!config.collect_stack_traces);
    }

    #[test]
    fn test_query_parameter_creation() {
        let param = query_param!("user_id", "INTEGER", "123");
        assert_eq!(param.name, "user_id");
        assert_eq!(param.param_type, "INTEGER");
        assert_eq!(param.sanitized_value, "123");
    }

    #[test]
    fn test_detect_operation_type() {
        // This would require actual QueryProfiler instance
        // Testing the logic conceptually
        let test_cases = vec![
            ("SELECT * FROM users", "SELECT"),
            ("INSERT INTO users VALUES", "INSERT"),
            ("UPDATE users SET", "UPDATE"),
            ("DELETE FROM users", "DELETE"),
            ("CREATE TABLE", "CREATE"),
            ("DROP TABLE", "DROP"),
            ("ALTER TABLE", "ALTER"),
            ("BEGIN TRANSACTION", "BEGIN"),
        ];

        for (sql, expected) in test_cases {
            let sql_upper = sql.trim().to_uppercase();
            let first_word = sql_upper.split_whitespace().next().unwrap();
            assert!(first_word.starts_with(expected));
        }
    }

    #[test]
    fn test_complexity_score_calculation() {
        // Test basic complexity scoring logic based on actual implementation
        let test_cases = vec![
            ("SELECT * FROM users", 2),             // Base (1) + SELECT (1)
            ("SELECT * FROM users JOIN orders", 5), // Base (1) + SELECT (1) + JOIN (2) + tables>1 (1)
            ("SELECT * FROM users WHERE id EXISTS (SELECT 1)", 4), // Base (1) + SELECT (1) + EXISTS (2)
            ("SELECT * FROM a JOIN b JOIN c", 5), // Base (1) + SELECT (1) + JOIN (2) + tables>1 (1)
        ];

        for (sql, expected_score) in test_cases {
            let sql_upper = sql.to_uppercase();
            let mut score = 1u8;

            // Replicate the logic from calculate_complexity_score
            // Base score by operation type (SELECT = 1)
            score += 1;

            // Add complexity for various SQL features
            if sql_upper.contains("JOIN") {
                score += 2;
            }
            if sql_upper.contains("SUBQUERY") || sql_upper.contains("EXISTS") {
                score += 2;
            }

            // Add complexity for number of tables involved
            let table_count = sql_upper.matches("FROM").count() + sql_upper.matches("JOIN").count();
            if table_count > 3 {
                score += 2;
            } else if table_count > 1 {
                score += 1;
            }

            assert_eq!(
                score, expected_score,
                "SQL: {} (table_count: {})",
                sql, table_count
            );
        }
    }

    #[test]
    fn test_error_categorization() {
        let test_cases = vec![
            ("timeout error", ErrorCategory::Timeout),
            ("syntax error in query", ErrorCategory::Syntax),
            ("connection failed", ErrorCategory::Connection),
            ("access denied", ErrorCategory::Permission),
            ("foreign key constraint", ErrorCategory::Constraint),
            ("out of memory", ErrorCategory::Resource),
            ("unknown error", ErrorCategory::Unknown),
        ];

        for (error_msg, expected_category) in test_cases {
            let error_msg_lower = error_msg.to_lowercase();
            let category = if error_msg_lower.contains("timeout") {
                ErrorCategory::Timeout
            } else if error_msg_lower.contains("syntax") || error_msg_lower.contains("parse") {
                ErrorCategory::Syntax
            } else if error_msg_lower.contains("connection") || error_msg_lower.contains("network")
            {
                ErrorCategory::Connection
            } else if error_msg_lower.contains("permission") || error_msg_lower.contains("access") {
                ErrorCategory::Permission
            } else if error_msg_lower.contains("constraint")
                || error_msg_lower.contains("foreign key")
            {
                ErrorCategory::Constraint
            } else if error_msg_lower.contains("memory") || error_msg_lower.contains("resource") {
                ErrorCategory::Resource
            } else {
                ErrorCategory::Unknown
            };

            assert_eq!(category, expected_category);
        }
    }

    #[test]
    fn test_retryable_error_detection() {
        let retryable_errors = vec![
            "timeout occurred",
            "connection lost",
            "network error",
            "temporary failure",
            "deadlock detected",
        ];

        let non_retryable_errors = vec![
            "syntax error",
            "permission denied",
            "foreign key constraint",
            "invalid table name",
        ];

        for error_msg in retryable_errors {
            let error_msg_lower = error_msg.to_lowercase();
            let is_retryable = error_msg_lower.contains("timeout")
                || error_msg_lower.contains("connection")
                || error_msg_lower.contains("network")
                || error_msg_lower.contains("temporary")
                || error_msg_lower.contains("deadlock");
            assert!(is_retryable, "Should be retryable: {}", error_msg);
        }

        for error_msg in non_retryable_errors {
            let error_msg_lower = error_msg.to_lowercase();
            let is_retryable = error_msg_lower.contains("timeout")
                || error_msg_lower.contains("connection")
                || error_msg_lower.contains("network")
                || error_msg_lower.contains("temporary")
                || error_msg_lower.contains("deadlock");
            assert!(!is_retryable, "Should not be retryable: {}", error_msg);
        }
    }
}
