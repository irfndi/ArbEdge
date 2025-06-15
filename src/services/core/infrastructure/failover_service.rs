//! Failover Service - Automatic Failover and Recovery Mechanisms
//!
//! This service provides comprehensive failover functionality including:
//! - Automatic detection of service failures and degradation
//! - Seamless failover to backup systems and degraded modes
//! - Coordinated recovery detection and restoration
//! - Integration with circuit breakers and health monitoring
//! - Graceful degradation strategies for service continuity

use crate::services::core::infrastructure::circuit_breaker_service::{
    CircuitBreakerService, CircuitBreakerType,
};
use crate::services::core::infrastructure::monitoring_module::alert_manager::AlertManager;
use crate::services::core::infrastructure::monitoring_module::health_monitor::{
    ComponentHealth, HealthMonitor,
};
use crate::services::core::infrastructure::UnifiedHealthCheckConfig;

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Failover service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfig {
    /// Enable failover functionality
    pub enabled: bool,
    /// Automatic failover detection interval (seconds)
    pub detection_interval_seconds: u64,
    /// Failover decision timeout (seconds)
    pub failover_timeout_seconds: u64,
    /// Recovery detection timeout (seconds)
    pub recovery_timeout_seconds: u64,
    /// Enable automatic recovery
    pub enable_auto_recovery: bool,
    /// Enable graceful degradation modes
    pub enable_degraded_modes: bool,
    /// Enable coordinated failover (multiple services)
    pub enable_coordinated_failover: bool,
    /// KV storage for failover state persistence
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    /// Maximum concurrent failover operations
    pub max_concurrent_failovers: usize,
    /// Health check integration
    pub health_check_threshold: f32,
    /// Circuit breaker integration
    pub circuit_breaker_integration: bool,
    /// Alert integration
    pub alert_integration: bool,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            detection_interval_seconds: 30,
            failover_timeout_seconds: 60,
            recovery_timeout_seconds: 120,
            enable_auto_recovery: true,
            enable_degraded_modes: true,
            enable_coordinated_failover: true,
            enable_kv_storage: true,
            kv_key_prefix: "failover".to_string(),
            max_concurrent_failovers: 10,
            health_check_threshold: 0.3,
            circuit_breaker_integration: true,
            alert_integration: true,
        }
    }
}

impl FailoverConfig {
    /// High availability configuration
    pub fn high_availability() -> Self {
        Self {
            detection_interval_seconds: 10,
            failover_timeout_seconds: 30,
            recovery_timeout_seconds: 60,
            enable_auto_recovery: true,
            enable_degraded_modes: true,
            enable_coordinated_failover: true,
            health_check_threshold: 0.5,
            max_concurrent_failovers: 20,
            ..Default::default()
        }
    }

