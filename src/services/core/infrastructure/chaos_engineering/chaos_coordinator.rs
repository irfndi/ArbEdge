//! Chaos Coordinator Module for Experiment Orchestration

use crate::utils::error::{ArbitrageError, ArbitrageResult};
use crate::utils::now_system_time;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::UNIX_EPOCH;
use worker::Env;

use super::{
    experiment_engine::{ExperimentEngine, ExperimentType},
    fault_injection::{FaultConfig, FaultInjector},
    recovery_verifier::{create_recovery_verifier, RecoveryVerificationResult, RecoveryVerifier},
    safety_controls::{SafetyController, SafetyViolation},
    ChaosEngineeringConfig,
};

/// Chaos Coordinator Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosCoordinatorMetrics {
    pub total_experiments_run: u64,
    pub successful_experiments: u64,
    pub failed_experiments: u64,
    pub aborted_experiments: u64,
    pub total_faults_injected: u64,
    pub total_safety_violations: u64,
    pub average_recovery_time_ms: f64,
    pub system_resilience_score: f64,
}

/// Chaos Coordinator Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosCoordinatorConfig {
    pub enabled: bool,
    pub max_concurrent_experiments: u32,
    pub default_experiment_timeout_seconds: u64,
    pub safety_check_interval_seconds: u64,
    pub auto_recovery_enabled: bool,
    pub metrics_collection_enabled: bool,
}

/// Experiment orchestration session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSession {
    pub session_id: String,
    pub experiment_id: String,
    pub current_phase: OrchestrationPhase,
    pub start_time: u64,
    pub fault_configs: Vec<FaultConfig>,
    pub safety_violations: Vec<SafetyViolation>,
    pub verification_results: Vec<RecoveryVerificationResult>,
    pub session_metrics: HashMap<String, f64>,
}

/// Phases of experiment orchestration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrchestrationPhase {
    Initializing,
    InjectingFaults,
    MonitoringEffects,
    RecoveringFaults,
    VerifyingRecovery,
    Completed,
    Aborted,
}

/// Main Chaos Coordinator for orchestrating experiments
pub struct ChaosCoordinator {
    config: ChaosEngineeringConfig,
    coordinator_config: ChaosCoordinatorConfig,
    experiment_engine: Option<ExperimentEngine>,
    fault_injector: Option<FaultInjector>,
    recovery_verifier: Option<RecoveryVerifier>,
    safety_controller: Option<SafetyController>,
    active_sessions: HashMap<String, OrchestrationSession>,
    metrics: ChaosCoordinatorMetrics,
    env: Option<worker::Env>,
    is_initialized: bool,
}

impl ChaosCoordinator {
    pub async fn new(config: &ChaosEngineeringConfig, env: &Env) -> ArbitrageResult<Self> {
        let coordinator_config = ChaosCoordinatorConfig {
            enabled: config.enabled,
            max_concurrent_experiments: config.max_concurrent_experiments,
            default_experiment_timeout_seconds: config.default_experiment_timeout_seconds,
            safety_check_interval_seconds: 10, // Default safety check interval
            auto_recovery_enabled: true,
            metrics_collection_enabled: true,
        };

        let mut coordinator = Self {
            config: config.clone(),
            coordinator_config,
            experiment_engine: None,
            fault_injector: None,
            recovery_verifier: None,
            safety_controller: None,
            active_sessions: HashMap::new(),
            metrics: ChaosCoordinatorMetrics::default(),
            env: Some(env.clone()),
            is_initialized: false,
        };

        // Initialize components
        coordinator.experiment_engine = Some(ExperimentEngine::new(config, env).await?);
        coordinator.fault_injector = Some(FaultInjector::new(config, env).await?);
        coordinator.recovery_verifier = Some(create_recovery_verifier(config));
        coordinator.safety_controller = Some(SafetyController::new(config, env).await?);

        coordinator.is_initialized = true;
        Ok(coordinator)
    }

