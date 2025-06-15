//! Data Synchronization Engine
//!
//! Comprehensive distributed data synchronization framework for ensuring data consistency
//! across KV, D1, and R2 storage systems with multiple sync strategies, conflict resolution,
//! and operator tools.
//!
//! ## Features
//! - **Multiple Sync Strategies**: Write-through, write-behind, read-repair, periodic reconciliation
//! - **Conflict Resolution**: Vector clock-based conflict detection with multiple resolution strategies
//! - **Diff-based Sync**: Efficient delta synchronization with compression and merkle trees
//! - **Operator Tools**: Manual sync controls, dashboards, and administrative APIs
//! - **Comprehensive Testing**: Integration testing with consistency validation
//!
//! ## Architecture
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │   KV Storage    │    │   D1 Database   │    │   R2 Storage    │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//!          │                       │                       │
//!          └───────────────────────┼───────────────────────┘
//!                                  │
//!                    ┌─────────────────┐
//!                    │ Sync Coordinator│
//!                    └─────────────────┘
//!                              │
//!            ┌─────────────────┼─────────────────┐
//!            │                 │                 │
//!   ┌────────────────┐ ┌──────────────┐ ┌──────────────┐
//!   │ Conflict       │ │ Diff Engine  │ │ Operator     │
//!   │ Resolution     │ │              │ │ Tools        │
//!   └────────────────┘ └──────────────┘ └──────────────┘
//! ```

pub mod conflict_resolver;
pub mod diff_engine;
pub mod operator_tools;
pub mod sync_coordinator;
pub mod sync_validation;

// NEW: Cloudflare-specific synchronization coordinator
pub mod cloudflare_sync_coordinator;

// Re-export main components
pub use sync_coordinator::{
    ReadRepairConfig, ReconciliationConfig, ReconciliationSchedule, SyncConfig, SyncCoordinator,
    SyncCoordinatorConfig, SyncCoordinatorMetrics, SyncEvent, SyncEventType, SyncOperation,
    SyncStats, SyncStatus, SyncStrategies, SyncStrategy, WriteMode,
};

pub use conflict_resolver::{
    ConflictAuditLog, ConflictDetector, ConflictEvent, ConflictMetrics, ConflictNotification,
    ConflictResolutionResult, ConflictResolutionStrategy, ConflictResolver, ConflictResolverConfig,
    ConflictResolverHealth, ConflictResolverMetrics, MergeStrategy, ResolutionPolicy, VectorClock,
};

pub use diff_engine::{
    CompressionEngine, DataDiff, DeltaSync, DiffCalculator, DiffEngine, DiffEngineConfig,
    DiffEngineHealth, DiffEngineMetrics, DiffMetrics, DiffOperation, DiffResult, DiffType,
    MerkleTree, PayloadCompression, RollingHash, SyncPayload,
};

pub use operator_tools::{
    ActiveSyncOperation, EventSeverity, ManualSyncRequest, ManualSyncTrigger, OperatorToolsConfig,
    OperatorToolsHealth, OperatorToolsMetrics, OperatorToolsService, StorageSystemStatus,
    SyncDashboard, SyncDashboardService, SyncPriority, SyncQueueStatus, SyncTriggerType,
};

pub use sync_validation::{
    ConsistencyChecker, ConsistencyReport, IntegrityValidator, SyncTestSuite, SyncValidationConfig,
    SyncValidationHealth, SyncValidationMetrics, SyncValidator, ValidationMetrics,
    ValidationResult, ValidationRule, ValidationSeverity,
};

// NEW: Cloudflare sync coordinator exports
pub use cloudflare_sync_coordinator::{
    CloudflareServiceType, CloudflareSyncConfig, CloudflareSyncCoordinator, CloudflareSyncStrategy,
    ConsistencyLevel, ServiceConfig, ServiceMetrics, ServiceOperationResult, SyncHealth,
    SyncMetrics, SyncOperationType, SyncResult,
};

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use worker::Env;

