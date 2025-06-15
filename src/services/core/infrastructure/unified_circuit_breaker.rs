// Unified Circuit Breaker System - Consolidates circuit breaker functionality to eliminate duplication
// Combines features from circuit_breaker_service.rs, cache_layer.rs, and impact_analysis.rs

use crate::utils::time::{now_instant, WasmInstant};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::utils::{ArbitrageError, ArbitrageResult};

/// Unified circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedCircuitBreakerConfig {
    /// Enable circuit breaker functionality
    pub enabled: bool,
    /// Failure threshold before opening circuit
    pub failure_threshold: u32,
    /// Success threshold for closing circuit from half-open state
    pub success_threshold: u32,
    /// Timeout before attempting recovery (seconds)
    pub timeout_seconds: u64,
    /// Retry timeout for additional recovery attempts
    pub retry_timeout_seconds: Option<u64>,
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

impl Default for UnifiedCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_threshold: 5,
            success_threshold: 3,
            timeout_seconds: 60,
            retry_timeout_seconds: Some(300), // 5 minutes
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

impl UnifiedCircuitBreakerConfig {
    /// High performance configuration
    pub fn high_performance() -> Self {
        Self {
            check_interval_seconds: 10,
            enable_metrics_collection: true,
            enable_auto_recovery: true,
            timeout_seconds: 30,
            retry_timeout_seconds: Some(120),
            max_circuit_breakers: 200,
            failure_threshold: 3,
            success_threshold: 2,
            ..Default::default()
        }
    }

    /// High reliability configuration
    pub fn high_reliability() -> Self {
        Self {
            failure_threshold: 10,
            success_threshold: 5,
            timeout_seconds: 120,
            retry_timeout_seconds: Some(600),
            min_success_count_half_open: 5,
            enable_health_integration: true,
            enable_alert_integration: true,
            enable_degraded_mode: true,
            degraded_mode_timeout_seconds: 600,
            ..Default::default()
        }
    }

    /// Cache-optimized configuration
    pub fn cache_optimized() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_seconds: 60,
            retry_timeout_seconds: Some(180),
            enable_degraded_mode: false, // Cache doesn't need degraded mode
            enable_kv_storage: false,    // Cache manages its own state
            check_interval_seconds: 15,
            ..Default::default()
        }
    }

    /// Impact analysis optimized configuration
    pub fn impact_analysis_optimized() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_seconds: 60,
            retry_timeout_seconds: Some(300),
            enable_degraded_mode: false, // Analysis doesn't need degraded mode
            enable_auto_recovery: true,
            check_interval_seconds: 60,
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.failure_threshold == 0 {
            return Err(ArbitrageError::config_error(
                "Failure threshold must be greater than 0",
            ));
        }

        if self.success_threshold == 0 {
            return Err(ArbitrageError::config_error(
                "Success threshold must be greater than 0",
            ));
        }

        if self.timeout_seconds == 0 {
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

/// Unified circuit breaker state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnifiedCircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl UnifiedCircuitBreakerState {
    pub fn as_str(&self) -> &str {
        match self {
            UnifiedCircuitBreakerState::Closed => "closed",
            UnifiedCircuitBreakerState::Open => "open",
            UnifiedCircuitBreakerState::HalfOpen => "half_open",
        }
    }

    pub fn is_operational(&self) -> bool {
        matches!(
            self,
            UnifiedCircuitBreakerState::Closed | UnifiedCircuitBreakerState::HalfOpen
        )
    }
}

/// Unified circuit breaker implementation
#[derive(Debug)]
pub struct UnifiedCircuitBreaker {
    id: String,
    config: UnifiedCircuitBreakerConfig,
    state: UnifiedCircuitBreakerState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<WasmInstant>,
    last_success_time: Option<WasmInstant>,
    last_state_change: WasmInstant,
    total_requests: u64,
    total_failures: u64,
    total_successes: u64,
    degraded_mode_active: bool,
    degraded_mode_start_time: Option<WasmInstant>,
    metadata: HashMap<String, String>,
}

impl UnifiedCircuitBreaker {
    pub fn new(id: String, config: UnifiedCircuitBreakerConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        Ok(Self {
            id,
            config,
            state: UnifiedCircuitBreakerState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            last_success_time: None,
            last_state_change: now_instant(),
            total_requests: 0,
            total_failures: 0,
            total_successes: 0,
            degraded_mode_active: false,
            degraded_mode_start_time: None,
            metadata: HashMap::new(),
        })
    }

    pub fn can_execute(&mut self) -> bool {
        if !self.config.enabled {
            return true;
        }

        self.check_degraded_mode_timeout();

        match self.state {
            UnifiedCircuitBreakerState::Closed => true,
            UnifiedCircuitBreakerState::HalfOpen => true,
            UnifiedCircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    let elapsed = last_failure.elapsed();
                    if elapsed.as_secs() >= self.config.timeout_seconds {
                        self.transition_to_half_open();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    }

    pub fn record_success(&mut self) {
        self.total_requests += 1;
        self.total_successes += 1;
        self.last_success_time = Some(now_instant());

        match self.state {
            UnifiedCircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    self.transition_to_closed();
                }
            }
            UnifiedCircuitBreakerState::Closed => {
                self.failure_count = 0; // Reset failure count on success
            }
            UnifiedCircuitBreakerState::Open => {
                // Should not happen, but reset if it does
                self.failure_count = 0;
            }
        }
    }

