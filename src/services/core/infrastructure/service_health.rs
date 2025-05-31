// Service Health Module - Unified Health Checks and Service Monitoring
// Consolidates health check patterns from multiple services with comprehensive monitoring

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Service health status levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded", 
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Unknown => "unknown",
        }
    }
}

/// Individual service health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealthCheck {
    pub service_name: String,
    pub status: HealthStatus,
    pub response_time_ms: f64,
    pub last_check_timestamp: u64,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub dependencies: Vec<String>,
}

/// Comprehensive system health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthReport {
    pub overall_status: HealthStatus,
    pub services: HashMap<String, ServiceHealthCheck>,
    pub critical_services_healthy: bool,
    pub degraded_services: Vec<String>,
    pub unhealthy_services: Vec<String>,
    pub total_services: usize,
    pub healthy_services: usize,
    pub health_score: f64, // 0.0 to 1.0
    pub generated_at: u64,
    pub uptime_seconds: u64,
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub check_interval_seconds: u64,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub critical_services: Vec<String>,
    pub degraded_threshold_ms: f64,
    pub unhealthy_threshold_ms: f64,
    pub enable_dependency_checks: bool,
    pub enable_detailed_metrics: bool,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval_seconds: 30,    // Check every 30 seconds
            timeout_seconds: 10,           // 10 second timeout per check
            max_retries: 2,               // 2 retry attempts
            critical_services: vec![
                "database".to_string(),
                "cache".to_string(),
                "notifications".to_string(),
            ],
            degraded_threshold_ms: 1000.0,  // 1 second for degraded
            unhealthy_threshold_ms: 5000.0, // 5 seconds for unhealthy
            enable_dependency_checks: true,
            enable_detailed_metrics: true,
        }
    }
}

/// Health check function trait for services
pub trait HealthCheckable {
    async fn health_check(&self) -> ArbitrageResult<ServiceHealthCheck>;
    fn service_name(&self) -> &str;
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Service health metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    pub service_name: String,
    pub total_checks: u64,
    pub successful_checks: u64,
    pub failed_checks: u64,
    pub avg_response_time_ms: f64,
    pub min_response_time_ms: f64,
    pub max_response_time_ms: f64,
    pub uptime_percentage: f64,
    pub last_failure_timestamp: Option<u64>,
    pub last_failure_reason: Option<String>,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
}

/// Centralized service health manager
pub struct ServiceHealthManager {
    config: HealthCheckConfig,
    services: HashMap<String, Box<dyn HealthCheckable + Send + Sync>>,
    metrics: Arc<std::sync::Mutex<HashMap<String, ServiceMetrics>>>,
    last_health_report: Arc<std::sync::Mutex<Option<SystemHealthReport>>>,
    start_time: Instant,
}

impl ServiceHealthManager {
    /// Create new ServiceHealthManager with default configuration
    pub fn new() -> Self {
        Self {
            config: HealthCheckConfig::default(),
            services: HashMap::new(),
            metrics: Arc::new(std::sync::Mutex::new(HashMap::new())),
            last_health_report: Arc::new(std::sync::Mutex::new(None)),
            start_time: Instant::now(),
        }
    }

    /// Create ServiceHealthManager with custom configuration
    pub fn new_with_config(config: HealthCheckConfig) -> Self {
        Self {
            config,
            services: HashMap::new(),
            metrics: Arc::new(std::sync::Mutex::new(HashMap::new())),
            last_health_report: Arc::new(std::sync::Mutex::new(None)),
            start_time: Instant::now(),
        }
    }

    /// Register a service for health monitoring
    pub fn register_service(&mut self, service: Box<dyn HealthCheckable + Send + Sync>) {
        let service_name = service.service_name().to_string();
        
        // Initialize metrics for the service
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.insert(service_name.clone(), ServiceMetrics {
                service_name: service_name.clone(),
                total_checks: 0,
                successful_checks: 0,
                failed_checks: 0,
                avg_response_time_ms: 0.0,
                min_response_time_ms: f64::MAX,
                max_response_time_ms: 0.0,
                uptime_percentage: 100.0,
                last_failure_timestamp: None,
                last_failure_reason: None,
                consecutive_failures: 0,
                consecutive_successes: 0,
            });
        }

