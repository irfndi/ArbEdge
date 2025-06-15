//! Circuit Breaker Service - Enhanced Circuit Breaker Implementation
//!
//! This service provides comprehensive circuit breaker functionality with:
//! - Multi-state circuit breaker patterns (Closed, Open, Half-Open)
//! - Integration with health monitoring and alerting systems
//! - Configurable thresholds and recovery strategies
//! - Metrics collection and performance tracking
//! - Automatic failover and recovery mechanisms

use crate::services::core::infrastructure::monitoring_module::alert_manager::{
    AlertCondition, AlertManager, AlertRule, AlertSeverity,
};
use crate::services::core::infrastructure::monitoring_module::health_monitor::{
    ComponentHealth, HealthMonitor,
};
use crate::services::core::infrastructure::shared_types::{CircuitBreaker, CircuitBreakerState};

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Enhanced circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Enable circuit breaker functionality
    pub enabled: bool,
    /// Default failure threshold before opening circuit
    pub default_failure_threshold: u32,
    /// Default timeout before attempting recovery (seconds)
    pub default_timeout_seconds: u64,
    /// Minimum success count in half-open state to close circuit
    pub min_success_count_half_open: u32,
    /// Enable automatic recovery detection
    pub enable_auto_recovery: bool,
    /// Enable integration with health monitoring
    pub enable_health_integration: bool,
    /// Enable integration with alert manager
    pub enable_alert_integration: bool,
    /// Enable metrics collection
    pub enable_metrics_collection: bool,
    /// KV storage for persistence
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    /// Circuit breaker check interval (seconds)
    pub check_interval_seconds: u64,
    /// Maximum number of circuit breakers to manage
    pub max_circuit_breakers: usize,
    /// Default degraded mode settings
    pub enable_degraded_mode: bool,
    pub degraded_mode_timeout_seconds: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_failure_threshold: 5,
            default_timeout_seconds: 60,
            min_success_count_half_open: 3,
            enable_auto_recovery: true,
            enable_health_integration: true,
            enable_alert_integration: true,
            enable_metrics_collection: true,
            enable_kv_storage: true,
            kv_key_prefix: "circuit_breaker".to_string(),
            check_interval_seconds: 30,
            max_circuit_breakers: 100,
            enable_degraded_mode: true,
            degraded_mode_timeout_seconds: 300,
        }
    }
}

impl CircuitBreakerConfig {
    /// High performance configuration
    pub fn high_performance() -> Self {
        Self {
            check_interval_seconds: 10,
            enable_metrics_collection: true,
            enable_auto_recovery: true,
            default_timeout_seconds: 30,
            max_circuit_breakers: 200,
            ..Default::default()
        }
    }

    /// High reliability configuration
    pub fn high_reliability() -> Self {
        Self {
            default_failure_threshold: 3,
            default_timeout_seconds: 120,
            min_success_count_half_open: 5,
            enable_health_integration: true,
            enable_alert_integration: true,
            enable_degraded_mode: true,
            degraded_mode_timeout_seconds: 600,
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.default_failure_threshold == 0 {
            return Err(ArbitrageError::config_error(
                "Failure threshold must be greater than 0",
            ));
        }

        if self.default_timeout_seconds == 0 {
            return Err(ArbitrageError::config_error(
                "Timeout must be greater than 0",
            ));
        }

        if self.min_success_count_half_open == 0 {
            return Err(ArbitrageError::config_error(
                "Minimum success count must be greater than 0",
            ));
        }

        if self.check_interval_seconds == 0 {
            return Err(ArbitrageError::config_error(
                "Check interval must be greater than 0",
            ));
        }

        if self.max_circuit_breakers == 0 {
            return Err(ArbitrageError::config_error(
                "Max circuit breakers must be greater than 0",
            ));
        }

        Ok(())
    }
}

/// Enhanced circuit breaker with additional features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedCircuitBreaker {
    /// Basic circuit breaker functionality
    pub circuit_breaker: CircuitBreaker,
    /// Circuit breaker identifier
    pub id: String,
    /// Circuit breaker name
    pub name: String,
    /// Service or component being protected
    pub service_name: String,
    /// Circuit breaker type
    pub breaker_type: CircuitBreakerType,
    /// Configuration
    pub config: CircuitBreakerConfig,
    /// Last check timestamp
    pub last_check_time: u64,
    /// Last state change timestamp
    pub last_state_change_time: u64,
    /// Degraded mode active
    pub degraded_mode_active: bool,
    /// Degraded mode start time
    pub degraded_mode_start_time: Option<u64>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Performance metrics
    pub avg_response_time_ms: f64,
    pub min_response_time_ms: f64,
    pub max_response_time_ms: f64,
    /// Alert configuration
    pub alert_on_open: bool,
    pub alert_on_half_open: bool,
    pub alert_on_degraded: bool,
}