/// Main data synchronization engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSynchronizationConfig {
    /// Sync coordinator configuration
    pub sync_coordinator: SyncCoordinatorConfig,
    /// Conflict resolution configuration
    pub conflict_resolver: ConflictResolverConfig,
    /// Diff engine configuration
    pub diff_engine: DiffEngineConfig,
    /// Operator tools configuration
    pub operator_tools: OperatorToolsConfig,
    /// Validation configuration
    pub validation: SyncValidationConfig,
    /// Feature flags for sync capabilities
    pub feature_flags: SyncFeatureFlags,
    /// NEW: Cloudflare sync coordinator configuration
    pub cloudflare_sync: CloudflareSyncConfig,
}

/// Feature flags for data synchronization capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFeatureFlags {
    /// Enable write-through synchronization
    pub enable_write_through: bool,
    /// Enable write-behind synchronization
    pub enable_write_behind: bool,
    /// Enable read-repair synchronization
    pub enable_read_repair: bool,
    /// Enable periodic reconciliation
    pub enable_periodic_reconciliation: bool,
    /// Enable vector clock conflict detection
    pub enable_vector_clocks: bool,
    /// Enable automatic conflict resolution
    pub enable_auto_conflict_resolution: bool,
    /// Enable diff-based synchronization
    pub enable_diff_sync: bool,
    /// Enable operator dashboard
    pub enable_operator_dashboard: bool,
    /// Enable manual sync triggers
    pub enable_manual_sync: bool,
    /// Enable compression for sync payloads
    pub enable_compression: bool,
    /// Enable metrics collection
    pub enable_metrics: bool,
    /// Enable audit logging
    pub enable_audit_logging: bool,
    /// NEW: Enable Cloudflare-specific synchronization
    pub enable_cloudflare_sync: bool,
    /// NEW: Enable cross-service data validation
    pub enable_cross_service_validation: bool,
    /// NEW: Enable automatic failover between services
    pub enable_automatic_failover: bool,
}

impl Default for SyncFeatureFlags {
    fn default() -> Self {
        Self {
            enable_write_through: true,
            enable_write_behind: true,
            enable_read_repair: true,
            enable_periodic_reconciliation: true,
            enable_vector_clocks: true,
            enable_auto_conflict_resolution: true,
            enable_diff_sync: true,
            enable_operator_dashboard: true,
            enable_manual_sync: true,
            enable_compression: true,
            enable_metrics: true,
            enable_audit_logging: true,
            enable_cloudflare_sync: true,
            enable_cross_service_validation: true,
            enable_automatic_failover: true,
        }
    }
}

impl Default for DataSynchronizationConfig {
    fn default() -> Self {
        Self {
            sync_coordinator: SyncCoordinatorConfig::default(),
            conflict_resolver: ConflictResolverConfig::default(),
            diff_engine: DiffEngineConfig::default(),
            operator_tools: OperatorToolsConfig::default(),
            validation: SyncValidationConfig::default(),
            feature_flags: SyncFeatureFlags::default(),
            cloudflare_sync: CloudflareSyncConfig::default(),
        }
    }
}

/// Main data synchronization engine
pub struct DataSynchronizationEngine {
    /// Sync coordinator
    sync_coordinator: Arc<SyncCoordinator>,
    /// Conflict resolver
    conflict_resolver: Arc<ConflictResolver>,
    /// Diff engine
    diff_engine: Arc<DiffEngine>,
    /// Operator tools
    operator_tools: Arc<OperatorToolsService>,
    /// Sync validator
    sync_validator: Arc<SyncValidator>,
    /// NEW: Cloudflare sync coordinator
    cloudflare_sync_coordinator: Option<Arc<CloudflareSyncCoordinator>>,
    /// Configuration
    config: DataSynchronizationConfig,
    /// Initialization status
    is_initialized: bool,
}