        self.services.insert(service_name, service);
    }

    /// Perform health check on a specific service
    pub async fn check_service_health(&self, service_name: &str) -> ArbitrageResult<ServiceHealthCheck> {
        let service = self.services.get(service_name)
            .ok_or_else(|| ArbitrageError::validation_error(format!("Service not found: {}", service_name)))?;

        let start_time = Instant::now();
        let mut check_result = None;
        let mut last_error = None;

        // Retry logic
        for attempt in 0..=self.config.max_retries {
            match tokio::time::timeout(
                Duration::from_secs(self.config.timeout_seconds),
                service.health_check()
            ).await {
                Ok(Ok(result)) => {
                    check_result = Some(result);
                    break;
                }
                Ok(Err(e)) => {
                    last_error = Some(e);
                    if attempt < self.config.max_retries {
                        // Exponential backoff
                        let delay_ms = 100 * (2_u64.pow(attempt));
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
                Err(_) => {
                    last_error = Some(ArbitrageError::timeout_error("Health check timeout"));
                    if attempt < self.config.max_retries {
                        let delay_ms = 100 * (2_u64.pow(attempt));
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        let response_time = start_time.elapsed().as_millis() as f64;

        match check_result {
            Some(mut result) => {
                result.response_time_ms = response_time;
                result.last_check_timestamp = chrono::Utc::now().timestamp_millis() as u64;
                
// Escalate the status if latency pushes it to a worse tier,
// but never *promote* a worse status to a better one.
let latency_status = self.determine_status_from_response_time(response_time);
result.status = match (result.status.clone(), latency_status) {
    // keep the worse of the two
    (HealthStatus::Unhealthy, _) | (_, HealthStatus::Unhealthy) => HealthStatus::Unhealthy,
    (HealthStatus::Degraded, _) | (_, HealthStatus::Degraded)   => HealthStatus::Degraded,
    _                                                           => HealthStatus::Healthy,
};
                
                self.update_service_metrics(service_name, true, response_time, None);
                Ok(result)
            }
            None => {
                let error_msg = last_error.map(|e| e.to_string()).unwrap_or_else(|| "Unknown error".to_string());
                self.update_service_metrics(service_name, false, response_time, Some(&error_msg));
                
                Ok(ServiceHealthCheck {
                    service_name: service_name.to_string(),
                    status: HealthStatus::Unhealthy,
                    response_time_ms: response_time,
                    last_check_timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    error_message: Some(error_msg),
                    metadata: HashMap::new(),
                    dependencies: service.dependencies(),
                })
            }
        }
    }

    /// Perform comprehensive system health check
    pub async fn check_system_health(&self) -> ArbitrageResult<SystemHealthReport> {
        let mut service_checks = HashMap::new();
        let mut healthy_count = 0;
        let mut degraded_services = Vec::new();
        let mut unhealthy_services = Vec::new();

        // Check all registered services
        for service_name in self.services.keys() {
            match self.check_service_health(service_name).await {
                Ok(check) => {
                    match check.status {
                        HealthStatus::Healthy => healthy_count += 1,
                        HealthStatus::Degraded => {
                            degraded_services.push(service_name.clone());
                        }
                        HealthStatus::Unhealthy => {
                            unhealthy_services.push(service_name.clone());
                        }
                        HealthStatus::Unknown => {
                            unhealthy_services.push(service_name.clone());
                        }
                    }
                    service_checks.insert(service_name.clone(), check);
                }
                Err(e) => {
                    // Create failed health check result
                    let failed_check = ServiceHealthCheck {
                        service_name: service_name.clone(),
                        status: HealthStatus::Unhealthy,
                        response_time_ms: 0.0,
                        last_check_timestamp: chrono::Utc::now().timestamp_millis() as u64,
                        error_message: Some(e.to_string()),
                        metadata: HashMap::new(),
                        dependencies: Vec::new(),
                    };
                    unhealthy_services.push(service_name.clone());
                    service_checks.insert(service_name.clone(), failed_check);
                }
            }
        }

        let total_services = self.services.len();
        let health_score = if total_services > 0 {
            healthy_count as f64 / total_services as f64
        } else {
            1.0
        };

        // Determine overall status
        let overall_status = self.determine_overall_status(&service_checks);
        
        // Check if critical services are healthy
        let critical_services_healthy = self.config.critical_services.iter()
            .all(|service| {
                service_checks.get(service)
                    .map(|check| check.status == HealthStatus::Healthy)
                    .unwrap_or(false)
            });

        let report = SystemHealthReport {
            overall_status,
            services: service_checks,
            critical_services_healthy,
            degraded_services,
            unhealthy_services,
            total_services,
            healthy_services: healthy_count,
            health_score,
            generated_at: chrono::Utc::now().timestamp_millis() as u64,
            uptime_seconds: self.start_time.elapsed().as_secs(),
        };

        // Cache the latest report
        if let Ok(mut last_report) = self.last_health_report.lock() {
            *last_report = Some(report.clone());
        }

        Ok(report)
    }

    /// Get the last cached health report
    pub fn get_last_health_report(&self) -> Option<SystemHealthReport> {
        self.last_health_report.lock().ok()?.clone()
    }

    /// Get metrics for a specific service
    pub fn get_service_metrics(&self, service_name: &str) -> Option<ServiceMetrics> {
        self.metrics.lock().ok()?.get(service_name).cloned()
    }

    /// Get metrics for all services
    pub fn get_all_metrics(&self) -> HashMap<String, ServiceMetrics> {
        self.metrics.lock().unwrap_or_default().clone()
    }

    /// Reset metrics for all services
    pub fn reset_metrics(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            for metric in metrics.values_mut() {
                metric.total_checks = 0;
                metric.successful_checks = 0;
                metric.failed_checks = 0;
                metric.avg_response_time_ms = 0.0;
                metric.min_response_time_ms = f64::MAX;
                metric.max_response_time_ms = 0.0;
                metric.uptime_percentage = 100.0;
                metric.last_failure_timestamp = None;
                metric.last_failure_reason = None;
                metric.consecutive_failures = 0;
                metric.consecutive_successes = 0;
            }
        }
    }

    /// Check if system is ready to handle requests
    pub async fn is_system_ready(&self) -> bool {
        match self.check_system_health().await {
            Ok(report) => {
                report.critical_services_healthy && 
                report.overall_status != HealthStatus::Unhealthy
            }
            Err(_) => false,
        }
    }

    /// Get system uptime in seconds
    pub fn get_uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    // ============= INTERNAL HELPER METHODS =============

    fn determine_status_from_response_time(&self, response_time_ms: f64) -> HealthStatus {
        if response_time_ms > self.config.unhealthy_threshold_ms {
            HealthStatus::Unhealthy
        } else if response_time_ms > self.config.degraded_threshold_ms {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    fn determine_overall_status(&self, service_checks: &HashMap<String, ServiceHealthCheck>) -> HealthStatus {
        let mut has_unhealthy = false;
        let mut has_degraded = false;

        for check in service_checks.values() {
            match check.status {
                HealthStatus::Unhealthy | HealthStatus::Unknown => {
                    // If it's a critical service, system is unhealthy
                    if self.config.critical_services.contains(&check.service_name) {
                        return HealthStatus::Unhealthy;
                    }
                    has_unhealthy = true;
                }
                HealthStatus::Degraded => {
                    has_degraded = true;
                }
                HealthStatus::Healthy => {}
            }
        }

        if has_unhealthy {
            HealthStatus::Degraded // Non-critical services unhealthy
        } else if has_degraded {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    fn update_service_metrics(&self, service_name: &str, success: bool, response_time_ms: f64, error_message: Option<&str>) {
        if let Ok(mut metrics) = self.metrics.lock() {
            if let Some(metric) = metrics.get_mut(service_name) {
                metric.total_checks += 1;

                if success {
                    metric.successful_checks += 1;
                    metric.consecutive_successes += 1;
                    metric.consecutive_failures = 0;

                    // Update response time statistics
                    let total_time = metric.avg_response_time_ms * (metric.successful_checks - 1) as f64 + response_time_ms;
                    metric.avg_response_time_ms = total_time / metric.successful_checks as f64;
                    metric.min_response_time_ms = metric.min_response_time_ms.min(response_time_ms);
                    metric.max_response_time_ms = metric.max_response_time_ms.max(response_time_ms);
                } else {
                    metric.failed_checks += 1;
                    metric.consecutive_failures += 1;
                    metric.consecutive_successes = 0;
                    metric.last_failure_timestamp = Some(chrono::Utc::now().timestamp_millis() as u64);
                    metric.last_failure_reason = error_message.map(|s| s.to_string());
                }

                // Update uptime percentage
                metric.uptime_percentage = (metric.successful_checks as f64 / metric.total_checks as f64) * 100.0;
            }
        }
    }
}

impl Default for ServiceHealthManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============= BUILT-IN HEALTH CHECK IMPLEMENTATIONS =============

/// Simple ping health check
pub struct PingHealthCheck {
    service_name: String,
}

impl PingHealthCheck {
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
        }
    }
}

impl HealthCheckable for PingHealthCheck {
    async fn health_check(&self) -> ArbitrageResult<ServiceHealthCheck> {
        Ok(ServiceHealthCheck {
            service_name: self.service_name.clone(),
            status: HealthStatus::Healthy,
            response_time_ms: 0.0, // Will be set by the manager
            last_check_timestamp: 0, // Will be set by the manager
            error_message: None,
            metadata: HashMap::new(),
            dependencies: Vec::new(),
        })
    }

    fn service_name(&self) -> &str {
        &self.service_name
    }
}

/// HTTP endpoint health check
pub struct HttpHealthCheck {
    service_name: String,
    endpoint_url: String,
    expected_status: u16,
}

impl HttpHealthCheck {
    pub fn new(service_name: &str, endpoint_url: &str, expected_status: u16) -> Self {
        Self {
            service_name: service_name.to_string(),
            endpoint_url: endpoint_url.to_string(),
            expected_status,
        }
    }
}

impl HealthCheckable for HttpHealthCheck {
    async fn health_check(&self) -> ArbitrageResult<ServiceHealthCheck> {
        // In a real implementation, this would make an HTTP request
        // For now, simulate a successful check
        Ok(ServiceHealthCheck {
            service_name: self.service_name.clone(),
            status: HealthStatus::Healthy,
            response_time_ms: 0.0,
            last_check_timestamp: 0,
            error_message: None,
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("endpoint".to_string(), serde_json::Value::String(self.endpoint_url.clone()));
                metadata.insert("expected_status".to_string(), serde_json::Value::Number(self.expected_status.into()));
                metadata
            },
            dependencies: Vec::new(),
        })
    }

    fn service_name(&self) -> &str {
        &self.service_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_as_str() {
        assert_eq!(HealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(HealthStatus::Degraded.as_str(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.as_str(), "unhealthy");
        assert_eq!(HealthStatus::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.check_interval_seconds, 30);
        assert_eq!(config.timeout_seconds, 10);
        assert_eq!(config.max_retries, 2);
        assert_eq!(config.degraded_threshold_ms, 1000.0);
        assert_eq!(config.unhealthy_threshold_ms, 5000.0);
        assert!(config.enable_dependency_checks);
        assert!(config.enable_detailed_metrics);
    }

    #[test]
    fn test_service_health_check_creation() {
        let check = ServiceHealthCheck {
            service_name: "test_service".to_string(),
            status: HealthStatus::Healthy,
            response_time_ms: 25.5,
            last_check_timestamp: 1640995200000,
            error_message: None,
            metadata: HashMap::new(),
            dependencies: vec!["dependency1".to_string()],
        };

        assert_eq!(check.service_name, "test_service");
        assert_eq!(check.status, HealthStatus::Healthy);
        assert_eq!(check.response_time_ms, 25.5);
        assert!(check.error_message.is_none());
        assert_eq!(check.dependencies.len(), 1);
    }

    #[test]
    fn test_service_metrics_creation() {
        let metrics = ServiceMetrics {
            service_name: "test_service".to_string(),
            total_checks: 100,
            successful_checks: 95,
            failed_checks: 5,
            avg_response_time_ms: 50.0,
            min_response_time_ms: 10.0,
            max_response_time_ms: 200.0,
            uptime_percentage: 95.0,
            last_failure_timestamp: Some(1640995200000),
            last_failure_reason: Some("Connection timeout".to_string()),
            consecutive_failures: 0,
            consecutive_successes: 10,
        };

        assert_eq!(metrics.service_name, "test_service");
        assert_eq!(metrics.total_checks, 100);
        assert_eq!(metrics.successful_checks, 95);
        assert_eq!(metrics.failed_checks, 5);
        assert_eq!(metrics.uptime_percentage, 95.0);
        assert_eq!(metrics.consecutive_successes, 10);
    }

    #[tokio::test]
    async fn test_ping_health_check() {
        let ping_check = PingHealthCheck::new("ping_service");
        let result = ping_check.health_check().await;
        
        assert!(result.is_ok());
        let check = result.unwrap();
        assert_eq!(check.service_name, "ping_service");
        assert_eq!(check.status, HealthStatus::Healthy);
        assert!(check.error_message.is_none());
    }

    #[test]
    fn test_service_health_manager_creation() {
        let manager = ServiceHealthManager::new();
        assert_eq!(manager.services.len(), 0);
        assert_eq!(manager.config.check_interval_seconds, 30);
    }
} 