    /// Start orchestrating a chaos experiment
    pub async fn orchestrate_experiment(
        &mut self,
        experiment_id: String,
        experiment_type: ExperimentType,
        fault_configs: Vec<FaultConfig>,
    ) -> ArbitrageResult<String> {
        if !self.config.enabled {
            return Err(ArbitrageError::new(
                crate::utils::error::ErrorKind::Internal,
                "Chaos engineering is disabled".to_string(),
            ));
        }

        if self.active_sessions.len() >= self.coordinator_config.max_concurrent_experiments as usize
        {
            return Err(ArbitrageError::new(
                crate::utils::error::ErrorKind::Internal,
                "Maximum concurrent orchestration sessions reached".to_string(),
            ));
        }

        let session_id = format!(
            "session-{}-{}",
            experiment_id,
            now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        let session = OrchestrationSession {
            session_id: session_id.clone(),
            experiment_id: experiment_id.clone(),
            current_phase: OrchestrationPhase::Initializing,
            start_time: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            fault_configs,
            safety_violations: Vec::new(),
            verification_results: Vec::new(),
            session_metrics: HashMap::new(),
        };

        self.active_sessions.insert(session_id.clone(), session);

        // Create experiment in engine
        if let Some(ref mut engine) = self.experiment_engine {
            engine.create_experiment(
                experiment_id.clone(),
                "Orchestrated Experiment".to_string(),
                "Experiment orchestrated by chaos coordinator".to_string(),
                experiment_type,
            )?;
        }

        // Start the orchestration process
        self.advance_orchestration_phase(&session_id).await?;

        Ok(session_id)
    }

    /// Advance the orchestration to the next phase
    async fn advance_orchestration_phase(&mut self, session_id: &str) -> ArbitrageResult<()> {
        let session = self.active_sessions.get_mut(session_id).ok_or_else(|| {
            ArbitrageError::new(
                crate::utils::error::ErrorKind::NotFound,
                format!("Orchestration session '{}' not found", session_id),
            )
        })?;

        match session.current_phase {
            OrchestrationPhase::Initializing => {
                session.current_phase = OrchestrationPhase::InjectingFaults;
                Box::pin(self.inject_faults_phase(session_id)).await?;
            }
            OrchestrationPhase::InjectingFaults => {
                session.current_phase = OrchestrationPhase::MonitoringEffects;
                Box::pin(self.monitor_effects_phase(session_id)).await?;
            }
            OrchestrationPhase::MonitoringEffects => {
                session.current_phase = OrchestrationPhase::RecoveringFaults;
                Box::pin(self.recover_faults_phase(session_id)).await?;
            }
            OrchestrationPhase::RecoveringFaults => {
                session.current_phase = OrchestrationPhase::VerifyingRecovery;
                Box::pin(self.verify_recovery_phase(session_id)).await?;
            }
            OrchestrationPhase::VerifyingRecovery => {
                session.current_phase = OrchestrationPhase::Completed;
                Box::pin(self.complete_orchestration(session_id)).await?;
            }
            _ => {
                // Already completed or aborted
            }
        }

        Ok(())
    }

    /// Inject faults phase
    async fn inject_faults_phase(&mut self, session_id: &str) -> ArbitrageResult<()> {
        let fault_configs = if let Some(session) = self.active_sessions.get(session_id) {
            session.fault_configs.clone()
        } else {
            return Err(ArbitrageError::new(
                crate::utils::error::ErrorKind::NotFound,
                format!("Session '{}' not found", session_id),
            ));
        };

        if let Some(ref mut injector) = self.fault_injector {
            for (i, fault_config) in fault_configs.iter().enumerate() {
                let fault_id = format!("{}-fault-{}", session_id, i);
                injector
                    .inject_fault(fault_id, fault_config.clone())
                    .await?;
            }
        }

        self.metrics.total_faults_injected += fault_configs.len() as u64;
        Ok(())
    }

    /// Monitor effects phase
    async fn monitor_effects_phase(&mut self, session_id: &str) -> ArbitrageResult<()> {
        // Simulate monitoring for a short period
        // In real implementation, this would collect actual metrics
        let mut metrics = HashMap::new();
        metrics.insert("error_rate_percent".to_string(), 25.0);
        metrics.insert("response_time_ms".to_string(), 1200.0);
        metrics.insert("availability_percent".to_string(), 95.0);

        // Check safety violations
        if let Some(ref mut safety) = self.safety_controller {
            let experiment_id = self
                .active_sessions
                .get(session_id)
                .map(|s| s.experiment_id.clone())
                .unwrap_or_default();

            let violations = safety.check_safety_violations(&experiment_id, &metrics)?;

            if let Some(session) = self.active_sessions.get_mut(session_id) {
                session.safety_violations.extend(violations.clone());
                session.session_metrics.extend(metrics);
            }

            // Check if experiment should be aborted
            if safety.should_abort_experiment(&experiment_id) {
                self.abort_orchestration(session_id, "Safety violations detected")
                    .await?;
                return Ok(());
            }

            self.metrics.total_safety_violations += violations.len() as u64;
        }

        // Continue to next phase after monitoring
        self.advance_orchestration_phase(session_id).await?;
        Ok(())
    }

    /// Recover faults phase
    async fn recover_faults_phase(&mut self, session_id: &str) -> ArbitrageResult<()> {
        // Remove all injected faults
        if let Some(ref mut injector) = self.fault_injector {
            let fault_count = if let Some(session) = self.active_sessions.get(session_id) {
                session.fault_configs.len()
            } else {
                0
            };

            for i in 0..fault_count {
                let fault_id = format!("{}-fault-{}", session_id, i);
                let _ = injector.remove_fault(&fault_id).await; // Ignore errors during cleanup
            }
        }

        // Continue to next phase
        self.advance_orchestration_phase(session_id).await?;
        Ok(())
    }

    /// Verify recovery phase
    async fn verify_recovery_phase(&mut self, session_id: &str) -> ArbitrageResult<()> {
        if let (Some(ref mut verifier), Some(ref env)) = (&mut self.recovery_verifier, &self.env) {
            // Verify data integrity
            let integrity_result = verifier.verify_data_integrity(session_id, env).await?;

            // Verify service availability
            let availability_result = verifier
                .verify_service_availability(session_id, env)
                .await?;

            // Store results in session
            if let Some(session) = self.active_sessions.get_mut(session_id) {
                session.verification_results.push(integrity_result);
                session.verification_results.push(availability_result);
            }
        }

        // Continue to completion
        self.advance_orchestration_phase(session_id).await?;
        Ok(())
    }

    /// Complete orchestration
    async fn complete_orchestration(&mut self, session_id: &str) -> ArbitrageResult<()> {
        if let Some(session) = self.active_sessions.get(session_id) {
            let success = session.verification_results.iter().all(|r| r.success)
                && session.safety_violations.is_empty();

            if success {
                self.metrics.successful_experiments += 1;
            } else {
                self.metrics.failed_experiments += 1;
            }

            self.metrics.total_experiments_run += 1;

            // Calculate recovery time
            let recovery_time_ms = session
                .verification_results
                .iter()
                .map(|r| r.duration_ms)
                .sum::<u64>();

            self.update_average_recovery_time(recovery_time_ms as f64);
        }

        Ok(())
    }

    /// Abort orchestration
    async fn abort_orchestration(
        &mut self,
        session_id: &str,
        _reason: &str,
    ) -> ArbitrageResult<()> {
        if let Some(session) = self.active_sessions.get_mut(session_id) {
            session.current_phase = OrchestrationPhase::Aborted;
            session
                .session_metrics
                .insert("abort_reason".to_string(), 1.0);
        }

        // Clean up any active faults
        self.recover_faults_phase(session_id).await?;

        self.metrics.aborted_experiments += 1;
        self.metrics.total_experiments_run += 1;

        Ok(())
    }

    /// Update average recovery time
    fn update_average_recovery_time(&mut self, new_recovery_time: f64) {
        let total_experiments = self.metrics.total_experiments_run as f64;
        if total_experiments > 0.0 {
            self.metrics.average_recovery_time_ms = (self.metrics.average_recovery_time_ms
                * (total_experiments - 1.0)
                + new_recovery_time)
                / total_experiments;
        }
    }

    /// Get coordinator metrics
    pub fn get_metrics(&self) -> &ChaosCoordinatorMetrics {
        &self.metrics
    }

    /// Get active sessions
    pub fn get_active_sessions(&self) -> &HashMap<String, OrchestrationSession> {
        &self.active_sessions
    }

    pub async fn is_healthy(&self) -> ArbitrageResult<bool> {
        Ok(self.is_initialized
            && self.experiment_engine.is_some()
            && self.fault_injector.is_some()
            && self.recovery_verifier.is_some()
            && self.safety_controller.is_some())
    }

    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        // Abort all active sessions
        let session_ids: Vec<String> = self.active_sessions.keys().cloned().collect();
        for session_id in session_ids {
            let _ = self
                .abort_orchestration(&session_id, "System shutdown")
                .await;
        }

        self.is_initialized = false;
        Ok(())
    }
}

impl Default for ChaosCoordinatorMetrics {
    fn default() -> Self {
        Self {
            total_experiments_run: 0,
            successful_experiments: 0,
            failed_experiments: 0,
            aborted_experiments: 0,
            total_faults_injected: 0,
            total_safety_violations: 0,
            average_recovery_time_ms: 0.0,
            system_resilience_score: 100.0, // Start with perfect score
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestration_session_creation() {
        let session = OrchestrationSession {
            session_id: "test-session-1".to_string(),
            experiment_id: "test-exp-1".to_string(),
            current_phase: OrchestrationPhase::Initializing,
            start_time: 1000000000,
            fault_configs: Vec::new(),
            safety_violations: Vec::new(),
            verification_results: Vec::new(),
            session_metrics: HashMap::new(),
        };

        assert_eq!(session.session_id, "test-session-1");
        assert_eq!(session.current_phase, OrchestrationPhase::Initializing);
    }

    #[test]
    fn test_chaos_coordinator_metrics_default() {
        let metrics = ChaosCoordinatorMetrics::default();
        assert_eq!(metrics.total_experiments_run, 0);
        assert_eq!(metrics.successful_experiments, 0);
        assert_eq!(metrics.system_resilience_score, 100.0);
    }
}