    /// High reliability configuration
    pub fn high_reliability() -> Self {
        Self {
            detection_interval_seconds: 15,
            failover_timeout_seconds: 45,
            recovery_timeout_seconds: 180,
            enable_auto_recovery: true,
            enable_degraded_modes: true,
            enable_coordinated_failover: true,
            health_check_threshold: 0.4,
            max_concurrent_failovers: 15,
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.detection_interval_seconds == 0 {
            return Err(ArbitrageError::config_error(
                "Detection interval must be greater than 0",
            ));
        }

        if self.failover_timeout_seconds == 0 {
            return Err(ArbitrageError::config_error(
                "Failover timeout must be greater than 0",
            ));
        }

        if self.recovery_timeout_seconds == 0 {
            return Err(ArbitrageError::config_error(
                "Recovery timeout must be greater than 0",
            ));
        }

        if self.health_check_threshold < 0.0 || self.health_check_threshold > 1.0 {
            return Err(ArbitrageError::config_error(
                "Health check threshold must be between 0.0 and 1.0",
            ));
        }

        if self.max_concurrent_failovers == 0 {
            return Err(ArbitrageError::config_error(
                "Max concurrent failovers must be greater than 0",
            ));
        }

        Ok(())
    }
}

/// Failover strategy definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverStrategy {
    /// Strategy identifier
    pub id: String,
    /// Strategy name
    pub name: String,
    /// Service being protected
    pub service_name: String,
    /// Failover type
    pub failover_type: FailoverType,
    /// Primary service endpoint/configuration
    pub primary_config: ServiceConfig,
    /// Backup service configurations
    pub backup_configs: Vec<ServiceConfig>,
    /// Degraded mode configuration
    pub degraded_config: Option<ServiceConfig>,
    /// Health check configuration
    pub health_check: HealthCheckConfig,
    /// Recovery configuration
    pub recovery_config: RecoveryConfig,
    /// Strategy priority (higher = more important)
    pub priority: u32,
    /// Enable automatic failover
    pub auto_failover_enabled: bool,
    /// Enable automatic recovery
    pub auto_recovery_enabled: bool,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl FailoverStrategy {
    pub fn new(
        id: String,
        name: String,
        service_name: String,
        failover_type: FailoverType,
        primary_config: ServiceConfig,
    ) -> Self {
        Self {
            id,
            name,
            service_name,
            failover_type,
            primary_config,
            backup_configs: Vec::new(),
            degraded_config: None,
            health_check: UnifiedHealthCheckConfig::failover_optimized(),
            recovery_config: RecoveryConfig::default(),
            priority: 1,
            auto_failover_enabled: true,
            auto_recovery_enabled: true,
            metadata: HashMap::new(),
        }
    }

    pub fn with_backup(mut self, backup_config: ServiceConfig) -> Self {
        self.backup_configs.push(backup_config);
        self
    }

    pub fn with_degraded_mode(mut self, degraded_config: ServiceConfig) -> Self {
        self.degraded_config = Some(degraded_config);
        self
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Failover type classification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FailoverType {
    /// HTTP API failover
    HttpApi,
    /// Database failover
    Database,
    /// KV store failover
    KvStore,
    /// Queue/messaging failover
    Queue,
    /// Storage (R2) failover
    Storage,
    /// Custom failover type
    Custom(String),
}

impl FailoverType {
    pub fn as_str(&self) -> &str {
        match self {
            FailoverType::HttpApi => "http_api",
            FailoverType::Database => "database",
            FailoverType::KvStore => "kv_store",
            FailoverType::Queue => "queue",
            FailoverType::Storage => "storage",
            FailoverType::Custom(name) => name,
        }
    }
}

/// Service configuration for failover targets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Configuration identifier
    pub id: String,
    /// Configuration name
    pub name: String,
    /// Service endpoint or connection string
    pub endpoint: String,
    /// Service type
    pub service_type: String,
    /// Configuration parameters
    pub parameters: HashMap<String, String>,
    /// Health check endpoint
    pub health_endpoint: Option<String>,
    /// Expected response time (ms)
    pub expected_response_time_ms: u64,
    /// Service priority (higher = preferred)
    pub priority: u32,
    /// Service capacity (0.0-1.0)
    pub capacity: f32,
    /// Service enabled
    pub enabled: bool,
}

impl ServiceConfig {
    pub fn new(id: String, name: String, endpoint: String, service_type: String) -> Self {
        Self {
            id,
            name,
            endpoint,
            service_type,
            parameters: HashMap::new(),
            health_endpoint: None,
            expected_response_time_ms: 1000,
            priority: 1,
            capacity: 1.0,
            enabled: true,
        }
    }

    pub fn with_parameter(mut self, key: String, value: String) -> Self {
        self.parameters.insert(key, value);
        self
    }

    pub fn with_health_endpoint(mut self, health_endpoint: String) -> Self {
        self.health_endpoint = Some(health_endpoint);
        self
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_capacity(mut self, capacity: f32) -> Self {
        self.capacity = capacity.clamp(0.0, 1.0);
        self
    }
}

// Use unified health check configuration
pub type HealthCheckConfig = UnifiedHealthCheckConfig;

// Health check method is now imported from unified_health_check

/// Recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Recovery detection interval (seconds)
    pub detection_interval_seconds: u64,
    /// Recovery validation timeout (seconds)
    pub validation_timeout_seconds: u64,
    /// Gradual recovery enabled
    pub gradual_recovery_enabled: bool,
    /// Recovery steps (for gradual recovery)
    pub recovery_steps: Vec<RecoveryStep>,
    /// Warmup period before full recovery (seconds)
    pub warmup_period_seconds: u64,
    /// Recovery validation checks
    pub validation_checks: Vec<String>,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            detection_interval_seconds: 60,
            validation_timeout_seconds: 30,
            gradual_recovery_enabled: true,
            recovery_steps: vec![
                RecoveryStep {
                    step_number: 1,
                    traffic_percentage: 10.0,
                    duration_seconds: 60,
                    validation_required: true,
                },
                RecoveryStep {
                    step_number: 2,
                    traffic_percentage: 50.0,
                    duration_seconds: 120,
                    validation_required: true,
                },
                RecoveryStep {
                    step_number: 3,
                    traffic_percentage: 100.0,
                    duration_seconds: 0,
                    validation_required: false,
                },
            ],
            warmup_period_seconds: 30,
            validation_checks: Vec::new(),
        }
    }
}

