// Infrastructure Engine Module - Main Orchestrator for All Infrastructure Services
// Provides service discovery, dependency management, configuration, and health monitoring

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use worker::kv::KvStore;

use super::{
    database_core::{DatabaseCore, DatabaseConfig},
    cache_manager::{CacheManager, CacheConfig},
    service_health::{ServiceHealth, HealthConfig},
    notification_engine::{NotificationEngine, NotificationEngineConfig},
    data_access_layer::{DataAccessLayer, DataAccessConfig},
    metrics_collector::{MetricsCollector, MetricsConfig},
};

/// Service types in the infrastructure
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    Database,
    Cache,
    Health,
    Notification,
    DataAccess,
    Metrics,
    External,
    Custom(String),
}

impl ServiceType {
    pub fn as_str(&self) -> &str {
        match self {
            ServiceType::Database => "database",
            ServiceType::Cache => "cache",
            ServiceType::Health => "health",
            ServiceType::Notification => "notification",
            ServiceType::DataAccess => "data_access",
            ServiceType::Metrics => "metrics",
            ServiceType::External => "external",
            ServiceType::Custom(name) => name,
        }
    }
}

/// Service status for monitoring
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
    Initializing,
    Stopped,
}

impl ServiceStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ServiceStatus::Healthy => "healthy",
            ServiceStatus::Degraded => "degraded",
            ServiceStatus::Unhealthy => "unhealthy",
            ServiceStatus::Unknown => "unknown",
            ServiceStatus::Initializing => "initializing",
            ServiceStatus::Stopped => "stopped",
        }
    }

    pub fn is_operational(&self) -> bool {
        matches!(self, ServiceStatus::Healthy | ServiceStatus::Degraded)
    }
}

/// Service dependency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDependency {
    pub service_name: String,
    pub service_type: ServiceType,
    pub is_critical: bool,
    pub timeout_ms: u64,
    pub retry_attempts: u32,
    pub health_check_interval_seconds: u64,
}

/// Service registration information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistration {
    pub service_name: String,
    pub service_type: ServiceType,
    pub version: String,
    pub description: String,
    pub dependencies: Vec<ServiceDependency>,
    pub health_check_endpoint: Option<String>,
    pub metrics_enabled: bool,
    pub auto_recovery: bool,
    pub priority: u8, // 1 = highest priority, 10 = lowest
    pub tags: HashMap<String, String>,
    pub configuration: HashMap<String, serde_json::Value>,
}

/// Service runtime information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub registration: ServiceRegistration,
    pub status: ServiceStatus,
    pub last_health_check: Option<u64>,
    pub uptime_seconds: u64,
    pub error_count: u64,
    pub restart_count: u64,
    pub last_error: Option<String>,
    pub performance_metrics: HashMap<String, f64>,
}

/// Infrastructure configuration
#[derive(Debug, Clone)]
pub struct InfrastructureConfig {
    pub enable_service_discovery: bool,
    pub enable_health_monitoring: bool,
    pub enable_auto_recovery: bool,
    pub enable_metrics_collection: bool,
    pub health_check_interval_seconds: u64,
    pub service_timeout_seconds: u64,
    pub max_restart_attempts: u32,
    pub restart_backoff_multiplier: f64,
    pub enable_circuit_breaker: bool,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_seconds: u64,
    pub enable_load_balancing: bool,
    pub enable_graceful_shutdown: bool,
    pub shutdown_timeout_seconds: u64,
}

impl Default for InfrastructureConfig {
    fn default() -> Self {
        Self {
            enable_service_discovery: true,
            enable_health_monitoring: true,
            enable_auto_recovery: true,
            enable_metrics_collection: true,
            health_check_interval_seconds: 30,
            service_timeout_seconds: 30,
            max_restart_attempts: 3,
            restart_backoff_multiplier: 2.0,
            enable_circuit_breaker: true,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_seconds: 60,
            enable_load_balancing: false, // Not needed for single instance
            enable_graceful_shutdown: true,
            shutdown_timeout_seconds: 30,
        }
    }
}

/// Circuit breaker state for service protection
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitBreakerState {
    Closed,  // Normal operation
    Open,    // Failing, requests blocked
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreaker {
    pub service_name: String,
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub last_failure_time: Option<u64>,
    pub next_attempt_time: Option<u64>,
    pub success_count: u32,
    pub threshold: u32,
    pub timeout_seconds: u64,
}

/// Infrastructure health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfrastructureHealth {
    pub overall_status: ServiceStatus,
    pub healthy_services: u32,
    pub degraded_services: u32,
    pub unhealthy_services: u32,
    pub total_services: u32,
    pub critical_services_healthy: bool,
    pub uptime_percentage: f64,
    pub last_updated: u64,
    pub service_statuses: HashMap<String, ServiceStatus>,
}

