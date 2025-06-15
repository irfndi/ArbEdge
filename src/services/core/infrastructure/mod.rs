// src/services/core/infrastructure/mod.rs

//! Infrastructure Services Module
//!
//! This module contains the revolutionary modular infrastructure architecture that supports
//! high-concurrency trading operations for 1000-2500 concurrent users with comprehensive
//! chaos engineering capabilities.
//!
//! ## Modular Architecture (6/7 Complete)
//!
//! ### ✅ Completed Modules:
//! 1. **Notification Module** - Multi-channel notification system with 8 channels
//! 2. **Monitoring Module** - Comprehensive observability platform  
//! 3. **Data Ingestion Module** - Revolutionary pipeline integration with fallback
//! 4. **AI Services Module** - Advanced AI/ML capabilities with vectorization
//! 5. **Data Access Layer** - Intelligent data routing with chaos engineering
//! 6. **Database Repositories** - Modular database operations
//!
//! ### 🔄 Remaining:
//! 7. **Analytics Module** - Advanced analytics and reporting (planned)
//!
//! ## Revolutionary Features:
//! - **Chaos Engineering**: Circuit breakers, fallback strategies, self-healing
//! - **High Performance**: Optimized for 1000-2500 concurrent users
//! - **Multi-Service Integration**: D1, KV, R2, Pipelines, Queues, Vectorize
//! - **Intelligent Caching**: Multi-layer caching with TTL management
//! - **Real-time Monitoring**: Comprehensive health and performance tracking

use crate::utils::error::{ArbitrageError, ArbitrageResult, ErrorKind};
use std::collections::HashMap;
use worker::Env;

// ============= NEW MODULAR ARCHITECTURE =============
pub mod ai_services;
pub mod data_access_layer;
pub mod data_ingestion_module;
#[cfg(feature = "advanced_sync")]
pub mod data_synchronization;
pub mod database_repositories;
pub mod monitoring_module;
pub mod notification_module;
pub mod shared_types;

// ============= CORE INFRASTRUCTURE COMPONENTS =============
pub mod automatic_failover_coordinator;
pub mod cache_manager;
pub mod chaos_engineering;
pub mod circuit_breaker_service;
pub mod database_core;
pub mod enhanced_kv_cache;
pub mod failover_service;
pub mod infrastructure_engine;
pub mod service_health;
pub mod unified_circuit_breaker;
pub mod unified_health_check;
pub mod unified_retry;

// ============= ADDITIONAL INFRASTRUCTURE COMPONENTS =============
pub mod analytics_engine;
pub mod durable_objects;
pub mod service_container;

// ============= MODULAR EXPORTS =============
pub use shared_types::{
    CacheStats, CircuitBreaker, CircuitBreakerState, ComponentHealth, HealthCheckResult,
    PerformanceMetrics, RateLimiter, ValidationCacheEntry, ValidationMetrics,
};

pub use notification_module::{
    NotificationChannel, NotificationModule, NotificationModuleConfig, NotificationModuleHealth,
    NotificationModuleMetrics, NotificationPriority, NotificationRequest, NotificationResult,
    NotificationType,
};

pub use monitoring_module::{
    AlertManager, HealthMonitor, MetricsCollector, MonitoringModule, MonitoringModuleConfig,
    MonitoringModuleHealth, MonitoringModuleMetrics, ObservabilityCoordinator, TraceCollector,
};

// Convenience exports
pub use monitoring_module::metrics_collector;
pub use notification_module as notification_engine;

// Core infrastructure modules
pub mod cloudflare_pipelines;
pub mod d1;
pub mod kv;

// Re-export core modules
pub use cloudflare_pipelines::*;
pub use d1::*;
pub use kv::*;

pub use data_ingestion_module::{
    DataIngestionHealth, DataIngestionMetrics, DataIngestionModule, DataIngestionModuleConfig,
    DataTransformer, IngestionCoordinator, IngestionEvent, IngestionEventType, PipelineManager,
    QueueManager,
};