/// Recovery step for gradual recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStep {
    /// Step number in recovery sequence
    pub step_number: u32,
    /// Percentage of traffic to route to recovered service
    pub traffic_percentage: f32,
    /// Duration to maintain this step (seconds)
    pub duration_seconds: u64,
    /// Whether validation is required for this step
    pub validation_required: bool,
}

/// Failover state tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverState {
    /// Strategy identifier
    pub strategy_id: String,
    /// Current failover status
    pub status: FailoverStatus,
    /// Currently active service configuration
    pub active_config: ServiceConfig,
    /// Failover start time
    pub failover_start_time: Option<u64>,
    /// Recovery start time
    pub recovery_start_time: Option<u64>,
    /// Failure count
    pub failure_count: u32,
    /// Success count (for recovery)
    pub success_count: u32,
    /// Last health check time
    pub last_health_check_time: u64,
    /// Last state change time
    pub last_state_change_time: u64,
    /// Recovery step (for gradual recovery)
    pub current_recovery_step: Option<u32>,
    /// Additional state metadata
    pub metadata: HashMap<String, String>,
}

/// Failover status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FailoverStatus {
    /// Normal operation (primary service)
    Normal,
    /// Service degraded but operational
    Degraded,
    /// Failed over to backup service
    FailedOver,
    /// In recovery process
    Recovering,
    /// Failed (no available services)
    Failed,
}

impl FailoverStatus {
    pub fn as_str(&self) -> &str {
        match self {
            FailoverStatus::Normal => "normal",
            FailoverStatus::Degraded => "degraded",
            FailoverStatus::FailedOver => "failed_over",
            FailoverStatus::Recovering => "recovering",
            FailoverStatus::Failed => "failed",
        }
    }

    pub fn is_operational(&self) -> bool {
        !matches!(self, FailoverStatus::Failed)
    }
}

/// Failover metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverMetrics {
    /// Total failover strategies
    pub total_strategies: u64,
    /// Normal operation count
    pub normal_strategies: u64,
    /// Degraded operation count
    pub degraded_strategies: u64,
    /// Failed over strategies
    pub failed_over_strategies: u64,
    /// Recovering strategies
    pub recovering_strategies: u64,
    /// Failed strategies
    pub failed_strategies: u64,
    /// Total failover events
    pub total_failover_events: u64,
    /// Total recovery events
    pub total_recovery_events: u64,
    /// Average failover duration (seconds)
    pub avg_failover_duration_seconds: f64,
    /// Average recovery duration (seconds)
    pub avg_recovery_duration_seconds: f64,
    /// Success rate
    pub success_rate: f32,
    /// Health checks performed
    pub health_checks_performed: u64,
    /// Failed health checks
    pub failed_health_checks: u64,
    /// Last updated timestamp
    pub last_updated: u64,
}