/// Main infrastructure engine
pub struct InfrastructureEngine {
    config: InfrastructureConfig,
    kv_store: KvStore,
    
    // Core infrastructure services
    database_core: Option<DatabaseCore>,
    cache_manager: Option<CacheManager>,
    service_health: Option<ServiceHealth>,
    notification_engine: Option<NotificationEngine>,
    data_access_layer: Option<DataAccessLayer>,
    metrics_collector: Option<MetricsCollector>,
    
    // Service management with async-aware mutexes
    services: Arc<Mutex<HashMap<String, ServiceInfo>>>,
    circuit_breakers: Arc<Mutex<HashMap<String, CircuitBreaker>>>,
    startup_time: u64,
    
    // Configuration management with async-aware mutex
    global_config: Arc<Mutex<HashMap<String, serde_json::Value>>>,
}

impl InfrastructureEngine {
    /// Create new InfrastructureEngine with default configuration
    pub fn new(kv_store: KvStore) -> Self {
        Self {
            config: InfrastructureConfig::default(),
            kv_store,
            database_core: None,
            cache_manager: None,
            service_health: None,
            notification_engine: None,
            data_access_layer: None,
            metrics_collector: None,
            services: Arc::new(Mutex::new(HashMap::new())),
            circuit_breakers: Arc::new(Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
            global_config: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create InfrastructureEngine with custom configuration
    pub fn new_with_config(kv_store: KvStore, config: InfrastructureConfig) -> Self {
        Self {
            config,
            kv_store,
            database_core: None,
            cache_manager: None,
            service_health: None,
            notification_engine: None,
            data_access_layer: None,
            metrics_collector: None,
            services: Arc::new(Mutex::new(HashMap::new())),
            circuit_breakers: Arc::new(Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
            global_config: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Initialize all infrastructure services
    pub async fn initialize(&mut self) -> ArbitrageResult<()> {
        // Initialize core services in dependency order
        
        // 1. Initialize metrics collector first (needed by other services)
        if self.config.enable_metrics_collection {
            let metrics_config = MetricsConfig::default();
            self.metrics_collector = Some(MetricsCollector::new_with_config(self.kv_store.clone(), metrics_config));
            self.register_service(ServiceRegistration {
                service_name: "metrics_collector".to_string(),
                service_type: ServiceType::Metrics,
                version: "1.0.0".to_string(),
                description: "Centralized metrics collection and analytics".to_string(),
                dependencies: vec![],
                health_check_endpoint: None,
                metrics_enabled: false, // Avoid circular dependency
                auto_recovery: true,
                priority: 1,
                tags: HashMap::new(),
                configuration: HashMap::new(),
            }).await?;
        }

        // 2. Initialize database core
        let database_config = DatabaseConfig::default();
        self.database_core = Some(DatabaseCore::new_with_config(self.kv_store.clone(), database_config));
        self.register_service(ServiceRegistration {
            service_name: "database_core".to_string(),
            service_type: ServiceType::Database,
            version: "1.0.0".to_string(),
            description: "Unified database operations with connection pooling".to_string(),
            dependencies: vec![],
            health_check_endpoint: None,
            metrics_enabled: true,
            auto_recovery: true,
            priority: 1,
            tags: HashMap::new(),
            configuration: HashMap::new(),
        }).await?;

        // 3. Initialize cache manager
        let cache_config = CacheConfig::default();
        self.cache_manager = Some(CacheManager::new_with_config(self.kv_store.clone(), cache_config));
        self.register_service(ServiceRegistration {
            service_name: "cache_manager".to_string(),
            service_type: ServiceType::Cache,
            version: "1.0.0".to_string(),
            description: "Centralized KV operations with intelligent TTL management".to_string(),
            dependencies: vec![],
            health_check_endpoint: None,
            metrics_enabled: true,
            auto_recovery: true,
            priority: 1,
            tags: HashMap::new(),
            configuration: HashMap::new(),
        }).await?;

        // 4. Initialize service health monitor
        if self.config.enable_health_monitoring {
            let health_config = HealthConfig::default();
            self.service_health = Some(ServiceHealth::new_with_config(self.kv_store.clone(), health_config));
            self.register_service(ServiceRegistration {
                service_name: "service_health".to_string(),
                service_type: ServiceType::Health,
                version: "1.0.0".to_string(),
                description: "Comprehensive system health reporting with dependency tracking".to_string(),
                dependencies: vec![
                    ServiceDependency {
                        service_name: "database_core".to_string(),
                        service_type: ServiceType::Database,
                        is_critical: true,
                        timeout_ms: 5000,
                        retry_attempts: 3,
                        health_check_interval_seconds: 30,
                    },
                    ServiceDependency {
                        service_name: "cache_manager".to_string(),
                        service_type: ServiceType::Cache,
                        is_critical: true,
                        timeout_ms: 2000,
                        retry_attempts: 3,
                        health_check_interval_seconds: 30,
                    },
                ],
                health_check_endpoint: None,
                metrics_enabled: true,
                auto_recovery: true,
                priority: 2,
                tags: HashMap::new(),
                configuration: HashMap::new(),
            }).await?;
        }

        // 5. Initialize notification engine
        let notification_config = NotificationEngineConfig::default();
        self.notification_engine = Some(NotificationEngine::new_with_config(
            self.kv_store.clone(),
            notification_config,
            Default::default(),
        ));
        self.register_service(ServiceRegistration {
            service_name: "notification_engine".to_string(),
            service_type: ServiceType::Notification,
            version: "1.0.0".to_string(),
            description: "Centralized notification delivery and template management".to_string(),
            dependencies: vec![
                ServiceDependency {
                    service_name: "cache_manager".to_string(),
                    service_type: ServiceType::Cache,
                    is_critical: false,
                    timeout_ms: 2000,
                    retry_attempts: 2,
                    health_check_interval_seconds: 60,
                },
            ],
            health_check_endpoint: None,
            metrics_enabled: true,
            auto_recovery: true,
            priority: 3,
            tags: HashMap::new(),
            configuration: HashMap::new(),
        }).await?;

        // 6. Initialize data access layer
        let data_access_config = DataAccessConfig::default();
        self.data_access_layer = Some(DataAccessLayer::new_with_config(self.kv_store.clone(), data_access_config));
        self.register_service(ServiceRegistration {
            service_name: "data_access_layer".to_string(),
            service_type: ServiceType::DataAccess,
            version: "1.0.0".to_string(),
            description: "Unified data access patterns with Pipeline → KV → API fallback".to_string(),
            dependencies: vec![
                ServiceDependency {
                    service_name: "cache_manager".to_string(),
                    service_type: ServiceType::Cache,
                    is_critical: true,
                    timeout_ms: 2000,
                    retry_attempts: 3,
                    health_check_interval_seconds: 30,
                },
                ServiceDependency {
                    service_name: "database_core".to_string(),
                    service_type: ServiceType::Database,
                    is_critical: false,
                    timeout_ms: 5000,
                    retry_attempts: 2,
                    health_check_interval_seconds: 60,
                },
            ],
            health_check_endpoint: None,
            metrics_enabled: true,
            auto_recovery: true,
            priority: 3,
            tags: HashMap::new(),
            configuration: HashMap::new(),
        }).await?;

        // Start health monitoring if enabled
        if self.config.enable_health_monitoring {
            self.start_health_monitoring().await?;
        }

        Ok(())
    }

    /// Register a service with the infrastructure engine
    pub async fn register_service(&self, registration: ServiceRegistration) -> ArbitrageResult<()> {
        if !self.config.enable_service_discovery {
            return Ok(());
        }

        let service_info = ServiceInfo {
            registration,
            status: ServiceStatus::Initializing,
            last_health_check: None,
            uptime_seconds: 0,
            error_count: 0,
            restart_count: 0,
            last_error: None,
            performance_metrics: HashMap::new(),
        };

        let mut services = self.services.lock().await;
        services.insert(service_info.registration.service_name.clone(), service_info);
        drop(services);

        // Initialize circuit breaker if enabled
        if self.config.enable_circuit_breaker {
            self.initialize_circuit_breaker(&service_info.registration.service_name).await?;
        }

        Ok(())
    }

    /// Get service information
    pub async fn get_service_info(&self, service_name: &str) -> Option<ServiceInfo> {
        let services = self.services.lock().await;
        services.get(service_name).cloned()
    }

    /// Get all registered services
    pub async fn get_all_services(&self) -> HashMap<String, ServiceInfo> {
        let services = self.services.lock().await;
        services.clone()
    }

    /// Get infrastructure health summary
    pub async fn get_infrastructure_health(&self) -> InfrastructureHealth {
        let services = self.services.lock().await;
        let services_clone = services.clone();
        drop(services);
        
        let mut healthy_count = 0;
        let mut degraded_count = 0;
        let mut unhealthy_count = 0;
        let mut critical_services_healthy = true;
        let mut service_statuses = HashMap::new();

        for (name, service) in &services_clone {
            service_statuses.insert(name.clone(), service.status.clone());
            
            match service.status {
                ServiceStatus::Healthy => healthy_count += 1,
                ServiceStatus::Degraded => degraded_count += 1,
                ServiceStatus::Unhealthy | ServiceStatus::Stopped => {
                    unhealthy_count += 1;
                    // Check if this is a critical service
                    if service.registration.priority <= 2 {
                        critical_services_healthy = false;
                    }
                }
                _ => {}
            }
        }

        let total_services = services_clone.len() as u32;
        let overall_status = if !critical_services_healthy {
            ServiceStatus::Unhealthy
        } else if unhealthy_count > 0 || degraded_count > healthy_count {
            ServiceStatus::Degraded
        } else {
            ServiceStatus::Healthy
        };

        let uptime_seconds = (chrono::Utc::now().timestamp_millis() as u64 - self.startup_time) / 1000;
        let uptime_percentage = if total_services > 0 {
            (healthy_count + degraded_count) as f64 / total_services as f64 * 100.0
        } else {
            100.0
        };

        InfrastructureHealth {
            overall_status,
            healthy_services: healthy_count,
            degraded_services: degraded_count,
            unhealthy_services: unhealthy_count,
            total_services,
            critical_services_healthy,
            uptime_percentage,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
            service_statuses,
        }
    }

    /// Update service status
    pub async fn update_service_status(&self, service_name: &str, status: ServiceStatus) -> ArbitrageResult<()> {
        let mut services = self.services.lock().await;
        if let Some(service) = services.get_mut(service_name) {
            service.status = status;
            service.last_health_check = Some(chrono::Utc::now().timestamp_millis() as u64);
            
            // Update uptime
            let current_time = chrono::Utc::now().timestamp_millis() as u64;
            service.uptime_seconds = (current_time - self.startup_time) / 1000;
        }
        Ok(())
    }

    /// Restart a service (if auto-recovery is enabled)
    pub async fn restart_service(&self, service_name: &str) -> ArbitrageResult<()> {
        if !self.config.enable_auto_recovery {
            return Err(ArbitrageError::operation_error("Auto-recovery is disabled"));
        }

        let mut services = self.services.lock().await;
        if let Some(service) = services.get_mut(service_name) {
            if service.restart_count >= self.config.max_restart_attempts {
                return Err(ArbitrageError::operation_error("Max restart attempts exceeded"));
            }

            service.restart_count += 1;
            service.status = ServiceStatus::Initializing;
            
            // Implement service-specific restart logic here
            // For now, just update status
            service.status = ServiceStatus::Healthy;
        }

        Ok(())
    }

    /// Get configuration value
    pub async fn get_config(&self, key: &str) -> Option<serde_json::Value> {
        let config = self.global_config.lock().await;
        config.get(key).cloned()
    }

    /// Set configuration value
    pub async fn set_config(&self, key: &str, value: serde_json::Value) -> ArbitrageResult<()> {
        let mut config = self.global_config.lock().await;
        config.insert(key.to_string(), value);
        Ok(())
    }

    /// Graceful shutdown of all services
    pub async fn shutdown(&self) -> ArbitrageResult<()> {
        if !self.config.enable_graceful_shutdown {
            return Ok(());
        }

        // Update all services to stopped status
        let mut services = self.services.lock().await;
        for service in services.values_mut() {
            service.status = ServiceStatus::Stopped;
        }

        // Additional cleanup logic would go here
        Ok(())
    }

    // ============= INTERNAL HELPER METHODS =============

    async fn start_health_monitoring(&self) -> ArbitrageResult<()> {
        // In a real implementation, this would start background tasks
        // For now, just mark that health monitoring is active
        Ok(())
    }

    async fn initialize_circuit_breaker(&self, service_name: &str) -> ArbitrageResult<()> {
        let circuit_breaker = CircuitBreaker {
            service_name: service_name.to_string(),
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            last_failure_time: None,
            next_attempt_time: None,
            success_count: 0,
            threshold: self.config.circuit_breaker_threshold,
            timeout_seconds: self.config.circuit_breaker_timeout_seconds,
        };

        let mut circuit_breakers = self.circuit_breakers.lock().await;
        circuit_breakers.insert(service_name.to_string(), circuit_breaker);
        Ok(())
    }

    async fn check_circuit_breaker(&self, service_name: &str) -> bool {
        let circuit_breakers = self.circuit_breakers.lock().await;
        if let Some(breaker) = circuit_breakers.get(service_name) {
            match breaker.state {
                CircuitBreakerState::Closed => true,
                CircuitBreakerState::Open => {
                    // Check if we should transition to half-open
                    if let Some(next_attempt) = breaker.next_attempt_time {
                        let current_time = chrono::Utc::now().timestamp_millis() as u64;
                        current_time >= next_attempt
                    } else {
                        false
                    }
                }
                CircuitBreakerState::HalfOpen => true,
            }
        } else {
            true // No circuit breaker configured, allow request
        }
    }

    async fn record_circuit_breaker_success(&self, service_name: &str) -> ArbitrageResult<()> {
        let mut circuit_breakers = self.circuit_breakers.lock().await;
        if let Some(breaker) = circuit_breakers.get_mut(service_name) {
            breaker.success_count += 1;
            breaker.failure_count = 0;
            breaker.state = CircuitBreakerState::Closed;
            breaker.next_attempt_time = None;
        }
        Ok(())
    }

    async fn record_circuit_breaker_failure(&self, service_name: &str) -> ArbitrageResult<()> {
        let mut circuit_breakers = self.circuit_breakers.lock().await;
        if let Some(breaker) = circuit_breakers.get_mut(service_name) {
            breaker.failure_count += 1;
            breaker.last_failure_time = Some(chrono::Utc::now().timestamp_millis() as u64);

            if breaker.failure_count >= breaker.threshold {
                breaker.state = CircuitBreakerState::Open;
                breaker.next_attempt_time = Some(
                    chrono::Utc::now().timestamp_millis() as u64 + (breaker.timeout_seconds * 1000)
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_type_as_str() {
        assert_eq!(ServiceType::Database.as_str(), "database");
        assert_eq!(ServiceType::Cache.as_str(), "cache");
        assert_eq!(ServiceType::Health.as_str(), "health");
        assert_eq!(ServiceType::Custom("test".to_string()).as_str(), "test");
    }

    #[test]
    fn test_service_status_operational() {
        assert!(ServiceStatus::Healthy.is_operational());
        assert!(ServiceStatus::Degraded.is_operational());
        assert!(!ServiceStatus::Unhealthy.is_operational());
        assert!(!ServiceStatus::Stopped.is_operational());
        assert!(!ServiceStatus::Unknown.is_operational());
    }

    #[test]
    fn test_infrastructure_config_default() {
        let config = InfrastructureConfig::default();
        assert!(config.enable_service_discovery);
        assert!(config.enable_health_monitoring);
        assert!(config.enable_auto_recovery);
        assert!(config.enable_metrics_collection);
        assert_eq!(config.health_check_interval_seconds, 30);
        assert_eq!(config.max_restart_attempts, 3);
        assert!(config.enable_circuit_breaker);
        assert_eq!(config.circuit_breaker_threshold, 5);
    }

    #[test]
    fn test_circuit_breaker_states() {
        assert_eq!(CircuitBreakerState::Closed, CircuitBreakerState::Closed);
        assert_ne!(CircuitBreakerState::Open, CircuitBreakerState::Closed);
        assert_ne!(CircuitBreakerState::HalfOpen, CircuitBreakerState::Open);
    }

    #[test]
    fn test_service_registration_creation() {
        let registration = ServiceRegistration {
            service_name: "test_service".to_string(),
            service_type: ServiceType::Database,
            version: "1.0.0".to_string(),
            description: "Test service".to_string(),
            dependencies: vec![],
            health_check_endpoint: None,
            metrics_enabled: true,
            auto_recovery: true,
            priority: 1,
            tags: HashMap::new(),
            configuration: HashMap::new(),
        };

        assert_eq!(registration.service_name, "test_service");
        assert_eq!(registration.service_type, ServiceType::Database);
        assert_eq!(registration.version, "1.0.0");
        assert!(registration.metrics_enabled);
        assert!(registration.auto_recovery);
        assert_eq!(registration.priority, 1);
    }
} 