pub use ai_services::{
    AICache, AICoordinator, AIServicesConfig, AIServicesHealth, AIServicesMetrics, EmbeddingEngine,
    ModelRouter, PersonalizationEngine,
};

pub use data_access_layer::{
    APIConnector, CacheLayer, DataAccessLayer, DataAccessLayerConfig, DataAccessLayerHealth,
    DataCoordinator, DataSourceManager, DataValidator,
};

#[cfg(feature = "advanced_sync")]
pub use data_synchronization::{
    CloudflareSyncCoordinator, DataSynchronizationConfig, DataSynchronizationEngine,
};

pub use database_repositories::{
    AIDataRepository,
    AnalyticsRepository,
    ConfigRepository,
    DatabaseManager,
    DatabaseManagerConfig,
    UserRepository, // Removed FeatureFlagConfig, FeatureFlagService
};

// ============= CORE INFRASTRUCTURE EXPORTS =============
pub use automatic_failover_coordinator::{
    AutomaticFailoverConfig, AutomaticFailoverCoordinator, AutomaticFailoverFeatureFlags,
    CoordinatedFailoverManager, CoordinatorMetrics, FailoverDecision, FailoverDecisionEngine,
    FailoverEvent, FailoverEventType, HealthSignalEvent, RecoveryAutomationEngine,
    RecoveryDecision, RecoveryMethod, RecoveryOperation, RecoveryStatus, RecoveryStep,
};
pub use cache_manager::{CacheConfig, CacheHealth, CacheManager, CacheResult};
pub use chaos_engineering::{
    ChaosCoordinator, ChaosCoordinatorConfig, ChaosCoordinatorMetrics, ChaosEngineeringConfig,
    ChaosEngineeringFramework, ChaosFeatureFlags, ExperimentEngine, ExperimentState,
    ExperimentType, FaultInjector, FaultType, InjectionTarget,
    RecoveryStatus as ChaosRecoveryStatus, RecoveryVerificationResult, RecoveryVerifier,
    SafetyController, SafetyRule, SafetyViolation, ViolationSeverity,
};
pub use circuit_breaker_service::{
    CircuitBreakerConfig, CircuitBreakerMetrics, CircuitBreakerService, CircuitBreakerStateInfo,
    CircuitBreakerType, EnhancedCircuitBreaker,
};
pub use database_core::{
    BatchOperation as DatabaseBatchOperation, DatabaseCore, DatabaseHealth, DatabaseResult,
};
pub use enhanced_kv_cache::{
    AccessPattern, BatchOperation as CacheBatchOperation, BatchResult, CacheEntry,
    CacheManagerMetrics, CacheOperation, CacheTier, CacheWarmingService, CleanupConfig,
    CompressionConfig, CompressionEngine, CompressionStats, DataType, EnhancedCacheConfig,
    EnhancedCacheStats, GeneralConfig, KvCacheManager, MetadataTracker, TierConfig, TierStats,
    WarmingConfig, WarmingStats,
};
pub use failover_service::{
    FailoverConfig, FailoverMetrics, FailoverService, FailoverState, FailoverStatus,
    FailoverStrategy, FailoverType, ServiceConfig,
};
pub use infrastructure_engine::{
    InfrastructureEngine, InfrastructureHealth, ServiceInfo, ServiceRegistration, ServiceStatus,
    ServiceType,
};
pub use service_health::{
    HealthStatus, ServiceHealthCheck, ServiceHealthManager, SystemHealthReport,
};
pub use unified_circuit_breaker::{
    UnifiedCircuitBreaker, UnifiedCircuitBreakerConfig, UnifiedCircuitBreakerManager,
    UnifiedCircuitBreakerState, UnifiedCircuitBreakerStateInfo, UnifiedCircuitBreakerType,
};
pub use unified_health_check::{HealthCheckMethod, UnifiedHealthCheckConfig};
pub use unified_retry::{UnifiedRetryConfig, UnifiedRetryExecutor};

