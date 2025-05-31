// Shared Infrastructure Types - Common types used across infrastructure modules
// Provides centralized definitions to avoid duplication and circular dependencies

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Component health status for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub is_healthy: bool,
    pub last_check: u64,
    pub error_count: u32,
    pub warning_count: u32,
    pub uptime_seconds: u64,
    pub performance_score: f32,
    pub resource_usage_percent: f32,
    pub last_error: Option<String>,
    pub last_warning: Option<String>,
    pub component_name: String,
    pub version: String,
}

impl Default for ComponentHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            last_check: chrono::Utc::now().timestamp_millis() as u64,
            error_count: 0,
            warning_count: 0,
            uptime_seconds: 0,
            performance_score: 0.0,
            resource_usage_percent: 0.0,
            last_error: None,
            last_warning: None,
            component_name: "unknown".to_string(),
            version: "1.0.0".to_string(),
        }
    }
}

/// Circuit breaker for managing service failures
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub threshold: u32,
    pub timeout_seconds: u64,
    pub last_failure_time: u64,
    pub success_count_in_half_open: u32,
    pub total_requests: u64,
    pub successful_requests: u64,
}

/// Circuit breaker state
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Circuit breaker is open, requests fail fast
    HalfOpen, // Testing if service is back
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            threshold: 5,
            timeout_seconds: 60,
            last_failure_time: 0,
            success_count_in_half_open: 0,
            total_requests: 0,
            successful_requests: 0,
        }
    }
}

impl CircuitBreaker {
    pub fn new(threshold: u32, timeout_seconds: u64) -> Self {
        Self {
            threshold,
            timeout_seconds,
            ..Default::default()
        }
    }

    pub fn can_execute(&mut self) -> bool {
        let current_time = chrono::Utc::now().timestamp_millis() as u64;

        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if current_time - self.last_failure_time > self.timeout_seconds * 1000 {
                    self.state = CircuitBreakerState::HalfOpen;
                    self.success_count_in_half_open = 0;
                    true
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    pub fn record_success(&mut self) {
        self.total_requests += 1;
        self.successful_requests += 1;

        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.success_count_in_half_open += 1;
                if self.success_count_in_half_open >= 3 {
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                }
            }
            CircuitBreakerState::Open => {}
        }
    }

    pub fn record_failure(&mut self) {
        self.total_requests += 1;
        self.failure_count += 1;
        self.last_failure_time = chrono::Utc::now().timestamp_millis() as u64;

        if self.failure_count >= self.threshold {
            self.state = CircuitBreakerState::Open;
        }
    }

    pub fn get_success_rate(&self) -> f32 {
        if self.total_requests > 0 {
            self.successful_requests as f32 / self.total_requests as f32
        } else {
            1.0
        }
    }
}

/// Validation metrics for data quality tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetrics {
    pub total_validations: u64,
    pub successful_validations: u64,
    pub failed_validations: u64,
    pub average_quality_score: f32,
    pub average_freshness_score: f32,
    pub avg_validation_time_ms: f64,
    pub stale_data_count: u64,
    pub invalid_data_count: u64,
    pub validation_errors_by_type: HashMap<String, u64>,
    pub data_sources_quality: HashMap<String, f32>,
    pub last_updated: u64,
}

