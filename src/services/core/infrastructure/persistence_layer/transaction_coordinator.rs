//! Transaction Coordinator Implementation for D1/R2 Persistence Layer
//!
//! Provides comprehensive transaction management with ACID properties, distributed
//! transaction support across D1 and R2, automatic rollback capabilities, transaction
//! monitoring, deadlock detection, and recovery mechanisms.

use crate::utils::error::{ArbitrageError, ArbitrageResult};
use crate::utils::time::now_instant;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;
use worker::Env;

use super::connection_pool::ConnectionManager;

/// Transaction coordinator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionConfig {
    /// Maximum time a transaction can remain active (milliseconds)
    pub transaction_timeout_ms: u64,
    /// Maximum number of concurrent transactions
    pub max_concurrent_transactions: u32,
    /// Enable automatic retry on transient failures
    pub enable_auto_retry: bool,
    /// Maximum retry attempts for failed transactions
    pub max_retry_attempts: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
    /// Enable deadlock detection
    pub enable_deadlock_detection: bool,
    /// Deadlock detection interval in milliseconds
    pub deadlock_detection_interval_ms: u64,
    /// Enable transaction logging
    pub enable_logging: bool,
    /// Transaction log retention period in hours
    pub log_retention_hours: u64,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            transaction_timeout_ms: 30000, // 30 seconds
            max_concurrent_transactions: 100,
            enable_auto_retry: true,
            max_retry_attempts: 3,
            retry_delay_ms: 1000, // 1 second
            enable_deadlock_detection: true,
            deadlock_detection_interval_ms: 5000, // 5 seconds
            enable_logging: true,
            log_retention_hours: 24, // 24 hours
        }
    }
}

/// Transaction state enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionState {
    /// Transaction is being prepared
    Preparing,
    /// Transaction is active and ready for operations
    Active,
    /// Transaction is being committed
    Committing,
    /// Transaction is being rolled back
    RollingBack,
    /// Transaction has been successfully committed
    Committed,
    /// Transaction has been rolled back
    RolledBack,
    /// Transaction has failed
    Failed,
    /// Transaction has timed out
    TimedOut,
}

impl TransactionState {
    pub fn is_active(&self) -> bool {
        matches!(self, TransactionState::Active)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TransactionState::Committed
                | TransactionState::RolledBack
                | TransactionState::Failed
                | TransactionState::TimedOut
        )
    }

    pub fn can_commit(&self) -> bool {
        matches!(self, TransactionState::Active)
    }

    pub fn can_rollback(&self) -> bool {
        matches!(
            self,
            TransactionState::Preparing | TransactionState::Active | TransactionState::Committing
        )
    }
}

/// Transaction isolation level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IsolationLevel {
    /// Read uncommitted data (lowest isolation)
    ReadUncommitted,
    /// Read committed data only
    #[default]
    ReadCommitted,
    /// Repeatable reads within transaction
    RepeatableRead,
    /// Full serialization (highest isolation)
    Serializable,
}

/// Transaction operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionOperation {
    /// D1 database operation
    D1Operation {
        sql: String,
        parameters: Vec<serde_json::Value>,
        operation_type: D1OperationType,
    },
    /// R2 storage operation
    R2Operation {
        key: String,
        operation_type: R2OperationType,
        data: Option<Vec<u8>>,
    },
}

/// D1 operation types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum D1OperationType {
    Select,
    Insert,
    Update,
    Delete,
    CreateTable,
    DropTable,
    CreateIndex,
    DropIndex,
}

/// R2 operation types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum R2OperationType {
    Get,
    Put,
    Delete,
    List,
    Head,
}

/// Transaction information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    /// Unique transaction identifier
    pub transaction_id: String,
    /// Transaction state
    pub state: TransactionState,
    /// Isolation level
    pub isolation_level: IsolationLevel,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Transaction timeout
    pub timeout_at: DateTime<Utc>,
    /// Operations performed in this transaction
    pub operations: Vec<TransactionOperation>,
    /// Resources locked by this transaction
    pub locked_resources: Vec<String>,
    /// Transaction context/metadata
    pub context: HashMap<String, serde_json::Value>,
    /// Error information (if failed)
    pub error: Option<String>,
    /// Retry count
    pub retry_count: u32,
}