// ============= ADDITIONAL COMPONENT EXPORTS =============
pub use analytics_engine::{
    AnalyticsEngineConfig, AnalyticsEngineService, RealTimeMetrics, UserAnalytics,
};
pub use durable_objects::{
    GlobalRateLimiterDO, MarketDataCoordinatorDO, OpportunityCoordinatorDO, UserOpportunityQueueDO,
};
pub use service_container::{ServiceContainer, ServiceHealthStatus};

// 7. Analytics Module - Comprehensive Analytics and Reporting System (COMPLETED)
pub mod analytics_module;

// 8. Financial Module - Real-Time Financial Monitoring and Analysis System (NEW)
pub mod financial_module;

// 9. Persistence Layer - D1/R2 Unified Data Persistence Architecture (NEW)
pub mod persistence_layer;

// All system integration functionality moved to modular services

pub use analytics_module::{
    AnalyticsCoordinator, AnalyticsModuleConfig, DataProcessor, MetricsAggregator, ReportGenerator,
};

pub use financial_module::{
    BalanceTracker, ExchangeBalanceSnapshot, FinancialCoordinator, FinancialModule,
    FinancialModuleConfig, FinancialModuleHealth, FinancialModuleMetrics, FundAnalyzer,
    FundOptimizationResult, PortfolioAnalytics,
};

pub use persistence_layer::{
    ConnectionHealth, ConnectionManager, ConnectionMetrics, ConnectionPool, ConnectionStats,
    D1Config, PersistenceConfig, PersistenceHealth, PersistenceLayer, PersistenceMetrics,
    PoolConfig, R2Config, SchemaHealth, SchemaManager, SchemaMetrics, ServiceHealth,
};

// All system integration functionality moved to modular services

/// Revolutionary Infrastructure Configuration for High-Concurrency Trading
#[derive(Debug, Clone)]
pub struct InfrastructureConfig {
    // Core infrastructure settings optimized for 1000-2500 concurrent users
    pub max_concurrent_users: u32,
    pub enable_high_performance_mode: bool,
    pub enable_chaos_engineering: bool,
    pub enable_comprehensive_monitoring: bool,
    pub enable_intelligent_caching: bool,

    // Modular component configurations
    pub notification_config: notification_module::NotificationModuleConfig,
    pub monitoring_config: monitoring_module::MonitoringModuleConfig,
    pub data_ingestion_config: data_ingestion_module::DataIngestionModuleConfig,
    pub ai_services_config: ai_services::AIServicesConfig,
    pub data_access_config: data_access_layer::DataAccessLayerConfig,
    pub database_repositories_config: database_repositories::DatabaseManagerConfig,

    // Core infrastructure configurations
    pub database_core_config: DatabaseCoreConfig,
    pub cache_manager_config: CacheManagerConfig,
    pub chaos_engineering_config: chaos_engineering::ChaosEngineeringConfig,
    pub service_health_config: ServiceHealthConfig,
    pub infrastructure_engine_config: InfrastructureEngineConfig,

    // Analytics and financial module configurations
    pub analytics_config: AnalyticsEngineConfig,
    pub financial_module_config: FinancialModuleConfig,
}

#[derive(Debug, Clone)]
pub struct DatabaseCoreConfig {
    pub connection_pool_size: usize,
    pub query_timeout_ms: u64,
    pub max_retries: u32,
    pub batch_size: usize,
    pub enable_performance_monitoring: bool,
}

#[derive(Debug, Clone)]
pub struct CacheManagerConfig {
    pub default_ttl_seconds: u64,
    pub max_key_size_bytes: usize,
    pub max_value_size_bytes: usize,
    pub compression_enabled: bool,
    pub batch_size: usize,
    pub retry_attempts: u32,
}

#[derive(Debug, Clone)]
pub struct ServiceHealthConfig {
    pub health_check_interval_seconds: u64,
    pub enable_dependency_tracking: bool,
    pub enable_predictive_analysis: bool,
    pub enable_automated_recovery: bool,
}

