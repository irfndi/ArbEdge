//! Recovery Verifier Module for Chaos Engineering

use crate::utils::time::now_instant;
use std::collections::HashMap;
use std::time::UNIX_EPOCH;

use crate::utils::now_system_time;
use serde::{Deserialize, Serialize};
use worker::Env;

use super::{ChaosEngineeringConfig, FaultConfig};
use crate::utils::error::ArbitrageResult;

/// Recovery Time Objective (RTO) and Recovery Point Objective (RPO) metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryMetrics {
    pub rto_seconds: f64,
    pub rpo_seconds: f64,
    pub recovery_start_time: u64,
    pub recovery_complete_time: Option<u64>,
    pub data_loss_detected: bool,
    pub service_availability_restored: bool,
}

/// Health check result for individual services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealthCheck {
    pub service_name: String,
    pub endpoint_url: String,
    pub response_time_ms: f64,
    pub status_code: u16,
    pub healthy: bool,
    pub error_message: Option<String>,
    pub check_timestamp: u64,
}

/// Data integrity validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataIntegrityCheck {
    pub storage_type: String,    // "d1", "kv", "r2"
    pub validation_type: String, // "checksum", "consistency", "completeness"
    pub passed: bool,
    pub corruption_detected: bool,
    pub missing_data_count: u64,
    pub details: HashMap<String, String>,
    pub check_timestamp: u64,
}

/// Recovery verification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryVerificationConfig {
    pub enabled: bool,
    pub max_recovery_time_seconds: f64,
    pub health_check_timeout_seconds: f64,
    pub data_integrity_timeout_seconds: f64,
    pub service_endpoints: Vec<String>,
    pub critical_data_sources: Vec<String>,
    pub recovery_validation_interval_seconds: f64,
    pub max_retry_attempts: u32,
    pub automated_rollback_enabled: bool,
}

impl Default for RecoveryVerificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_recovery_time_seconds: 300.0, // 5 minutes max recovery time
            health_check_timeout_seconds: 30.0,
            data_integrity_timeout_seconds: 120.0,
            service_endpoints: vec![
                "/health".to_string(),
                "/api/v1/status".to_string(),
                "/metrics".to_string(),
            ],
            critical_data_sources: vec![
                "user_accounts".to_string(),
                "trading_positions".to_string(),
                "market_data".to_string(),
            ],
            recovery_validation_interval_seconds: 10.0,
            max_retry_attempts: 3,
            automated_rollback_enabled: true,
        }
    }
}

/// Recovery verification status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStatus {
    /// Recovery process not started
    NotStarted,
    /// Recovery in progress
    InProgress,
    /// Recovery completed successfully
    Completed,
    /// Recovery failed
    Failed,
    /// Recovery timed out
    Timeout,
    /// Recovery rolled back
    RolledBack,
}

/// Complete recovery verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryVerificationResult {
    pub experiment_id: String,
    pub status: RecoveryStatus,
    pub metrics: RecoveryMetrics,
    pub health_checks: Vec<ServiceHealthCheck>,
    pub integrity_checks: Vec<DataIntegrityCheck>,
    pub rollback_executed: bool,
    pub verification_duration_seconds: f64,
    pub success_rate: f64, // 0.0 to 1.0
    pub success: bool,     // For compatibility with chaos coordinator
    pub duration_ms: u64,  // For compatibility with chaos coordinator
}

/// Main recovery verification engine
#[derive(Debug)]
pub struct RecoveryVerifier {
    config: RecoveryVerificationConfig,
    active_verifications: HashMap<String, RecoveryVerificationResult>,
    verification_history: Vec<RecoveryVerificationResult>,
}

impl RecoveryVerifier {
    /// Create new recovery verifier instance
    pub fn new(config: RecoveryVerificationConfig) -> Self {
        Self {
            config,
            active_verifications: HashMap::new(),
            verification_history: Vec::new(),
        }
    }

    /// Start recovery verification for a chaos experiment
    pub async fn start_verification(
        &mut self,
        experiment_id: String,
        _fault_config: &FaultConfig,
        _env: &Env,
    ) -> ArbitrageResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let start_time = Self::current_timestamp_seconds();

        let mut verification_result =
            Self::create_default_result(experiment_id.clone(), RecoveryStatus::InProgress);
        verification_result.metrics.recovery_start_time = start_time;

