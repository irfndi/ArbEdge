//! Experiment Engine
//!
//! Core experiment orchestration and state management for chaos engineering

use crate::utils::error::{ArbitrageError, ArbitrageResult, ErrorKind};
use crate::utils::now_system_time;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::UNIX_EPOCH;
use worker::Env;

use super::ChaosEngineeringConfig;

/// Types of chaos experiments
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExperimentType {
    /// Storage system failures (KV, D1, R2)
    StorageFailure,
    /// Network chaos (latency, partitions, packet loss)
    NetworkChaos,
    /// Resource exhaustion (memory, CPU)
    ResourceExhaustion,
    /// Combined multi-system failures
    MultiSystemFailure,
}

/// Experiment execution states
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExperimentState {
    /// Experiment created but not started
    Created,
    /// Experiment is running
    Running,
    /// Faults have been injected
    FaultInjected,
    /// Recovering from faults
    RecoveringFaults,
    /// Experiment completed successfully
    Completed,
    /// Recovery has been verified
    RecoveryVerified,
    /// Experiment failed
    Failed,
    /// Experiment was aborted for safety
    Aborted,
}

/// Chaos Experiment Definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosExperiment {
    /// Unique experiment identifier
    pub id: String,
    /// Human-readable experiment name
    pub name: String,
    /// Experiment description and purpose
    pub description: String,
    /// Type of chaos experiment
    pub experiment_type: ExperimentType,
    /// Experiment duration in seconds
    pub duration_seconds: u64,
    /// Fault injection intensity (0.0 to 1.0)
    pub intensity: f64,
    /// Experiment state tracking
    pub state: ExperimentState,
    /// Timestamps for experiment lifecycle
    pub timestamps: ExperimentTimestamps,
    /// Metrics and results
    pub metrics: Option<ExperimentMetrics>,
}

/// Experiment lifecycle timestamps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentTimestamps {
    /// When experiment was created
    pub created_at: u64,
    /// When experiment started
    pub started_at: Option<u64>,
    /// When faults were injected
    pub fault_injected_at: Option<u64>,
    /// When faults were removed
    pub fault_removed_at: Option<u64>,
    /// When experiment completed
    pub completed_at: Option<u64>,
    /// When recovery was verified
    pub recovery_verified_at: Option<u64>,
}

/// Experiment metrics and results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentMetrics {
    /// Recovery time in milliseconds
    pub recovery_time_ms: Option<u64>,
    /// Data integrity check results
    pub data_integrity_ok: bool,
    /// Service availability during experiment
    pub service_availability_percent: f64,
    /// Error rate during experiment
    pub error_rate_percent: f64,
    /// Number of safety violations
    pub safety_violations: u32,
    /// Custom metrics
    pub custom_metrics: HashMap<String, f64>,
}

/// Experiment Engine for orchestrating chaos experiments
#[derive(Debug)]
pub struct ExperimentEngine {
    config: ChaosEngineeringConfig,
    active_experiments: HashMap<String, ChaosExperiment>,
    experiment_history: Vec<ChaosExperiment>,
    is_initialized: bool,
}

impl ExperimentEngine {
    /// Create a new experiment engine
    pub async fn new(config: &ChaosEngineeringConfig, _env: &Env) -> ArbitrageResult<Self> {
        Ok(Self {
            config: config.clone(),
            active_experiments: HashMap::new(),
            experiment_history: Vec::new(),
            is_initialized: true,
        })
    }

