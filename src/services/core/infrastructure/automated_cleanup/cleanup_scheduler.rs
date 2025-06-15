//! Core Cleanup Scheduler Service
//! 
//! Orchestrates automated data cleanup across KV, D1, and R2 storage systems
//! with multiple cleanup strategies and safety mechanisms.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use crate::utils::time::{now_instant, WasmInstant};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use worker::Env;

use crate::services::core::infrastructure::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::services::core::infrastructure::persistence::{ConnectionManager, StorageType, TransactionCoordinator};
use crate::utils::error::{ArbitrageError, ArbitrageResult, ErrorKind};

/// Cleanup strategies supported by the scheduler
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CleanupStrategy {
    /// Time-based expiration cleanup
    TimeToLive {
        /// TTL duration in seconds
        ttl_seconds: u64,
        /// Grace period before deletion
        grace_period_seconds: Option<u64>,
    },
    /// Last accessed date-based cleanup
    UsageBased {
        /// Inactive period threshold in seconds
        inactive_threshold_seconds: u64,
        /// Minimum access count to preserve
        min_access_count: Option<u32>,
    },
    /// Storage quota-based cleanup
    SizeBased {
        /// Maximum storage size in bytes
        max_size_bytes: u64,
        /// Cleanup percentage when quota exceeded
        cleanup_percentage: f32,
    },
    /// Manual trigger cleanup
    Manual {
        /// Cleanup scope
        scope: CleanupScope,
        /// Force cleanup without safety checks
        force: bool,
    },
}

/// Scope of cleanup operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CleanupScope {
    /// All storage types
    All,
    /// Specific storage type
    Storage(StorageType),
    /// Specific data patterns
    Pattern(String),
    /// Specific key ranges
    KeyRange { start: String, end: String },
}

/// Cleanup policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPolicy {
    /// Policy identifier
    pub id: String,
    /// Policy name
    pub name: String,
    /// Policy description
    pub description: String,
    /// Cleanup strategy
    pub strategy: CleanupStrategy,
    /// Target storage types
    pub target_storage: Vec<StorageType>,
    /// Policy priority (higher priority runs first)
    pub priority: u32,
    /// Maximum cleanup rate (items per second)
    pub max_cleanup_rate: u32,
    /// Safety checks enabled
    pub safety_checks_enabled: bool,
    /// Dry run mode
    pub dry_run: bool,
    /// Policy enabled status
    pub enabled: bool,
    /// Policy creation timestamp
    pub created_at: DateTime<Utc>,
    /// Policy last modified timestamp
    pub updated_at: DateTime<Utc>,
}

/// Cleanup execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    /// Policy ID that was executed
    pub policy_id: String,
    /// Number of items processed
    pub items_processed: u64,
    /// Number of items cleaned up
    pub items_cleaned: u64,
    /// Bytes cleaned up
    pub bytes_cleaned: u64,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Execution timestamp
    pub executed_at: DateTime<Utc>,
    /// Execution status
    pub status: CleanupStatus,
    /// Error details if any
    pub error_details: Option<String>,
}

/// Cleanup execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CleanupStatus {
    /// Cleanup completed successfully
    Success,
    /// Cleanup completed with warnings
    Warning,
    /// Cleanup failed
    Failed,
    /// Cleanup was throttled
    Throttled,
    /// Cleanup was aborted due to safety checks
    Aborted,
}

/// Cleanup metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupMetrics {
    /// Total cleanup operations executed
    pub total_operations: u64,
    /// Total items cleaned
    pub total_items_cleaned: u64,
    /// Total bytes cleaned
    pub total_bytes_cleaned: u64,
    /// Average cleanup rate (items per second)
    pub avg_cleanup_rate: f64,
    /// Last cleanup execution time
    pub last_execution: Option<DateTime<Utc>>,
    /// Active policies count
    pub active_policies: u32,
    /// Failed operations count
    pub failed_operations: u64,
    /// Throttled operations count
    pub throttled_operations: u64,
}