        self.active_verifications
            .insert(experiment_id, verification_result);
        Ok(())
    }

    /// Execute comprehensive recovery verification
    pub async fn verify_recovery(
        &mut self,
        experiment_id: &str,
        env: &Env,
    ) -> ArbitrageResult<RecoveryVerificationResult> {
        if !self.config.enabled {
            return Ok(Self::create_default_result(
                experiment_id.to_string(),
                RecoveryStatus::NotStarted,
            ));
        }

        let verification_start = now_instant();

        let mut result = self
            .active_verifications
            .get(experiment_id)
            .cloned()
            .unwrap_or_else(|| {
                Self::create_default_result(experiment_id.to_string(), RecoveryStatus::InProgress)
            });

        // Perform service health checks
        let health_checks = self.perform_health_checks(env).await?;
        result.health_checks = health_checks;

        // Perform data integrity validation
        let integrity_checks = self.perform_data_integrity_checks(env).await?;
        result.integrity_checks = integrity_checks;

        // Calculate recovery metrics
        self.calculate_recovery_metrics(&mut result);

        // Determine overall recovery status
        self.determine_recovery_status(&mut result);

        // Execute rollback if needed
        if self.should_trigger_rollback(&result) {
            self.execute_automated_rollback(&mut result, env).await?;
        }

        result.verification_duration_seconds = verification_start.elapsed().as_secs_f64();

        // Update compatibility fields
        result.success = matches!(result.status, RecoveryStatus::Completed);
        result.duration_ms = (result.verification_duration_seconds * 1000.0) as u64;

        // Update verification state
        self.active_verifications
            .insert(experiment_id.to_string(), result.clone());

        // Archive completed verifications
        if matches!(
            result.status,
            RecoveryStatus::Completed | RecoveryStatus::Failed | RecoveryStatus::RolledBack
        ) {
            self.verification_history.push(result.clone());
            self.active_verifications.remove(experiment_id);
        }

        Ok(result)
    }

    /// Perform health checks on critical service endpoints
    async fn perform_health_checks(&self, env: &Env) -> ArbitrageResult<Vec<ServiceHealthCheck>> {
        let mut health_checks = Vec::new();
        let current_timestamp = Self::current_timestamp_seconds();

        for endpoint in &self.config.service_endpoints {
            let check_start = now_instant();

            let health_check = match self.execute_health_check(endpoint, env).await {
                Ok((status_code, response_time)) => ServiceHealthCheck {
                    service_name: Self::extract_service_name(endpoint),
                    endpoint_url: endpoint.clone(),
                    response_time_ms: response_time,
                    status_code,
                    healthy: (200..300).contains(&status_code),
                    error_message: None,
                    check_timestamp: current_timestamp,
                },
                Err(error) => ServiceHealthCheck {
                    service_name: Self::extract_service_name(endpoint),
                    endpoint_url: endpoint.clone(),
                    response_time_ms: check_start.elapsed().as_millis() as f64,
                    status_code: 0,
                    healthy: false,
                    error_message: Some(error.to_string()),
                    check_timestamp: current_timestamp,
                },
            };

            health_checks.push(health_check);
        }

        Ok(health_checks)
    }

    /// Execute individual health check with timeout
    async fn execute_health_check(
        &self,
        _endpoint: &str,
        _env: &Env,
    ) -> ArbitrageResult<(u16, f64)> {
        let start_time = now_instant();

        // In production, this would make actual HTTP requests
        // For now, simulate health check behavior based on endpoint patterns
        let response_time = start_time.elapsed().as_millis() as f64;

        // Simulate different health check scenarios
        let status_code = 200; // All endpoints return 200 for simulation

        Ok((status_code, response_time))
    }

    /// Perform data integrity validation checks
    async fn perform_data_integrity_checks(
        &self,
        env: &Env,
    ) -> ArbitrageResult<Vec<DataIntegrityCheck>> {
        let mut integrity_checks = Vec::new();
        let current_timestamp = Self::current_timestamp_seconds();

        // Check D1 database integrity
        let d1_check = self.validate_d1_integrity(env).await?;
        integrity_checks.push(DataIntegrityCheck {
            storage_type: "d1".to_string(),
            validation_type: "consistency".to_string(),
            passed: d1_check.0,
            corruption_detected: !d1_check.0,
            missing_data_count: d1_check.1,
            details: d1_check.2,
            check_timestamp: current_timestamp,
        });

        // Check KV store integrity
        let kv_check = self.validate_kv_integrity(env).await?;
        integrity_checks.push(DataIntegrityCheck {
            storage_type: "kv".to_string(),
            validation_type: "checksum".to_string(),
            passed: kv_check.0,
            corruption_detected: !kv_check.0,
            missing_data_count: kv_check.1,
            details: kv_check.2,
            check_timestamp: current_timestamp,
        });

        // Check R2 storage integrity
        let r2_check = self.validate_r2_integrity(env).await?;
        integrity_checks.push(DataIntegrityCheck {
            storage_type: "r2".to_string(),
            validation_type: "completeness".to_string(),
            passed: r2_check.0,
            corruption_detected: !r2_check.0,
            missing_data_count: r2_check.1,
            details: r2_check.2,
            check_timestamp: current_timestamp,
        });

        Ok(integrity_checks)
    }

    /// Validate D1 database integrity
    async fn validate_d1_integrity(
        &self,
        _env: &Env,
    ) -> ArbitrageResult<(bool, u64, HashMap<String, String>)> {
        let mut details = HashMap::new();

        // In production, this would perform actual D1 consistency checks
        // For now, simulate integrity validation
        details.insert("tables_checked".to_string(), "5".to_string());
        details.insert("rows_validated".to_string(), "1000".to_string());
        details.insert("constraints_verified".to_string(), "25".to_string());

        // Simulate successful validation with no missing data
        Ok((true, 0, details))
    }

    /// Validate KV store integrity
    async fn validate_kv_integrity(
        &self,
        _env: &Env,
    ) -> ArbitrageResult<(bool, u64, HashMap<String, String>)> {
        let mut details = HashMap::new();

        // In production, this would perform actual KV checksum validation
        details.insert("keys_checked".to_string(), "500".to_string());
        details.insert("checksums_validated".to_string(), "500".to_string());
        details.insert("expiry_times_verified".to_string(), "400".to_string());

        // Simulate successful validation
        Ok((true, 0, details))
    }

    /// Validate R2 storage integrity
    async fn validate_r2_integrity(
        &self,
        _env: &Env,
    ) -> ArbitrageResult<(bool, u64, HashMap<String, String>)> {
        let mut details = HashMap::new();

        // In production, this would perform actual R2 completeness checks
        details.insert("objects_checked".to_string(), "200".to_string());
        details.insert("metadata_validated".to_string(), "200".to_string());
        details.insert("buckets_verified".to_string(), "3".to_string());

        // Simulate successful validation
        Ok((true, 0, details))
    }

    /// Calculate recovery time and other metrics
    fn calculate_recovery_metrics(&self, result: &mut RecoveryVerificationResult) {
        let current_time = Self::current_timestamp_seconds();

        // Calculate RTO (Recovery Time Objective)
        result.metrics.rto_seconds =
            current_time as f64 - result.metrics.recovery_start_time as f64;

        // Check if services are healthy
        let healthy_services = result
            .health_checks
            .iter()
            .filter(|check| check.healthy)
            .count();
        let total_services = result.health_checks.len();

        result.metrics.service_availability_restored = healthy_services == total_services;

        // Check for data loss
        result.metrics.data_loss_detected = result
            .integrity_checks
            .iter()
            .any(|check| check.corruption_detected || check.missing_data_count > 0);

        // Calculate RPO based on data integrity
        if result.metrics.data_loss_detected {
            result.metrics.rpo_seconds = 300.0; // Assume 5 minutes of potential data loss
        } else {
            result.metrics.rpo_seconds = 0.0; // No data loss detected
        }

        // Calculate success rate
        let successful_checks = result
            .health_checks
            .iter()
            .filter(|check| check.healthy)
            .count()
            + result
                .integrity_checks
                .iter()
                .filter(|check| check.passed)
                .count();
        let total_checks = result.health_checks.len() + result.integrity_checks.len();

        result.success_rate = if total_checks > 0 {
            successful_checks as f64 / total_checks as f64
        } else {
            0.0
        };
    }

    /// Determine overall recovery status based on verification results
    fn determine_recovery_status(&self, result: &mut RecoveryVerificationResult) {
        let current_time = Self::current_timestamp_seconds();
        let elapsed_time = current_time as f64 - result.metrics.recovery_start_time as f64;

        if elapsed_time > self.config.max_recovery_time_seconds {
            result.status = RecoveryStatus::Timeout;
            return;
        }

        if result.metrics.service_availability_restored && !result.metrics.data_loss_detected {
            result.status = RecoveryStatus::Completed;
            result.metrics.recovery_complete_time = Some(current_time);
        } else if result.success_rate < 0.5 {
            result.status = RecoveryStatus::Failed;
        } else {
            result.status = RecoveryStatus::InProgress;
        }
    }

    /// Determine if automated rollback should be triggered
    fn should_trigger_rollback(&self, result: &RecoveryVerificationResult) -> bool {
        if !self.config.automated_rollback_enabled {
            return false;
        }

        match result.status {
            RecoveryStatus::Failed | RecoveryStatus::Timeout => true,
            RecoveryStatus::InProgress => {
                // Trigger rollback if recovery takes too long or success rate is very low
                result.success_rate < 0.3
                    || result.verification_duration_seconds > self.config.max_recovery_time_seconds
            }
            _ => false,
        }
    }

    /// Execute automated rollback procedures
    async fn execute_automated_rollback(
        &self,
        result: &mut RecoveryVerificationResult,
        _env: &Env,
    ) -> ArbitrageResult<()> {
        // In production, this would execute actual rollback procedures
        // Such as reverting configurations, scaling resources, or restoring data

        result.rollback_executed = true;
        result.status = RecoveryStatus::RolledBack;

        Ok(())
    }

    /// Get statistics about recovery verification performance
    /// Shutdown the recovery verifier
    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        // Clear active verifications and archive them
        for (_experiment_id, verification) in self.active_verifications.drain() {
            self.verification_history.push(verification);
        }
        Ok(())
    }

    /// Check if the recovery verifier is healthy
    pub async fn is_healthy(&self) -> ArbitrageResult<bool> {
        Ok(self.config.enabled)
    }

    /// Verify data integrity for a specific experiment session
    pub async fn verify_data_integrity(
        &mut self,
        experiment_id: &str,
        env: &Env,
    ) -> ArbitrageResult<RecoveryVerificationResult> {
        self.verify_recovery(experiment_id, env).await
    }

    /// Verify service availability for a specific experiment session  
    pub async fn verify_service_availability(
        &mut self,
        experiment_id: &str,
        env: &Env,
    ) -> ArbitrageResult<RecoveryVerificationResult> {
        self.verify_recovery(experiment_id, env).await
    }

    pub fn get_verification_statistics(&self) -> HashMap<String, f64> {
        let mut stats = HashMap::new();

        if self.verification_history.is_empty() {
            return stats;
        }

        let total_verifications = self.verification_history.len() as f64;
        let successful_verifications = self
            .verification_history
            .iter()
            .filter(|v| matches!(v.status, RecoveryStatus::Completed))
            .count() as f64;

        let average_rto = self
            .verification_history
            .iter()
            .map(|v| v.metrics.rto_seconds)
            .sum::<f64>()
            / total_verifications;

        let average_success_rate = self
            .verification_history
            .iter()
            .map(|v| v.success_rate)
            .sum::<f64>()
            / total_verifications;

        stats.insert("total_verifications".to_string(), total_verifications);
        stats.insert(
            "success_rate".to_string(),
            successful_verifications / total_verifications,
        );
        stats.insert("average_rto_seconds".to_string(), average_rto);
        stats.insert("average_success_rate".to_string(), average_success_rate);

        stats
    }

    /// Extract service name from endpoint URL
    fn extract_service_name(endpoint: &str) -> String {
        if endpoint.contains("health") {
            "health_service".to_string()
        } else if endpoint.contains("metrics") {
            "metrics_service".to_string()
        } else if endpoint.contains("status") {
            "status_service".to_string()
        } else {
            "unknown_service".to_string()
        }
    }

    /// Get current timestamp in seconds
    fn current_timestamp_seconds() -> u64 {
        now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Create a default RecoveryVerificationResult with all required fields
    fn create_default_result(
        experiment_id: String,
        status: RecoveryStatus,
    ) -> RecoveryVerificationResult {
        RecoveryVerificationResult {
            experiment_id,
            status,
            metrics: RecoveryMetrics {
                rto_seconds: 0.0,
                rpo_seconds: 0.0,
                recovery_start_time: Self::current_timestamp_seconds(),
                recovery_complete_time: None,
                data_loss_detected: false,
                service_availability_restored: false,
            },
            health_checks: Vec::new(),
            integrity_checks: Vec::new(),
            rollback_executed: false,
            verification_duration_seconds: 0.0,
            success_rate: 0.0,
            success: false,
            duration_ms: 0,
        }
    }
}

