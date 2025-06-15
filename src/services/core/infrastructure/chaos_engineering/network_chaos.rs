// src/services/core/infrastructure/chaos_engineering/network_chaos.rs

//! Network Chaos Simulation for Cloudflare Workers
//!
//! This module provides comprehensive network fault injection capabilities specifically
//! designed for the Cloudflare Workers runtime environment. It implements chaos engineering
//! patterns for:
//! - Fetch API latency injection and timeouts
//! - Subrequest limit exhaustion simulation
//! - Connection failure patterns
//! - TCP socket connection simulation (where available)
//! - Service binding communication faults
//!
//! All implementations are based on official Cloudflare Workers documentation and constraints:
//! - Max 6 simultaneous connections per invocation
//! - 50/1000 subrequests per request (Free/Paid)
//! - Workers-specific network stack limitations

use crate::utils::time::{now_instant, WasmInstant};
use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use worker::Env;

use super::{ChaosEngineeringConfig, FaultConfig};
use crate::utils::error::ArbitrageResult;

/// Network operations available in Cloudflare Workers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkOperation {
    /// HTTP fetch requests (most common)
    FetchRequest,
    /// Service binding calls to other Workers
    ServiceBinding,
    /// TCP socket connections (where available)
    TcpSocket,
    /// Cache API operations
    CacheOperation,
    /// WebSocket connections
    WebSocket,
    /// Internal subrequests to Cloudflare services
    InternalSubrequest,
}

/// Types of network faults that can be injected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkFaultType {
    /// Artificial latency injection
    LatencyInjection {
        /// Additional milliseconds to add
        additional_ms: u64,
        /// Whether to add to connection or data transfer
        inject_on_connect: bool,
    },
    /// Connection timeout simulation
    ConnectionTimeout {
        /// Timeout duration in milliseconds
        timeout_ms: u64,
    },
    /// Connection refused/failed
    ConnectionFailure {
        /// HTTP error code to simulate
        error_code: u16,
        /// Error message
        error_message: String,
    },
    /// Simulate reaching subrequest limits
    SubrequestLimitExhaustion,
    /// Simulate connection limit exhaustion
    ConnectionLimitExhaustion,
    /// DNS resolution failure
    DnsFailure,
    /// SSL/TLS handshake failure
    TlsHandshakeFailure,
    /// Bandwidth throttling simulation
    BandwidthThrottling {
        /// Maximum bytes per second
        max_bytes_per_second: u64,
    },
}

/// Parameters for network fault injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkFaultParams {
    /// Target operations to affect
    pub target_operations: Vec<NetworkOperation>,
    /// URL patterns to match (supports wildcards)
    pub url_patterns: Vec<String>,
    /// HTTP methods to target
    pub target_methods: Vec<String>,
    /// Headers to match for triggering faults
    pub header_patterns: HashMap<String, String>,
    /// Origin domains to target
    pub target_origins: Vec<String>,
    /// Maximum concurrent connections to simulate
    pub connection_limit: Option<u32>,
    /// Subrequest count threshold
    pub subrequest_limit: Option<u32>,
    /// Enable Worker-to-Worker fault injection
    pub enable_service_binding_faults: bool,
    /// Enable internal service faults (KV, D1, R2)
    pub enable_internal_service_faults: bool,
}

/// Network fault injection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkFaultInjection {
    /// Type of fault injected
    pub fault_type: NetworkFaultType,
    /// Additional metadata for tracking
    pub metadata: HashMap<String, String>,
}

/// Network fault injection statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkFaultStats {
    /// Total network operations monitored
    pub total_operations: u64,
    /// Total faults injected
    pub total_faults_injected: u64,
    /// Latency injections performed
    pub latency_injections: u64,
    /// Connection timeouts simulated
    pub connection_timeouts: u64,
    /// Connection failures simulated
    pub connection_failures: u64,
    /// Subrequest limit violations triggered
    pub subrequest_limit_violations: u64,
    /// Connection limit violations triggered
    pub connection_limit_violations: u64,
    /// DNS failures simulated
    pub dns_failures: u64,
    /// TLS handshake failures simulated
    pub tls_handshake_failures: u64,
    /// Bandwidth throttling events
    pub bandwidth_throttling_events: u64,
    /// Last injection timestamp
    pub last_injection_at: Option<String>,
}