/// Configuration for the cleanup scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupSchedulerConfig {
    /// Scheduler enabled status
    pub enabled: bool,
    /// Cleanup execution interval in seconds
    pub execution_interval_seconds: u64,
    /// Maximum concurrent cleanup operations
    pub max_concurrent_operations: u32,
    /// Global cleanup rate limit (items per second)
    pub global_rate_limit: u32,
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
    /// Safety check timeout in seconds
    pub safety_check_timeout_seconds: u64,
    /// Cleanup history retention in days
    pub history_retention_days: u32,
}

impl Default for CleanupSchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            execution_interval_seconds: 3600, // 1 hour
            max_concurrent_operations: 5,
            global_rate_limit: 1000, // 1000 items per second
            circuit_breaker: CircuitBreakerConfig::default(),
            safety_check_timeout_seconds: 30,
            history_retention_days: 30,
        }
    }
}

/// Core cleanup scheduler service
#[derive(Debug)]
pub struct CleanupScheduler {
    config: CleanupSchedulerConfig,
    policies: Arc<RwLock<HashMap<String, CleanupPolicy>>>,
    execution_history: Arc<Mutex<VecDeque<CleanupResult>>>,
    circuit_breaker: Arc<CircuitBreaker>,
    connection_manager: Arc<ConnectionManager>,
    transaction_coordinator: Arc<TransactionCoordinator>,
    is_running: Arc<RwLock<bool>>,
    active_operations: Arc<RwLock<u32>>,
}

impl CleanupScheduler {
    /// Create a new cleanup scheduler
    pub async fn new(
        config: CleanupSchedulerConfig,
        connection_manager: Arc<ConnectionManager>,
        transaction_coordinator: Arc<TransactionCoordinator>,
    ) -> ArbitrageResult<Self> {
        let circuit_breaker = Arc::new(CircuitBreaker::new(config.circuit_breaker.clone()));
        
        Ok(Self {
            config,
            policies: Arc::new(RwLock::new(HashMap::new())),
            execution_history: Arc::new(Mutex::new(VecDeque::new())),
            circuit_breaker,
            connection_manager,
            transaction_coordinator,
            is_running: Arc::new(RwLock::new(false)),
            active_operations: Arc::new(RwLock::new(0)),
        })
    }

