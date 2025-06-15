//! R2 Object Storage Fault Injection Module
//!
//! Provides fault injection capabilities specifically for Cloudflare R2 object storage:
//! - HTTP 5xx errors and capacity limitations
//! - 403/CORS issues and access denied scenarios
//! - Connection timeouts and bucket unavailability
//! - Multipart upload failures and object corruption
//! - Gradual degradation and intermittent failures

use crate::utils::error::{ArbitrageError, ArbitrageResult, ErrorKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::utils::time::{now_instant, WasmInstant};
use worker::Env;

use super::{ChaosEngineeringConfig, FaultConfig};

/// R2-specific fault injection parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R2FaultParams {
    /// Target bucket name (if specific)
    pub bucket_name: Option<String>,
    /// Object key patterns to affect (regex)
    pub key_patterns: Vec<String>,
    /// R2 operations to affect
    pub operations: Vec<R2Operation>,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Latency injection in milliseconds
    pub latency_ms: Option<u64>,
    /// Object size threshold (affect only large objects)
    pub size_threshold_bytes: Option<u64>,
    /// HTTP status codes to inject
    pub http_status_codes: Vec<u16>,
    /// Specific R2 error types to inject
    pub error_types: Vec<R2ErrorType>,
    /// Concurrent request limit for capacity simulation
    pub concurrent_request_limit: Option<u32>,
}

/// R2 operations that can be affected by faults
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum R2Operation {
    /// GET object
    Get,
    /// PUT object
    Put,
    /// DELETE object
    Delete,
    /// HEAD object (metadata only)
    Head,
    /// LIST objects in bucket
    List,
    /// COPY object
    Copy,
    /// Multipart upload initiation
    MultipartInitiate,
    /// Multipart upload part
    MultipartUpload,
    /// Multipart upload completion
    MultipartComplete,
    /// Multipart upload abort
    MultipartAbort,
    /// Bucket operations (create, delete, list)
    Bucket,
}

/// R2 error types based on Cloudflare documentation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum R2ErrorType {
    /// HTTP 500 - Internal server error
    InternalServerError,
    /// HTTP 502 - Bad gateway
    BadGateway,
    /// HTTP 503 - Service unavailable
    ServiceUnavailable,
    /// HTTP 504 - Gateway timeout
    GatewayTimeout,
    /// HTTP 403 - Access denied / CORS issues
    AccessDenied,
    /// HTTP 404 - Object not found
    NotFound,
    /// HTTP 429 - Too many requests / rate limited
    TooManyRequests,
    /// Connection timeout
    ConnectionTimeout,
    /// Bucket capacity exhausted (5xx error simulation)
    CapacityExhausted,
    /// Multipart upload failure
    MultipartUploadFailure,
    /// Checksum mismatch
    ChecksumMismatch,
    /// Object corruption
    ObjectCorruption,
    /// Network unreachable
    NetworkUnreachable,
    /// SSL/TLS handshake failure
    SSLHandshakeFailure,
}

/// R2 fault injection state
#[derive(Debug, Clone)]
struct R2FaultState {
    #[allow(dead_code)]
    fault_id: String,
    fault_config: FaultConfig,
    params: R2FaultParams,
    activated_at: WasmInstant,
    stats: R2FaultStats,
    concurrent_requests: u32,
}

/// Statistics for R2 fault injection
#[derive(Debug, Clone, Default)]
struct R2FaultStats {
    total_operations: u64,
    failed_operations: u64,
    timeout_operations: u64,
    delayed_operations: u64,
    access_denied_operations: u64,
    capacity_exhausted_operations: u64,
    multipart_failures: u64,
    corruption_operations: u64,
}

/// R2 Object Storage Fault Injector
#[derive(Debug)]
pub struct R2FaultInjector {
    #[allow(dead_code)]
    config: ChaosEngineeringConfig,
    active_faults: HashMap<String, R2FaultState>,
    global_stats: R2FaultStats,
    global_concurrent_requests: u32,
    is_enabled: bool,
    is_initialized: bool,
}

impl R2FaultInjector {
    pub async fn new(config: &ChaosEngineeringConfig, _env: &Env) -> ArbitrageResult<Self> {
        Ok(Self {
            config: config.clone(),
            active_faults: HashMap::new(),
            global_stats: R2FaultStats::default(),
            global_concurrent_requests: 0,
            is_enabled: config.feature_flags.storage_fault_injection,
            is_initialized: true,
        })
    }