impl Default for ValidationMetrics {
    fn default() -> Self {
        Self {
            total_validations: 0,
            successful_validations: 0,
            failed_validations: 0,
            average_quality_score: 0.0,
            average_freshness_score: 0.0,
            avg_validation_time_ms: 0.0,
            stale_data_count: 0,
            invalid_data_count: 0,
            validation_errors_by_type: HashMap::new(),
            data_sources_quality: HashMap::new(),
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Validation cache entry for storing validation rules and results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCacheEntry {
    pub key: String,
    pub validation_rules: Vec<String>,
    pub freshness_rules: HashMap<String, u64>,
    pub last_validation: Option<u64>,
    pub validation_result: Option<bool>,
    pub quality_score: Option<f32>,
    pub created_at: u64,
    pub expires_at: u64,
    pub access_count: u64,
}

impl ValidationCacheEntry {
    pub fn new(key: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            key,
            validation_rules: Vec::new(),
            freshness_rules: HashMap::new(),
            last_validation: None,
            validation_result: None,
            quality_score: None,
            created_at: now,
            expires_at: now + 3600000, // 1 hour default TTL
            access_count: 0,
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp_millis() as u64 > self.expires_at
    }

    pub fn record_access(&mut self) {
        self.access_count += 1;
    }
}

/// Cache statistics for monitoring cache performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: u64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub total_sets: u64,
    pub total_deletes: u64,
    pub total_size_bytes: u64,
    pub compressed_entries: u64,
    pub expired_entries: u64,
    pub hit_rate_percent: f32,
    pub compression_ratio_percent: f32,
    pub average_freshness_score: f32,
    pub last_updated: u64,
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            total_entries: 0,
            total_hits: 0,
            total_misses: 0,
            total_sets: 0,
            total_deletes: 0,
            total_size_bytes: 0,
            compressed_entries: 0,
            expired_entries: 0,
            hit_rate_percent: 0.0,
            compression_ratio_percent: 0.0,
            average_freshness_score: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Performance metrics for tracking operation performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub operations_per_second: f64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub last_updated: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            operations_per_second: 0.0,
            average_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            max_latency_ms: 0.0,
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Rate limiter for controlling request flow
#[derive(Debug, Clone)]
pub struct RateLimiter {
    pub requests_per_second: u32,
    pub window_start: u64,
    pub request_count: u32,
    pub total_requests: u64,
    pub rejected_requests: u64,
}

impl RateLimiter {
    pub fn new(requests_per_second: u32) -> Self {
        Self {
            requests_per_second,
            window_start: chrono::Utc::now().timestamp_millis() as u64,
            request_count: 0,
            total_requests: 0,
            rejected_requests: 0,
        }
    }

    pub fn can_proceed(&mut self) -> bool {
        let current_time = chrono::Utc::now().timestamp_millis() as u64;
        self.total_requests += 1;

        // Reset window if a second has passed
        if current_time - self.window_start >= 1000 {
            self.window_start = current_time;
            self.request_count = 0;
        }

        if self.request_count < self.requests_per_second {
            self.request_count += 1;
            true
        } else {
            self.rejected_requests += 1;
            false
        }
    }

    pub fn get_rejection_rate(&self) -> f32 {
        if self.total_requests > 0 {
            self.rejected_requests as f32 / self.total_requests as f32
        } else {
            0.0
        }
    }
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub component: String,
    pub is_healthy: bool,
    pub response_time_ms: u64,
    pub error_message: Option<String>,
    pub details: HashMap<String, serde_json::Value>,
    pub timestamp: u64,
}

impl HealthCheckResult {
    pub fn healthy(component: String, response_time_ms: u64) -> Self {
        Self {
            component,
            is_healthy: true,
            response_time_ms,
            error_message: None,
            details: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn unhealthy(component: String, response_time_ms: u64, error: String) -> Self {
        Self {
            component,
            is_healthy: false,
            response_time_ms,
            error_message: Some(error),
            details: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn with_detail(mut self, key: String, value: serde_json::Value) -> Self {
        self.details.insert(key, value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_health_default() {
        let health = ComponentHealth::default();
        assert!(!health.is_healthy);
        assert_eq!(health.error_count, 0);
        assert_eq!(health.component_name, "unknown");
    }

    #[test]
    fn test_circuit_breaker_default() {
        let breaker = CircuitBreaker::default();
        assert_eq!(breaker.state, CircuitBreakerState::Closed);
        assert_eq!(breaker.threshold, 5);
        assert!(breaker.can_execute());
    }

    #[test]
    fn test_circuit_breaker_failure_handling() {
        let mut breaker = CircuitBreaker::new(2, 60);
        
        // Should be closed initially
        assert!(breaker.can_execute());
        
        // Record failures
        breaker.record_failure();
        assert!(breaker.can_execute()); // Still closed
        
        breaker.record_failure();
        assert!(!breaker.can_execute()); // Now open
        assert_eq!(breaker.state, CircuitBreakerState::Open);
    }

    #[test]
    fn test_validation_cache_entry() {
        let entry = ValidationCacheEntry::new("test_key".to_string());
        assert_eq!(entry.key, "test_key");
        assert!(!entry.is_expired());
        assert_eq!(entry.access_count, 0);
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(2);
        
        assert!(limiter.can_proceed());
        assert!(limiter.can_proceed());
        assert!(!limiter.can_proceed()); // Rate limit exceeded
        
        assert_eq!(limiter.total_requests, 3);
        assert_eq!(limiter.rejected_requests, 1);
    }

    #[test]
    fn test_health_check_result() {
        let healthy = HealthCheckResult::healthy("test_component".to_string(), 100);
        assert!(healthy.is_healthy);
        assert_eq!(healthy.response_time_ms, 100);
        assert!(healthy.error_message.is_none());

        let unhealthy = HealthCheckResult::unhealthy(
            "test_component".to_string(),
            500,
            "Connection failed".to_string(),
        );
        assert!(!unhealthy.is_healthy);
        assert_eq!(unhealthy.response_time_ms, 500);
        assert!(unhealthy.error_message.is_some());
    }
} 