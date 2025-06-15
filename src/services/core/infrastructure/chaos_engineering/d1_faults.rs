//! D1 Database Fault Injection Module
//!
//! Provides fault injection capabilities specifically for Cloudflare D1 database:
//! - SQL query failures and execution errors
//! - Connection timeouts and database unavailability
//! - Type errors and column not found errors
//! - Prepared statement failures and transaction errors
//! - Gradual degradation and intermittent failures

use crate::utils::error::{ArbitrageError, ArbitrageResult, ErrorKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::utils::time::{now_instant, WasmInstant};
use worker::Env;

use super::{ChaosEngineeringConfig, FaultConfig};

/// D1-specific fault injection parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct D1FaultParams {
    /// Target database binding name
    pub database_binding: Option<String>,
    /// SQL operation types to affect
    pub operations: Vec<D1Operation>,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Latency injection in milliseconds
    pub latency_ms: Option<u64>,
    /// Connection timeout override
    pub timeout_override_ms: Option<u64>,
    /// Specific error types to inject
    pub error_types: Vec<D1ErrorType>,
    /// Affected table patterns (regex)
    pub table_patterns: Vec<String>,
    /// Query complexity threshold (affect only complex queries)
    pub complexity_threshold: Option<u64>,
}

/// D1 operations that can be affected by faults
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum D1Operation {
    /// SELECT queries
    Select,
    /// INSERT statements
    Insert,
    /// UPDATE statements
    Update,
    /// DELETE statements
    Delete,
    /// Database schema operations (CREATE, ALTER, DROP)
    Schema,
    /// Prepared statement execution
    PreparedStatement,
    /// Batch operations
    Batch,
    /// Transaction operations
    Transaction,
    /// Database connection establishment
    Connection,
}

/// D1 error types based on Cloudflare documentation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum D1ErrorType {
    /// D1_ERROR - Generic database error
    GenericError,
    /// D1_TYPE_ERROR - Type mismatch between column and value
    TypeError,
    /// D1_COLUMN_NOTFOUND - Column not found
    ColumnNotFound,
    /// D1_DUMP_ERROR - Database dump error
    DumpError,
    /// D1_EXEC_ERROR - SQL execution error with syntax issues
    ExecError,
    /// Connection timeout
    ConnectionTimeout,
    /// Database unavailable
    DatabaseUnavailable,
    /// Query timeout
    QueryTimeout,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Transaction rollback
    TransactionRollback,
}

/// D1 fault injection state
#[derive(Debug, Clone)]
struct D1FaultState {
    #[allow(dead_code)]
    fault_id: String,
    fault_config: FaultConfig,
    params: D1FaultParams,
    activated_at: WasmInstant,
    stats: D1FaultStats,
}

/// Statistics for D1 fault injection
#[derive(Debug, Clone, Default)]
struct D1FaultStats {
    total_operations: u64,
    failed_operations: u64,
    timeout_operations: u64,
    delayed_operations: u64,
    type_error_operations: u64,
    exec_error_operations: u64,
    connection_failures: u64,
}

/// D1 Database Fault Injector
#[derive(Debug)]
pub struct D1FaultInjector {
    #[allow(dead_code)]
    config: ChaosEngineeringConfig,
    active_faults: HashMap<String, D1FaultState>,
    global_stats: D1FaultStats,
    is_enabled: bool,
    is_initialized: bool,
}

impl D1FaultInjector {
    pub async fn new(config: &ChaosEngineeringConfig, _env: &Env) -> ArbitrageResult<Self> {
        Ok(Self {
            config: config.clone(),
            active_faults: HashMap::new(),
            global_stats: D1FaultStats::default(),
            is_enabled: config.feature_flags.storage_fault_injection,
            is_initialized: true,
        })
    }