impl EnhancedCircuitBreaker {
    /// Create a new enhanced circuit breaker
    pub fn new(
        id: String,
        name: String,
        service_name: String,
        breaker_type: CircuitBreakerType,
        config: CircuitBreakerConfig,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;

        Self {
            circuit_breaker: CircuitBreaker::new(
                config.default_failure_threshold,
                config.default_timeout_seconds,
            ),
            id,
            name,
            service_name,
            breaker_type,
            config,
            last_check_time: now,
            last_state_change_time: now,
            degraded_mode_active: false,
            degraded_mode_start_time: None,
            metadata: HashMap::new(),
            avg_response_time_ms: 0.0,
            min_response_time_ms: f64::MAX,
            max_response_time_ms: 0.0,
            alert_on_open: true,
            alert_on_half_open: true,
            alert_on_degraded: true,
        }
    }

    /// Check if operation can proceed
    pub fn can_execute(&mut self) -> bool {
        self.last_check_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check for degraded mode timeout
        if self.degraded_mode_active {
            if let Some(start_time) = self.degraded_mode_start_time {
                let elapsed = (self.last_check_time - start_time) / 1000;
                if elapsed > self.config.degraded_mode_timeout_seconds {
                    self.exit_degraded_mode();
                }
            }
        }

        self.circuit_breaker.can_execute()
    }

    /// Record successful operation
    pub fn record_success(&mut self, response_time_ms: f64) {
        let previous_state = self.circuit_breaker.state.clone();
        self.circuit_breaker.record_success();

        // Update response time metrics
        self.update_response_time_metrics(response_time_ms);

        // Check for state change
        if previous_state != self.circuit_breaker.state {
            self.last_state_change_time = chrono::Utc::now().timestamp_millis() as u64;

            // Exit degraded mode if circuit is now closed
            if matches!(self.circuit_breaker.state, CircuitBreakerState::Closed) {
                self.exit_degraded_mode();
            }
        }
    }

    /// Record failed operation
    pub fn record_failure(&mut self, response_time_ms: Option<f64>) {
        let previous_state = self.circuit_breaker.state.clone();
        self.circuit_breaker.record_failure();

        // Update response time metrics if available
        if let Some(time) = response_time_ms {
            self.update_response_time_metrics(time);
        }

        // Check for state change
        if previous_state != self.circuit_breaker.state {
            self.last_state_change_time = chrono::Utc::now().timestamp_millis() as u64;

            // Enter degraded mode if circuit is now open and degraded mode is enabled
            if matches!(self.circuit_breaker.state, CircuitBreakerState::Open)
                && self.config.enable_degraded_mode
            {
                self.enter_degraded_mode();
            }
        }
    }

    /// Enter degraded mode
    pub fn enter_degraded_mode(&mut self) {
        if !self.degraded_mode_active {
            self.degraded_mode_active = true;
            self.degraded_mode_start_time = Some(chrono::Utc::now().timestamp_millis() as u64);
        }
    }

    /// Exit degraded mode
    pub fn exit_degraded_mode(&mut self) {
        self.degraded_mode_active = false;
        self.degraded_mode_start_time = None;
    }