    /// Create a new chaos experiment
    pub fn create_experiment(
        &mut self,
        id: String,
        name: String,
        description: String,
        experiment_type: ExperimentType,
    ) -> ArbitrageResult<&ChaosExperiment> {
        if self.active_experiments.len() >= self.config.max_concurrent_experiments as usize {
            return Err(ArbitrageError::new(
                ErrorKind::Internal,
                "Maximum concurrent experiments reached".to_string(),
            ));
        }

        if self.active_experiments.contains_key(&id) {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                format!("Experiment with id '{}' already exists", id),
            ));
        }

        let experiment = ChaosExperiment::new(id.clone(), name, description, experiment_type);
        self.active_experiments.insert(id.clone(), experiment);

        Ok(self.active_experiments.get(&id).unwrap())
    }

    /// Start an experiment
    pub fn start_experiment(&mut self, experiment_id: &str) -> ArbitrageResult<()> {
        let experiment = self
            .active_experiments
            .get_mut(experiment_id)
            .ok_or_else(|| {
                ArbitrageError::new(
                    ErrorKind::NotFound,
                    format!("Experiment '{}' not found", experiment_id),
                )
            })?;

        if !matches!(experiment.state, ExperimentState::Created) {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                "Experiment can only be started from Created state".to_string(),
            ));
        }

        experiment.update_state(ExperimentState::Running);
        Ok(())
    }

    /// Complete an experiment
    pub fn complete_experiment(&mut self, experiment_id: &str) -> ArbitrageResult<ChaosExperiment> {
        let mut experiment = self
            .active_experiments
            .remove(experiment_id)
            .ok_or_else(|| {
                ArbitrageError::new(
                    ErrorKind::NotFound,
                    format!("Experiment '{}' not found", experiment_id),
                )
            })?;

        experiment.update_state(ExperimentState::Completed);

        // Move to history
        self.experiment_history.push(experiment.clone());

        Ok(experiment)
    }

    /// Abort an experiment for safety reasons
    pub fn abort_experiment(
        &mut self,
        experiment_id: &str,
        _reason: String,
    ) -> ArbitrageResult<ChaosExperiment> {
        let mut experiment = self
            .active_experiments
            .remove(experiment_id)
            .ok_or_else(|| {
                ArbitrageError::new(
                    ErrorKind::NotFound,
                    format!("Experiment '{}' not found", experiment_id),
                )
            })?;

        experiment.update_state(ExperimentState::Aborted);

        // Add abort reason to metrics
        if experiment.metrics.is_none() {
            experiment.metrics = Some(ExperimentMetrics {
                recovery_time_ms: None,
                data_integrity_ok: false,
                service_availability_percent: 0.0,
                error_rate_percent: 100.0,
                safety_violations: 1,
                custom_metrics: HashMap::new(),
            });
        }

        if let Some(ref mut metrics) = experiment.metrics {
            metrics
                .custom_metrics
                .insert("abort_reason".to_string(), 1.0);
        }

        // Move to history
        self.experiment_history.push(experiment.clone());

        Ok(experiment)
    }

    /// Get active experiments
    pub fn get_active_experiments(&self) -> &HashMap<String, ChaosExperiment> {
        &self.active_experiments
    }

    /// Get experiment history
    pub fn get_experiment_history(&self) -> &Vec<ChaosExperiment> {
        &self.experiment_history
    }

    /// Get experiment by ID
    pub fn get_experiment(&self, experiment_id: &str) -> Option<&ChaosExperiment> {
        self.active_experiments.get(experiment_id)
    }

    /// Update experiment metrics
    pub fn update_experiment_metrics(
        &mut self,
        experiment_id: &str,
        metrics: ExperimentMetrics,
    ) -> ArbitrageResult<()> {
        let experiment = self
            .active_experiments
            .get_mut(experiment_id)
            .ok_or_else(|| {
                ArbitrageError::new(
                    ErrorKind::NotFound,
                    format!("Experiment '{}' not found", experiment_id),
                )
            })?;

        experiment.metrics = Some(metrics);
        Ok(())
    }

    /// Check if engine is healthy
    pub async fn is_healthy(&self) -> ArbitrageResult<bool> {
        Ok(self.is_initialized)
    }

    /// Shutdown the experiment engine
    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        self.is_initialized = false;
        Ok(())
    }
}

impl ChaosExperiment {
    /// Create a new chaos experiment
    pub fn new(
        id: String,
        name: String,
        description: String,
        experiment_type: ExperimentType,
    ) -> Self {
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            name,
            description,
            experiment_type,
            duration_seconds: 300, // Default 5 minutes
            intensity: 0.5,        // Default 50%
            state: ExperimentState::Created,
            timestamps: ExperimentTimestamps {
                created_at: now,
                started_at: None,
                fault_injected_at: None,
                fault_removed_at: None,
                completed_at: None,
                recovery_verified_at: None,
            },
            metrics: None,
        }
    }

    /// Update experiment state and timestamp
    pub fn update_state(&mut self, state: ExperimentState) {
        self.state = state;
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        match state {
            ExperimentState::Running => self.timestamps.started_at = Some(now),
            ExperimentState::FaultInjected => self.timestamps.fault_injected_at = Some(now),
            ExperimentState::RecoveringFaults => self.timestamps.fault_removed_at = Some(now),
            ExperimentState::Completed => self.timestamps.completed_at = Some(now),
            ExperimentState::RecoveryVerified => self.timestamps.recovery_verified_at = Some(now),
            _ => {}
        }
    }

    /// Check if experiment has timed out
    pub fn is_timed_out(&self, timeout_seconds: u64) -> bool {
        if let Some(started_at) = self.timestamps.started_at {
            let now = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            return (now - started_at) > timeout_seconds;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_experiment_creation() {
        let experiment = ChaosExperiment::new(
            "test-1".to_string(),
            "Test Experiment".to_string(),
            "A test experiment".to_string(),
            ExperimentType::StorageFailure,
        );

        assert_eq!(experiment.id, "test-1");
        assert_eq!(experiment.name, "Test Experiment");
        assert!(matches!(experiment.state, ExperimentState::Created));
        assert!(experiment.timestamps.created_at > 0);
    }

    #[test]
    fn test_experiment_state_updates() {
        let mut experiment = ChaosExperiment::new(
            "test-2".to_string(),
            "State Test".to_string(),
            "Testing state updates".to_string(),
            ExperimentType::NetworkChaos,
        );

        experiment.update_state(ExperimentState::Running);
        assert!(matches!(experiment.state, ExperimentState::Running));
        assert!(experiment.timestamps.started_at.is_some());

        experiment.update_state(ExperimentState::FaultInjected);
        assert!(matches!(experiment.state, ExperimentState::FaultInjected));
        assert!(experiment.timestamps.fault_injected_at.is_some());
    }
}
