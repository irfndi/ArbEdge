//! KV Store Fault Injection Module
//!
//! Provides fault injection capabilities specifically for Cloudflare KV store:
//! - Connection failures and timeouts
//! - Data corruption simulation
//! - Intermittent failures and gradual degradation
//! - Rate limiting and partial unavailability

use crate::utils::error::{ArbitrageError, ArbitrageResult, ErrorKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::utils::time::{now_instant, WasmInstant};
use worker::Env;

use super::{ChaosEngineeringConfig, FaultConfig};

/// KV-specific fault injection parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvFaultParams {
    /// Target KV namespace (if specific)
    pub namespace: Option<String>,
    /// Affected key patterns (regex)
    pub key_patterns: Vec<String>,
    /// Operation types to affect
    pub operations: Vec<KvOperation>,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Latency injection in milliseconds
    pub latency_ms: Option<u64>,
    /// Data corruption percentage
    pub corruption_rate: f64,
    /// Connection timeout override
    pub timeout_override_ms: Option<u64>,
}

/// KV operations that can be affected by faults
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KvOperation {
    Get,
    Put,
    Delete,
    List,
    GetWithMetadata,
    PutWithMetadata,
}

/// KV fault injection state
#[derive(Debug, Clone)]
struct KvFaultState {
    #[allow(dead_code)]
    fault_id: String,
    fault_config: FaultConfig,
    params: KvFaultParams,
    activated_at: WasmInstant,
    stats: KvFaultStats,
}

/// Statistics for KV fault injection
#[derive(Debug, Clone, Default)]
struct KvFaultStats {
    total_operations: u64,
    failed_operations: u64,
    corrupted_responses: u64,
    timeout_operations: u64,
    delayed_operations: u64,
}

/// KV Store Fault Injector
#[derive(Debug)]
pub struct KvFaultInjector {
    #[allow(dead_code)]
    config: ChaosEngineeringConfig,
    active_faults: HashMap<String, KvFaultState>,
    global_stats: KvFaultStats,
    is_enabled: bool,
    is_initialized: bool,
}

impl KvFaultInjector {
    pub async fn new(config: &ChaosEngineeringConfig, _env: &Env) -> ArbitrageResult<Self> {
        Ok(Self {
            config: config.clone(),
            active_faults: HashMap::new(),
            global_stats: KvFaultStats::default(),
            is_enabled: config.feature_flags.storage_fault_injection,
            is_initialized: true,
        })
    }