    /// Inject a D1-specific fault
    pub async fn inject_fault(
        &mut self,
        fault_id: &str,
        fault_config: &FaultConfig,
    ) -> ArbitrageResult<()> {
        if !self.is_enabled {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                "D1 fault injection is disabled".to_string(),
            ));
        }

        // Parse D1-specific parameters
        let params = self.parse_d1_params(&fault_config.parameters)?;

        let fault_state = D1FaultState {
            fault_id: fault_id.to_string(),
            fault_config: fault_config.clone(),
            params,
            activated_at: now_instant(),
            stats: D1FaultStats::default(),
        };

        self.active_faults.insert(fault_id.to_string(), fault_state);
        Ok(())
    }

    /// Remove a D1 fault
    pub async fn remove_fault(&mut self, fault_id: &str) -> ArbitrageResult<()> {
        if let Some(fault_state) = self.active_faults.remove(fault_id) {
            // Aggregate stats
            self.global_stats.total_operations += fault_state.stats.total_operations;
            self.global_stats.failed_operations += fault_state.stats.failed_operations;
            self.global_stats.timeout_operations += fault_state.stats.timeout_operations;
            self.global_stats.delayed_operations += fault_state.stats.delayed_operations;
            self.global_stats.type_error_operations += fault_state.stats.type_error_operations;
            self.global_stats.exec_error_operations += fault_state.stats.exec_error_operations;
            self.global_stats.connection_failures += fault_state.stats.connection_failures;
        }
        Ok(())
    }

    /// Check if D1 operation should be affected by fault injection
    pub async fn should_inject_fault(
        &mut self,
        database_binding: &str,
        operation: &D1Operation,
        query: Option<&str>,
    ) -> ArbitrageResult<Option<D1FaultInjection>> {
        if !self.is_enabled || self.active_faults.is_empty() {
            return Ok(None);
        }

        // Collect matching faults first to avoid borrow conflicts
        let matching_faults: Vec<(String, D1FaultState)> = self
            .active_faults
            .iter()
            .filter(|(_, fault_state)| {
                // Check if fault has expired
                if fault_state.activated_at.elapsed()
                    > Duration::from_secs(fault_state.fault_config.duration_seconds)
                {
                    return false;
                }

                // Check if this operation matches the fault criteria
                Self::matches_fault_criteria_static(
                    &fault_state.params,
                    database_binding,
                    operation,
                    query,
                )
            })
            .map(|(id, fault_state)| (id.clone(), fault_state.clone()))
            .collect();

        if let Some((fault_id, fault_state)) = matching_faults.first() {
            // Update stats for the matching fault
            if let Some(active_fault) = self.active_faults.get_mut(fault_id) {
                active_fault.stats.total_operations += 1;

                // Determine what type of fault injection to apply
                let injection = Self::determine_fault_injection_static(
                    &fault_state.fault_config,
                    &fault_state.params,
                )?;

                if let Some(ref injection) = injection {
                    match injection.injection_type {
                        D1FaultInjectionType::Failure(_) => {
                            active_fault.stats.failed_operations += 1;
                        }
                        D1FaultInjectionType::Timeout => {
                            active_fault.stats.timeout_operations += 1;
                        }
                        D1FaultInjectionType::Latency(_) => {
                            active_fault.stats.delayed_operations += 1;
                        }
                        D1FaultInjectionType::TypeError => {
                            active_fault.stats.type_error_operations += 1;
                        }
                        D1FaultInjectionType::ExecError => {
                            active_fault.stats.exec_error_operations += 1;
                        }
                        D1FaultInjectionType::ConnectionFailure => {
                            active_fault.stats.connection_failures += 1;
                        }
                        _ => {}
                    }
                }

                return Ok(injection);
            }
        }

        Ok(None)
    }

    /// Parse D1-specific parameters from fault configuration
    fn parse_d1_params(
        &self,
        parameters: &HashMap<String, String>,
    ) -> ArbitrageResult<D1FaultParams> {
        let database_binding = parameters.get("database_binding").cloned();

        let operations = parameters
            .get("operations")
            .map(|s| {
                s.split(',')
                    .filter_map(|op| match op.trim().to_lowercase().as_str() {
                        "select" => Some(D1Operation::Select),
                        "insert" => Some(D1Operation::Insert),
                        "update" => Some(D1Operation::Update),
                        "delete" => Some(D1Operation::Delete),
                        "schema" => Some(D1Operation::Schema),
                        "preparedstatement" => Some(D1Operation::PreparedStatement),
                        "batch" => Some(D1Operation::Batch),
                        "transaction" => Some(D1Operation::Transaction),
                        "connection" => Some(D1Operation::Connection),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    D1Operation::Select,
                    D1Operation::Insert,
                    D1Operation::Update,
                    D1Operation::Delete,
                ]
            });

        let error_rate = parameters
            .get("error_rate")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.1);

        let latency_ms = parameters.get("latency_ms").and_then(|s| s.parse().ok());

        let timeout_override_ms = parameters
            .get("timeout_override_ms")
            .and_then(|s| s.parse().ok());

        let error_types = parameters
            .get("error_types")
            .map(|s| {
                s.split(',')
                    .filter_map(|et| match et.trim().to_lowercase().as_str() {
                        "generic" => Some(D1ErrorType::GenericError),
                        "type" => Some(D1ErrorType::TypeError),
                        "column_not_found" => Some(D1ErrorType::ColumnNotFound),
                        "dump" => Some(D1ErrorType::DumpError),
                        "exec" => Some(D1ErrorType::ExecError),
                        "connection_timeout" => Some(D1ErrorType::ConnectionTimeout),
                        "database_unavailable" => Some(D1ErrorType::DatabaseUnavailable),
                        "query_timeout" => Some(D1ErrorType::QueryTimeout),
                        "resource_exhaustion" => Some(D1ErrorType::ResourceExhaustion),
                        "transaction_rollback" => Some(D1ErrorType::TransactionRollback),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![D1ErrorType::GenericError]);

        let table_patterns = parameters
            .get("table_patterns")
            .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_else(|| vec![".*".to_string()]);

        let complexity_threshold = parameters
            .get("complexity_threshold")
            .and_then(|s| s.parse().ok());

        Ok(D1FaultParams {
            database_binding,
            operations,
            error_rate,
            latency_ms,
            timeout_override_ms,
            error_types,
            table_patterns,
            complexity_threshold,
        })
    }

    /// Check if operation matches fault criteria (instance method for tests)
    #[allow(dead_code)]
    fn matches_fault_criteria(
        &self,
        params: &D1FaultParams,
        database_binding: &str,
        operation: &D1Operation,
        query: Option<&str>,
    ) -> bool {
        Self::matches_fault_criteria_static(params, database_binding, operation, query)
    }

    /// Check if operation matches fault criteria (static version)
    fn matches_fault_criteria_static(
        params: &D1FaultParams,
        database_binding: &str,
        operation: &D1Operation,
        query: Option<&str>,
    ) -> bool {
        // Check database binding
        if let Some(ref target_binding) = params.database_binding {
            if target_binding != database_binding {
                return false;
            }
        }

        // Check operation type
        if !params.operations.contains(operation) {
            return false;
        }

        // Check table patterns if query is provided
        if let Some(query_str) = query {
            if !params.table_patterns.iter().any(|pattern| {
                // Simple pattern matching for table names in queries
                if pattern == ".*" {
                    true
                } else {
                    query_str.to_lowercase().contains(&pattern.to_lowercase())
                }
            }) {
                return false;
            }
        }

        // Check complexity threshold if set
        if let Some(threshold) = params.complexity_threshold {
            if let Some(query_str) = query {
                let complexity = Self::estimate_query_complexity_static(query_str);
                if complexity < threshold {
                    return false;
                }
            }
        }

        true
    }

    /// Estimate query complexity based on query structure (static version)
    /// Estimate query complexity (instance method for tests)
    #[allow(dead_code)]
    fn estimate_query_complexity(&self, query: &str) -> u64 {
        Self::estimate_query_complexity_static(query)
    }

    fn estimate_query_complexity_static(query: &str) -> u64 {
        let query_lower = query.to_lowercase();
        let mut complexity = 1;

        // Basic complexity estimation
        if query_lower.contains("join") {
            complexity += 3;
        }
        if query_lower.contains("group by") {
            complexity += 2;
        }
        if query_lower.contains("order by") {
            complexity += 1;
        }
        if query_lower.contains("having") {
            complexity += 2;
        }
        if query_lower.contains("subquery")
            || query_lower.contains("select") && query_lower.matches("select").count() > 1
        {
            complexity += 4;
        }

        complexity
    }

    /// Determine what fault injection to apply (static version)
    fn determine_fault_injection_static(
        fault_config: &FaultConfig,
        params: &D1FaultParams,
    ) -> ArbitrageResult<Option<D1FaultInjection>> {
        // Use deterministic pseudo-random based on current time and intensity
        let random_value =
            (chrono::Utc::now().timestamp_millis() as f64 * fault_config.intensity) % 1.0;

        if random_value > params.error_rate {
            return Ok(None);
        }

        let injection_type = match fault_config.fault_type.as_str() {
            "Timeout" => D1FaultInjectionType::Timeout,
            "Latency" => {
                if let Some(latency_ms) = params.latency_ms {
                    D1FaultInjectionType::Latency(Duration::from_millis(latency_ms))
                } else {
                    D1FaultInjectionType::Latency(Duration::from_millis(500))
                }
            }
            "DataCorruption" => {
                // Randomly select an error type from configured types
                let error_idx = (random_value * params.error_types.len() as f64) as usize;
                let selected_error = params
                    .error_types
                    .get(error_idx)
                    .unwrap_or(&D1ErrorType::GenericError);

                match selected_error {
                    D1ErrorType::TypeError => D1FaultInjectionType::TypeError,
                    D1ErrorType::ColumnNotFound => D1FaultInjectionType::ColumnNotFound,
                    D1ErrorType::ExecError => D1FaultInjectionType::ExecError,
                    D1ErrorType::ConnectionTimeout => D1FaultInjectionType::ConnectionFailure,
                    D1ErrorType::DatabaseUnavailable => {
                        D1FaultInjectionType::Failure(D1ErrorType::DatabaseUnavailable)
                    }
                    _ => D1FaultInjectionType::Failure(selected_error.clone()),
                }
            }
            "Unavailability" => D1FaultInjectionType::Failure(D1ErrorType::DatabaseUnavailable),
            "ConnectionPoolExhaustion" => D1FaultInjectionType::ConnectionFailure,
            _ => D1FaultInjectionType::Failure(D1ErrorType::GenericError),
        };

        let metadata = Self::create_fault_metadata_static(fault_config, params);

        Ok(Some(D1FaultInjection {
            injection_type,
            metadata,
        }))
    }

    /// Create metadata for fault injection (static version)
    fn create_fault_metadata_static(
        fault_config: &FaultConfig,
        params: &D1FaultParams,
    ) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "fault_intensity".to_string(),
            fault_config.intensity.to_string(),
        );
        metadata.insert("error_rate".to_string(), params.error_rate.to_string());

        if let Some(binding) = &params.database_binding {
            metadata.insert("database_binding".to_string(), binding.clone());
        }

        metadata.insert(
            "operations".to_string(),
            params
                .operations
                .iter()
                .map(|op| format!("{:?}", op))
                .collect::<Vec<_>>()
                .join(","),
        );

        metadata
    }

    /// Get D1 fault injection statistics
    pub fn get_statistics(&self) -> D1FaultStatistics {
        let active_faults_count = self.active_faults.len();
        let total_operations = self.global_stats.total_operations;
        let failed_operations = self.global_stats.failed_operations;

        let failure_rate = if total_operations > 0 {
            failed_operations as f64 / total_operations as f64
        } else {
            0.0
        };

        let type_error_rate = if total_operations > 0 {
            self.global_stats.type_error_operations as f64 / total_operations as f64
        } else {
            0.0
        };

        let exec_error_rate = if total_operations > 0 {
            self.global_stats.exec_error_operations as f64 / total_operations as f64
        } else {
            0.0
        };

        let connection_failure_rate = if total_operations > 0 {
            self.global_stats.connection_failures as f64 / total_operations as f64
        } else {
            0.0
        };

        D1FaultStatistics {
            active_faults_count,
            total_operations,
            failed_operations,
            timeout_operations: self.global_stats.timeout_operations,
            delayed_operations: self.global_stats.delayed_operations,
            type_error_operations: self.global_stats.type_error_operations,
            exec_error_operations: self.global_stats.exec_error_operations,
            connection_failures: self.global_stats.connection_failures,
            failure_rate,
            type_error_rate,
            exec_error_rate,
            connection_failure_rate,
        }
    }

    /// Health check
    pub async fn is_healthy(&self) -> ArbitrageResult<bool> {
        Ok(self.is_initialized)
    }

    /// Shutdown
    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        self.active_faults.clear();
        self.is_initialized = false;
        Ok(())
    }
}