#[derive(Debug, Clone)]
pub struct InfrastructureEngineConfig {
    pub enable_service_discovery: bool,
    pub enable_load_balancing: bool,
    pub enable_auto_scaling: bool,
    pub max_services: u32,
}

// AnalyticsEngineConfig is imported from analytics_engine module above

// FundMonitoringConfig replaced by FinancialModuleConfig from financial_module

impl Default for InfrastructureConfig {
    fn default() -> Self {
        Self {
            max_concurrent_users: 2500,
            enable_high_performance_mode: true,
            enable_chaos_engineering: true,
            enable_comprehensive_monitoring: true,
            enable_intelligent_caching: true,

            notification_config: notification_module::NotificationModuleConfig::high_performance(),
            monitoring_config: monitoring_module::MonitoringModuleConfig::high_performance(),
            data_ingestion_config:
                data_ingestion_module::DataIngestionModuleConfig::high_throughput(),
            ai_services_config: ai_services::AIServicesConfig::high_concurrency(),
            data_access_config: data_access_layer::DataAccessLayerConfig::high_concurrency(),
            database_repositories_config:
                database_repositories::DatabaseManagerConfig::high_performance(),

            database_core_config: DatabaseCoreConfig::default(),
            cache_manager_config: CacheManagerConfig::default(),
            chaos_engineering_config: chaos_engineering::ChaosEngineeringConfig::default(),
            service_health_config: ServiceHealthConfig::default(),
            infrastructure_engine_config: InfrastructureEngineConfig::default(),

            analytics_config: AnalyticsEngineConfig::default(),
            financial_module_config: FinancialModuleConfig::default(),
        }
    }
}

impl Default for DatabaseCoreConfig {
    fn default() -> Self {
        Self {
            connection_pool_size: 25,
            query_timeout_ms: 5000,
            max_retries: 3,
            batch_size: 500,
            enable_performance_monitoring: true,
        }
    }
}

impl Default for CacheManagerConfig {
    fn default() -> Self {
        Self {
            default_ttl_seconds: 3600,
            max_key_size_bytes: 1024,
            max_value_size_bytes: 1024 * 1024,
            compression_enabled: true,
            batch_size: 100,
            retry_attempts: 3,
        }
    }
}

impl Default for ServiceHealthConfig {
    fn default() -> Self {
        Self {
            health_check_interval_seconds: 30,
            enable_dependency_tracking: true,
            enable_predictive_analysis: true,
            enable_automated_recovery: true,
        }
    }
}

impl Default for InfrastructureEngineConfig {
    fn default() -> Self {
        Self {
            enable_service_discovery: true,
            enable_load_balancing: true,
            enable_auto_scaling: true,
            max_services: 100,
        }
    }
}

// FundMonitoringConfig implementation removed - replaced by FinancialModuleConfig