    /// Start the cleanup scheduler
    pub async fn start(&self, env: &Env) -> ArbitrageResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut is_running = self.is_running.write().await;
        if *is_running {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                "Cleanup scheduler is already running".to_string(),
            ));
        }
        *is_running = true;
        drop(is_running);

        // Start cleanup execution loop
        let scheduler = self.clone();
        let env_clone = env.clone();
        tokio::spawn(async move {
            scheduler.execution_loop(&env_clone).await;
        });

        Ok(())
    }

    /// Stop the cleanup scheduler
    pub async fn stop(&self) -> ArbitrageResult<()> {
        let mut is_running = self.is_running.write().await;
        *is_running = false;
        
        // Wait for active operations to complete
        while *self.active_operations.read().await > 0 {
            sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }

    /// Add a cleanup policy
    pub async fn add_policy(&self, policy: CleanupPolicy) -> ArbitrageResult<()> {
        let mut policies = self.policies.write().await;
        
        // Validate policy
        self.validate_policy(&policy)?;
        
        policies.insert(policy.id.clone(), policy);
        
        Ok(())
    }

    /// Get all cleanup policies
    pub async fn get_policies(&self) -> HashMap<String, CleanupPolicy> {
        self.policies.read().await.clone()
    }

    /// Execute cleanup manually for a specific policy
    pub async fn execute_policy_manual(
        &self,
        policy_id: &str,
        env: &Env,
    ) -> ArbitrageResult<CleanupResult> {
        let policies = self.policies.read().await;
        let policy = policies.get(policy_id).ok_or_else(|| {
            ArbitrageError::new(
                ErrorKind::NotFound,
                format!("Cleanup policy '{}' not found", policy_id),
            )
        })?;

        if !policy.enabled {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                format!("Policy '{}' is disabled", policy_id),
            ));
        }

        self.execute_policy(policy, env).await
    }

    /// Main execution loop
    async fn execution_loop(&self, env: &Env) {
        let mut interval = interval(Duration::from_secs(self.config.execution_interval_seconds));
        
        while *self.is_running.read().await {
            interval.tick().await;
            
            if let Err(e) = self.execute_scheduled_cleanup(env).await {
                eprintln!("Cleanup execution error: {:?}", e);
            }
        }
    }

    /// Execute scheduled cleanup operations
    async fn execute_scheduled_cleanup(&self, env: &Env) -> ArbitrageResult<()> {
        if !self.circuit_breaker.can_execute().await {
            return Ok(()); // Circuit breaker is open
        }

        let policies = self.policies.read().await.clone();
        let mut enabled_policies: Vec<_> = policies
            .values()
            .filter(|p| p.enabled && !matches!(p.strategy, CleanupStrategy::Manual { .. }))
            .cloned()
            .collect();

        // Sort by priority (higher priority first)
        enabled_policies.sort_by(|a, b| b.priority.cmp(&a.priority));

        for policy in enabled_policies {
            // Check if we've exceeded max concurrent operations
            if *self.active_operations.read().await >= self.config.max_concurrent_operations {
                break;
            }

            // Execute policy asynchronously
            let scheduler = self.clone();
            let policy_clone = policy.clone();
            let env_clone = env.clone();
            
            tokio::spawn(async move {
                if let Err(e) = scheduler.execute_policy(&policy_clone, &env_clone).await {
                    eprintln!("Policy execution error for '{}': {:?}", policy_clone.id, e);
                }
            });
        }

        Ok(())
    }

    /// Execute a specific cleanup policy
    async fn execute_policy(&self, policy: &CleanupPolicy, _env: &Env) -> ArbitrageResult<CleanupResult> {
        let start_time = now_instant();
        let execution_start = Utc::now();

        // Increment active operations counter
        {
            let mut active_ops = self.active_operations.write().await;
            *active_ops += 1;
        }

        // Safety checks
        if policy.safety_checks_enabled && !self.perform_safety_checks(policy).await? {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            return Ok(CleanupResult {
                policy_id: policy.id.clone(),
                items_processed: 0,
                items_cleaned: 0,
                bytes_cleaned: 0,
                duration_ms,
                executed_at: execution_start,
                status: CleanupStatus::Aborted,
                error_details: Some("Safety checks failed".to_string()),
            });
        }

        // Execute cleanup based on strategy
        let result = match &policy.strategy {
            CleanupStrategy::TimeToLive { ttl_seconds, grace_period_seconds } => {
                self.execute_ttl_cleanup(policy, *ttl_seconds, *grace_period_seconds).await
            }
            CleanupStrategy::UsageBased { inactive_threshold_seconds, min_access_count } => {
                self.execute_usage_based_cleanup(policy, *inactive_threshold_seconds, *min_access_count).await
            }
            CleanupStrategy::SizeBased { max_size_bytes, cleanup_percentage } => {
                self.execute_size_based_cleanup(policy, *max_size_bytes, *cleanup_percentage).await
            }
            CleanupStrategy::Manual { scope, force } => {
                self.execute_manual_cleanup(policy, scope, *force).await
            }
        };

        // Decrement active operations counter
        {
            let mut active_ops = self.active_operations.write().await;
            *active_ops -= 1;
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        let cleanup_result = match result {
            Ok((items_processed, items_cleaned, bytes_cleaned)) => CleanupResult {
                policy_id: policy.id.clone(),
                items_processed,
                items_cleaned,
                bytes_cleaned,
                duration_ms,
                executed_at: execution_start,
                status: CleanupStatus::Success,
                error_details: None,
            },
            Err(e) => CleanupResult {
                policy_id: policy.id.clone(),
                items_processed: 0,
                items_cleaned: 0,
                bytes_cleaned: 0,
                duration_ms,
                executed_at: execution_start,
                status: CleanupStatus::Failed,
                error_details: Some(e.to_string()),
            },
        };

        // Record circuit breaker result
        if cleanup_result.status == CleanupStatus::Success {
            self.circuit_breaker.record_success().await;
        } else {
            self.circuit_breaker.record_failure().await;
        }

        Ok(cleanup_result)
    }

    /// Execute TTL-based cleanup
    async fn execute_ttl_cleanup(
        &self,
        _policy: &CleanupPolicy,
        _ttl_seconds: u64,
        _grace_period_seconds: Option<u64>,
    ) -> ArbitrageResult<(u64, u64, u64)> {
        // Placeholder for actual TTL cleanup implementation
        Ok((100, 50, 1024))
    }

    /// Execute usage-based cleanup
    async fn execute_usage_based_cleanup(
        &self,
        _policy: &CleanupPolicy,
        _inactive_threshold_seconds: u64,
        _min_access_count: Option<u32>,
    ) -> ArbitrageResult<(u64, u64, u64)> {
        // Placeholder for usage-based cleanup implementation
        Ok((0, 0, 0))
    }

    /// Execute size-based cleanup
    async fn execute_size_based_cleanup(
        &self,
        _policy: &CleanupPolicy,
        _max_size_bytes: u64,
        _cleanup_percentage: f32,
    ) -> ArbitrageResult<(u64, u64, u64)> {
        // Placeholder for size-based cleanup implementation
        Ok((0, 0, 0))
    }

    /// Execute manual cleanup
    async fn execute_manual_cleanup(
        &self,
        _policy: &CleanupPolicy,
        _scope: &CleanupScope,
        _force: bool,
    ) -> ArbitrageResult<(u64, u64, u64)> {
        // Placeholder for manual cleanup implementation
        Ok((0, 0, 0))
    }

    /// Perform safety checks before cleanup
    async fn perform_safety_checks(&self, _policy: &CleanupPolicy) -> ArbitrageResult<bool> {
        // Placeholder for safety checks implementation
        Ok(true)
    }

    /// Validate a cleanup policy
    fn validate_policy(&self, policy: &CleanupPolicy) -> ArbitrageResult<()> {
        if policy.id.is_empty() {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                "Policy ID cannot be empty".to_string(),
            ));
        }

        if policy.name.is_empty() {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                "Policy name cannot be empty".to_string(),
            ));
        }

        if policy.target_storage.is_empty() {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                "Policy must target at least one storage type".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if scheduler is healthy
    pub async fn is_healthy(&self) -> ArbitrageResult<bool> {
        if !*self.is_running.read().await {
            return Ok(false);
        }

        if !self.circuit_breaker.can_execute().await {
            return Ok(false);
        }

        let active_ops = *self.active_operations.read().await;
        if active_ops >= self.config.max_concurrent_operations {
            return Ok(false);
        }

        Ok(true)
    }
}