    pub fn record_failure(&mut self) {
        self.total_requests += 1;
        self.total_failures += 1;
        self.failure_count += 1;
        self.last_failure_time = Some(now_instant());

        match self.state {
            UnifiedCircuitBreakerState::Closed => {
                if self.failure_count >= self.config.failure_threshold {
                    self.transition_to_open();
                }
            }
            UnifiedCircuitBreakerState::HalfOpen => {
                self.transition_to_open();
            }
            UnifiedCircuitBreakerState::Open => {
                // Already open, just update failure time
            }
        }
    }

    pub fn force_state(&mut self, state: UnifiedCircuitBreakerState) {
        self.state = state;
        self.last_state_change = now_instant();
        self.failure_count = 0;
        self.success_count = 0;
    }

    pub fn enter_degraded_mode(&mut self) {
        if self.config.enable_degraded_mode {
            self.degraded_mode_active = true;
            self.degraded_mode_start_time = Some(now_instant());
        }
    }

    pub fn exit_degraded_mode(&mut self) {
        self.degraded_mode_active = false;
        self.degraded_mode_start_time = None;
    }

    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    pub fn get_state(&self) -> &UnifiedCircuitBreakerState {
        &self.state
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }

    pub fn get_failure_count(&self) -> u32 {
        self.failure_count
    }

    pub fn get_success_count(&self) -> u32 {
        self.success_count
    }

    pub fn get_success_rate(&self) -> f32 {
        if self.total_requests == 0 {
            return 1.0;
        }
        self.total_successes as f32 / self.total_requests as f32
    }

    pub fn is_degraded_mode_active(&self) -> bool {
        self.degraded_mode_active
    }

    pub fn get_metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    pub fn get_state_info(&self) -> UnifiedCircuitBreakerStateInfo {
        UnifiedCircuitBreakerStateInfo {
            id: self.id.clone(),
            state: self.state.clone(),
            failure_count: self.failure_count,
            success_count: self.success_count,
            success_rate: self.get_success_rate(),
            total_requests: self.total_requests,
            degraded_mode_active: self.degraded_mode_active,
            last_state_change: self.last_state_change,
            time_in_current_state: self.last_state_change.elapsed(),
        }
    }

    fn transition_to_open(&mut self) {
        self.state = UnifiedCircuitBreakerState::Open;
        self.last_state_change = now_instant();
        self.success_count = 0;

        if self.config.enable_degraded_mode {
            self.enter_degraded_mode();
        }
    }

    fn transition_to_half_open(&mut self) {
        self.state = UnifiedCircuitBreakerState::HalfOpen;
        self.last_state_change = now_instant();
        self.success_count = 0;
        self.failure_count = 0;
    }

    fn transition_to_closed(&mut self) {
        self.state = UnifiedCircuitBreakerState::Closed;
        self.last_state_change = now_instant();
        self.failure_count = 0;
        self.success_count = 0;
        self.exit_degraded_mode();
    }

    fn check_degraded_mode_timeout(&mut self) {
        if self.degraded_mode_active {
            if let Some(start_time) = self.degraded_mode_start_time {
                let elapsed = start_time.elapsed();
                if elapsed.as_secs() > self.config.degraded_mode_timeout_seconds {
                    self.exit_degraded_mode();
                }
            }
        }
    }
}

/// Circuit breaker state information
#[derive(Debug, Clone)]
pub struct UnifiedCircuitBreakerStateInfo {
    pub id: String,
    pub state: UnifiedCircuitBreakerState,
    pub failure_count: u32,
    pub success_count: u32,
    pub success_rate: f32,
    pub total_requests: u64,
    pub degraded_mode_active: bool,
    pub last_state_change: WasmInstant,
    pub time_in_current_state: Duration,
}

/// Circuit breaker type for categorization
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnifiedCircuitBreakerType {
    HttpApi,
    Database,
    KvStore,
    Cache,
    ExternalService,
    InternalService,
    ImpactAnalysis,
    Custom(String),
}

impl UnifiedCircuitBreakerType {
    pub fn as_str(&self) -> &str {
        match self {
            UnifiedCircuitBreakerType::HttpApi => "http_api",
            UnifiedCircuitBreakerType::Database => "database",
            UnifiedCircuitBreakerType::KvStore => "kv_store",
            UnifiedCircuitBreakerType::Cache => "cache",
            UnifiedCircuitBreakerType::ExternalService => "external_service",
            UnifiedCircuitBreakerType::InternalService => "internal_service",
            UnifiedCircuitBreakerType::ImpactAnalysis => "impact_analysis",
            UnifiedCircuitBreakerType::Custom(name) => name,
        }
    }
}