impl DataSynchronizationEngine {
    /// Create new data synchronization engine
    pub async fn new(env: &Env, config: DataSynchronizationConfig) -> ArbitrageResult<Self> {
        // Initialize sync coordinator
        let sync_coordinator = Arc::new(
            SyncCoordinator::new(env, &config.sync_coordinator, &config.feature_flags).await?,
        );

        // Initialize conflict resolver
        let conflict_resolver = Arc::new(
            ConflictResolver::new(&config.conflict_resolver, &config.feature_flags).await?,
        );

        // Initialize diff engine
        let diff_engine =
            Arc::new(DiffEngine::new(&config.diff_engine, &config.feature_flags).await?);

        // Initialize operator tools
        let operator_tools = Arc::new(
            OperatorToolsService::new(
                &config.operator_tools,
                &config.feature_flags,
                Arc::clone(&sync_coordinator),
            )
            .await?,
        );

        // Initialize sync validator
        let sync_validator =
            Arc::new(SyncValidator::new(&config.validation, &config.feature_flags).await?);

        // NEW: Initialize Cloudflare sync coordinator if enabled
        let cloudflare_sync_coordinator = if config.feature_flags.enable_cloudflare_sync {
            // Create circuit breaker for Cloudflare services
            let circuit_breaker = Arc::new(
                crate::services::core::infrastructure::circuit_breaker_service::CircuitBreakerService::new(
                    crate::services::core::infrastructure::circuit_breaker_service::CircuitBreakerConfig::default(),
                    env.kv("ArbEdgeKV")?,
                    env
                ).await?
            );

            let coordinator = CloudflareSyncCoordinator::new(
                env.clone(),
                config.cloudflare_sync.clone(),
                circuit_breaker,
            )
            .await?;

            Some(Arc::new(coordinator))
        } else {
            None
        };

        Ok(Self {
            sync_coordinator,
            conflict_resolver,
            diff_engine,
            operator_tools,
            sync_validator,
            cloudflare_sync_coordinator,
            config,
            is_initialized: false,
        })
    }

    /// Initialize the data synchronization engine
    pub async fn initialize(&mut self) -> ArbitrageResult<()> {
        if self.is_initialized {
            return Ok(());
        }

        // Initialize all components
        self.sync_coordinator.initialize().await?;
        self.conflict_resolver.initialize().await?;
        self.diff_engine.initialize().await?;
        self.operator_tools.initialize().await?;
        self.sync_validator.initialize().await?;

        // NEW: Initialize Cloudflare sync coordinator if available
        if let Some(cloudflare_coordinator) = &self.cloudflare_sync_coordinator {
            cloudflare_coordinator.initialize().await?;
        }

        self.is_initialized = true;
        Ok(())
    }

    /// Get sync coordinator
    pub fn sync_coordinator(&self) -> Arc<SyncCoordinator> {
        Arc::clone(&self.sync_coordinator)
    }

    /// Get conflict resolver
    pub fn conflict_resolver(&self) -> Arc<ConflictResolver> {
        Arc::clone(&self.conflict_resolver)
    }

    /// Get diff engine
    pub fn diff_engine(&self) -> Arc<DiffEngine> {
        Arc::clone(&self.diff_engine)
    }

    /// Get operator tools
    pub fn operator_tools(&self) -> Arc<OperatorToolsService> {
        Arc::clone(&self.operator_tools)
    }

    /// Get sync validator
    pub fn sync_validator(&self) -> Arc<SyncValidator> {
        Arc::clone(&self.sync_validator)
    }

    /// NEW: Get Cloudflare sync coordinator
    pub fn cloudflare_sync_coordinator(&self) -> Option<Arc<CloudflareSyncCoordinator>> {
        self.cloudflare_sync_coordinator.as_ref().map(Arc::clone)
    }