/// Active network fault state
#[derive(Debug)]
struct NetworkFaultState {
    #[allow(dead_code)]
    fault_id: String,
    fault_config: FaultConfig,
    params: NetworkFaultParams,
    stats: NetworkFaultStats,
    activated_at: WasmInstant,
    connection_count: u32,
    subrequest_count: u32,
}

/// Network chaos injector for Cloudflare Workers
#[derive(Debug)]
pub struct NetworkChaosInjector {
    #[allow(dead_code)]
    config: ChaosEngineeringConfig,
    active_faults: HashMap<String, NetworkFaultState>,
    global_stats: NetworkFaultStats,
    is_enabled: bool,
    is_initialized: bool,
}

impl NetworkChaosInjector {
    /// Create a new network chaos injector
    pub fn new(config: ChaosEngineeringConfig) -> Self {
        Self {
            config,
            active_faults: HashMap::new(),
            global_stats: NetworkFaultStats::default(),
            is_enabled: false,
            is_initialized: false,
        }
    }

    /// Initialize the network chaos injector
    pub async fn initialize(&mut self, _env: &Env) -> ArbitrageResult<()> {
        if self.is_initialized {
            return Ok(());
        }

        self.is_enabled = self.config.feature_flags.network_chaos_simulation;
        self.is_initialized = true;

        Ok(())
    }

    /// Register a new network fault injection
    pub async fn register_fault(
        &mut self,
        fault_id: String,
        fault_config: FaultConfig,
        params: NetworkFaultParams,
    ) -> ArbitrageResult<()> {
        if !self.is_enabled {
            return Ok(());
        }

        let fault_state = NetworkFaultState {
            fault_id: fault_id.clone(),
            fault_config,
            params,
            stats: NetworkFaultStats::default(),
            activated_at: now_instant(),
            connection_count: 0,
            subrequest_count: 0,
        };

        self.active_faults.insert(fault_id, fault_state);
        Ok(())
    }

    /// Check if network operation should be affected by fault injection
    pub async fn should_inject_fault(
        &mut self,
        url: &str,
        method: &str,
        headers: &HashMap<String, String>,
        operation: &NetworkOperation,
    ) -> ArbitrageResult<Option<NetworkFaultInjection>> {
        if !self.is_enabled || self.active_faults.is_empty() {
            return Ok(None);
        }

        for fault_state in self.active_faults.values_mut() {
            // Check if fault has expired
            if fault_state.activated_at.elapsed()
                > Duration::from_secs(fault_state.fault_config.duration_seconds)
            {
                continue;
            }

            // Check if this operation matches the fault criteria
            if Self::matches_fault_criteria_static(
                &fault_state.params,
                url,
                method,
                headers,
                operation,
            ) {
                fault_state.stats.total_operations += 1;

                // Check connection and subrequest limits (Workers-specific)
                if let Some(limit) = fault_state.params.connection_limit {
                    if fault_state.connection_count >= limit {
                        fault_state.stats.connection_limit_violations += 1;
                        return Ok(Some(NetworkFaultInjection {
                            fault_type: NetworkFaultType::ConnectionLimitExhaustion,
                            metadata: Self::create_fault_metadata_static(
                                &fault_state.fault_config,
                                &fault_state.params,
                            ),
                        }));
                    }
                }

                if let Some(limit) = fault_state.params.subrequest_limit {
                    if fault_state.subrequest_count >= limit {
                        fault_state.stats.subrequest_limit_violations += 1;
                        return Ok(Some(NetworkFaultInjection {
                            fault_type: NetworkFaultType::SubrequestLimitExhaustion,
                            metadata: Self::create_fault_metadata_static(
                                &fault_state.fault_config,
                                &fault_state.params,
                            ),
                        }));
                    }
                }

                // Determine fault injection based on probability
                if let Some(injection) = Self::determine_fault_injection_static(
                    &fault_state.fault_config,
                    &fault_state.params,
                )? {
                    fault_state.stats.total_faults_injected += 1;

                    // Update specific fault counters
                    match &injection.fault_type {
                        NetworkFaultType::LatencyInjection { .. } => {
                            fault_state.stats.latency_injections += 1;
                        }
                        NetworkFaultType::ConnectionTimeout { .. } => {
                            fault_state.stats.connection_timeouts += 1;
                        }
                        NetworkFaultType::ConnectionFailure { .. } => {
                            fault_state.stats.connection_failures += 1;
                        }
                        NetworkFaultType::SubrequestLimitExhaustion => {
                            fault_state.stats.subrequest_limit_violations += 1;
                        }
                        NetworkFaultType::ConnectionLimitExhaustion => {
                            fault_state.stats.connection_limit_violations += 1;
                        }
                        NetworkFaultType::DnsFailure => {
                            fault_state.stats.dns_failures += 1;
                        }
                        NetworkFaultType::TlsHandshakeFailure => {
                            fault_state.stats.tls_handshake_failures += 1;
                        }
                        NetworkFaultType::BandwidthThrottling { .. } => {
                            fault_state.stats.bandwidth_throttling_events += 1;
                        }
                    }

                    fault_state.stats.last_injection_at = Some(chrono::Utc::now().to_rfc3339());
                    fault_state.connection_count += 1;
                    fault_state.subrequest_count += 1;

                    return Ok(Some(injection));
                }
            }
        }

        Ok(None)
    }