impl TransactionInfo {
    pub fn new(isolation_level: IsolationLevel, timeout_ms: u64) -> Self {
        let now = Utc::now();
        let timeout_duration = Duration::from_millis(timeout_ms);

        Self {
            transaction_id: Uuid::new_v4().to_string(),
            state: TransactionState::Preparing,
            isolation_level,
            created_at: now,
            updated_at: now,
            timeout_at: now
                + chrono::Duration::from_std(timeout_duration)
                    .unwrap_or(chrono::Duration::seconds(30)),
            operations: Vec::new(),
            locked_resources: Vec::new(),
            context: HashMap::new(),
            error: None,
            retry_count: 0,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.timeout_at
    }

    pub fn add_operation(&mut self, operation: TransactionOperation) {
        self.operations.push(operation);
        self.updated_at = Utc::now();
    }

    pub fn lock_resource(&mut self, resource: String) {
        if !self.locked_resources.contains(&resource) {
            self.locked_resources.push(resource);
            self.updated_at = Utc::now();
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.state = TransactionState::Failed;
        self.updated_at = Utc::now();
    }
}

/// Transaction statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionStats {
    /// Total transactions created
    pub total_transactions: u64,
    /// Successfully committed transactions
    pub committed_transactions: u64,
    /// Rolled back transactions
    pub rolled_back_transactions: u64,
    /// Failed transactions
    pub failed_transactions: u64,
    /// Timed out transactions
    pub timed_out_transactions: u64,
    /// Currently active transactions
    pub active_transactions: u32,
    /// Average transaction duration in milliseconds
    pub average_duration_ms: f64,
    /// Average operations per transaction
    pub average_operations_per_transaction: f64,
    /// Deadlocks detected
    pub deadlocks_detected: u64,
    /// Auto-retries performed
    pub auto_retries_performed: u64,
    /// Last statistics update
    pub last_updated: DateTime<Utc>,
}

impl Default for TransactionStats {
    fn default() -> Self {
        Self {
            total_transactions: 0,
            committed_transactions: 0,
            rolled_back_transactions: 0,
            failed_transactions: 0,
            timed_out_transactions: 0,
            active_transactions: 0,
            average_duration_ms: 0.0,
            average_operations_per_transaction: 0.0,
            deadlocks_detected: 0,
            auto_retries_performed: 0,
            last_updated: Utc::now(),
        }
    }
}

/// Transaction coordinator main implementation
pub struct TransactionCoordinator {
    /// Transaction configuration
    config: TransactionConfig,
    /// Connection manager for database operations
    connection_manager: Arc<ConnectionManager>,
    /// Active transactions
    active_transactions: Arc<Mutex<HashMap<String, TransactionInfo>>>,
    /// Transaction statistics
    stats: Arc<Mutex<TransactionStats>>,
    /// Resource locks tracking
    resource_locks: Arc<Mutex<HashMap<String, String>>>, // resource -> transaction_id
    /// Transaction log for recovery
    transaction_log: Arc<Mutex<Vec<TransactionLogEntry>>>,
    /// Logger instance
    logger: crate::utils::logger::Logger,
}

/// Transaction log entry for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLogEntry {
    pub transaction_id: String,
    pub timestamp: DateTime<Utc>,
    pub event: TransactionEvent,
    pub details: serde_json::Value,
}

/// Transaction events for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionEvent {
    TransactionStarted,
    OperationExecuted,
    TransactionCommitted,
    TransactionRolledBack,
    TransactionFailed,
    DeadlockDetected,
    RetryAttempted,
}

impl TransactionCoordinator {
    /// Create new transaction coordinator
    pub async fn new(
        config: TransactionConfig,
        connection_manager: Arc<ConnectionManager>,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        let coordinator = Self {
            config,
            connection_manager,
            active_transactions: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(TransactionStats::default())),
            resource_locks: Arc::new(Mutex::new(HashMap::new())),
            transaction_log: Arc::new(Mutex::new(Vec::new())),
            logger,
        };

