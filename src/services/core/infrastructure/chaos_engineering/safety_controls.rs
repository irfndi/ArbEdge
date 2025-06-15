//! Safety Controls Module for Chaos Engineering

use crate::utils::error::{ArbitrageError, ArbitrageResult};
use crate::utils::now_system_time;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::Env;

use super::ChaosEngineeringConfig;

/// Safety rule types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SafetyRuleType {
    /// Maximum error rate threshold
    ErrorRateThreshold,
    /// Maximum response time threshold
    ResponseTimeThreshold,
    /// Minimum service availability
    AvailabilityThreshold,
    /// Maximum resource utilization
    ResourceUtilizationThreshold,
    /// Custom safety rule
    Custom,
}

/// Safety rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyRule {
    pub rule_id: String,
    pub rule_type: SafetyRuleType,
    pub threshold_value: f64,
    pub check_interval_seconds: u64,
    pub enabled: bool,
    pub description: String,
}

/// Safety violation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyViolation {
    pub violation_id: String,
    pub rule_id: String,
    pub experiment_id: String,
    pub violation_time: u64,
    pub current_value: f64,
    pub threshold_value: f64,
    pub severity: ViolationSeverity,
    pub details: String,
}

/// Severity levels for safety violations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ViolationSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Safety Controller for experiment safety management
#[derive(Debug)]
pub struct SafetyController {
    #[allow(dead_code)]
    config: ChaosEngineeringConfig,
    safety_rules: HashMap<String, SafetyRule>,
    violations: Vec<SafetyViolation>,
    is_initialized: bool,
}

impl SafetyController {
    pub async fn new(config: &ChaosEngineeringConfig, _env: &Env) -> ArbitrageResult<Self> {
        let mut controller = Self {
            config: config.clone(),
            safety_rules: HashMap::new(),
            violations: Vec::new(),
            is_initialized: true,
        };

        // Initialize default safety rules
        controller.initialize_default_rules()?;

        Ok(controller)
    }

    /// Initialize default safety rules
    fn initialize_default_rules(&mut self) -> ArbitrageResult<()> {
        // Error rate threshold
        let error_rule = SafetyRule {
            rule_id: "default-error-rate".to_string(),
            rule_type: SafetyRuleType::ErrorRateThreshold,
            threshold_value: 50.0, // 50% error rate
            check_interval_seconds: 30,
            enabled: true,
            description: "Abort experiment if error rate exceeds 50%".to_string(),
        };
        self.safety_rules
            .insert(error_rule.rule_id.clone(), error_rule);

        // Response time threshold
        let response_rule = SafetyRule {
            rule_id: "default-response-time".to_string(),
            rule_type: SafetyRuleType::ResponseTimeThreshold,
            threshold_value: 5000.0, // 5 seconds
            check_interval_seconds: 30,
            enabled: true,
            description: "Abort experiment if response time exceeds 5 seconds".to_string(),
        };
        self.safety_rules
            .insert(response_rule.rule_id.clone(), response_rule);

        // Availability threshold
        let availability_rule = SafetyRule {
            rule_id: "default-availability".to_string(),
            rule_type: SafetyRuleType::AvailabilityThreshold,
            threshold_value: 80.0, // 80% availability
            check_interval_seconds: 60,
            enabled: true,
            description: "Abort experiment if availability drops below 80%".to_string(),
        };
        self.safety_rules
            .insert(availability_rule.rule_id.clone(), availability_rule);

        Ok(())
    }

    /// Add a new safety rule
    pub fn add_safety_rule(&mut self, rule: SafetyRule) -> ArbitrageResult<()> {
        if self.safety_rules.contains_key(&rule.rule_id) {
            return Err(ArbitrageError::new(
                crate::utils::error::ErrorKind::ValidationError,
                format!("Safety rule '{}' already exists", rule.rule_id),
            ));
        }

        self.safety_rules.insert(rule.rule_id.clone(), rule);
        Ok(())
    }

    /// Remove a safety rule
    pub fn remove_safety_rule(&mut self, rule_id: &str) -> ArbitrageResult<()> {
        self.safety_rules.remove(rule_id).ok_or_else(|| {
            ArbitrageError::new(
                crate::utils::error::ErrorKind::NotFound,
                format!("Safety rule '{}' not found", rule_id),
            )
        })?;
        Ok(())
    }