    /// Inject an R2-specific fault
    pub async fn inject_fault(
        &mut self,
        fault_id: &str,
        fault_config: &FaultConfig,
    ) -> ArbitrageResult<()> {
        if !self.is_enabled {
            return Err(ArbitrageError::new(
                ErrorKind::ValidationError,
                "R2 fault injection is disabled".to_string(),
            ));
        }

        // Parse R2-specific parameters
        let params = self.parse_r2_params(&fault_config.parameters)?;

        let fault_state = R2FaultState {
            fault_id: fault_id.to_string(),
            fault_config: fault_config.clone(),
            params,
            activated_at: now_instant(),
            stats: R2FaultStats::default(),
            concurrent_requests: 0,
        };

        self.active_faults.insert(fault_id.to_string(), fault_state);
        Ok(())
    }

    /// Remove an R2 fault
    pub async fn remove_fault(&mut self, fault_id: &str) -> ArbitrageResult<()> {
        if let Some(fault_state) = self.active_faults.remove(fault_id) {
            // Aggregate stats
            self.global_stats.total_operations += fault_state.stats.total_operations;
            self.global_stats.failed_operations += fault_state.stats.failed_operations;
            self.global_stats.timeout_operations += fault_state.stats.timeout_operations;
            self.global_stats.delayed_operations += fault_state.stats.delayed_operations;
            self.global_stats.access_denied_operations +=
                fault_state.stats.access_denied_operations;
            self.global_stats.capacity_exhausted_operations +=
                fault_state.stats.capacity_exhausted_operations;
            self.global_stats.multipart_failures += fault_state.stats.multipart_failures;
            self.global_stats.corruption_operations += fault_state.stats.corruption_operations;
        }
        Ok(())
    }