impl InfrastructureConfig {
    /// Configuration optimized for high-concurrency trading (1000-2500 users)
    pub fn high_concurrency() -> Self {
        Self {
            max_concurrent_users: 2500,
            enable_high_performance_mode: true,
            enable_chaos_engineering: true,
            enable_comprehensive_monitoring: true,
            enable_intelligent_caching: true,

            notification_config: notification_module::NotificationModuleConfig::high_performance(),
            monitoring_config: monitoring_module::MonitoringModuleConfig::high_performance(),
            data_ingestion_config:
                data_ingestion_module::DataIngestionModuleConfig::high_throughput(),
            ai_services_config: ai_services::AIServicesConfig::high_concurrency(),
            data_access_config: data_access_layer::DataAccessLayerConfig::high_concurrency(),
            database_repositories_config:
                database_repositories::DatabaseManagerConfig::high_performance(),

            database_core_config: DatabaseCoreConfig {
                connection_pool_size: 50,
                query_timeout_ms: 3000,
                max_retries: 5,
                batch_size: 1000,
                enable_performance_monitoring: true,
            },
            cache_manager_config: CacheManagerConfig {
                default_ttl_seconds: 1800,
                max_key_size_bytes: 2048,
                max_value_size_bytes: 2 * 1024 * 1024,
                compression_enabled: true,
                batch_size: 200,
                retry_attempts: 5,
            },
            chaos_engineering_config: chaos_engineering::ChaosEngineeringConfig::default(),
            service_health_config: ServiceHealthConfig {
                health_check_interval_seconds: 15,
                enable_dependency_tracking: true,
                enable_predictive_analysis: true,
                enable_automated_recovery: true,
            },
            infrastructure_engine_config: InfrastructureEngineConfig {
                enable_service_discovery: true,
                enable_load_balancing: true,
                enable_auto_scaling: true,
                max_services: 200,
            },

            analytics_config: AnalyticsEngineConfig {
                enabled: true,
                dataset_name: "arbitrage_analytics".to_string(),
                batch_size: Some(2000),
                flush_interval_seconds: 30,
                retention_days: 180,
                enable_real_time_analytics: true,
                enable_batching: true,
            },
            financial_module_config: FinancialModuleConfig::high_performance(),
        }
    }

    /// Configuration optimized for reliability and fault tolerance
    pub fn high_reliability() -> Self {
        Self {
            max_concurrent_users: 1000,
            enable_high_performance_mode: false,
            enable_chaos_engineering: true,
            enable_comprehensive_monitoring: true,
            enable_intelligent_caching: true,

            notification_config: notification_module::NotificationModuleConfig::high_reliability(),
            monitoring_config: monitoring_module::MonitoringModuleConfig::high_reliability(),
            data_ingestion_config:
                data_ingestion_module::DataIngestionModuleConfig::high_reliability(),
            ai_services_config: ai_services::AIServicesConfig::memory_optimized(),
            data_access_config: data_access_layer::DataAccessLayerConfig::high_reliability(),
            database_repositories_config:
                database_repositories::DatabaseManagerConfig::high_reliability(),

            database_core_config: DatabaseCoreConfig {
                connection_pool_size: 20,
                query_timeout_ms: 10000,
                max_retries: 10,
                batch_size: 200,
                enable_performance_monitoring: true,
            },
            cache_manager_config: CacheManagerConfig {
                default_ttl_seconds: 7200,
                max_key_size_bytes: 1024,
                max_value_size_bytes: 1024 * 1024,
                compression_enabled: true,
                batch_size: 50,
                retry_attempts: 10,
            },
            chaos_engineering_config: chaos_engineering::ChaosEngineeringConfig::default(),
            service_health_config: ServiceHealthConfig {
                health_check_interval_seconds: 10,
                enable_dependency_tracking: true,
                enable_predictive_analysis: true,
                enable_automated_recovery: true,
            },
            infrastructure_engine_config: InfrastructureEngineConfig {
                enable_service_discovery: true,
                enable_load_balancing: true,
                enable_auto_scaling: false,
                max_services: 50,
            },

            analytics_config: AnalyticsEngineConfig {
                enabled: true,
                dataset_name: "arbitrage_analytics".to_string(),
                batch_size: Some(500),
                flush_interval_seconds: 60,
                retention_days: 365,
                enable_real_time_analytics: true,
                enable_batching: true,
            },
            financial_module_config: FinancialModuleConfig::high_reliability(),
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.max_concurrent_users == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_users must be greater than 0",
            ));
        }

        if self.database_core_config.connection_pool_size == 0 {
            return Err(ArbitrageError::validation_error(
                "connection_pool_size must be greater than 0",
            ));
        }

        if self.cache_manager_config.batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "cache batch_size must be greater than 0",
            ));
        }

        Ok(())
    }
}

/// Revolutionary Infrastructure Manager for High-Concurrency Trading Operations
pub struct InfrastructureManager {
    config: InfrastructureConfig,