    /// Check safety rules against current metrics
    pub fn check_safety_violations(
        &mut self,
        experiment_id: &str,
        metrics: &HashMap<String, f64>,
    ) -> ArbitrageResult<Vec<SafetyViolation>> {
        let mut violations = Vec::new();
        let current_time = now_system_time()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for rule in self.safety_rules.values() {
            if !rule.enabled {
                continue;
            }

            let current_value = match rule.rule_type {
                SafetyRuleType::ErrorRateThreshold => {
                    metrics.get("error_rate_percent").copied().unwrap_or(0.0)
                }
                SafetyRuleType::ResponseTimeThreshold => {
                    metrics.get("response_time_ms").copied().unwrap_or(0.0)
                }
                SafetyRuleType::AvailabilityThreshold => metrics
                    .get("availability_percent")
                    .copied()
                    .unwrap_or(100.0),
                SafetyRuleType::ResourceUtilizationThreshold => metrics
                    .get("resource_utilization_percent")
                    .copied()
                    .unwrap_or(0.0),
                SafetyRuleType::Custom => metrics.get(&rule.rule_id).copied().unwrap_or(0.0),
            };

            let violation_occurred = match rule.rule_type {
                SafetyRuleType::ErrorRateThreshold
                | SafetyRuleType::ResponseTimeThreshold
                | SafetyRuleType::ResourceUtilizationThreshold => {
                    current_value > rule.threshold_value
                }
                SafetyRuleType::AvailabilityThreshold => current_value < rule.threshold_value,
                SafetyRuleType::Custom => {
                    current_value > rule.threshold_value // Default behavior for custom rules
                }
            };

            if violation_occurred {
                let severity = self.calculate_violation_severity(
                    &rule.rule_type,
                    current_value,
                    rule.threshold_value,
                );

                let violation = SafetyViolation {
                    violation_id: format!("{}-{}-{}", experiment_id, rule.rule_id, current_time),
                    rule_id: rule.rule_id.clone(),
                    experiment_id: experiment_id.to_string(),
                    violation_time: current_time,
                    current_value,
                    threshold_value: rule.threshold_value,
                    severity,
                    details: format!(
                        "Safety rule '{}' violated: current value {} exceeds threshold {}",
                        rule.rule_id, current_value, rule.threshold_value
                    ),
                };

                violations.push(violation.clone());
                self.violations.push(violation);
            }
        }

        Ok(violations)
    }

    /// Calculate violation severity based on how much the threshold is exceeded
    fn calculate_violation_severity(
        &self,
        rule_type: &SafetyRuleType,
        current: f64,
        threshold: f64,
    ) -> ViolationSeverity {
        let ratio = match rule_type {
            SafetyRuleType::AvailabilityThreshold => {
                // For availability, lower is worse
                threshold / current.max(1.0)
            }
            _ => {
                // For other metrics, higher is worse
                current / threshold.max(1.0)
            }
        };

        if ratio >= 2.0 {
            ViolationSeverity::Critical
        } else if ratio >= 1.5 {
            ViolationSeverity::High
        } else if ratio >= 1.2 {
            ViolationSeverity::Medium
        } else {
            ViolationSeverity::Low
        }
    }

    /// Get all safety rules
    pub fn get_safety_rules(&self) -> &HashMap<String, SafetyRule> {
        &self.safety_rules
    }

    /// Get all violations
    pub fn get_violations(&self) -> &Vec<SafetyViolation> {
        &self.violations
    }

    /// Get violations for a specific experiment
    pub fn get_experiment_violations(&self, experiment_id: &str) -> Vec<&SafetyViolation> {
        self.violations
            .iter()
            .filter(|v| v.experiment_id == experiment_id)
            .collect()
    }

    /// Check if an experiment should be aborted based on violations
    pub fn should_abort_experiment(&self, experiment_id: &str) -> bool {
        let critical_violations = self
            .get_experiment_violations(experiment_id)
            .iter()
            .any(|v| v.severity == ViolationSeverity::Critical);

        let high_violations_count = self
            .get_experiment_violations(experiment_id)
            .iter()
            .filter(|v| v.severity == ViolationSeverity::High)
            .count();

        critical_violations || high_violations_count >= 3
    }

    pub async fn is_healthy(&self) -> ArbitrageResult<bool> {
        Ok(self.is_initialized)
    }

    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        self.is_initialized = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ChaosEngineeringConfig {
        ChaosEngineeringConfig::default()
    }

    #[test]
    fn test_safety_rule_creation() {
        let rule = SafetyRule {
            rule_id: "test-rule".to_string(),
            rule_type: SafetyRuleType::ErrorRateThreshold,
            threshold_value: 25.0,
            check_interval_seconds: 30,
            enabled: true,
            description: "Test rule".to_string(),
        };

        assert_eq!(rule.rule_id, "test-rule");
        assert_eq!(rule.threshold_value, 25.0);
        assert!(rule.enabled);
    }

    #[test]
    fn test_violation_severity_calculation() {
        let config = create_test_config();
        let controller = SafetyController {
            config,
            safety_rules: HashMap::new(),
            violations: Vec::new(),
            is_initialized: true,
        };

        // Test critical violation (2x threshold)
        let severity = controller.calculate_violation_severity(
            &SafetyRuleType::ErrorRateThreshold,
            100.0,
            50.0,
        );
        assert_eq!(severity, ViolationSeverity::Critical);

        // Test high violation (1.5x threshold)
        let severity = controller.calculate_violation_severity(
            &SafetyRuleType::ErrorRateThreshold,
            75.0,
            50.0,
        );
        assert_eq!(severity, ViolationSeverity::High);

        // Test medium violation (1.2x threshold)
        let severity = controller.calculate_violation_severity(
            &SafetyRuleType::ErrorRateThreshold,
            60.0,
            50.0,
        );
        assert_eq!(severity, ViolationSeverity::Medium);
    }
}