impl Default for FailoverMetrics {
    fn default() -> Self {
        Self {
            total_strategies: 0,
            normal_strategies: 0,
            degraded_strategies: 0,
            failed_over_strategies: 0,
            recovering_strategies: 0,
            failed_strategies: 0,
            total_failover_events: 0,
            total_recovery_events: 0,
            avg_failover_duration_seconds: 0.0,
            avg_recovery_duration_seconds: 0.0,
            success_rate: 1.0,
            health_checks_performed: 0,
            failed_health_checks: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Main failover service
pub struct FailoverService {
    config: FailoverConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Failover management
    strategies: Arc<Mutex<HashMap<String, FailoverStrategy>>>,
    states: Arc<Mutex<HashMap<String, FailoverState>>>,

    // Integration with other services
    health_monitor: Option<Arc<HealthMonitor>>,
    alert_manager: Option<Arc<AlertManager>>,
    circuit_breaker_service: Option<Arc<CircuitBreakerService>>,

    // Metrics and performance
    metrics: Arc<Mutex<FailoverMetrics>>,
}

impl FailoverService {
    /// Create a new failover service
    pub async fn new(
        config: FailoverConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        config.validate()?;

        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        let service = Self {
            config,
            logger,
            kv_store,
            strategies: Arc::new(Mutex::new(HashMap::new())),
            states: Arc::new(Mutex::new(HashMap::new())),
            health_monitor: None,
            alert_manager: None,
            circuit_breaker_service: None,
            metrics: Arc::new(Mutex::new(FailoverMetrics::default())),
        };

        service.logger.info("Failover Service initialized");
        Ok(service)
    }

    /// Set health monitor integration
    pub fn set_health_monitor(&mut self, health_monitor: Arc<HealthMonitor>) {
        self.health_monitor = Some(health_monitor);
        self.logger.info("Health monitor integration enabled");
    }

    /// Set alert manager integration
    pub fn set_alert_manager(&mut self, alert_manager: Arc<AlertManager>) {
        self.alert_manager = Some(alert_manager);
        self.logger.info("Alert manager integration enabled");
    }

    /// Set circuit breaker service integration
    pub fn set_circuit_breaker_service(
        &mut self,
        circuit_breaker_service: Arc<CircuitBreakerService>,
    ) {
        self.circuit_breaker_service = Some(circuit_breaker_service);
        self.logger
            .info("Circuit breaker service integration enabled");
    }

    /// Register a failover strategy
    pub async fn register_strategy(&self, strategy: FailoverStrategy) -> ArbitrageResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check for existing strategy and register strategy/state
        {
            let mut strategies = self.strategies.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire strategies lock: {}", e))
            })?;

            if strategies.contains_key(&strategy.id) {
                return Err(ArbitrageError::config_error(format!(
                    "Failover strategy {} already exists",
                    strategy.id
                )));
            }

            // Initialize failover state
            let state = FailoverState {
                strategy_id: strategy.id.clone(),
                status: FailoverStatus::Normal,
                active_config: strategy.primary_config.clone(),
                failover_start_time: None,
                recovery_start_time: None,
                failure_count: 0,
                success_count: 0,
                last_health_check_time: chrono::Utc::now().timestamp_millis() as u64,
                last_state_change_time: chrono::Utc::now().timestamp_millis() as u64,
                current_recovery_step: None,
                metadata: HashMap::new(),
            };

            {
                let mut states = self.states.lock().map_err(|e| {
                    ArbitrageError::internal_error(format!("Failed to acquire states lock: {}", e))
                })?;
                states.insert(strategy.id.clone(), state);
            }

            strategies.insert(strategy.id.clone(), strategy.clone());
        } // Lock is dropped here

        // Register with health monitor if available
        if let Some(health_monitor) = &self.health_monitor {
            let component_health = ComponentHealth::new(
                strategy.id.clone(),
                strategy.name.clone(),
                "failover_strategy".to_string(),
            )
            .with_tag("service".to_string(), strategy.service_name.clone());

            let _ = health_monitor.register_component(component_health).await;
        }

        // Set up circuit breaker if integration is enabled
        if self.config.circuit_breaker_integration {
            if let Some(cb_service) = &self.circuit_breaker_service {
                let cb_type = match strategy.failover_type {
                    FailoverType::HttpApi => CircuitBreakerType::HttpApi,
                    FailoverType::Database => CircuitBreakerType::Database,
                    FailoverType::KvStore => CircuitBreakerType::KvStore,
                    _ => CircuitBreakerType::ExternalService,
                };

                let _ = cb_service
                    .register_circuit_breaker(
                        format!("failover_{}", strategy.id),
                        format!("Failover Circuit Breaker for {}", strategy.name),
                        strategy.service_name.clone(),
                        cb_type,
                    )
                    .await;
            }
        }

        self.logger.info(&format!(
            "Registered failover strategy: {} for service: {}",
            strategy.name, strategy.service_name
        ));
        Ok(())
    }