    /// Update response time metrics
    fn update_response_time_metrics(&mut self, response_time_ms: f64) {
        if self.circuit_breaker.total_requests == 1 {
            // First request
            self.avg_response_time_ms = response_time_ms;
            self.min_response_time_ms = response_time_ms;
            self.max_response_time_ms = response_time_ms;
        } else {
            // Update average
            let total_requests = self.circuit_breaker.total_requests as f64;
            self.avg_response_time_ms = (self.avg_response_time_ms * (total_requests - 1.0)
                + response_time_ms)
                / total_requests;

            // Update min/max
            self.min_response_time_ms = self.min_response_time_ms.min(response_time_ms);
            self.max_response_time_ms = self.max_response_time_ms.max(response_time_ms);
        }
    }

    /// Get current state information
    pub fn get_state_info(&self) -> CircuitBreakerStateInfo {
        CircuitBreakerStateInfo {
            state: self.circuit_breaker.state.clone(),
            failure_count: self.circuit_breaker.failure_count,
            success_rate: self.circuit_breaker.get_success_rate(),
            total_requests: self.circuit_breaker.total_requests,
            degraded_mode_active: self.degraded_mode_active,
            last_state_change: self.last_state_change_time,
            avg_response_time_ms: self.avg_response_time_ms,
        }
    }

    /// Check if circuit breaker needs attention
    pub fn needs_attention(&self) -> bool {
        matches!(
            self.circuit_breaker.state,
            CircuitBreakerState::Open | CircuitBreakerState::HalfOpen
        ) || self.degraded_mode_active
            || self.circuit_breaker.failure_count >= (self.circuit_breaker.threshold / 2)
    }

    /// Add metadata
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }
}

/// Circuit breaker type classification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CircuitBreakerType {
    /// HTTP API circuit breaker
    HttpApi,
    /// Database circuit breaker
    Database,
    /// KV store circuit breaker
    KvStore,
    /// External service circuit breaker
    ExternalService,
    /// Internal service circuit breaker
    InternalService,
    /// Custom circuit breaker type
    Custom(String),
}

impl CircuitBreakerType {
    pub fn as_str(&self) -> &str {
        match self {
            CircuitBreakerType::HttpApi => "http_api",
            CircuitBreakerType::Database => "database",
            CircuitBreakerType::KvStore => "kv_store",
            CircuitBreakerType::ExternalService => "external_service",
            CircuitBreakerType::InternalService => "internal_service",
            CircuitBreakerType::Custom(name) => name,
        }
    }
}

/// Circuit breaker state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStateInfo {
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub success_rate: f32,
    pub total_requests: u64,
    pub degraded_mode_active: bool,
    pub last_state_change: u64,
    pub avg_response_time_ms: f64,
}

/// Circuit breaker service metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerMetrics {
    pub total_circuit_breakers: u64,
    pub open_circuit_breakers: u64,
    pub half_open_circuit_breakers: u64,
    pub closed_circuit_breakers: u64,
    pub degraded_mode_count: u64,
    pub total_requests: u64,
    pub total_failures: u64,
    pub average_success_rate: f32,
    pub state_changes_last_hour: u64,
    pub alerts_triggered: u64,
    pub auto_recoveries: u64,
    pub manual_overrides: u64,
    pub last_updated: u64,
}