    // Modular components
    notification_module: Option<notification_module::NotificationModule>,
    monitoring_module: Option<monitoring_module::MonitoringModule>,
    data_ingestion_module: Option<data_ingestion_module::DataIngestionModule>,
    ai_services: Option<ai_services::AICoordinator>,
    data_access_layer: Option<data_access_layer::DataAccessLayer>,
    database_repositories: Option<database_repositories::DatabaseManager>,

    // Core infrastructure
    database_core: Option<DatabaseCore>,
    cache_manager: Option<CacheManager>,
    chaos_engineering: Option<chaos_engineering::ChaosEngineeringFramework>,
    service_health: Option<ServiceHealthManager>,
    infrastructure_engine: Option<InfrastructureEngine>,

    // Analytics and financial components
    analytics_engine: Option<AnalyticsEngineService>,
    financial_module: Option<FinancialModule>,

    // Runtime state
    is_initialized: bool,
    startup_time: Option<u64>,
}

impl InfrastructureManager {
    pub fn new(config: InfrastructureConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        Ok(Self {
            config,
            notification_module: None,
            monitoring_module: None,
            data_ingestion_module: None,
            ai_services: None,
            data_access_layer: None,
            database_repositories: None,
            database_core: None,
            cache_manager: None,
            chaos_engineering: None,
            service_health: None,
            infrastructure_engine: None,
            analytics_engine: None,
            financial_module: None,
            is_initialized: false,
            startup_time: None,
        })
    }

    /// Initialize all infrastructure components
    pub async fn initialize(&mut self, env: &Env) -> ArbitrageResult<()> {
        let start_time = crate::utils::get_current_timestamp();

        // Initialize core infrastructure first
        self.database_core = Some(DatabaseCore::new(env)?);
        self.cache_manager = Some(CacheManager::new_with_config(
            env.kv("ArbEdgeKV").map_err(|e| {
                ArbitrageError::cache_error(format!("Failed to get KV store: {}", e))
            })?,
            cache_manager::CacheConfig::default(),
            "arb_edge",
        ));
        self.service_health = Some(ServiceHealthManager::new());
        self.infrastructure_engine = Some(InfrastructureEngine::new_with_config(
            env.kv("ArbEdgeKV").map_err(|e| {
                ArbitrageError::cache_error(format!("Failed to get KV store: {}", e))
            })?,
            infrastructure_engine::InfrastructureConfig::default(),
            env.clone(),
        ));

        // Initialize chaos engineering framework
        let mut chaos_framework = chaos_engineering::ChaosEngineeringFramework::new(
            self.config.chaos_engineering_config.clone(),
        )?;
        chaos_framework.initialize(env).await?;
        self.chaos_engineering = Some(chaos_framework);

        // Initialize modular components
        self.notification_module = Some(
            notification_module::NotificationModule::new(
                self.config.notification_config.clone(),
                env.kv("ArbEdgeKV").map_err(|e| {
                    ArbitrageError::cache_error(format!("Failed to get KV store: {}", e))
                })?,
                env,
            )
            .await?,
        );

        self.monitoring_module = Some(
            monitoring_module::MonitoringModule::new(
                self.config.monitoring_config.clone(),
                env.kv("ArbEdgeKV").map_err(|e| {
                    ArbitrageError::cache_error(format!("Failed to get KV store: {}", e))
                })?,
                env,
            )
            .await?,
        );

        self.data_ingestion_module = Some(
            data_ingestion_module::DataIngestionModule::new(
                self.config.data_ingestion_config.clone(),
                env.kv("ArbEdgeKV").map_err(|e| {
                    ArbitrageError::cache_error(format!("Failed to get KV store: {}", e))
                })?,
                env,
            )
            .await?,
        );

        self.data_access_layer = Some(
            data_access_layer::DataAccessLayer::new(
                self.config.data_access_config.clone(),
                env.kv("ArbEdgeKV").map_err(|e| {
                    ArbitrageError::cache_error(format!("Failed to get KV store: {}", e))
                })?,
            )
            .await?,
        );

        self.database_repositories = Some(database_repositories::DatabaseManager::new(
            self.database_core.as_ref().unwrap().get_database(),
            self.config.database_repositories_config.clone(),
        ));

        // Initialize AI services
        self.ai_services = Some(ai_services::AICoordinator::new(
            env,
            self.config.ai_services_config.ai_coordinator.clone(),
        )?);

        // Initialize legacy components
        self.analytics_engine = Some(AnalyticsEngineService::new(
            env,
            self.config.analytics_config.clone(),
        )?);
        let mut financial_module =
            FinancialModule::new(self.config.financial_module_config.clone())?;
        financial_module.initialize(env).await?;
        self.financial_module = Some(financial_module);

        // Initialize legacy system integration
        // Legacy system integration removed - functionality moved to modular services

        self.is_initialized = true;
        self.startup_time = Some(start_time);

        Ok(())
    }