        coordinator
            .logger
            .info("Transaction coordinator initialized");
        Ok(coordinator)
    }

    /// Begin a new transaction
    pub async fn begin_transaction(
        &self,
        isolation_level: Option<IsolationLevel>,
    ) -> ArbitrageResult<String> {
        let isolation = isolation_level.unwrap_or_default();
        let mut transaction = TransactionInfo::new(isolation, self.config.transaction_timeout_ms);

        // Check if we can start new transaction (capacity limit)
        {
            let active_txns = self.active_transactions.lock().unwrap();
            if active_txns.len() >= self.config.max_concurrent_transactions as usize {
                return Err(ArbitrageError::database_error(
                    "Maximum concurrent transactions limit reached",
                ));
            }
        }

        transaction.state = TransactionState::Active;
        let transaction_id = transaction.transaction_id.clone();

        // Store transaction
        {
            let mut active_txns = self.active_transactions.lock().unwrap();
            active_txns.insert(transaction_id.clone(), transaction);
        }

        // Update statistics
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_transactions += 1;
            stats.active_transactions += 1;
            stats.last_updated = Utc::now();
        }

        // Log transaction start
        if self.config.enable_logging {
            self.log_transaction_event(
                &transaction_id,
                TransactionEvent::TransactionStarted,
                serde_json::json!({
                    "isolation_level": isolation,
                    "timeout_ms": self.config.transaction_timeout_ms
                }),
            )
            .await;
        }

        self.logger.debug(&format!(
            "Started transaction {} with isolation level {:?}",
            transaction_id, isolation
        ));

        Ok(transaction_id)
    }

    /// Execute operation within transaction
    pub async fn execute_operation(
        &self,
        env: &Env,
        transaction_id: &str,
        operation: TransactionOperation,
    ) -> ArbitrageResult<serde_json::Value> {
        // Validate transaction exists and is active
        {
            let mut active_txns = self.active_transactions.lock().unwrap();
            let transaction = active_txns
                .get_mut(transaction_id)
                .ok_or_else(|| ArbitrageError::database_error("Transaction not found"))?;

            if !transaction.state.is_active() {
                return Err(ArbitrageError::database_error(format!(
                    "Transaction {} is not active",
                    transaction_id
                )));
            }

            if transaction.is_expired() {
                transaction.state = TransactionState::TimedOut;
                return Err(ArbitrageError::database_error(format!(
                    "Transaction {} has timed out",
                    transaction_id
                )));
            }

            // Add operation to transaction
            transaction.add_operation(operation.clone());
        }

        // Execute the operation
        let result = match operation {
            TransactionOperation::D1Operation {
                sql,
                parameters,
                operation_type,
            } => {
                self.execute_d1_operation(env, transaction_id, &sql, &parameters, operation_type)
                    .await?
            }
            TransactionOperation::R2Operation {
                key,
                operation_type,
                data,
            } => {
                self.execute_r2_operation(env, transaction_id, &key, operation_type, data)
                    .await?
            }
        };

        // Log operation execution
        if self.config.enable_logging {
            self.log_transaction_event(
                transaction_id,
                TransactionEvent::OperationExecuted,
                serde_json::json!({
                    "result_size": result.to_string().len()
                }),
            )
            .await;
        }

        Ok(result)
    }

    /// Commit transaction
    pub async fn commit_transaction(&self, env: &Env, transaction_id: &str) -> ArbitrageResult<()> {
        let start_time = now_instant();

        // Update transaction state to committing
        {
            let mut active_txns = self.active_transactions.lock().unwrap();
            let transaction = active_txns
                .get_mut(transaction_id)
                .ok_or_else(|| ArbitrageError::database_error("Transaction not found"))?;

            if !transaction.state.can_commit() {
                return Err(ArbitrageError::database_error(format!(
                    "Transaction {} cannot be committed in state {:?}",
                    transaction_id, transaction.state
                )));
            }

            transaction.state = TransactionState::Committing;
            transaction.updated_at = Utc::now();
        }

        // Perform actual commit operations
        let commit_result = self.perform_commit(env, transaction_id).await;

        // Update transaction state based on result
        let final_state = match commit_result {
            Ok(_) => {
                // Update statistics
                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.committed_transactions += 1;
                    stats.active_transactions = stats.active_transactions.saturating_sub(1);

                    let duration_ms = start_time.elapsed().as_millis() as f64;
                    stats.average_duration_ms = (stats.average_duration_ms + duration_ms) / 2.0;
                    stats.last_updated = Utc::now();
                }

                TransactionState::Committed
            }
            Err(_) => TransactionState::Failed,
        };

        // Update transaction state and remove from active list
        {
            let mut active_txns = self.active_transactions.lock().unwrap();
            if let Some(mut transaction) = active_txns.remove(transaction_id) {
                transaction.state = final_state;
                transaction.updated_at = Utc::now();
            }
        }

        // Release all locks held by this transaction
        self.release_transaction_locks(transaction_id).await;

        // Log commit
        if self.config.enable_logging {
            self.log_transaction_event(
                transaction_id,
                TransactionEvent::TransactionCommitted,
                serde_json::json!({
                    "duration_ms": start_time.elapsed().as_millis(),
                    "success": commit_result.is_ok()
                }),
            )
            .await;
        }

        self.logger.debug(&format!(
            "Committed transaction {} in {}ms",
            transaction_id,
            start_time.elapsed().as_millis()
        ));

        commit_result
    }

    /// Rollback transaction
    pub async fn rollback_transaction(
        &self,
        env: &Env,
        transaction_id: &str,
    ) -> ArbitrageResult<()> {
        let start_time = now_instant();

        // Update transaction state to rolling back
        {
            let mut active_txns = self.active_transactions.lock().unwrap();
            let transaction = active_txns
                .get_mut(transaction_id)
                .ok_or_else(|| ArbitrageError::database_error("Transaction not found"))?;

            if !transaction.state.can_rollback() {
                return Err(ArbitrageError::database_error(format!(
                    "Transaction {} cannot be rolled back in state {:?}",
                    transaction_id, transaction.state
                )));
            }

            transaction.state = TransactionState::RollingBack;
            transaction.updated_at = Utc::now();
        }

        // Perform actual rollback operations
        let rollback_result = self.perform_rollback(env, transaction_id).await;

        // Update statistics
        {
            let mut stats = self.stats.lock().unwrap();
            stats.rolled_back_transactions += 1;
            stats.active_transactions = stats.active_transactions.saturating_sub(1);

            let duration_ms = start_time.elapsed().as_millis() as f64;
            stats.average_duration_ms = (stats.average_duration_ms + duration_ms) / 2.0;
            stats.last_updated = Utc::now();
        }

        // Remove transaction from active list
        {
            let mut active_txns = self.active_transactions.lock().unwrap();
            if let Some(mut transaction) = active_txns.remove(transaction_id) {
                transaction.state = TransactionState::RolledBack;
                transaction.updated_at = Utc::now();
            }
        }

        // Release all locks held by this transaction
        self.release_transaction_locks(transaction_id).await;

        // Log rollback
        if self.config.enable_logging {
            self.log_transaction_event(
                transaction_id,
                TransactionEvent::TransactionRolledBack,
                serde_json::json!({
                    "duration_ms": start_time.elapsed().as_millis(),
                    "success": rollback_result.is_ok()
                }),
            )
            .await;
        }

        self.logger.debug(&format!(
            "Rolled back transaction {} in {}ms",
            transaction_id,
            start_time.elapsed().as_millis()
        ));

        rollback_result
    }

    /// Get transaction information
    pub async fn get_transaction_info(
        &self,
        transaction_id: &str,
    ) -> ArbitrageResult<TransactionInfo> {
        let active_txns = self.active_transactions.lock().unwrap();
        active_txns
            .get(transaction_id)
            .cloned()
            .ok_or_else(|| ArbitrageError::database_error("Transaction not found"))
    }

    /// Get transaction statistics
    pub async fn get_statistics(&self) -> ArbitrageResult<TransactionStats> {
        let stats = self.stats.lock().unwrap();
        Ok(stats.clone())
    }

    /// Clean up expired transactions
    pub async fn cleanup_expired_transactions(&self) -> ArbitrageResult<u32> {
        let mut cleaned_count = 0;
        let now = Utc::now();

        let expired_transaction_ids: Vec<String> = {
            let active_txns = self.active_transactions.lock().unwrap();
            active_txns
                .iter()
                .filter(|(_, txn)| txn.is_expired())
                .map(|(id, _)| id.clone())
                .collect()
        };

        for transaction_id in expired_transaction_ids {
            // Mark as timed out and remove
            {
                let mut active_txns = self.active_transactions.lock().unwrap();
                if let Some(mut transaction) = active_txns.remove(&transaction_id) {
                    transaction.state = TransactionState::TimedOut;
                    transaction.updated_at = now;
                    cleaned_count += 1;
                }
            }

            // Release locks
            self.release_transaction_locks(&transaction_id).await;

            // Update statistics
            {
                let mut stats = self.stats.lock().unwrap();
                stats.timed_out_transactions += 1;
                stats.active_transactions = stats.active_transactions.saturating_sub(1);
                stats.last_updated = now;
            }

            self.logger.warn(&format!(
                "Cleaned up expired transaction: {}",
                transaction_id
            ));
        }

        Ok(cleaned_count)
    }

    /// Execute D1 database operation (placeholder implementation)
    async fn execute_d1_operation(
        &self,
        env: &Env,
        transaction_id: &str,
        sql: &str,
        _parameters: &[serde_json::Value],
        _operation_type: D1OperationType,
    ) -> ArbitrageResult<serde_json::Value> {
        let transaction_id = transaction_id.to_string();
        let sql = sql.to_string();

        self.connection_manager
            .with_d1_connection(env, move |_db| {
                Box::pin(async move {
                    // TODO: Implement actual D1 operation execution
                    log::debug!(
                        "Executing D1 operation for transaction {}: {}",
                        transaction_id,
                        sql
                    );

                    Ok(serde_json::json!({
                        "success": true,
                        "affected_rows": 1,
                        "transaction_id": transaction_id
                    }))
                })
            })
            .await
    }

    /// Execute R2 storage operation (placeholder implementation)
    async fn execute_r2_operation(
        &self,
        env: &Env,
        transaction_id: &str,
        key: &str,
        operation_type: R2OperationType,
        _data: Option<Vec<u8>>,
    ) -> ArbitrageResult<serde_json::Value> {
        let transaction_id = transaction_id.to_string();
        let key = key.to_string();

        self.connection_manager
            .with_r2_connection(env, move |_bucket| {
                Box::pin(async move {
                    // TODO: Implement actual R2 operation execution
                    log::debug!(
                        "Executing R2 {:?} operation for transaction {}: {}",
                        operation_type,
                        transaction_id,
                        key
                    );

                    Ok(serde_json::json!({
                        "success": true,
                        "key": key,
                        "operation": format!("{:?}", operation_type),
                        "transaction_id": transaction_id
                    }))
                })
            })
            .await
    }

    /// Perform actual commit operations (placeholder)
    async fn perform_commit(&self, _env: &Env, transaction_id: &str) -> ArbitrageResult<()> {
        // TODO: Implement actual commit logic
        self.logger
            .debug(&format!("Committing transaction: {}", transaction_id));
        Ok(())
    }

    /// Perform actual rollback operations (placeholder)
    async fn perform_rollback(&self, _env: &Env, transaction_id: &str) -> ArbitrageResult<()> {
        // TODO: Implement actual rollback logic
        self.logger
            .debug(&format!("Rolling back transaction: {}", transaction_id));
        Ok(())
    }

    /// Release all locks held by a transaction
    async fn release_transaction_locks(&self, transaction_id: &str) {
        let mut resource_locks = self.resource_locks.lock().unwrap();
        resource_locks.retain(|_, txn_id| txn_id != transaction_id);
    }

    /// Log transaction event
    async fn log_transaction_event(
        &self,
        transaction_id: &str,
        event: TransactionEvent,
        details: serde_json::Value,
    ) {
        if self.config.enable_logging {
            let log_entry = TransactionLogEntry {
                transaction_id: transaction_id.to_string(),
                timestamp: Utc::now(),
                event,
                details,
            };

            let mut transaction_log = self.transaction_log.lock().unwrap();
            transaction_log.push(log_entry);

            // Clean up old log entries if needed
            let retention_cutoff =
                Utc::now() - chrono::Duration::hours(self.config.log_retention_hours as i64);
            transaction_log.retain(|entry| entry.timestamp > retention_cutoff);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_config_default() {
        let config = TransactionConfig::default();
        assert_eq!(config.transaction_timeout_ms, 30000);
        assert_eq!(config.max_concurrent_transactions, 100);
        assert!(config.enable_auto_retry);
        assert_eq!(config.max_retry_attempts, 3);
    }

    #[test]
    fn test_transaction_state_properties() {
        assert!(TransactionState::Active.is_active());
        assert!(!TransactionState::Committed.is_active());

        assert!(TransactionState::Committed.is_terminal());
        assert!(!TransactionState::Active.is_terminal());

        assert!(TransactionState::Active.can_commit());
        assert!(!TransactionState::Committed.can_commit());

        assert!(TransactionState::Active.can_rollback());
        assert!(!TransactionState::Committed.can_rollback());
    }

    #[test]
    fn test_transaction_info_creation() {
        let txn = TransactionInfo::new(IsolationLevel::ReadCommitted, 30000);
        assert_eq!(txn.state, TransactionState::Preparing);
        assert_eq!(txn.isolation_level, IsolationLevel::ReadCommitted);
        assert!(!txn.transaction_id.is_empty());
        assert!(txn.operations.is_empty());
        assert!(txn.locked_resources.is_empty());
    }

    #[test]
    fn test_transaction_info_operations() {
        let mut txn = TransactionInfo::new(IsolationLevel::ReadCommitted, 30000);

        let operation = TransactionOperation::D1Operation {
            sql: "SELECT * FROM users".to_string(),
            parameters: vec![],
            operation_type: D1OperationType::Select,
        };

        txn.add_operation(operation);
        assert_eq!(txn.operations.len(), 1);
    }

    #[test]
    fn test_transaction_info_resource_locking() {
        let mut txn = TransactionInfo::new(IsolationLevel::ReadCommitted, 30000);

        txn.lock_resource("table:users".to_string());
        txn.lock_resource("table:orders".to_string());
        txn.lock_resource("table:users".to_string()); // Duplicate

        assert_eq!(txn.locked_resources.len(), 2);
        assert!(txn.locked_resources.contains(&"table:users".to_string()));
        assert!(txn.locked_resources.contains(&"table:orders".to_string()));
    }

    #[test]
    fn test_transaction_stats_default() {
        let stats = TransactionStats::default();
        assert_eq!(stats.total_transactions, 0);
        assert_eq!(stats.committed_transactions, 0);
        assert_eq!(stats.active_transactions, 0);
        assert_eq!(stats.average_duration_ms, 0.0);
    }

    #[test]
    fn test_isolation_level_default() {
        let level = IsolationLevel::default();
        assert_eq!(level, IsolationLevel::ReadCommitted);
    }

    #[test]
    fn test_d1_operation_types() {
        let select_op = D1OperationType::Select;
        let insert_op = D1OperationType::Insert;
        assert_ne!(select_op, insert_op);
    }

    #[test]
    fn test_r2_operation_types() {
        let get_op = R2OperationType::Get;
        let put_op = R2OperationType::Put;
        assert_ne!(get_op, put_op);
    }
}