impl Default for CircuitBreakerMetrics {
    fn default() -> Self {
        Self {
            total_circuit_breakers: 0,
            open_circuit_breakers: 0,
            half_open_circuit_breakers: 0,
            closed_circuit_breakers: 0,
            degraded_mode_count: 0,
            total_requests: 0,
            total_failures: 0,
            average_success_rate: 1.0,
            state_changes_last_hour: 0,
            alerts_triggered: 0,
            auto_recoveries: 0,
            manual_overrides: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Main circuit breaker service
pub struct CircuitBreakerService {
    config: CircuitBreakerConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Circuit breaker management
    circuit_breakers: Arc<Mutex<HashMap<String, EnhancedCircuitBreaker>>>,

    // Integration with other services
    health_monitor: Option<Arc<HealthMonitor>>,
    alert_manager: Option<Arc<AlertManager>>,

    // Metrics and performance
    metrics: Arc<Mutex<CircuitBreakerMetrics>>,
}

impl CircuitBreakerService {
    /// Create a new circuit breaker service
    pub async fn new(
        config: CircuitBreakerConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        config.validate()?;

        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        let service = Self {
            config,
            logger,
            kv_store,
            circuit_breakers: Arc::new(Mutex::new(HashMap::new())),
            health_monitor: None,
            alert_manager: None,
            metrics: Arc::new(Mutex::new(CircuitBreakerMetrics::default())),
        };

        service.logger.info("Circuit Breaker Service initialized");
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

    /// Register a new circuit breaker
    pub async fn register_circuit_breaker(
        &self,
        id: String,
        name: String,
        service_name: String,
        breaker_type: CircuitBreakerType,
    ) -> ArbitrageResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check constraints and insert circuit breaker
        {
            let mut circuit_breakers = self.circuit_breakers.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire lock: {}", e))
            })?;

            if circuit_breakers.len() >= self.config.max_circuit_breakers {
                return Err(ArbitrageError::config_error(
                    "Maximum number of circuit breakers reached",
                ));
            }

            if circuit_breakers.contains_key(&id) {
                return Err(ArbitrageError::config_error(format!(
                    "Circuit breaker {} already exists",
                    id
                )));
            }

            let circuit_breaker = EnhancedCircuitBreaker::new(
                id.clone(),
                name.clone(),
                service_name.clone(),
                breaker_type,
                self.config.clone(),
            );

            circuit_breakers.insert(id.clone(), circuit_breaker);
        } // Lock is dropped here

        // Register with health monitor if available
        if let Some(health_monitor) = &self.health_monitor {
            let component_health =
                ComponentHealth::new(id.clone(), name.clone(), "circuit_breaker".to_string())
                    .with_tag("service".to_string(), service_name.clone());

            let _ = health_monitor.register_component(component_health).await;
        }

        // Set up alert rules if alert manager is available
        if let Some(alert_manager) = &self.alert_manager {
            let _ = self.setup_alert_rules(&id, alert_manager).await;
        }

        self.logger.info(&format!(
            "Registered circuit breaker: {} for service: {}",
            name, service_name
        ));
        Ok(())
    }

    /// Execute operation with circuit breaker protection
    pub async fn execute<F, T, E>(
        &self,
        circuit_breaker_id: &str,
        operation: F,
    ) -> ArbitrageResult<T>
    where
        F: FnOnce() -> Result<T, E> + Send,
        E: std::fmt::Display + Send,
    {
        if !self.config.enabled {
            return operation()
                .map_err(|e| ArbitrageError::api_error(format!("Operation failed: {}", e)));
        }

        let start_time = crate::utils::time::now_instant();

        // Get circuit breaker
        let can_execute = {
            let mut circuit_breakers = self.circuit_breakers.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire lock: {}", e))
            })?;

            if let Some(cb) = circuit_breakers.get_mut(circuit_breaker_id) {
                cb.can_execute()
            } else {
                return Err(ArbitrageError::not_found(format!(
                    "Circuit breaker {} not found",
                    circuit_breaker_id
                )));
            }
        };

        if !can_execute {
            self.logger.warn(&format!(
                "Circuit breaker {} is open, operation blocked",
                circuit_breaker_id
            ));
            return Err(ArbitrageError::service_unavailable(
                "Circuit breaker is open",
            ));
        }

        // Execute operation
        let result = operation();
        let response_time = start_time.elapsed().as_millis() as f64;