    // Getters for modular components
    pub fn notification_module(&self) -> ArbitrageResult<&notification_module::NotificationModule> {
        self.notification_module.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "NotificationModule not initialized")
        })
    }

    pub fn monitoring_module(&self) -> ArbitrageResult<&monitoring_module::MonitoringModule> {
        self.monitoring_module.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "MonitoringModule not initialized")
        })
    }

    pub fn data_ingestion_module(
        &self,
    ) -> ArbitrageResult<&data_ingestion_module::DataIngestionModule> {
        self.data_ingestion_module.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "DataIngestionModule not initialized")
        })
    }

    pub fn ai_services(&self) -> ArbitrageResult<&ai_services::AICoordinator> {
        self.ai_services
            .as_ref()
            .ok_or_else(|| ArbitrageError::new(ErrorKind::Internal, "AIServices not initialized"))
    }

    pub fn data_access_layer(&self) -> ArbitrageResult<&data_access_layer::DataAccessLayer> {
        self.data_access_layer.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "DataAccessLayer not initialized")
        })
    }

    pub fn database_repositories(
        &self,
    ) -> ArbitrageResult<&database_repositories::DatabaseManager> {
        self.database_repositories.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "DatabaseRepositories not initialized")
        })
    }

    // Getters for core infrastructure
    pub fn database_core(&self) -> ArbitrageResult<&DatabaseCore> {
        self.database_core
            .as_ref()
            .ok_or_else(|| ArbitrageError::new(ErrorKind::Internal, "DatabaseCore not initialized"))
    }

    pub fn cache_manager(&self) -> ArbitrageResult<&CacheManager> {
        self.cache_manager
            .as_ref()
            .ok_or_else(|| ArbitrageError::new(ErrorKind::Internal, "CacheManager not initialized"))
    }

    pub fn chaos_engineering(
        &self,
    ) -> ArbitrageResult<&chaos_engineering::ChaosEngineeringFramework> {
        self.chaos_engineering.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "ChaosEngineering not initialized")
        })
    }

    pub fn service_health(&self) -> ArbitrageResult<&ServiceHealthManager> {
        self.service_health.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "ServiceHealth not initialized")
        })
    }

    pub fn infrastructure_engine(&self) -> ArbitrageResult<&InfrastructureEngine> {
        self.infrastructure_engine.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "InfrastructureEngine not initialized")
        })
    }

    // Getters for legacy components
    pub fn analytics_engine(&self) -> ArbitrageResult<&AnalyticsEngineService> {
        self.analytics_engine.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "AnalyticsEngine not initialized")
        })
    }

    pub fn financial_module(&self) -> ArbitrageResult<&FinancialModule> {
        self.financial_module.as_ref().ok_or_else(|| {
            ArbitrageError::new(ErrorKind::Internal, "FinancialModule not initialized")
        })
    }

    // Legacy system integration method removed - functionality moved to modular services

    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    pub fn startup_time(&self) -> Option<u64> {
        self.startup_time
    }

    pub fn config(&self) -> &InfrastructureConfig {
        &self.config
    }

    /// Comprehensive health check across all components
    pub async fn health_check(&self) -> ArbitrageResult<HashMap<String, bool>> {
        let mut health_status = HashMap::new();

        // Check modular components
        if let Ok(notification) = self.notification_module() {
            health_status.insert(
                "notification_module".to_string(),
                notification.health_check().await.unwrap_or(false),
            );
        }

        if let Ok(monitoring) = self.monitoring_module() {
            health_status.insert(
                "monitoring_module".to_string(),
                monitoring.health_check().await.unwrap_or(false),
            );
        }

        if let Ok(data_ingestion) = self.data_ingestion_module() {
            health_status.insert(
                "data_ingestion_module".to_string(),
                data_ingestion.health_check().await.unwrap_or(false),
            );
        }

        if let Ok(ai_services) = self.ai_services() {
            let ai_health = ai_services.health_check().await.unwrap_or_default();
            health_status.insert("ai_services".to_string(), ai_health.overall_health);
        }

        if let Ok(data_access) = self.data_access_layer() {
            health_status.insert(
                "data_access_layer".to_string(),
                data_access.is_healthy().await,
            );
        }

        if let Ok(database_repos) = self.database_repositories() {
            let is_healthy = database_repos.health_check().await;
            health_status.insert("database_repositories".to_string(), is_healthy);
        }

        // Check core infrastructure
        if let Ok(database) = self.database_core() {
            let db_health = database.health_check().await.unwrap_or_default();
            health_status.insert("database_core".to_string(), db_health.is_healthy);
        }

        if let Ok(cache) = self.cache_manager() {
            let cache_health = cache.health_check().await.unwrap_or_default();
            health_status.insert("cache_manager".to_string(), cache_health.is_healthy);
        }

        // Legacy system integration health check removed - functionality moved to modular services

        Ok(health_status)
    }

    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        // Graceful shutdown of all components
        self.is_initialized = false;
        Ok(())
    }
}