/// Circuit breaker manager for handling multiple circuit breakers
#[derive(Debug)]
pub struct UnifiedCircuitBreakerManager {
    config: UnifiedCircuitBreakerConfig,
    circuit_breakers: Arc<RwLock<HashMap<String, Arc<Mutex<UnifiedCircuitBreaker>>>>>,
}

impl UnifiedCircuitBreakerManager {
    pub fn new(config: UnifiedCircuitBreakerConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        Ok(Self {
            config,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn get_or_create_circuit_breaker(
        &self,
        id: String,
        breaker_type: UnifiedCircuitBreakerType,
    ) -> ArbitrageResult<Arc<Mutex<UnifiedCircuitBreaker>>> {
        let circuit_breakers = self.circuit_breakers.read().unwrap();

        if let Some(breaker) = circuit_breakers.get(&id) {
            return Ok(breaker.clone());
        }

        drop(circuit_breakers);

        // Create new circuit breaker
        let mut config = self.config.clone();

        // Customize config based on type
        match breaker_type {
            UnifiedCircuitBreakerType::Cache => {
                config = UnifiedCircuitBreakerConfig::cache_optimized();
            }
            UnifiedCircuitBreakerType::ImpactAnalysis => {
                config = UnifiedCircuitBreakerConfig::impact_analysis_optimized();
            }
            _ => {}
        }

        let breaker = Arc::new(Mutex::new(UnifiedCircuitBreaker::new(id.clone(), config)?));

        let mut circuit_breakers = self.circuit_breakers.write().unwrap();
        circuit_breakers.insert(id, breaker.clone());

        Ok(breaker)
    }

    pub async fn execute_with_circuit_breaker<F, T, E>(
        &self,
        id: String,
        breaker_type: UnifiedCircuitBreakerType,
        operation: F,
    ) -> Result<T, ArbitrageError>
    where
        F: FnOnce() -> Result<T, E>,
        E: std::fmt::Display,
    {
        let breaker = self.get_or_create_circuit_breaker(id, breaker_type).await?;

        let can_execute = {
            let mut breaker_guard = breaker.lock().unwrap();
            breaker_guard.can_execute()
        };

        if !can_execute {
            return Err(ArbitrageError::infrastructure_error(
                "Circuit breaker is open",
            ));
        }

        match operation() {
            Ok(result) => {
                let mut breaker_guard = breaker.lock().unwrap();
                breaker_guard.record_success();
                Ok(result)
            }
            Err(error) => {
                let mut breaker_guard = breaker.lock().unwrap();
                breaker_guard.record_failure();
                Err(ArbitrageError::infrastructure_error(format!(
                    "Operation failed: {}",
                    error
                )))
            }
        }
    }

    pub async fn get_all_states(&self) -> HashMap<String, UnifiedCircuitBreakerStateInfo> {
        let circuit_breakers = self.circuit_breakers.read().unwrap();
        let mut states = HashMap::new();

        for (id, breaker) in circuit_breakers.iter() {
            let breaker_guard = breaker.lock().unwrap();
            states.insert(id.clone(), breaker_guard.get_state_info());
        }

        states
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_circuit_breaker_config_default() {
        let config = UnifiedCircuitBreakerConfig::default();
        assert!(config.enabled);
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 3);
        assert_eq!(config.timeout_seconds, 60);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_unified_circuit_breaker_config_validation() {
        let config = UnifiedCircuitBreakerConfig {
            failure_threshold: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_unified_circuit_breaker_creation() {
        let config = UnifiedCircuitBreakerConfig::default();
        let breaker = UnifiedCircuitBreaker::new("test".to_string(), config);
        assert!(breaker.is_ok());

        let breaker = breaker.unwrap();
        assert_eq!(breaker.get_id(), "test");
        assert_eq!(*breaker.get_state(), UnifiedCircuitBreakerState::Closed);
    }

    #[test]
    fn test_circuit_breaker_state_transitions() {
        let config = UnifiedCircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            ..Default::default()
        };

        let mut breaker = UnifiedCircuitBreaker::new("test".to_string(), config).unwrap();

        // Start in closed state
        assert_eq!(*breaker.get_state(), UnifiedCircuitBreakerState::Closed);
        assert!(breaker.can_execute());

        // Record failures to open circuit
        breaker.record_failure();
        assert_eq!(*breaker.get_state(), UnifiedCircuitBreakerState::Closed);

        breaker.record_failure();
        assert_eq!(*breaker.get_state(), UnifiedCircuitBreakerState::Open);

        // Force to half-open
        breaker.force_state(UnifiedCircuitBreakerState::HalfOpen);
        assert_eq!(*breaker.get_state(), UnifiedCircuitBreakerState::HalfOpen);

        // Record successes to close circuit
        breaker.record_success();
        breaker.record_success();
        assert_eq!(*breaker.get_state(), UnifiedCircuitBreakerState::Closed);
    }
}