    /// NEW: Execute Cloudflare sync operation
    pub async fn execute_cloudflare_sync(
        &self,
        operation: cloudflare_sync_coordinator::SyncOperation,
    ) -> ArbitrageResult<cloudflare_sync_coordinator::SyncResult> {
        if let Some(coordinator) = &self.cloudflare_sync_coordinator {
            coordinator.sync_operation(operation).await
        } else {
            Err(ArbitrageError::configuration_error(
                "Cloudflare sync coordinator not enabled",
            ))
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<DataSyncHealth> {
        let coordinator_health = self.sync_coordinator.health_check().await?;
        let resolver_health = self.conflict_resolver.health_check().await?;
        let diff_health = self.diff_engine.health_check().await?;
        let operator_health = self.operator_tools.health_check().await?;
        let validator_health = self.sync_validator.health_check().await?;

        // NEW: Check Cloudflare sync coordinator health
        let cloudflare_health = if let Some(coordinator) = &self.cloudflare_sync_coordinator {
            Some(coordinator.health_check().await?)
        } else {
            None
        };

        let overall_healthy = coordinator_health.is_healthy
            && resolver_health.is_healthy
            && diff_health.is_healthy
            && operator_health.is_healthy
            && validator_health.is_healthy
            && cloudflare_health
                .as_ref()
                .map(|h| h.is_healthy)
                .unwrap_or(true);

        Ok(DataSyncHealth {
            overall_healthy,
            sync_coordinator_health:
                crate::services::core::infrastructure::shared_types::HealthCheckResult {
                    component: "sync_coordinator".to_string(),
                    is_healthy: coordinator_health.is_healthy,
                    response_time_ms: 0,
                    error_message: coordinator_health.last_error.clone(),
                    details: std::collections::HashMap::new(),
                    timestamp: coordinator_health.last_check,
                },
            conflict_resolver_health: resolver_health,
            diff_engine_health: diff_health,
            operator_tools_health: operator_health,
            sync_validator_health: validator_health,
            cloudflare_sync_health: cloudflare_health,
            last_check: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Get comprehensive metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<DataSyncMetrics> {
        let coordinator_metrics = self.sync_coordinator.get_metrics().await?;
        let resolver_metrics = self.conflict_resolver.get_metrics().await?;
        let diff_metrics = self.diff_engine.get_metrics().await?;
        let operator_metrics = self.operator_tools.get_metrics().await?;
        let validator_metrics = self.sync_validator.get_metrics().await?;

        // NEW: Get Cloudflare sync metrics
        let cloudflare_metrics = if let Some(coordinator) = &self.cloudflare_sync_coordinator {
            Some(coordinator.get_metrics().await)
        } else {
            None
        };

        Ok(DataSyncMetrics {
            sync_coordinator_metrics: coordinator_metrics,
            conflict_resolver_metrics: resolver_metrics,
            diff_engine_metrics: diff_metrics,
            operator_tools_metrics: operator_metrics,
            sync_validator_metrics: validator_metrics,
            cloudflare_sync_metrics: cloudflare_metrics,
            collected_at: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Check if engine is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Get configuration
    pub fn config(&self) -> &DataSynchronizationConfig {
        &self.config
    }

    /// Shutdown the engine gracefully
    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        if !self.is_initialized {
            return Ok(());
        }

        // Shutdown all components
        self.sync_coordinator.shutdown().await?;
        self.conflict_resolver.shutdown().await?;
        self.diff_engine.shutdown().await?;
        self.operator_tools.shutdown().await?;
        self.sync_validator.shutdown().await?;

        // NEW: Shutdown Cloudflare sync coordinator if available
        if let Some(coordinator) = &self.cloudflare_sync_coordinator {
            coordinator.shutdown().await?;
        }

        self.is_initialized = false;
        Ok(())
    }
}

/// Overall data synchronization health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSyncHealth {
    pub overall_healthy: bool,
    pub sync_coordinator_health:
        crate::services::core::infrastructure::shared_types::HealthCheckResult,
    pub conflict_resolver_health: ConflictResolverHealth,
    pub diff_engine_health: DiffEngineHealth,
    pub operator_tools_health: OperatorToolsHealth,
    pub sync_validator_health: SyncValidationHealth,
    /// NEW: Cloudflare sync coordinator health
    pub cloudflare_sync_health:
        Option<crate::services::core::infrastructure::shared_types::HealthCheckResult>,
    pub last_check: u64,
}

/// Comprehensive data synchronization metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSyncMetrics {
    pub sync_coordinator_metrics: SyncCoordinatorMetrics,
    pub conflict_resolver_metrics: ConflictResolverMetrics,
    pub diff_engine_metrics: DiffEngineMetrics,
    pub operator_tools_metrics: OperatorToolsMetrics,
    pub sync_validator_metrics: SyncValidationMetrics,
    /// NEW: Cloudflare sync coordinator metrics
    pub cloudflare_sync_metrics: Option<SyncMetrics>,
    pub collected_at: u64,
}