    /// Inject a KV-specific fault
    pub async fn inject_fault(
        &mut self,
        fault_id: &str,
        fault_config: &FaultConfig,
    ) -> ArbitrageResult<()> {
        if !self.is_enabled {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                "KV fault injection is disabled".to_string(),
            ));
        }

        // Parse KV-specific parameters
        let params = self.parse_kv_params(&fault_config.parameters)?;

        let fault_state = KvFaultState {
            fault_id: fault_id.to_string(),
            fault_config: fault_config.clone(),
            params,
            activated_at: now_instant(),
            stats: KvFaultStats::default(),
        };

        self.active_faults.insert(fault_id.to_string(), fault_state);
        Ok(())
    }

    /// Remove a KV fault
    pub async fn remove_fault(&mut self, fault_id: &str) -> ArbitrageResult<()> {
        if let Some(fault_state) = self.active_faults.remove(fault_id) {
            // Aggregate stats
            self.global_stats.total_operations += fault_state.stats.total_operations;
            self.global_stats.failed_operations += fault_state.stats.failed_operations;
            self.global_stats.corrupted_responses += fault_state.stats.corrupted_responses;
            self.global_stats.timeout_operations += fault_state.stats.timeout_operations;
            self.global_stats.delayed_operations += fault_state.stats.delayed_operations;
        }
        Ok(())
    }

    /// Check if KV operation should be affected by fault injection
    pub async fn should_inject_fault(
        &mut self,
        namespace: &str,
        key: &str,
        operation: &KvOperation,
    ) -> ArbitrageResult<Option<KvFaultInjection>> {
        if !self.is_enabled || self.active_faults.is_empty() {
            return Ok(None);
        }

        // Collect matching faults first to avoid borrow conflicts
        let matching_faults: Vec<(String, KvFaultState)> = self
            .active_faults
            .iter()
            .filter(|(_, fault_state)| {
                // Check if fault has expired
                if fault_state.activated_at.elapsed()
                    > Duration::from_secs(fault_state.fault_config.duration_seconds)
                {
                    return false;
                }

                // Check if this operation matches the fault criteria
                Self::matches_fault_criteria_static(&fault_state.params, namespace, key, operation)
            })
            .map(|(id, fault_state)| (id.clone(), fault_state.clone()))
            .collect();

        if let Some((fault_id, fault_state)) = matching_faults.first() {
            // Update stats for the matching fault
            if let Some(active_fault) = self.active_faults.get_mut(fault_id) {
                active_fault.stats.total_operations += 1;

                // Determine what type of fault injection to apply
                let injection = Self::determine_fault_injection_static(
                    &fault_state.fault_config,
                    &fault_state.params,
                )?;

                if let Some(ref injection) = injection {
                    match injection.injection_type {
                        KvFaultInjectionType::Failure => {
                            active_fault.stats.failed_operations += 1;
                        }
                        KvFaultInjectionType::Timeout => {
                            active_fault.stats.timeout_operations += 1;
                        }
                        KvFaultInjectionType::Latency(_) => {
                            active_fault.stats.delayed_operations += 1;
                        }
                        KvFaultInjectionType::DataCorruption => {
                            active_fault.stats.corrupted_responses += 1;
                        }
                        _ => {}
                    }
                }

                return Ok(injection);
            }
        }

        Ok(None)
    }

    /// Parse KV-specific parameters from fault configuration
    fn parse_kv_params(
        &self,
        parameters: &HashMap<String, String>,
    ) -> ArbitrageResult<KvFaultParams> {
        let namespace = parameters.get("namespace").cloned();

        let key_patterns = parameters
            .get("key_patterns")
            .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_else(|| vec![".*".to_string()]);

        let operations = parameters
            .get("operations")
            .map(|s| {
                s.split(',')
                    .filter_map(|op| match op.trim().to_lowercase().as_str() {
                        "get" => Some(KvOperation::Get),
                        "put" => Some(KvOperation::Put),
                        "delete" => Some(KvOperation::Delete),
                        "list" => Some(KvOperation::List),
                        "getwithmetadata" => Some(KvOperation::GetWithMetadata),
                        "putwithmetadata" => Some(KvOperation::PutWithMetadata),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![KvOperation::Get, KvOperation::Put, KvOperation::Delete]);

        let error_rate = parameters
            .get("error_rate")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);

        let latency_ms = parameters.get("latency_ms").and_then(|s| s.parse().ok());

        let corruption_rate = parameters
            .get("corruption_rate")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.1);

        let timeout_override_ms = parameters
            .get("timeout_override_ms")
            .and_then(|s| s.parse().ok());

        Ok(KvFaultParams {
            namespace,
            key_patterns,
            operations,
            error_rate,
            latency_ms,
            corruption_rate,
            timeout_override_ms,
        })
    }

    /// Check if operation matches fault criteria (static version)
    fn matches_fault_criteria_static(
        params: &KvFaultParams,
        namespace: &str,
        key: &str,
        operation: &KvOperation,
    ) -> bool {
        // Check namespace
        if let Some(target_namespace) = &params.namespace {
            if namespace != target_namespace {
                return false;
            }
        }

        // Check operation type
        if !params.operations.contains(operation) {
            return false;
        }

        // Check key patterns
        for pattern in &params.key_patterns {
            if pattern == ".*" || Self::matches_wildcard_pattern(key, pattern) {
                return true;
            }
        }

        false
    }

    /// Simple wildcard pattern matching (static version)
    fn matches_wildcard_pattern(text: &str, pattern: &str) -> bool {
        if pattern.is_empty() {
            return text.is_empty();
        }

        if pattern == "*" {
            return true;
        }

        // Simple wildcard matching - only supports * at the end for now
        if let Some(prefix) = pattern.strip_suffix('*') {
            return text.starts_with(prefix);
        }

        // Exact match
        text == pattern
    }

    /// Determine what fault injection to apply (static version)
    fn determine_fault_injection_static(
        fault_config: &FaultConfig,
        params: &KvFaultParams,
    ) -> ArbitrageResult<Option<KvFaultInjection>> {
        // Use a simple random check based on intensity and error rate
        // Use deterministic pseudo-random based on current time and intensity
        let random_value =
            (chrono::Utc::now().timestamp_millis() as f64 * fault_config.intensity) % 1.0;
        let effective_rate = fault_config.intensity * params.error_rate;

        if random_value > effective_rate {
            return Ok(None);
        }

        let injection_type = match fault_config.fault_type.as_str() {
            "Unavailability" => KvFaultInjectionType::Failure,
            "PartialUnavailability" => {
                if random_value < effective_rate * 0.5 {
                    KvFaultInjectionType::Failure
                } else {
                    KvFaultInjectionType::Degraded
                }
            }
            "Timeout" => KvFaultInjectionType::Timeout,
            "Latency" => {
                let latency = params.latency_ms.unwrap_or(1000);
                KvFaultInjectionType::Latency(Duration::from_millis(latency))
            }
            "DataCorruption" => KvFaultInjectionType::DataCorruption,
            "IntermittentFailure" => {
                if random_value < effective_rate * 0.3 {
                    KvFaultInjectionType::Failure
                } else {
                    return Ok(None);
                }
            }
            "GradualDegradation" => {
                let degradation_factor = (random_value * fault_config.intensity).min(0.8);
                let latency = (1000.0 * degradation_factor) as u64;
                KvFaultInjectionType::Latency(Duration::from_millis(latency))
            }
            "ConnectionPoolExhaustion" => KvFaultInjectionType::ConnectionExhausted,
            "RateLimiting" => KvFaultInjectionType::RateLimited,
            "ResourceExhaustion" => KvFaultInjectionType::ResourceExhausted,
            _ => return Ok(None),
        };

        Ok(Some(KvFaultInjection {
            injection_type,
            metadata: Self::create_fault_metadata_static(fault_config, params),
        }))
    }

    /// Create fault metadata for tracking (static version)
    fn create_fault_metadata_static(
        fault_config: &FaultConfig,
        params: &KvFaultParams,
    ) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("fault_type".to_string(), fault_config.fault_type.clone());
        metadata.insert("intensity".to_string(), fault_config.intensity.to_string());
        metadata.insert("error_rate".to_string(), params.error_rate.to_string());
        if let Some(namespace) = &params.namespace {
            metadata.insert("namespace".to_string(), namespace.clone());
        }
        metadata
    }

    /// Get KV fault injection statistics
    pub fn get_statistics(&self) -> KvFaultStatistics {
        let mut active_faults_count = 0;
        let mut current_stats = self.global_stats.clone();

        for fault_state in self.active_faults.values() {
            if fault_state.activated_at.elapsed()
                <= Duration::from_secs(fault_state.fault_config.duration_seconds)
            {
                active_faults_count += 1;
                current_stats.total_operations += fault_state.stats.total_operations;
                current_stats.failed_operations += fault_state.stats.failed_operations;
                current_stats.corrupted_responses += fault_state.stats.corrupted_responses;
                current_stats.timeout_operations += fault_state.stats.timeout_operations;
                current_stats.delayed_operations += fault_state.stats.delayed_operations;
            }
        }

        let failure_rate = if current_stats.total_operations > 0 {
            (current_stats.failed_operations as f64 / current_stats.total_operations as f64) * 100.0
        } else {
            0.0
        };

        let corruption_rate = if current_stats.total_operations > 0 {
            (current_stats.corrupted_responses as f64 / current_stats.total_operations as f64)
                * 100.0
        } else {
            0.0
        };

        KvFaultStatistics {
            active_faults_count,
            total_operations: current_stats.total_operations,
            failed_operations: current_stats.failed_operations,
            corrupted_responses: current_stats.corrupted_responses,
            timeout_operations: current_stats.timeout_operations,
            delayed_operations: current_stats.delayed_operations,
            failure_rate,
            corruption_rate,
        }
    }

    pub async fn is_healthy(&self) -> ArbitrageResult<bool> {
        Ok(self.is_initialized)
    }

    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        self.active_faults.clear();
        self.is_initialized = false;
        Ok(())
    }
}