        // Record result and prepare for async operations
        let (cb_id, should_trigger_alert, should_update_health) = {
            let mut circuit_breakers = self.circuit_breakers.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire lock: {}", e))
            })?;

            if let Some(cb) = circuit_breakers.get_mut(circuit_breaker_id) {
                let cb_id = cb.id.clone();
                let mut should_trigger_alert = false;
                let should_update_health = self.health_monitor.is_some();

                match &result {
                    Ok(_) => {
                        cb.record_success(response_time);
                    }
                    Err(e) => {
                        cb.record_failure(Some(response_time));
                        self.logger.error(&format!(
                            "Operation failed for {}: {}",
                            circuit_breaker_id, e
                        ));

                        // Check if alert should be triggered
                        if matches!(cb.circuit_breaker.state, CircuitBreakerState::Open)
                            && cb.alert_on_open
                        {
                            should_trigger_alert = true;
                        }
                    }
                }

                (Some(cb_id), should_trigger_alert, should_update_health)
            } else {
                (None, false, false)
            }
        }; // Lock is dropped here

        // Perform async operations after releasing the lock
        if let Some(cb_id) = cb_id {
            if should_update_health {
                if let Some(health_monitor) = &self.health_monitor {
                    match &result {
                        Ok(_) => {
                            let _ = health_monitor
                                .update_component_health(
                                    &cb_id,
                                    true,
                                    Some(format!("Operation successful in {}ms", response_time)),
                                )
                                .await;
                        }
                        Err(e) => {
                            let _ = health_monitor
                                .update_component_health(
                                    &cb_id,
                                    false,
                                    Some(format!("Operation failed: {}", e)),
                                )
                                .await;
                        }
                    }
                }
            }

            if should_trigger_alert {
                // Get circuit breaker for alert triggering
                if let Some(cb) = self.get_circuit_breaker_for_alert(circuit_breaker_id).await {
                    let _ = self.trigger_circuit_breaker_alert(&cb).await;
                }
            }
        }

        // Update metrics
        self.update_metrics().await;

        result.map_err(|e| ArbitrageError::api_error(format!("Operation failed: {}", e)))
    }

    /// Get circuit breaker state
    pub async fn get_circuit_breaker_state(
        &self,
        circuit_breaker_id: &str,
    ) -> Option<CircuitBreakerStateInfo> {
        let circuit_breakers = self.circuit_breakers.lock().ok()?;
        circuit_breakers
            .get(circuit_breaker_id)
            .map(|cb| cb.get_state_info())
    }

    /// Helper method to get circuit breaker for alert triggering
    async fn get_circuit_breaker_for_alert(
        &self,
        circuit_breaker_id: &str,
    ) -> Option<EnhancedCircuitBreaker> {
        let circuit_breakers = self.circuit_breakers.lock().ok()?;
        circuit_breakers.get(circuit_breaker_id).cloned()
    }

    /// Get all circuit breaker states
    pub async fn get_all_circuit_breaker_states(&self) -> HashMap<String, CircuitBreakerStateInfo> {
        let circuit_breakers = self
            .circuit_breakers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        circuit_breakers
            .iter()
            .map(|(id, cb)| (id.clone(), cb.get_state_info()))
            .collect()
    }

    /// Force circuit breaker state (for testing/manual override)
    pub async fn force_circuit_breaker_state(
        &self,
        circuit_breaker_id: &str,
        state: CircuitBreakerState,
    ) -> ArbitrageResult<()> {
        let mut circuit_breakers = self.circuit_breakers.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire lock: {}", e))
        })?;

        if let Some(cb) = circuit_breakers.get_mut(circuit_breaker_id) {
            cb.circuit_breaker.state = state.clone();
            cb.last_state_change_time = chrono::Utc::now().timestamp_millis() as u64;

            // Update metrics for manual override
            let mut metrics = self.metrics.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire metrics lock: {}", e))
            })?;
            metrics.manual_overrides += 1;

            self.logger.info(&format!(
                "Manually set circuit breaker {} to state: {:?}",
                circuit_breaker_id, state
            ));
            Ok(())
        } else {
            Err(ArbitrageError::not_found(format!(
                "Circuit breaker {} not found",
                circuit_breaker_id
            )))
        }
    }

    /// Get service metrics
    pub async fn get_metrics(&self) -> CircuitBreakerMetrics {
        self.metrics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Health check for the circuit breaker service
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Check if service is responsive
        let circuit_breaker_count = {
            let circuit_breakers = self.circuit_breakers.lock().map_err(|_| {
                ArbitrageError::internal_error("Circuit breaker service unresponsive")
            })?;
            circuit_breakers.len()
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
            "Circuit breaker service health check passed. Managing {} circuit breakers",
            circuit_breaker_count
        ));
        Ok(true)
    }

    /// Setup alert rules for a circuit breaker
    async fn setup_alert_rules(
        &self,
        circuit_breaker_id: &str,
        alert_manager: &AlertManager,
    ) -> ArbitrageResult<()> {
        // Alert when circuit breaker opens
        let open_alert_rule = AlertRule::new(
            format!("circuit_breaker_open_{}", circuit_breaker_id),
            circuit_breaker_id.to_string(),
            "circuit_breaker_state".to_string(),
            AlertCondition::Equal,
            AlertSeverity::Critical,
            1.0, // Open state = 1
        )
        .with_description(format!("Circuit breaker {} is open", circuit_breaker_id))
        .with_duration(0); // Immediate alert

        // Alert when circuit breaker has high failure rate
        let failure_rate_alert_rule = AlertRule::new(
            format!("circuit_breaker_high_failure_rate_{}", circuit_breaker_id),
            circuit_breaker_id.to_string(),
            "circuit_breaker_failure_rate".to_string(),
            AlertCondition::GreaterThan,
            AlertSeverity::Warning,
            0.5, // 50% failure rate
        )
        .with_description(format!(
            "Circuit breaker {} has high failure rate",
            circuit_breaker_id
        ))
        .with_duration(300); // 5 minutes

        let _ = alert_manager.add_rule(open_alert_rule).await;
        let _ = alert_manager.add_rule(failure_rate_alert_rule).await;

        Ok(())
    }

    /// Trigger circuit breaker alert
    async fn trigger_circuit_breaker_alert(
        &self,
        circuit_breaker: &EnhancedCircuitBreaker,
    ) -> ArbitrageResult<()> {
        if let Some(alert_manager) = &self.alert_manager {
            let _ = alert_manager
                .evaluate_metric(
                    &circuit_breaker.id,
                    "circuit_breaker_state",
                    1.0, // Open state
                )
                .await;
        }
        Ok(())
    }

    /// Update service metrics
    async fn update_metrics(&self) {
        if let Ok(circuit_breakers) = self.circuit_breakers.lock() {
            if let Ok(mut metrics) = self.metrics.lock() {
                metrics.total_circuit_breakers = circuit_breakers.len() as u64;
                metrics.open_circuit_breakers = circuit_breakers
                    .values()
                    .filter(|cb| matches!(cb.circuit_breaker.state, CircuitBreakerState::Open))
                    .count() as u64;
                metrics.half_open_circuit_breakers = circuit_breakers
                    .values()
                    .filter(|cb| matches!(cb.circuit_breaker.state, CircuitBreakerState::HalfOpen))
                    .count() as u64;
                metrics.closed_circuit_breakers = circuit_breakers
                    .values()
                    .filter(|cb| matches!(cb.circuit_breaker.state, CircuitBreakerState::Closed))
                    .count() as u64;
                metrics.degraded_mode_count = circuit_breakers
                    .values()
                    .filter(|cb| cb.degraded_mode_active)
                    .count() as u64;

                let total_requests: u64 = circuit_breakers
                    .values()
                    .map(|cb| cb.circuit_breaker.total_requests)
                    .sum();
                let total_successful: u64 = circuit_breakers
                    .values()
                    .map(|cb| cb.circuit_breaker.successful_requests)
                    .sum();

                metrics.total_requests = total_requests;
                metrics.total_failures = total_requests - total_successful;
                metrics.average_success_rate = if total_requests > 0 {
                    total_successful as f32 / total_requests as f32
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
    fn test_circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        assert!(config.enabled);
        assert_eq!(config.default_failure_threshold, 5);
        assert_eq!(config.default_timeout_seconds, 60);
        assert_eq!(config.min_success_count_half_open, 3);
    }

    #[test]
    fn test_circuit_breaker_config_validation() {
        let mut config = CircuitBreakerConfig::default();
        assert!(config.validate().is_ok());

        config.default_failure_threshold = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_enhanced_circuit_breaker_creation() {
        let config = CircuitBreakerConfig::default();
        let cb = EnhancedCircuitBreaker::new(
            "test-cb".to_string(),
            "Test Circuit Breaker".to_string(),
            "test-service".to_string(),
            CircuitBreakerType::HttpApi,
            config,
        );

        assert_eq!(cb.id, "test-cb");
        assert_eq!(cb.name, "Test Circuit Breaker");
        assert_eq!(cb.service_name, "test-service");
        assert_eq!(cb.breaker_type, CircuitBreakerType::HttpApi);
        assert!(matches!(
            cb.circuit_breaker.state,
            CircuitBreakerState::Closed
        ));
    }

    #[test]
    fn test_enhanced_circuit_breaker_state_transitions() {
        let config = CircuitBreakerConfig::default();
        let mut cb = EnhancedCircuitBreaker::new(
            "test-cb".to_string(),
            "Test Circuit Breaker".to_string(),
            "test-service".to_string(),
            CircuitBreakerType::HttpApi,
            config,
        );

        // Initial state should be closed
        assert!(matches!(
            cb.circuit_breaker.state,
            CircuitBreakerState::Closed
        ));
        assert!(cb.can_execute());

        // Record failures to trigger open state
        for _ in 0..5 {
            cb.record_failure(Some(100.0));
        }

        assert!(matches!(
            cb.circuit_breaker.state,
            CircuitBreakerState::Open
        ));
        assert!(!cb.can_execute());
    }

    #[test]
    fn test_degraded_mode() {
        let config = CircuitBreakerConfig {
            enable_degraded_mode: true,
            ..Default::default()
        };

        let mut cb = EnhancedCircuitBreaker::new(
            "test-cb".to_string(),
            "Test Circuit Breaker".to_string(),
            "test-service".to_string(),
            CircuitBreakerType::HttpApi,
            config,
        );

        assert!(!cb.degraded_mode_active);

        cb.enter_degraded_mode();
        assert!(cb.degraded_mode_active);
        assert!(cb.degraded_mode_start_time.is_some());

        cb.exit_degraded_mode();
        assert!(!cb.degraded_mode_active);
        assert!(cb.degraded_mode_start_time.is_none());
    }

    #[test]
    fn test_response_time_metrics() {
        let config = CircuitBreakerConfig::default();
        let mut cb = EnhancedCircuitBreaker::new(
            "test-cb".to_string(),
            "Test Circuit Breaker".to_string(),
            "test-service".to_string(),
            CircuitBreakerType::HttpApi,
            config,
        );

        cb.record_success(100.0);
        assert_eq!(cb.avg_response_time_ms, 100.0);
        assert_eq!(cb.min_response_time_ms, 100.0);
        assert_eq!(cb.max_response_time_ms, 100.0);

        cb.record_success(200.0);
        assert_eq!(cb.avg_response_time_ms, 150.0);
        assert_eq!(cb.min_response_time_ms, 100.0);
        assert_eq!(cb.max_response_time_ms, 200.0);
    }

    #[test]
    fn test_circuit_breaker_type_as_str() {
        assert_eq!(CircuitBreakerType::HttpApi.as_str(), "http_api");
        assert_eq!(CircuitBreakerType::Database.as_str(), "database");
        assert_eq!(CircuitBreakerType::KvStore.as_str(), "kv_store");
        assert_eq!(
            CircuitBreakerType::Custom("test".to_string()).as_str(),
            "test"
        );
    }

    #[test]
    fn test_circuit_breaker_metrics_default() {
        let metrics = CircuitBreakerMetrics::default();
        assert_eq!(metrics.total_circuit_breakers, 0);
        assert_eq!(metrics.average_success_rate, 1.0);
        assert_eq!(metrics.alerts_triggered, 0);
    }
}