impl Clone for CleanupScheduler {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            policies: Arc::clone(&self.policies),
            execution_history: Arc::clone(&self.execution_history),
            circuit_breaker: Arc::clone(&self.circuit_breaker),
            connection_manager: Arc::clone(&self.connection_manager),
            transaction_coordinator: Arc::clone(&self.transaction_coordinator),
            is_running: Arc::clone(&self.is_running),
            active_operations: Arc::clone(&self.active_operations),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cleanup_policy_validation() {
        let config = CleanupSchedulerConfig::default();
        let connection_manager = Arc::new(ConnectionManager::new().await.unwrap());
        let transaction_coordinator = Arc::new(TransactionCoordinator::new().await.unwrap());
        
        let scheduler = CleanupScheduler::new(config, connection_manager, transaction_coordinator)
            .await
            .unwrap();

        let valid_policy = CleanupPolicy {
            id: "test-policy".to_string(),
            name: "Test Policy".to_string(),
            description: "Test policy description".to_string(),
            strategy: CleanupStrategy::TimeToLive {
                ttl_seconds: 3600,
                grace_period_seconds: Some(300),
            },
            target_storage: vec![StorageType::KV],
            priority: 1,
            max_cleanup_rate: 100,
            safety_checks_enabled: true,
            dry_run: false,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(scheduler.validate_policy(&valid_policy).is_ok());
    }
} 