    /// Check if operation matches fault criteria (instance method for tests)
    #[allow(dead_code)]
    fn matches_fault_criteria(
        &self,
        params: &NetworkFaultParams,
        url: &str,
        method: &str,
        headers: &HashMap<String, String>,
        operation: &NetworkOperation,
    ) -> bool {
        Self::matches_fault_criteria_static(params, url, method, headers, operation)
    }

    /// Check if operation matches fault criteria (static version)
    fn matches_fault_criteria_static(
        params: &NetworkFaultParams,
        url: &str,
        method: &str,
        headers: &HashMap<String, String>,
        operation: &NetworkOperation,
    ) -> bool {
        // Check operation type
        if !params.target_operations.contains(operation) {
            return false;
        }

        // Check HTTP method
        if !params.target_methods.is_empty()
            && !params.target_methods.contains(&method.to_uppercase())
        {
            return false;
        }

        // Check URL patterns
        if !params.url_patterns.is_empty() {
            let matches_pattern = params
                .url_patterns
                .iter()
                .any(|pattern| Self::matches_url_pattern_static(url, pattern));
            if !matches_pattern {
                return false;
            }
        }

        // Check origin domains
        if !params.target_origins.is_empty() {
            if let Ok(parsed_url) = url::Url::parse(url) {
                if let Some(host) = parsed_url.host_str() {
                    if !params
                        .target_origins
                        .iter()
                        .any(|origin| host.contains(origin))
                    {
                        return false;
                    }
                }
            }
        }

        // Check header patterns
        for (header_name, header_pattern) in &params.header_patterns {
            if let Some(header_value) = headers.get(header_name) {
                if !Self::matches_header_pattern_static(header_value, header_pattern) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Simple URL pattern matching (static version)
    fn matches_url_pattern_static(url: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        // Simple wildcard matching - supports * at beginning, end, or both
        if let Some(stripped) = pattern.strip_prefix('*') {
            if let Some(suffix) = stripped.strip_suffix('*') {
                // Pattern like "*middle*"
                return url.contains(suffix);
            } else {
                // Pattern like "*suffix"
                return url.ends_with(stripped);
            }
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            // Pattern like "prefix*"
            return url.starts_with(prefix);
        }

        // Exact match
        url == pattern
    }

    /// Simple header pattern matching (static version)
    fn matches_header_pattern_static(header_value: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        // Simple wildcard matching for headers
        if let Some(stripped) = pattern.strip_prefix('*') {
            if let Some(suffix) = stripped.strip_suffix('*') {
                return header_value.contains(suffix);
            } else {
                return header_value.ends_with(stripped);
            }
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            return header_value.starts_with(prefix);
        }

        header_value == pattern
    }

    /// Determine what fault injection to apply (static version)
    fn determine_fault_injection_static(
        fault_config: &FaultConfig,
        _params: &NetworkFaultParams,
    ) -> ArbitrageResult<Option<NetworkFaultInjection>> {
        // Use deterministic pseudo-random based on current time and intensity
        let random_value =
            (chrono::Utc::now().timestamp_millis() as f64 * fault_config.intensity) % 1.0;

        if random_value > fault_config.intensity {
            return Ok(None);
        }

        // Determine fault type based on configuration and Workers constraints
        let fault_type = match fault_config.fault_type.as_str() {
            "latency" => NetworkFaultType::LatencyInjection {
                additional_ms: ((fault_config.intensity * 1000.0) as u64).max(10),
                inject_on_connect: random_value > 0.5,
            },
            "timeout" => NetworkFaultType::ConnectionTimeout {
                timeout_ms: ((fault_config.intensity * 5000.0) as u64).max(100),
            },
            "connection_failure" => NetworkFaultType::ConnectionFailure {
                error_code: if random_value > 0.7 { 503 } else { 502 },
                error_message: "Simulated connection failure".to_string(),
            },
            "subrequest_limit" => NetworkFaultType::SubrequestLimitExhaustion,
            "connection_limit" => NetworkFaultType::ConnectionLimitExhaustion,
            "dns_failure" => NetworkFaultType::DnsFailure,
            "tls_failure" => NetworkFaultType::TlsHandshakeFailure,
            "bandwidth_throttling" => NetworkFaultType::BandwidthThrottling {
                max_bytes_per_second: ((fault_config.intensity * 1024.0 * 1024.0) as u64).max(1024), // Min 1KB/s
            },
            _ => NetworkFaultType::LatencyInjection {
                additional_ms: 100,
                inject_on_connect: false,
            },
        };

        Ok(Some(NetworkFaultInjection {
            fault_type,
            metadata: Self::create_fault_metadata_static(fault_config, _params),
        }))
    }

    /// Create metadata for fault injection (static version)
    fn create_fault_metadata_static(
        fault_config: &FaultConfig,
        params: &NetworkFaultParams,
    ) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("fault_id".to_string(), fault_config.id.clone());
        metadata.insert("fault_type".to_string(), fault_config.fault_type.clone());
        metadata.insert("intensity".to_string(), fault_config.intensity.to_string());
        metadata.insert(
            "duration_seconds".to_string(),
            fault_config.duration_seconds.to_string(),
        );
        metadata.insert(
            "target_operations_count".to_string(),
            params.target_operations.len().to_string(),
        );
        metadata.insert(
            "url_patterns_count".to_string(),
            params.url_patterns.len().to_string(),
        );
        metadata.insert(
            "target_methods_count".to_string(),
            params.target_methods.len().to_string(),
        );
        metadata.insert(
            "connection_limit".to_string(),
            params
                .connection_limit
                .map_or("none".to_string(), |v| v.to_string()),
        );
        metadata.insert(
            "subrequest_limit".to_string(),
            params
                .subrequest_limit
                .map_or("none".to_string(), |v| v.to_string()),
        );
        metadata.insert(
            "service_binding_faults".to_string(),
            params.enable_service_binding_faults.to_string(),
        );
        metadata.insert(
            "internal_service_faults".to_string(),
            params.enable_internal_service_faults.to_string(),
        );
        metadata.insert("injected_at".to_string(), chrono::Utc::now().to_rfc3339());
        metadata
    }

    /// Remove a fault injection
    pub async fn remove_fault(
        &mut self,
        fault_id: &str,
    ) -> ArbitrageResult<Option<NetworkFaultStats>> {
        if let Some(fault_state) = self.active_faults.remove(fault_id) {
            // Update global stats
            self.global_stats.total_operations += fault_state.stats.total_operations;
            self.global_stats.total_faults_injected += fault_state.stats.total_faults_injected;
            self.global_stats.latency_injections += fault_state.stats.latency_injections;
            self.global_stats.connection_timeouts += fault_state.stats.connection_timeouts;
            self.global_stats.connection_failures += fault_state.stats.connection_failures;
            self.global_stats.subrequest_limit_violations +=
                fault_state.stats.subrequest_limit_violations;
            self.global_stats.connection_limit_violations +=
                fault_state.stats.connection_limit_violations;
            self.global_stats.dns_failures += fault_state.stats.dns_failures;
            self.global_stats.tls_handshake_failures += fault_state.stats.tls_handshake_failures;
            self.global_stats.bandwidth_throttling_events +=
                fault_state.stats.bandwidth_throttling_events;

            Ok(Some(fault_state.stats))
        } else {
            Ok(None)
        }
    }

    /// Get global network fault injection statistics
    pub fn get_global_stats(&self) -> &NetworkFaultStats {
        &self.global_stats
    }

    /// Get fault injection statistics for a specific fault
    pub fn get_fault_stats(&self, fault_id: &str) -> Option<&NetworkFaultStats> {
        self.active_faults.get(fault_id).map(|state| &state.stats)
    }

    /// List all active fault IDs
    pub fn list_active_faults(&self) -> Vec<String> {
        self.active_faults.keys().cloned().collect()
    }

    /// Clear all active faults
    pub async fn clear_all_faults(&mut self) -> ArbitrageResult<()> {
        self.active_faults.clear();
        Ok(())
    }

    /// Check if network chaos injection is enabled
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }
}

impl Default for NetworkFaultParams {
    fn default() -> Self {
        Self {
            target_operations: vec![NetworkOperation::FetchRequest],
            url_patterns: vec!["*".to_string()],
            target_methods: vec!["GET".to_string(), "POST".to_string()],
            header_patterns: HashMap::new(),
            target_origins: Vec::new(),
            connection_limit: Some(6),  // Cloudflare Workers default limit
            subrequest_limit: Some(50), // Free plan limit
            enable_service_binding_faults: false,
            enable_internal_service_faults: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_fault_params_default() {
        let params = NetworkFaultParams::default();
        assert!(!params.target_operations.is_empty());
        assert!(!params.url_patterns.is_empty());
        assert!(!params.target_methods.is_empty());
        assert_eq!(params.connection_limit, Some(6));
        assert_eq!(params.subrequest_limit, Some(50));
    }

    #[test]
    fn test_url_pattern_matching() {
        // Test wildcard patterns
        assert!(NetworkChaosInjector::matches_url_pattern_static(
            "https://api.example.com/test",
            "*"
        ));
        assert!(NetworkChaosInjector::matches_url_pattern_static(
            "https://api.example.com/test",
            "https://*"
        ));
        assert!(NetworkChaosInjector::matches_url_pattern_static(
            "https://api.example.com/test",
            "*example.com*"
        ));
        assert!(NetworkChaosInjector::matches_url_pattern_static(
            "https://api.example.com/test",
            "*/test"
        ));

        // Test exact match
        assert!(NetworkChaosInjector::matches_url_pattern_static(
            "https://api.example.com/test",
            "https://api.example.com/test"
        ));
        assert!(!NetworkChaosInjector::matches_url_pattern_static(
            "https://api.example.com/test",
            "https://api.example.com/other"
        ));
    }

    #[test]
    fn test_fault_criteria_matching() {
        let params = NetworkFaultParams {
            target_operations: vec![NetworkOperation::FetchRequest],
            url_patterns: vec!["*api.example.com*".to_string()],
            target_methods: vec!["GET".to_string()],
            header_patterns: HashMap::new(),
            target_origins: Vec::new(),
            connection_limit: Some(6),
            subrequest_limit: Some(50),
            enable_service_binding_faults: false,
            enable_internal_service_faults: false,
        };

        let headers = HashMap::new();

        // Should match
        assert!(NetworkChaosInjector::matches_fault_criteria_static(
            &params,
            "https://api.example.com/test",
            "GET",
            &headers,
            &NetworkOperation::FetchRequest
        ));

        // Should not match - wrong operation
        assert!(!NetworkChaosInjector::matches_fault_criteria_static(
            &params,
            "https://api.example.com/test",
            "GET",
            &headers,
            &NetworkOperation::ServiceBinding
        ));

        // Should not match - wrong method
        assert!(!NetworkChaosInjector::matches_fault_criteria_static(
            &params,
            "https://api.example.com/test",
            "POST",
            &headers,
            &NetworkOperation::FetchRequest
        ));

        // Should not match - wrong URL pattern
        assert!(!NetworkChaosInjector::matches_fault_criteria_static(
            &params,
            "https://other.example.com/test",
            "GET",
            &headers,
            &NetworkOperation::FetchRequest
        ));
    }
}