/// D1 fault injection result
#[derive(Debug, Clone)]
pub struct D1FaultInjection {
    pub injection_type: D1FaultInjectionType,
    pub metadata: HashMap<String, String>,
}

/// Types of D1 fault injection
#[derive(Debug, Clone)]
pub enum D1FaultInjectionType {
    /// Complete operation failure with specific D1 error
    Failure(D1ErrorType),
    /// Operation timeout
    Timeout,
    /// Latency injection
    Latency(Duration),
    /// D1_TYPE_ERROR - Type mismatch
    TypeError,
    /// D1_COLUMN_NOTFOUND - Column not found
    ColumnNotFound,
    /// D1_EXEC_ERROR - SQL execution error
    ExecError,
    /// Connection failure
    ConnectionFailure,
    /// Degraded performance
    Degraded,
    /// Resource exhausted
    ResourceExhausted,
    /// Transaction rollback
    TransactionRollback,
}

/// D1 fault injection statistics
#[derive(Debug, Clone)]
pub struct D1FaultStatistics {
    pub active_faults_count: usize,
    pub total_operations: u64,
    pub failed_operations: u64,
    pub timeout_operations: u64,
    pub delayed_operations: u64,
    pub type_error_operations: u64,
    pub exec_error_operations: u64,
    pub connection_failures: u64,
    pub failure_rate: f64,
    pub type_error_rate: f64,
    pub exec_error_rate: f64,
    pub connection_failure_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ChaosEngineeringConfig {
        crate::services::core::infrastructure::chaos_engineering::ChaosEngineeringConfig::default()
    }