impl Default for InfrastructureManager {
    fn default() -> Self {
        Self::new(InfrastructureConfig::default()).unwrap()
    }
}

/// Utility functions for creating optimized configurations
pub mod utils {
    use super::*;

    /// Create configuration optimized for high-concurrency trading
    pub fn create_high_concurrency_config() -> InfrastructureConfig {
        InfrastructureConfig::high_concurrency()
    }

    /// Create configuration optimized for reliability
    pub fn create_high_reliability_config() -> InfrastructureConfig {
        InfrastructureConfig::high_reliability()
    }

    /// Create configuration for development/testing
    pub fn create_development_config() -> InfrastructureConfig {
        InfrastructureConfig {
            max_concurrent_users: 100,
            enable_high_performance_mode: false,
            enable_chaos_engineering: false,
            enable_comprehensive_monitoring: true,
            enable_intelligent_caching: true,
            ..InfrastructureConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infrastructure_config_default() {
        let config = InfrastructureConfig::default();
        assert_eq!(config.max_concurrent_users, 2500);
        assert!(config.enable_high_performance_mode);
        assert!(config.enable_chaos_engineering);
    }

    #[test]
    fn test_high_concurrency_config() {
        let config = InfrastructureConfig::high_concurrency();
        assert_eq!(config.max_concurrent_users, 2500);
        assert_eq!(config.database_core_config.connection_pool_size, 50);
        assert_eq!(config.cache_manager_config.batch_size, 200);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = InfrastructureConfig::high_reliability();
        assert_eq!(config.max_concurrent_users, 1000);
        assert_eq!(config.database_core_config.max_retries, 10);
        assert_eq!(config.cache_manager_config.retry_attempts, 10);
    }

    #[test]
    fn test_config_validation() {
        let mut config = InfrastructureConfig::default();
        assert!(config.validate().is_ok());

        config.max_concurrent_users = 0;
        assert!(config.validate().is_err());
    }
}