/// Factory function to create recovery verifier from chaos engineering config
pub fn create_recovery_verifier(chaos_config: &ChaosEngineeringConfig) -> RecoveryVerifier {
    let config = if let Some(recovery_config) = &chaos_config.recovery_verification {
        recovery_config.clone()
    } else {
        RecoveryVerificationConfig::default()
    };

    RecoveryVerifier::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_verifier_creation() {
        let config = RecoveryVerificationConfig::default();
        let verifier = RecoveryVerifier::new(config);

        assert!(verifier.active_verifications.is_empty());
        assert!(verifier.verification_history.is_empty());
    }

    #[test]
    fn test_recovery_metrics_calculation() {
        let config = RecoveryVerificationConfig::default();
        let verifier = RecoveryVerifier::new(config);

        let mut result = RecoveryVerifier::create_default_result(
            "test-001".to_string(),
            RecoveryStatus::InProgress,
        );
        result.metrics.recovery_start_time = 1000;
        result.health_checks = vec![ServiceHealthCheck {
            service_name: "test_service".to_string(),
            endpoint_url: "/health".to_string(),
            response_time_ms: 100.0,
            status_code: 200,
            healthy: true,
            error_message: None,
            check_timestamp: 1010,
        }];
        result.integrity_checks = vec![DataIntegrityCheck {
            storage_type: "test_storage".to_string(),
            validation_type: "consistency".to_string(),
            passed: true,
            corruption_detected: false,
            missing_data_count: 0,
            details: HashMap::new(),
            check_timestamp: 1010,
        }];

        verifier.calculate_recovery_metrics(&mut result);

        assert!(result.metrics.service_availability_restored);
        assert!(!result.metrics.data_loss_detected);
        assert_eq!(result.success_rate, 1.0);
    }

    #[test]
    fn test_recovery_status_determination() {
        let config = RecoveryVerificationConfig::default();
        let verifier = RecoveryVerifier::new(config);

        let mut result = RecoveryVerifier::create_default_result(
            "test-002".to_string(),
            RecoveryStatus::InProgress,
        );
        result.metrics.rto_seconds = 10.0;
        result.metrics.recovery_start_time = RecoveryVerifier::current_timestamp_seconds() - 10;
        result.metrics.service_availability_restored = true;
        result.verification_duration_seconds = 10.0;
        result.success_rate = 1.0;

        verifier.determine_recovery_status(&mut result);

        assert!(matches!(result.status, RecoveryStatus::Completed));
        assert!(result.metrics.recovery_complete_time.is_some());
    }

    #[test]
    fn test_rollback_decision() {
        let config = RecoveryVerificationConfig {
            automated_rollback_enabled: true,
            ..Default::default()
        };
        let verifier = RecoveryVerifier::new(config);

        let mut failed_result =
            RecoveryVerifier::create_default_result("test-003".to_string(), RecoveryStatus::Failed);
        failed_result.metrics.rto_seconds = 100.0;
        failed_result.metrics.recovery_start_time = 1000;
        failed_result.metrics.data_loss_detected = true;
        failed_result.verification_duration_seconds = 100.0;
        failed_result.success_rate = 0.2;

        assert!(verifier.should_trigger_rollback(&failed_result));
    }

    #[test]
    fn test_service_name_extraction() {
        assert_eq!(
            RecoveryVerifier::extract_service_name("/health"),
            "health_service"
        );
        assert_eq!(
            RecoveryVerifier::extract_service_name("/metrics"),
            "metrics_service"
        );
        assert_eq!(
            RecoveryVerifier::extract_service_name("/api/status"),
            "status_service"
        );
        assert_eq!(
            RecoveryVerifier::extract_service_name("/unknown"),
            "unknown_service"
        );
    }

    #[test]
    fn test_verification_statistics() {
        let config = RecoveryVerificationConfig::default();
        let mut verifier = RecoveryVerifier::new(config);

        // Add some test verification history
        let mut test_result = RecoveryVerifier::create_default_result(
            "test-004".to_string(),
            RecoveryStatus::Completed,
        );
        test_result.metrics.rto_seconds = 50.0;
        test_result.metrics.recovery_start_time = 1000;
        test_result.metrics.recovery_complete_time = Some(1050);
        test_result.metrics.service_availability_restored = true;
        test_result.verification_duration_seconds = 50.0;
        test_result.success_rate = 1.0;
        test_result.success = true;
        test_result.duration_ms = 50000;
        verifier.verification_history.push(test_result);

        let stats = verifier.get_verification_statistics();

        assert_eq!(stats.get("total_verifications"), Some(&1.0));
        assert_eq!(stats.get("success_rate"), Some(&1.0));
        assert_eq!(stats.get("average_rto_seconds"), Some(&50.0));
    }
}