    #[test]
    fn test_d1_operation_variants() {
        assert_eq!(D1Operation::Select, D1Operation::Select);
        assert_ne!(D1Operation::Select, D1Operation::Insert);
    }

    #[test]
    fn test_d1_error_type_variants() {
        assert_eq!(D1ErrorType::GenericError, D1ErrorType::GenericError);
        assert_ne!(D1ErrorType::GenericError, D1ErrorType::TypeError);
    }

    #[test]
    fn test_d1_fault_params_creation() {
        let mut params = HashMap::new();
        params.insert("database_binding".to_string(), "test_db".to_string());
        params.insert("operations".to_string(), "select,insert".to_string());
        params.insert("error_rate".to_string(), "0.2".to_string());
        params.insert("error_types".to_string(), "type,exec".to_string());

        let injector = D1FaultInjector {
            config: create_test_config(),
            active_faults: HashMap::new(),
            global_stats: D1FaultStats::default(),
            is_enabled: true,
            is_initialized: true,
        };

        let parsed_params = injector.parse_d1_params(&params).unwrap();
        assert_eq!(parsed_params.database_binding, Some("test_db".to_string()));
        assert_eq!(parsed_params.error_rate, 0.2);
        assert!(parsed_params.operations.contains(&D1Operation::Select));
        assert!(parsed_params.operations.contains(&D1Operation::Insert));
        assert!(parsed_params.error_types.contains(&D1ErrorType::TypeError));
        assert!(parsed_params.error_types.contains(&D1ErrorType::ExecError));
    }