    /// Execute operation with failover protection
    pub async fn execute_with_failover<F, T, E>(
        &self,
        strategy_id: &str,
        operation: F,
    ) -> ArbitrageResult<T>
    where
        F: Fn(&ServiceConfig) -> Result<T, E> + Send + Sync,
        E: std::fmt::Display + Send,
    {
        if !self.config.enabled {
            return Err(ArbitrageError::service_unavailable(
                "Failover service disabled",
            ));
        }

        // Get current active configuration
        let active_config = {
            let states = self.states.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire states lock: {}", e))
            })?;

            if let Some(state) = states.get(strategy_id) {
                state.active_config.clone()
            } else {
                return Err(ArbitrageError::not_found(format!(
                    "Failover strategy {} not found",
                    strategy_id
                )));
            }
        };

        // Try operation with circuit breaker protection
        let start_time = crate::utils::time::now_instant();
        let result = if self.config.circuit_breaker_integration {
            if let Some(cb_service) = &self.circuit_breaker_service {
                let cb_id = format!("failover_{}", strategy_id);
                cb_service
                    .execute(&cb_id, || operation(&active_config))
                    .await
            } else {
                operation(&active_config)
                    .map_err(|e| ArbitrageError::api_error(format!("Operation failed: {}", e)))
            }
        } else {
            operation(&active_config)
                .map_err(|e| ArbitrageError::api_error(format!("Operation failed: {}", e)))
        };

        let response_time = start_time.elapsed().as_millis() as u64;

        // Handle result and update state
        match &result {
            Ok(_) => {
                self.record_success(strategy_id, response_time).await;
            }
            Err(_) => {
                self.record_failure(strategy_id, response_time).await;

                // Attempt failover if necessary
                if self.should_failover(strategy_id).await {
                    return self.attempt_failover(strategy_id, &operation).await;
                }
            }
        }

        result
    }

    /// Check if failover should be triggered
    async fn should_failover(&self, strategy_id: &str) -> bool {
        let states = match self.states.lock() {
            Ok(states) => states,
            Err(_) => return false,
        };

        if let Some(state) = states.get(strategy_id) {
            // Check failure threshold from strategy
            if let Ok(strategies) = self.strategies.lock() {
                if let Some(strategy) = strategies.get(strategy_id) {
                    return state.failure_count >= strategy.health_check.failure_threshold
                        && matches!(
                            state.status,
                            FailoverStatus::Normal | FailoverStatus::Degraded
                        );
                }
            }
        }

        false
    }

    /// Attempt failover to backup service
    async fn attempt_failover<F, T, E>(
        &self,
        strategy_id: &str,
        operation: &F,
    ) -> ArbitrageResult<T>
    where
        F: Fn(&ServiceConfig) -> Result<T, E> + Send + Sync,
        E: std::fmt::Display + Send,
    {
        self.logger.warn(&format!(
            "Attempting failover for strategy: {}",
            strategy_id
        ));

        // Get strategy
        let strategy = {
            let strategies = self.strategies.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire strategies lock: {}", e))
            })?;

            strategies
                .get(strategy_id)
                .ok_or_else(|| {
                    ArbitrageError::not_found(format!("Strategy {} not found", strategy_id))
                })?
                .clone()
        };

        // Find next available backup configuration
        let next_config = {
            // Try backup configurations in priority order
            let mut sorted_backups = strategy.backup_configs.clone();
            sorted_backups.sort_by(|a, b| b.priority.cmp(&a.priority));

            // Find first enabled backup
            let mut found_config = None;
            for backup in &sorted_backups {
                if backup.enabled {
                    found_config = Some(backup.clone());
                    break;
                }
            }

            if let Some(config) = found_config {
                config
            } else {
                // If no backups available, try degraded mode
                if let Some(degraded_config) = &strategy.degraded_config {
                    if degraded_config.enabled {
                        degraded_config.clone()
                    } else {
                        return Err(ArbitrageError::service_unavailable(
                            "No backup services available",
                        ));
                    }
                } else {
                    return Err(ArbitrageError::service_unavailable(
                        "No backup services available",
                    ));
                }
            }
        };

        // Update state to failed over
        {
            let mut states = self.states.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire states lock: {}", e))
            })?;

            if let Some(state) = states.get_mut(strategy_id) {
                state.status = if next_config.id
                    == *strategy
                        .degraded_config
                        .as_ref()
                        .map(|c| &c.id)
                        .unwrap_or(&String::new())
                {
                    FailoverStatus::Degraded
                } else {
                    FailoverStatus::FailedOver
                };
                state.active_config = next_config.clone();
                state.failover_start_time = Some(chrono::Utc::now().timestamp_millis() as u64);
                state.last_state_change_time = chrono::Utc::now().timestamp_millis() as u64;
                state.failure_count = 0; // Reset for new service
            }
        }

        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());
            metrics.total_failover_events += 1;
        }

        // Trigger alert
        if let Some(alert_manager) = &self.alert_manager {
            let _ = alert_manager
                .evaluate_metric(strategy_id, "failover_event", 1.0)
                .await;
        }

        // Try operation with backup configuration
        let result = operation(&next_config);

        match result {
            Ok(value) => {
                self.logger.info(&format!(
                    "Failover successful for strategy: {} to config: {}",
                    strategy_id, next_config.name
                ));
                Ok(value)
            }
            Err(e) => {
                self.logger.error(&format!(
                    "Failover failed for strategy: {}: {}",
                    strategy_id, e
                ));

                // Mark as completely failed
                {
                    let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(state) = states.get_mut(strategy_id) {
                        state.status = FailoverStatus::Failed;
                        state.last_state_change_time = chrono::Utc::now().timestamp_millis() as u64;
                    }
                }

                Err(ArbitrageError::service_unavailable(format!(
                    "All failover options exhausted: {}",
                    e
                )))
            }
        }
    }

    /// Record successful operation
    async fn record_success(&self, strategy_id: &str, _response_time_ms: u64) {
        let should_start_recovery = {
            let mut states = match self.states.lock() {
                Ok(states) => states,
                Err(_) => return,
            };

            if let Some(state) = states.get_mut(strategy_id) {
                state.success_count += 1;
                state.failure_count = 0; // Reset failure count on success
                state.last_health_check_time = chrono::Utc::now().timestamp_millis() as u64;

                // Check if we should start recovery process
                if matches!(
                    state.status,
                    FailoverStatus::FailedOver | FailoverStatus::Degraded
                ) {
                    if let Ok(strategies) = self.strategies.lock() {
                        if let Some(strategy) = strategies.get(strategy_id) {
                            state.success_count >= strategy.health_check.success_threshold
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }; // Locks are dropped here

        if should_start_recovery {
            self.start_recovery(strategy_id).await;
        }
    }

    /// Record failed operation
    async fn record_failure(&self, strategy_id: &str, _response_time_ms: u64) {
        if let Ok(mut states) = self.states.lock() {
            if let Some(state) = states.get_mut(strategy_id) {
                state.failure_count += 1;
                state.success_count = 0; // Reset success count on failure
                state.last_health_check_time = chrono::Utc::now().timestamp_millis() as u64;
            }
        }
    }

    /// Start recovery process
    async fn start_recovery(&self, strategy_id: &str) {
        self.logger
            .info(&format!("Starting recovery for strategy: {}", strategy_id));

        if let Ok(mut states) = self.states.lock() {
            if let Some(state) = states.get_mut(strategy_id) {
                state.status = FailoverStatus::Recovering;
                state.recovery_start_time = Some(chrono::Utc::now().timestamp_millis() as u64);
                state.current_recovery_step = Some(1);
                state.last_state_change_time = chrono::Utc::now().timestamp_millis() as u64;
            }
        }

        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());
            metrics.total_recovery_events += 1;
        }
    }

    /// Get failover state
    pub async fn get_failover_state(&self, strategy_id: &str) -> Option<FailoverState> {
        let states = self.states.lock().ok()?;
        states.get(strategy_id).cloned()
    }

    /// Get all failover states
    pub async fn get_all_failover_states(&self) -> HashMap<String, FailoverState> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states.clone()
    }

    /// Force failover state (for testing/manual intervention)
    pub async fn force_failover_state(
        &self,
        strategy_id: &str,
        status: FailoverStatus,
    ) -> ArbitrageResult<()> {
        let mut states = self.states.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire states lock: {}", e))
        })?;

        if let Some(state) = states.get_mut(strategy_id) {
            state.status = status;
            state.last_state_change_time = chrono::Utc::now().timestamp_millis() as u64;

            self.logger.info(&format!(
                "Manually set failover state for {} to: {:?}",
                strategy_id, state.status
            ));
            Ok(())
        } else {
            Err(ArbitrageError::not_found(format!(
                "Failover strategy {} not found",
                strategy_id
            )))
        }
    }

    /// Get service metrics
    pub async fn get_metrics(&self) -> FailoverMetrics {
        self.metrics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Health check for the failover service
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Check if service is responsive
        let strategy_count = {
            let strategies = self
                .strategies
                .lock()
                .map_err(|_| ArbitrageError::internal_error("Failover service unresponsive"))?;
            strategies.len()
        }; // Lock is dropped here

        // Check KV store accessibility
        if self.config.enable_kv_storage {
            let test_key = format!("{}:health_check", self.config.kv_key_prefix);
            let test_value = chrono::Utc::now().timestamp().to_string();

            match self.kv_store.put(&test_key, &test_value) {
                Ok(_) => {
                    let _ = self.kv_store.delete(&test_key).await;
                }
                Err(_) => {
                    return Err(ArbitrageError::internal_error("KV store unavailable"));
                }
            }
        }

        self.logger.debug(&format!(
            "Failover service health check passed. Managing {} strategies",
            strategy_count
        ));
        Ok(true)
    }

    /// Update service metrics
    #[allow(dead_code)]
    async fn update_metrics(&self) {
        if let Ok(states) = self.states.lock() {
            if let Ok(mut metrics) = self.metrics.lock() {
                metrics.total_strategies = states.len() as u64;
                metrics.normal_strategies = states
                    .values()
                    .filter(|s| matches!(s.status, FailoverStatus::Normal))
                    .count() as u64;
                metrics.degraded_strategies = states
                    .values()
                    .filter(|s| matches!(s.status, FailoverStatus::Degraded))
                    .count() as u64;
                metrics.failed_over_strategies = states
                    .values()
                    .filter(|s| matches!(s.status, FailoverStatus::FailedOver))
                    .count() as u64;
                metrics.recovering_strategies = states
                    .values()
                    .filter(|s| matches!(s.status, FailoverStatus::Recovering))
                    .count() as u64;
                metrics.failed_strategies = states
                    .values()
                    .filter(|s| matches!(s.status, FailoverStatus::Failed))
                    .count() as u64;

                // Calculate success rate
                let operational_strategies = metrics.normal_strategies
                    + metrics.degraded_strategies
                    + metrics.failed_over_strategies
                    + metrics.recovering_strategies;
                metrics.success_rate = if metrics.total_strategies > 0 {
                    operational_strategies as f32 / metrics.total_strategies as f32
                } else {
                    1.0
                };

                metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failover_config_default() {
        let config = FailoverConfig::default();
        assert!(config.enabled);
        assert_eq!(config.detection_interval_seconds, 30);
        assert_eq!(config.failover_timeout_seconds, 60);
        assert_eq!(config.recovery_timeout_seconds, 120);
    }

    #[test]
    fn test_failover_config_validation() {
        let mut config = FailoverConfig::default();
        assert!(config.validate().is_ok());

        config.detection_interval_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_failover_strategy_creation() {
        let primary_config = ServiceConfig::new(
            "primary".to_string(),
            "Primary Service".to_string(),
            "http://primary.example.com".to_string(),
            "http_api".to_string(),
        );

        let strategy = FailoverStrategy::new(
            "test-strategy".to_string(),
            "Test Strategy".to_string(),
            "test-service".to_string(),
            FailoverType::HttpApi,
            primary_config,
        );

        assert_eq!(strategy.id, "test-strategy");
        assert_eq!(strategy.name, "Test Strategy");
        assert_eq!(strategy.service_name, "test-service");
        assert_eq!(strategy.failover_type, FailoverType::HttpApi);
        assert!(strategy.auto_failover_enabled);
        assert!(strategy.auto_recovery_enabled);
    }

    #[test]
    fn test_service_config_creation() {
        let config = ServiceConfig::new(
            "test-config".to_string(),
            "Test Config".to_string(),
            "http://test.example.com".to_string(),
            "http_api".to_string(),
        )
        .with_parameter("timeout".to_string(), "5000".to_string())
        .with_priority(10)
        .with_capacity(0.8);

        assert_eq!(config.id, "test-config");
        assert_eq!(config.name, "Test Config");
        assert_eq!(config.endpoint, "http://test.example.com");
        assert_eq!(config.service_type, "http_api");
        assert_eq!(config.parameters.get("timeout"), Some(&"5000".to_string()));
        assert_eq!(config.priority, 10);
        assert_eq!(config.capacity, 0.8);
    }

    #[test]
    fn test_failover_status_properties() {
        assert_eq!(FailoverStatus::Normal.as_str(), "normal");
        assert_eq!(FailoverStatus::Degraded.as_str(), "degraded");
        assert_eq!(FailoverStatus::FailedOver.as_str(), "failed_over");
        assert_eq!(FailoverStatus::Recovering.as_str(), "recovering");
        assert_eq!(FailoverStatus::Failed.as_str(), "failed");

        assert!(FailoverStatus::Normal.is_operational());
        assert!(FailoverStatus::Degraded.is_operational());
        assert!(FailoverStatus::FailedOver.is_operational());
        assert!(FailoverStatus::Recovering.is_operational());
        assert!(!FailoverStatus::Failed.is_operational());
    }

    #[test]
    fn test_recovery_config_default() {
        let config = RecoveryConfig::default();
        assert_eq!(config.detection_interval_seconds, 60);
        assert_eq!(config.validation_timeout_seconds, 30);
        assert!(config.gradual_recovery_enabled);
        assert_eq!(config.recovery_steps.len(), 3);
    }

    #[test]
    fn test_recovery_steps() {
        let config = RecoveryConfig::default();
        let steps = config.recovery_steps;

        assert_eq!(steps[0].traffic_percentage, 10.0);
        assert_eq!(steps[1].traffic_percentage, 50.0);
        assert_eq!(steps[2].traffic_percentage, 100.0);

        assert!(steps[0].validation_required);
        assert!(steps[1].validation_required);
        assert!(!steps[2].validation_required);
    }

    #[test]
    fn test_failover_metrics_default() {
        let metrics = FailoverMetrics::default();
        assert_eq!(metrics.total_strategies, 0);
        assert_eq!(metrics.success_rate, 1.0);
        assert_eq!(metrics.total_failover_events, 0);
        assert_eq!(metrics.total_recovery_events, 0);
    }
}