    /// Check if R2 operation should be affected by fault injection
    pub async fn should_inject_fault(
        &mut self,
        bucket_name: &str,
        object_key: &str,
        operation: &R2Operation,
        object_size: Option<u64>,
    ) -> ArbitrageResult<Option<R2FaultInjection>> {
        if !self.is_enabled || self.active_faults.is_empty() {
            return Ok(None);
        }

        // Track concurrent requests
        self.global_concurrent_requests += 1;

        // Collect matching faults first to avoid borrow conflicts
        let matching_faults: Vec<(String, R2FaultState)> = self
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
                Self::matches_fault_criteria_static(
                    &fault_state.params,
                    bucket_name,
                    object_key,
                    operation,
                    object_size,
                )
            })
            .map(|(id, fault_state)| (id.clone(), fault_state.clone()))
            .collect();

        if let Some((fault_id, fault_state)) = matching_faults.first() {
            // Update stats for the matching fault
            if let Some(active_fault) = self.active_faults.get_mut(fault_id) {
                active_fault.stats.total_operations += 1;
                active_fault.concurrent_requests += 1;

                // Check concurrent request limits
                if let Some(limit) = fault_state.params.concurrent_request_limit {
                    if active_fault.concurrent_requests > limit {
                        active_fault.stats.capacity_exhausted_operations += 1;
                        self.global_concurrent_requests -= 1;
                        return Ok(Some(R2FaultInjection {
                            injection_type: R2FaultInjectionType::CapacityExhausted,
                            metadata: Self::create_fault_metadata_static(
                                &fault_state.fault_config,
                                &fault_state.params,
                            ),
                        }));
                    }
                }

                // Determine what type of fault injection to apply
                let injection = Self::determine_fault_injection_static(
                    &fault_state.fault_config,
                    &fault_state.params,
                )?;

                if let Some(ref injection) = injection {
                    match injection.injection_type {
                        R2FaultInjectionType::Failure(_) => {
                            active_fault.stats.failed_operations += 1;
                        }
                        R2FaultInjectionType::Timeout => {
                            active_fault.stats.timeout_operations += 1;
                        }
                        R2FaultInjectionType::Latency(_) => {
                            active_fault.stats.delayed_operations += 1;
                        }
                        R2FaultInjectionType::AccessDenied => {
                            active_fault.stats.access_denied_operations += 1;
                        }
                        R2FaultInjectionType::CapacityExhausted => {
                            active_fault.stats.capacity_exhausted_operations += 1;
                        }
                        R2FaultInjectionType::MultipartFailure => {
                            active_fault.stats.multipart_failures += 1;
                        }
                        R2FaultInjectionType::ObjectCorruption => {
                            active_fault.stats.corruption_operations += 1;
                        }
                        _ => {}
                    }
                }

                // Decrement concurrent request counter when done
                active_fault.concurrent_requests =
                    active_fault.concurrent_requests.saturating_sub(1);
                self.global_concurrent_requests = self.global_concurrent_requests.saturating_sub(1);

                return Ok(injection);
            }
        }

        // Decrement if no fault matched
        self.global_concurrent_requests = self.global_concurrent_requests.saturating_sub(1);
        Ok(None)
    }

    /// Parse R2-specific parameters from fault configuration
    fn parse_r2_params(
        &self,
        parameters: &HashMap<String, String>,
    ) -> ArbitrageResult<R2FaultParams> {
        let bucket_name = parameters.get("bucket_name").cloned();

        let key_patterns = parameters
            .get("key_patterns")
            .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_else(|| vec![".*".to_string()]);

        let operations = parameters
            .get("operations")
            .map(|s| {
                s.split(',')
                    .filter_map(|op| match op.trim().to_lowercase().as_str() {
                        "get" => Some(R2Operation::Get),
                        "put" => Some(R2Operation::Put),
                        "delete" => Some(R2Operation::Delete),
                        "head" => Some(R2Operation::Head),
                        "list" => Some(R2Operation::List),
                        "copy" => Some(R2Operation::Copy),
                        "multipart_initiate" => Some(R2Operation::MultipartInitiate),
                        "multipart_upload" => Some(R2Operation::MultipartUpload),
                        "multipart_complete" => Some(R2Operation::MultipartComplete),
                        "multipart_abort" => Some(R2Operation::MultipartAbort),
                        "bucket" => Some(R2Operation::Bucket),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![R2Operation::Get, R2Operation::Put, R2Operation::Delete]);

        let error_rate = parameters
            .get("error_rate")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.1);

        let latency_ms = parameters.get("latency_ms").and_then(|s| s.parse().ok());

        let size_threshold_bytes = parameters
            .get("size_threshold_bytes")
            .and_then(|s| s.parse().ok());

        let http_status_codes = parameters
            .get("http_status_codes")
            .map(|s| {
                s.split(',')
                    .filter_map(|code| code.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_else(|| vec![500, 502, 503, 504]);

        let error_types = parameters
            .get("error_types")
            .map(|s| {
                s.split(',')
                    .filter_map(|et| match et.trim().to_lowercase().as_str() {
                        "internal_server_error" => Some(R2ErrorType::InternalServerError),
                        "bad_gateway" => Some(R2ErrorType::BadGateway),
                        "service_unavailable" => Some(R2ErrorType::ServiceUnavailable),
                        "gateway_timeout" => Some(R2ErrorType::GatewayTimeout),
                        "access_denied" => Some(R2ErrorType::AccessDenied),
                        "not_found" => Some(R2ErrorType::NotFound),
                        "too_many_requests" => Some(R2ErrorType::TooManyRequests),
                        "connection_timeout" => Some(R2ErrorType::ConnectionTimeout),
                        "capacity_exhausted" => Some(R2ErrorType::CapacityExhausted),
                        "multipart_upload_failure" => Some(R2ErrorType::MultipartUploadFailure),
                        "checksum_mismatch" => Some(R2ErrorType::ChecksumMismatch),
                        "object_corruption" => Some(R2ErrorType::ObjectCorruption),
                        "network_unreachable" => Some(R2ErrorType::NetworkUnreachable),
                        "ssl_handshake_failure" => Some(R2ErrorType::SSLHandshakeFailure),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![R2ErrorType::InternalServerError]);

        let concurrent_request_limit = parameters
            .get("concurrent_request_limit")
            .and_then(|s| s.parse().ok());

        Ok(R2FaultParams {
            bucket_name,
            key_patterns,
            operations,
            error_rate,
            latency_ms,
            size_threshold_bytes,
            http_status_codes,
            error_types,
            concurrent_request_limit,
        })
    }

    /// Check if operation matches fault criteria (instance method for tests)
    #[allow(dead_code)]
    fn matches_fault_criteria(
        &self,
        params: &R2FaultParams,
        bucket_name: &str,
        object_key: &str,
        operation: &R2Operation,
        object_size: Option<u64>,
    ) -> bool {
        Self::matches_fault_criteria_static(params, bucket_name, object_key, operation, object_size)
    }

    /// Check if operation matches fault criteria (static version)
    fn matches_fault_criteria_static(
        params: &R2FaultParams,
        bucket_name: &str,
        object_key: &str,
        operation: &R2Operation,
        object_size: Option<u64>,
    ) -> bool {
        // Check bucket name
        if let Some(ref target_bucket) = params.bucket_name {
            if target_bucket != bucket_name {
                return false;
            }
        }

        // Check operation type
        if !params.operations.contains(operation) {
            return false;
        }

        // Check key patterns
        if !params.key_patterns.iter().any(|pattern| {
            if pattern == ".*" {
                true
            } else {
                // Simple pattern matching for object keys
                object_key.contains(pattern)
                    || Self::matches_regex_pattern_static(object_key, pattern)
            }
        }) {
            return false;
        }

        // Check size threshold if set
        if let Some(threshold) = params.size_threshold_bytes {
            if let Some(size) = object_size {
                if size < threshold {
                    return false;
                }
            }
        }

        true
    }

    /// Simple regex pattern matching (instance method for tests)
    #[allow(dead_code)]
    fn matches_regex_pattern(&self, text: &str, pattern: &str) -> bool {
        Self::matches_regex_pattern_static(text, pattern)
    }

    /// Simple regex pattern matching (static version)
    fn matches_regex_pattern_static(text: &str, pattern: &str) -> bool {
        // Simple wildcard matching for basic patterns
        if pattern == "*" {
            return true;
        }

        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                // Pattern like "prefix*" or "*suffix" or "prefix*suffix"
                let prefix = parts[0];
                let suffix = parts[1];
                return text.starts_with(prefix) && text.ends_with(suffix);
            } else if parts.len() == 3 && parts[0].is_empty() && parts[2].is_empty() {
                // Pattern like "*middle*"
                let middle = parts[1];
                return text.contains(middle);
            } else if parts.len() > 2 {
                // More complex patterns with multiple asterisks
                // For now, just check if text contains all non-empty parts in order
                let mut current_pos = 0;
                for part in parts {
                    if !part.is_empty() {
                        if let Some(pos) = text[current_pos..].find(part) {
                            current_pos += pos + part.len();
                        } else {
                            return false;
                        }
                    }
                }
                return true;
            }
        }
        text == pattern
    }

    /// Determine what fault injection to apply (static version)
    fn determine_fault_injection_static(
        fault_config: &FaultConfig,
        params: &R2FaultParams,
    ) -> ArbitrageResult<Option<R2FaultInjection>> {
        // Use deterministic pseudo-random based on current time and intensity
        let random_value =
            (chrono::Utc::now().timestamp_millis() as f64 * fault_config.intensity) % 1.0;

        if random_value > params.error_rate {
            return Ok(None);
        }

        let injection_type = match fault_config.fault_type.as_str() {
            "Timeout" => R2FaultInjectionType::Timeout,
            "Latency" => {
                if let Some(latency_ms) = params.latency_ms {
                    R2FaultInjectionType::Latency(Duration::from_millis(latency_ms))
                } else {
                    R2FaultInjectionType::Latency(Duration::from_millis(1000))
                }
            }
            "Unavailability" => R2FaultInjectionType::Failure(R2ErrorType::ServiceUnavailable),
            "ResourceExhaustion" => R2FaultInjectionType::CapacityExhausted,
            "DataCorruption" => {
                // Randomly select an error type from configured types
                let error_idx = (random_value * params.error_types.len() as f64) as usize;
                let selected_error = params
                    .error_types
                    .get(error_idx)
                    .unwrap_or(&R2ErrorType::InternalServerError);

                match selected_error {
                    R2ErrorType::AccessDenied => R2FaultInjectionType::AccessDenied,
                    R2ErrorType::CapacityExhausted => R2FaultInjectionType::CapacityExhausted,
                    R2ErrorType::MultipartUploadFailure => R2FaultInjectionType::MultipartFailure,
                    R2ErrorType::ObjectCorruption | R2ErrorType::ChecksumMismatch => {
                        R2FaultInjectionType::ObjectCorruption
                    }
                    _ => R2FaultInjectionType::Failure(selected_error.clone()),
                }
            }
            _ => R2FaultInjectionType::Failure(R2ErrorType::InternalServerError),
        };

        let metadata = Self::create_fault_metadata_static(fault_config, params);

        Ok(Some(R2FaultInjection {
            injection_type,
            metadata,
        }))
    }

    /// Create metadata for fault injection (static version)
    fn create_fault_metadata_static(
        fault_config: &FaultConfig,
        params: &R2FaultParams,
    ) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "fault_intensity".to_string(),
            fault_config.intensity.to_string(),
        );
        metadata.insert("error_rate".to_string(), params.error_rate.to_string());

        if let Some(bucket) = &params.bucket_name {
            metadata.insert("bucket_name".to_string(), bucket.clone());
        }

        metadata.insert(
            "operations".to_string(),
            params
                .operations
                .iter()
                .map(|op| format!("{:?}", op))
                .collect::<Vec<_>>()
                .join(","),
        );

        metadata.insert(
            "http_status_codes".to_string(),
            params
                .http_status_codes
                .iter()
                .map(|code| code.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );

        metadata
    }

    /// Get R2 fault injection statistics
    pub fn get_statistics(&self) -> R2FaultStatistics {
        let active_faults_count = self.active_faults.len();
        let total_operations = self.global_stats.total_operations;
        let failed_operations = self.global_stats.failed_operations;

        let failure_rate = if total_operations > 0 {
            failed_operations as f64 / total_operations as f64
        } else {
            0.0
        };

        let access_denied_rate = if total_operations > 0 {
            self.global_stats.access_denied_operations as f64 / total_operations as f64
        } else {
            0.0
        };

        let corruption_rate = if total_operations > 0 {
            self.global_stats.corruption_operations as f64 / total_operations as f64
        } else {
            0.0
        };

        let multipart_failure_rate = if total_operations > 0 {
            self.global_stats.multipart_failures as f64 / total_operations as f64
        } else {
            0.0
        };

        R2FaultStatistics {
            active_faults_count,
            total_operations,
            failed_operations,
            timeout_operations: self.global_stats.timeout_operations,
            delayed_operations: self.global_stats.delayed_operations,
            access_denied_operations: self.global_stats.access_denied_operations,
            capacity_exhausted_operations: self.global_stats.capacity_exhausted_operations,
            multipart_failures: self.global_stats.multipart_failures,
            corruption_operations: self.global_stats.corruption_operations,
            concurrent_requests: self.global_concurrent_requests,
            failure_rate,
            access_denied_rate,
            corruption_rate,
            multipart_failure_rate,
        }
    }

    /// Health check
    pub async fn is_healthy(&self) -> ArbitrageResult<bool> {
        Ok(self.is_initialized)
    }

    /// Shutdown
    pub async fn shutdown(&mut self) -> ArbitrageResult<()> {
        self.active_faults.clear();
        self.global_concurrent_requests = 0;
        self.is_initialized = false;
        Ok(())
    }
}

/// R2 fault injection result
#[derive(Debug, Clone)]
pub struct R2FaultInjection {
    pub injection_type: R2FaultInjectionType,
    pub metadata: HashMap<String, String>,
}

/// Types of R2 fault injection
#[derive(Debug, Clone)]
pub enum R2FaultInjectionType {
    /// Complete operation failure with specific R2 error
    Failure(R2ErrorType),
    /// Operation timeout
    Timeout,
    /// Latency injection
    Latency(Duration),
    /// Access denied / CORS issues
    AccessDenied,
    /// Capacity exhausted (5xx error simulation)
    CapacityExhausted,
    /// Multipart upload failure
    MultipartFailure,
    /// Object corruption
    ObjectCorruption,
    /// Degraded performance
    Degraded,
    /// Network unreachable
    NetworkUnreachable,
    /// SSL handshake failure
    SSLHandshakeFailure,
}

/// R2 fault injection statistics
#[derive(Debug, Clone)]
pub struct R2FaultStatistics {
    pub active_faults_count: usize,
    pub total_operations: u64,
    pub failed_operations: u64,
    pub timeout_operations: u64,
    pub delayed_operations: u64,
    pub access_denied_operations: u64,
    pub capacity_exhausted_operations: u64,
    pub multipart_failures: u64,
    pub corruption_operations: u64,
    pub concurrent_requests: u32,
    pub failure_rate: f64,
    pub access_denied_rate: f64,
    pub corruption_rate: f64,
    pub multipart_failure_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ChaosEngineeringConfig {
        crate::services::core::infrastructure::chaos_engineering::ChaosEngineeringConfig::default()
    }

    #[test]
    fn test_r2_operation_variants() {
        assert_eq!(R2Operation::Get, R2Operation::Get);
        assert_ne!(R2Operation::Get, R2Operation::Put);
    }

    #[test]
    fn test_r2_error_type_variants() {
        assert_eq!(
            R2ErrorType::InternalServerError,
            R2ErrorType::InternalServerError
        );
        assert_ne!(R2ErrorType::InternalServerError, R2ErrorType::AccessDenied);
    }

    #[test]
    fn test_r2_fault_params_creation() {
        let mut params = HashMap::new();
        params.insert("bucket_name".to_string(), "test-bucket".to_string());
        params.insert("operations".to_string(), "get,put".to_string());
        params.insert("error_rate".to_string(), "0.15".to_string());
        params.insert(
            "error_types".to_string(),
            "access_denied,capacity_exhausted".to_string(),
        );

        let injector = R2FaultInjector {
            config: create_test_config(),
            active_faults: HashMap::new(),
            global_stats: R2FaultStats::default(),
            global_concurrent_requests: 0,
            is_enabled: true,
            is_initialized: true,
        };

        let parsed_params = injector.parse_r2_params(&params).unwrap();
        assert_eq!(parsed_params.bucket_name, Some("test-bucket".to_string()));
        assert_eq!(parsed_params.error_rate, 0.15);
        assert!(parsed_params.operations.contains(&R2Operation::Get));
        assert!(parsed_params.operations.contains(&R2Operation::Put));
        assert!(parsed_params
            .error_types
            .contains(&R2ErrorType::AccessDenied));
        assert!(parsed_params
            .error_types
            .contains(&R2ErrorType::CapacityExhausted));
    }

    #[test]
    fn test_pattern_matching() {
        let injector = R2FaultInjector {
            config: create_test_config(),
            active_faults: HashMap::new(),
            global_stats: R2FaultStats::default(),
            global_concurrent_requests: 0,
            is_enabled: true,
            is_initialized: true,
        };

        // Test wildcard matching
        assert!(injector.matches_regex_pattern("test-file.jpg", "test-*"));
        assert!(injector.matches_regex_pattern("prefix-test-suffix", "*test*"));
        assert!(!injector.matches_regex_pattern("other-file.jpg", "test-*"));

        // Test exact matching
        assert!(injector.matches_regex_pattern("exact-match", "exact-match"));
        assert!(!injector.matches_regex_pattern("exact-match", "different"));
    }

    #[test]
    fn test_fault_criteria_matching() {
        let params = R2FaultParams {
            bucket_name: Some("test-bucket".to_string()),
            key_patterns: vec!["images/*".to_string()],
            operations: vec![R2Operation::Get, R2Operation::Put],
            error_rate: 0.1,
            latency_ms: None,
            size_threshold_bytes: Some(1024),
            http_status_codes: vec![500],
            error_types: vec![R2ErrorType::InternalServerError],
            concurrent_request_limit: None,
        };

        let injector = R2FaultInjector {
            config: create_test_config(),
            active_faults: HashMap::new(),
            global_stats: R2FaultStats::default(),
            global_concurrent_requests: 0,
            is_enabled: true,
            is_initialized: true,
        };

        // Should match
        assert!(injector.matches_fault_criteria(
            &params,
            "test-bucket",
            "images/photo.jpg",
            &R2Operation::Get,
            Some(2048)
        ));

        // Should not match - wrong bucket
        assert!(!injector.matches_fault_criteria(
            &params,
            "other-bucket",
            "images/photo.jpg",
            &R2Operation::Get,
            Some(2048)
        ));

        // Should not match - wrong key pattern
        assert!(!injector.matches_fault_criteria(
            &params,
            "test-bucket",
            "documents/file.pdf",
            &R2Operation::Get,
            Some(2048)
        ));

        // Should not match - wrong operation
        assert!(!injector.matches_fault_criteria(
            &params,
            "test-bucket",
            "images/photo.jpg",
            &R2Operation::Delete,
            Some(2048)
        ));

        // Should not match - file too small
        assert!(!injector.matches_fault_criteria(
            &params,
            "test-bucket",
            "images/photo.jpg",
            &R2Operation::Get,
            Some(512)
        ));
    }

    #[test]
    fn test_r2_fault_statistics_calculation() {
        let stats = R2FaultStatistics {
            active_faults_count: 3,
            total_operations: 200,
            failed_operations: 20,
            timeout_operations: 8,
            delayed_operations: 25,
            access_denied_operations: 5,
            capacity_exhausted_operations: 3,
            multipart_failures: 2,
            corruption_operations: 4,
            concurrent_requests: 15,
            failure_rate: 0.1,
            access_denied_rate: 0.025,
            corruption_rate: 0.02,
            multipart_failure_rate: 0.01,
        };

        assert_eq!(stats.active_faults_count, 3);
        assert_eq!(stats.failure_rate, 0.1);
        assert_eq!(stats.access_denied_rate, 0.025);
        assert_eq!(stats.concurrent_requests, 15);
    }
}