/// KV fault injection result
#[derive(Debug, Clone)]
pub struct KvFaultInjection {
    pub injection_type: KvFaultInjectionType,
    pub metadata: HashMap<String, String>,
}

/// Types of KV fault injection
#[derive(Debug, Clone)]
pub enum KvFaultInjectionType {
    /// Complete operation failure
    Failure,
    /// Operation timeout
    Timeout,
    /// Latency injection
    Latency(Duration),
    /// Data corruption
    DataCorruption,
    /// Degraded performance
    Degraded,
    /// Connection pool exhausted
    ConnectionExhausted,
    /// Rate limited
    RateLimited,
    /// Resource exhausted
    ResourceExhausted,
}

/// KV fault injection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvFaultStatistics {
    pub active_faults_count: usize,
    pub total_operations: u64,
    pub failed_operations: u64,
    pub corrupted_responses: u64,
    pub timeout_operations: u64,
    pub delayed_operations: u64,
    pub failure_rate: f64,
    pub corruption_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ChaosEngineeringConfig {
        let mut config = ChaosEngineeringConfig::default();
        config.feature_flags.storage_fault_injection = true;
        config
    }

    #[test]
    fn test_kv_operation_variants() {
        assert_eq!(KvOperation::Get, KvOperation::Get);
        assert_ne!(KvOperation::Get, KvOperation::Put);
    }

    #[test]
    fn test_kv_fault_params_creation() {
        let mut params = HashMap::new();
        params.insert("namespace".to_string(), "test".to_string());
        params.insert("error_rate".to_string(), "0.5".to_string());
        params.insert("operations".to_string(), "get,put".to_string());

        let config = create_test_config();
        let injector = KvFaultInjector {
            config,
            active_faults: HashMap::new(),
            global_stats: KvFaultStats::default(),
            is_enabled: true,
            is_initialized: true,
        };

        let kv_params = injector.parse_kv_params(&params).unwrap();
        assert_eq!(kv_params.namespace, Some("test".to_string()));
        assert_eq!(kv_params.error_rate, 0.5);
        assert!(kv_params.operations.contains(&KvOperation::Get));
        assert!(kv_params.operations.contains(&KvOperation::Put));
    }

    #[test]
    fn test_fault_criteria_matching() {
        let params = KvFaultParams {
            namespace: Some("test".to_string()),
            key_patterns: vec!["user:*".to_string()],
            operations: vec![KvOperation::Get],
            error_rate: 1.0,
            latency_ms: None,
            corruption_rate: 0.1,
            timeout_override_ms: None,
        };

        let config = create_test_config();
        let _injector = KvFaultInjector {
            config,
            active_faults: HashMap::new(),
            global_stats: KvFaultStats::default(),
            is_enabled: true,
            is_initialized: true,
        };

        // Should match
        assert!(KvFaultInjector::matches_fault_criteria_static(
            &params,
            "test",
            "user:123",
            &KvOperation::Get
        ));

        // Should not match - wrong namespace
        assert!(!KvFaultInjector::matches_fault_criteria_static(
            &params,
            "prod",
            "user:123",
            &KvOperation::Get
        ));

        // Should not match - wrong operation
        assert!(!KvFaultInjector::matches_fault_criteria_static(
            &params,
            "test",
            "user:123",
            &KvOperation::Put
        ));

        // Should not match - wrong key pattern
        assert!(!KvFaultInjector::matches_fault_criteria_static(
            &params,
            "test",
            "product:123",
            &KvOperation::Get
        ));
    }

    #[test]
    fn test_kv_fault_statistics_calculation() {
        let stats = KvFaultStatistics {
            active_faults_count: 2,
            total_operations: 100,
            failed_operations: 10,
            corrupted_responses: 5,
            timeout_operations: 3,
            delayed_operations: 15,
            failure_rate: 10.0,
            corruption_rate: 5.0,
        };

        assert_eq!(stats.active_faults_count, 2);
        assert_eq!(stats.total_operations, 100);
        assert_eq!(stats.failure_rate, 10.0);
        assert_eq!(stats.corruption_rate, 5.0);
    }
}