    #[test]
    fn test_query_complexity_estimation() {
        let injector = D1FaultInjector {
            config: create_test_config(),
            active_faults: HashMap::new(),
            global_stats: D1FaultStats::default(),
            is_enabled: true,
            is_initialized: true,
        };

        let simple_query = "SELECT * FROM users";
        let complex_query = "SELECT u.*, p.* FROM users u JOIN profiles p ON u.id = p.user_id GROUP BY u.department ORDER BY u.created_at";

        assert!(
            injector.estimate_query_complexity(complex_query)
                > injector.estimate_query_complexity(simple_query)
        );
    }

    #[test]
    fn test_fault_criteria_matching() {
        let params = D1FaultParams {
            database_binding: Some("test_db".to_string()),
            operations: vec![D1Operation::Select, D1Operation::Insert],
            error_rate: 0.1,
            latency_ms: None,
            timeout_override_ms: None,
            error_types: vec![D1ErrorType::GenericError],
            table_patterns: vec!["users".to_string()],
            complexity_threshold: None,
        };

        let injector = D1FaultInjector {
            config: create_test_config(),
            active_faults: HashMap::new(),
            global_stats: D1FaultStats::default(),
            is_enabled: true,
            is_initialized: true,
        };

        // Should match
        assert!(injector.matches_fault_criteria(
            &params,
            "test_db",
            &D1Operation::Select,
            Some("SELECT * FROM users")
        ));

        // Should not match - wrong database
        assert!(!injector.matches_fault_criteria(
            &params,
            "other_db",
            &D1Operation::Select,
            Some("SELECT * FROM users")
        ));

        // Should not match - wrong operation
        assert!(!injector.matches_fault_criteria(
            &params,
            "test_db",
            &D1Operation::Update,
            Some("SELECT * FROM users")
        ));

        // Should not match - wrong table
        assert!(!injector.matches_fault_criteria(
            &params,
            "test_db",
            &D1Operation::Select,
            Some("SELECT * FROM orders")
        ));
    }

    #[test]
    fn test_d1_fault_statistics_calculation() {
        let stats = D1FaultStatistics {
            active_faults_count: 2,
            total_operations: 100,
            failed_operations: 10,
            timeout_operations: 5,
            delayed_operations: 15,
            type_error_operations: 3,
            exec_error_operations: 2,
            connection_failures: 1,
            failure_rate: 0.1,
            type_error_rate: 0.03,
            exec_error_rate: 0.02,
            connection_failure_rate: 0.01,
        };

        assert_eq!(stats.active_faults_count, 2);
        assert_eq!(stats.failure_rate, 0.1);
        assert_eq!(stats.type_error_rate, 0.03);
    }
}
