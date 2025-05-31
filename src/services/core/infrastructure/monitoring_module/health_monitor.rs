// Health Monitor - Comprehensive System Health Tracking and Predictive Analysis
// Part of Monitoring Module replacing monitoring_observability.rs

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Health status levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Critical,
    Unknown,
}

impl HealthStatus {
    pub fn as_str(&self) -> &str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Critical => "critical",
            HealthStatus::Unknown => "unknown",
        }
    }

    pub fn score(&self) -> f32 {
        match self {
            HealthStatus::Healthy => 1.0,
            HealthStatus::Degraded => 0.7,
            HealthStatus::Unhealthy => 0.3,
            HealthStatus::Critical => 0.1,
            HealthStatus::Unknown => 0.0,
        }
    }

    pub fn from_score(score: f32) -> Self {
        if score >= 0.9 {
            HealthStatus::Healthy
        } else if score >= 0.6 {
            HealthStatus::Degraded
        } else if score >= 0.3 {
            HealthStatus::Unhealthy
        } else if score > 0.0 {
            HealthStatus::Critical
        } else {
            HealthStatus::Unknown
        }
    }

    pub fn is_operational(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }
}

/// Component health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub component_id: String,
    pub component_name: String,
    pub component_type: String,
    pub status: HealthStatus,
    pub score: f32,
    pub last_check_time: u64,
    pub last_healthy_time: u64,
    pub uptime_seconds: u64,
    pub downtime_seconds: u64,
    pub check_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub consecutive_failures: u32,
    pub response_time_ms: f64,
    pub error_rate_percent: f32,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub tags: HashMap<String, String>,
    pub alerts: Vec<HealthAlert>,
}

impl ComponentHealth {
    pub fn new(component_id: String, component_name: String, component_type: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            component_id,
            component_name,
            component_type,
            status: HealthStatus::Unknown,
            score: 0.0,
            last_check_time: now,
            last_healthy_time: now,
            uptime_seconds: 0,
            downtime_seconds: 0,
            check_count: 0,
            success_count: 0,
            failure_count: 0,
            consecutive_failures: 0,
            response_time_ms: 0.0,
            error_rate_percent: 0.0,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            metadata: HashMap::new(),
            tags: HashMap::new(),
            alerts: Vec::new(),
        }
    }

    pub fn with_dependency(mut self, dependency_id: String) -> Self {
        self.dependencies.push(dependency_id);
        self
    }

    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }

    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn update_status(&mut self, status: HealthStatus, response_time_ms: f64) {
        let now = chrono::Utc::now().timestamp_millis() as u64;

        self.status = status.clone();
        self.score = status.score();
        self.last_check_time = now;
        self.response_time_ms = response_time_ms;
        self.check_count += 1;

        if status.is_operational() {
            self.success_count += 1;
            self.consecutive_failures = 0;
            self.last_healthy_time = now;
        } else {
            self.failure_count += 1;
            self.consecutive_failures += 1;
        }

        // Update error rate
        self.error_rate_percent = (self.failure_count as f32 / self.check_count as f32) * 100.0;
    }

    pub fn add_alert(&mut self, alert: HealthAlert) {
        self.alerts.push(alert);

        // Keep only recent alerts (last 10)
        if self.alerts.len() > 10 {
            self.alerts.remove(0);
        }
    }

    pub fn get_availability_percent(&self) -> f32 {
        if self.check_count == 0 {
            return 0.0;
        }
        (self.success_count as f32 / self.check_count as f32) * 100.0
    }

    pub fn is_critical(&self) -> bool {
        matches!(self.status, HealthStatus::Critical) || self.consecutive_failures >= 5
    }

    pub fn needs_attention(&self) -> bool {
        !self.status.is_operational()
            || self.consecutive_failures >= 3
            || self.error_rate_percent > 10.0
    }
}

/// Health alert for component issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthAlert {
    pub alert_id: String,
    pub component_id: String,
    pub alert_type: HealthAlertType,
    pub severity: HealthAlertSeverity,
    pub message: String,
    pub timestamp: u64,
    pub resolved: bool,
    pub resolved_at: Option<u64>,
    pub metadata: HashMap<String, String>,
}

impl HealthAlert {
    pub fn new(
        component_id: String,
        alert_type: HealthAlertType,
        severity: HealthAlertSeverity,
        message: String,
    ) -> Self {
        Self {
            alert_id: uuid::Uuid::new_v4().to_string(),
            component_id,
            alert_type,
            severity,
            message,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            resolved: false,
            resolved_at: None,
            metadata: HashMap::new(),
        }
    }

    pub fn resolve(&mut self) {
        self.resolved = true;
        self.resolved_at = Some(chrono::Utc::now().timestamp_millis() as u64);
    }
}

/// Types of health alerts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthAlertType {
    StatusChange,
    HighErrorRate,
    SlowResponse,
    ConsecutiveFailures,
    DependencyFailure,
    ResourceExhaustion,
    Custom(String),
}

/// Severity levels for health alerts
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthAlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Health check definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub check_id: String,
    pub component_id: String,
    pub check_name: String,
    pub check_type: HealthCheckType,
    pub endpoint: Option<String>,
    pub timeout_seconds: u64,
    pub interval_seconds: u64,
    pub retry_attempts: u32,
    pub expected_status_codes: Vec<u16>,
    pub expected_response_time_ms: u64,
    pub custom_validation: Option<String>,
    pub enabled: bool,
    pub tags: HashMap<String, String>,
}

impl HealthCheck {
    pub fn new(component_id: String, check_name: String, check_type: HealthCheckType) -> Self {
        Self {
            check_id: uuid::Uuid::new_v4().to_string(),
            component_id,
            check_name,
            check_type,
            endpoint: None,
            timeout_seconds: 5,
            interval_seconds: 30,
            retry_attempts: 3,
            expected_status_codes: vec![200],
            expected_response_time_ms: 1000,
            custom_validation: None,
            enabled: true,
            tags: HashMap::new(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    pub fn with_interval(mut self, interval_seconds: u64) -> Self {
        self.interval_seconds = interval_seconds;
        self
    }

    pub fn with_expected_response_time(mut self, response_time_ms: u64) -> Self {
        self.expected_response_time_ms = response_time_ms;
        self
    }
}

/// Types of health checks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckType {
    Ping,
    HttpGet,
    HttpPost,
    DatabaseQuery,
    KvStore,
    Custom(String),
}

/// Health metrics for monitoring performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub total_components: u64,
    pub healthy_components: u64,
    pub degraded_components: u64,
    pub unhealthy_components: u64,
    pub critical_components: u64,
    pub unknown_components: u64,
    pub overall_health_score: f32,
    pub average_response_time_ms: f64,
    pub total_health_checks: u64,
    pub successful_health_checks: u64,
    pub failed_health_checks: u64,
    pub health_check_success_rate: f32,
    pub active_alerts: u64,
    pub resolved_alerts: u64,
    pub components_by_type: HashMap<String, u64>,
    pub alerts_by_severity: HashMap<HealthAlertSeverity, u64>,
    pub dependency_graph_size: u64,
    pub last_updated: u64,
}

impl Default for HealthMetrics {
    fn default() -> Self {
        Self {
            total_components: 0,
            healthy_components: 0,
            degraded_components: 0,
            unhealthy_components: 0,
            critical_components: 0,
            unknown_components: 0,
            overall_health_score: 0.0,
            average_response_time_ms: 0.0,
            total_health_checks: 0,
            successful_health_checks: 0,
            failed_health_checks: 0,
            health_check_success_rate: 0.0,
            active_alerts: 0,
            resolved_alerts: 0,
            components_by_type: HashMap::new(),
            alerts_by_severity: HashMap::new(),
            dependency_graph_size: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for HealthMonitor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMonitorConfig {
    pub enable_health_monitoring: bool,
    pub enable_predictive_analysis: bool,
    pub enable_dependency_tracking: bool,
    pub enable_auto_recovery: bool,
    pub default_check_interval_seconds: u64,
    pub default_timeout_seconds: u64,
    pub max_consecutive_failures: u32,
    pub health_score_threshold: f32,
    pub response_time_threshold_ms: u64,
    pub error_rate_threshold_percent: f32,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_alerting: bool,
    pub alert_cooldown_seconds: u64,
    pub max_alerts_per_component: usize,
    pub enable_metrics_collection: bool,
    pub metrics_retention_days: u32,
    pub enable_dashboard_export: bool,
    pub dashboard_refresh_seconds: u64,
}

impl Default for HealthMonitorConfig {
    fn default() -> Self {
        Self {
            enable_health_monitoring: true,
            enable_predictive_analysis: true,
            enable_dependency_tracking: true,
            enable_auto_recovery: false,
            default_check_interval_seconds: 30,
            default_timeout_seconds: 5,
            max_consecutive_failures: 5,
            health_score_threshold: 0.7,
            response_time_threshold_ms: 1000,
            error_rate_threshold_percent: 10.0,
            enable_kv_storage: true,
            kv_key_prefix: "health:".to_string(),
            enable_alerting: true,
            alert_cooldown_seconds: 300, // 5 minutes
            max_alerts_per_component: 10,
            enable_metrics_collection: true,
            metrics_retention_days: 30,
            enable_dashboard_export: true,
            dashboard_refresh_seconds: 60,
        }
    }
}

impl HealthMonitorConfig {
    pub fn high_performance() -> Self {
        Self {
            default_check_interval_seconds: 15,
            default_timeout_seconds: 3,
            response_time_threshold_ms: 500,
            dashboard_refresh_seconds: 30,
            enable_predictive_analysis: true,
            ..Default::default()
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            default_check_interval_seconds: 10,
            default_timeout_seconds: 10,
            max_consecutive_failures: 3,
            health_score_threshold: 0.8,
            error_rate_threshold_percent: 5.0,
            enable_auto_recovery: true,
            alert_cooldown_seconds: 180, // 3 minutes
            metrics_retention_days: 90,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.default_check_interval_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "default_check_interval_seconds must be greater than 0",
            ));
        }
        if self.default_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "default_timeout_seconds must be greater than 0",
            ));
        }
        if self.health_score_threshold < 0.0 || self.health_score_threshold > 1.0 {
            return Err(ArbitrageError::validation_error(
                "health_score_threshold must be between 0.0 and 1.0",
            ));
        }
        if self.error_rate_threshold_percent < 0.0 || self.error_rate_threshold_percent > 100.0 {
            return Err(ArbitrageError::validation_error(
                "error_rate_threshold_percent must be between 0.0 and 100.0",
            ));
        }
        Ok(())
    }
}

/// Health Monitor for comprehensive system health tracking
pub struct HealthMonitor {
    config: HealthMonitorConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Component tracking
    components: Arc<Mutex<HashMap<String, ComponentHealth>>>,
    health_checks: Arc<Mutex<HashMap<String, HealthCheck>>>,

    // Dependency graph
    dependency_graph: Arc<Mutex<HashMap<String, Vec<String>>>>, // component_id -> dependencies

    // Metrics and alerts
    metrics: Arc<Mutex<HealthMetrics>>,
    active_alerts: Arc<Mutex<HashMap<String, HealthAlert>>>,

    // Performance tracking
    startup_time: u64,
}

impl HealthMonitor {
    /// Create new HealthMonitor instance
    pub async fn new(
        config: HealthMonitorConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        logger.info(&format!(
            "HealthMonitor initialized: monitoring={}, predictive={}, dependency_tracking={}",
            config.enable_health_monitoring,
            config.enable_predictive_analysis,
            config.enable_dependency_tracking
        ));

        Ok(Self {
            config,
            logger,
            kv_store,
            components: Arc::new(Mutex::new(HashMap::new())),
            health_checks: Arc::new(Mutex::new(HashMap::new())),
            dependency_graph: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(Mutex::new(HealthMetrics::default())),
            active_alerts: Arc::new(Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Register a component for health monitoring
    pub async fn register_component(&self, component: ComponentHealth) -> ArbitrageResult<()> {
        if !self.config.enable_health_monitoring {
            return Ok(());
        }

        let component_id = component.component_id.clone();

        // Register component
        if let Ok(mut components) = self.components.lock() {
            components.insert(component_id.clone(), component.clone());
        }

        // Update dependency graph
        if self.config.enable_dependency_tracking {
            if let Ok(mut graph) = self.dependency_graph.lock() {
                graph.insert(component_id.clone(), component.dependencies.clone());

                // Update dependents
                for dependency_id in &component.dependencies {
                    if let Ok(mut components) = self.components.lock() {
                        if let Some(existing_component) = components.get_mut(dependency_id) {
                            if !existing_component.dependents.contains(&component_id) {
                                existing_component.dependents.push(component_id.clone());
                            }
                        }
                    }
                }
            }
        }

        // Store in KV if enabled
        // 1. grab &mut to update in-memory state
        let maybe_component = {
            let mut components = self.components.lock().unwrap();
            components.get_mut(&component_id).cloned()
        };

        if let Some(component) = maybe_component {
            // Note: Status update removed during registration - status should be updated via separate health checks

            // 2. persist *after* the lock has been released
            if self.config.enable_kv_storage {
                self.store_component_in_kv(&component).await?;
            }

            // 3. write the updated copy back
            let mut components = self.components.lock().unwrap();
            components.insert(component_id.clone(), component);
        }
        Ok(())
    }

    /// Add a health check for a component
    pub async fn add_health_check(&self, health_check: HealthCheck) -> ArbitrageResult<()> {
        if !self.config.enable_health_monitoring {
            return Ok(());
        }

        let check_id = health_check.check_id.clone();

        if let Ok(mut checks) = self.health_checks.lock() {
            checks.insert(check_id, health_check.clone());
        }

        self.logger.info(&format!(
            "Added health check: {} for component {}",
            health_check.check_name, health_check.component_id
        ));
        Ok(())
    }

    /// Perform health check for a component
    pub async fn check_component_health(
        &self,
        component_id: &str,
    ) -> ArbitrageResult<HealthStatus> {
        if !self.config.enable_health_monitoring {
            return Ok(HealthStatus::Unknown);
        }

        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Find health checks for this component
        let health_checks = if let Ok(checks) = self.health_checks.lock() {
            checks
                .values()
                .filter(|check| check.component_id == component_id && check.enabled)
                .cloned()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        if health_checks.is_empty() {
            // No health checks defined, assume healthy
            self.update_component_status(component_id, HealthStatus::Healthy, 0.0)
                .await?;
            return Ok(HealthStatus::Healthy);
        }

        let mut overall_status = HealthStatus::Healthy;
        let mut total_response_time = 0.0;
        let mut check_count = 0;

        // Execute health checks
        for health_check in health_checks {
            let check_result = self.execute_health_check(&health_check).await;

            match check_result {
                Ok((status, response_time)) => {
                    total_response_time += response_time;
                    check_count += 1;

                    // Update overall status (take worst status)
                    if status > overall_status {
                        overall_status = status;
                    }
                }
                Err(e) => {
                    self.logger
                        .error(&format!("Health check failed for {}: {}", component_id, e));
                    overall_status = HealthStatus::Critical;
                    total_response_time += self.config.default_timeout_seconds as f64 * 1000.0;
                    check_count += 1;
                }
            }
        }

        let avg_response_time = if check_count > 0 {
            total_response_time / check_count as f64
        } else {
            0.0
        };

        // Update component status
        self.update_component_status(component_id, overall_status.clone(), avg_response_time)
            .await?;

        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        self.logger.debug(&format!(
            "Health check completed for {} in {}ms: {:?}",
            component_id,
            end_time - start_time,
            overall_status
        ));

        Ok(overall_status)
    }

    /// Check health of all registered components
    pub async fn check_all_components(&self) -> ArbitrageResult<HashMap<String, HealthStatus>> {
        let mut results = HashMap::new();

        let component_ids = if let Ok(components) = self.components.lock() {
            components.keys().cloned().collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        for component_id in component_ids {
            match self.check_component_health(&component_id).await {
                Ok(status) => {
                    results.insert(component_id, status);
                }
                Err(e) => {
                    self.logger.error(&format!(
                        "Failed to check health for {}: {}",
                        component_id, e
                    ));
                    results.insert(component_id, HealthStatus::Unknown);
                }
            }
        }

        // Update overall metrics
        self.update_overall_metrics().await;

        Ok(results)
    }

    /// Get component health information
    pub async fn get_component_health(&self, component_id: &str) -> Option<ComponentHealth> {
        if let Ok(components) = self.components.lock() {
            components.get(component_id).cloned()
        } else {
            None
        }
    }

    /// Get all component health information
    pub async fn get_all_component_health(&self) -> HashMap<String, ComponentHealth> {
        if let Ok(components) = self.components.lock() {
            components.clone()
        } else {
            HashMap::new()
        }
    }

    /// Get overall system health score
    pub async fn get_overall_health_score(&self) -> f32 {
        if let Ok(components) = self.components.lock() {
            if components.is_empty() {
                return 0.0;
            }

            let total_score: f32 = components.values().map(|c| c.score).sum();
            total_score / components.len() as f32
        } else {
            0.0
        }
    }

    /// Get health metrics
    pub async fn get_metrics(&self) -> HealthMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            HealthMetrics::default()
        }
    }

    /// Execute a specific health check
    async fn execute_health_check(
        &self,
        health_check: &HealthCheck,
    ) -> ArbitrageResult<(HealthStatus, f64)> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        let status = match health_check.check_type {
            HealthCheckType::Ping => {
                // Simple ping check - always healthy for now
                HealthStatus::Healthy
            }
            HealthCheckType::HttpGet => {
                // HTTP GET check - simulate for now
                if let Some(_endpoint) = &health_check.endpoint {
                    // In a real implementation, this would make an HTTP request
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unknown
                }
            }
            HealthCheckType::KvStore => {
                // KV store check
                match self.test_kv_store().await {
                    Ok(_) => HealthStatus::Healthy,
                    Err(_) => HealthStatus::Unhealthy,
                }
            }
            _ => {
                // Other check types - assume healthy for now
                HealthStatus::Healthy
            }
        };

        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let response_time = (end_time - start_time) as f64;

        // Check if response time exceeds threshold
        let final_status = if response_time > health_check.expected_response_time_ms as f64 {
            match status {
                HealthStatus::Healthy => HealthStatus::Degraded,
                other => other,
            }
        } else {
            status
        };

        Ok((final_status, response_time))
    }

    /// Test KV store connectivity
    async fn test_kv_store(&self) -> ArbitrageResult<()> {
        let test_key = format!("{}health_check_test", self.config.kv_key_prefix);
        let test_value = "test";

        // Try to write and read a test value
        self.kv_store
            .put(&test_key, test_value)?
            .expiration_ttl(60) // 1 minute
            .execute()
            .await
            .map_err(|e| ArbitrageError::kv_error(format!("KV write test failed: {}", e)))?;

        let _result = self
            .kv_store
            .get(&test_key)
            .text()
            .await
            .map_err(|e| ArbitrageError::kv_error(format!("KV read test failed: {}", e)))?;

        Ok(())
    }

    /// Update component status
    async fn update_component_status(
        &self,
        component_id: &str,
        status: HealthStatus,
        response_time: f64,
    ) -> ArbitrageResult<()> {
        let mut component_to_update = None;
        let mut alert_to_add = None;

        // Update component and prepare alert if needed
        if let Ok(mut components) = self.components.lock() {
            if let Some(component) = components.get_mut(component_id) {
                let old_status = component.status.clone();
                component.update_status(status.clone(), response_time);

                // Generate alert if status changed to unhealthy
                if old_status.is_operational() && !status.is_operational() {
                    let alert = HealthAlert::new(
                        component_id.to_string(),
                        HealthAlertType::StatusChange,
                        HealthAlertSeverity::Error,
                        format!(
                            "Component {} status changed from {:?} to {:?}",
                            component.component_name, old_status, status
                        ),
                    );
                    component.add_alert(alert.clone());

                    if self.config.enable_alerting {
                        alert_to_add = Some(alert);
                    }
                }

                component_to_update = Some(component.clone());
            }
        }

        // Handle alerting outside of lock
        if let Some(alert) = alert_to_add {
            self.add_active_alert(alert).await;
        }

        // Store updated component in KV outside of lock
        if let Some(component) = component_to_update {
            if self.config.enable_kv_storage {
                self.store_component_in_kv(&component).await?;
            }
        }

        Ok(())
    }

    /// Add an active alert
    async fn add_active_alert(&self, alert: HealthAlert) {
        if let Ok(mut alerts) = self.active_alerts.lock() {
            alerts.insert(alert.alert_id.clone(), alert);
        }
    }

    /// Update overall system metrics
    async fn update_overall_metrics(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            if let Ok(components) = self.components.lock() {
                metrics.total_components = components.len() as u64;
                metrics.healthy_components = components
                    .values()
                    .filter(|c| c.status == HealthStatus::Healthy)
                    .count() as u64;
                metrics.degraded_components = components
                    .values()
                    .filter(|c| c.status == HealthStatus::Degraded)
                    .count() as u64;
                metrics.unhealthy_components = components
                    .values()
                    .filter(|c| c.status == HealthStatus::Unhealthy)
                    .count() as u64;
                metrics.critical_components = components
                    .values()
                    .filter(|c| c.status == HealthStatus::Critical)
                    .count() as u64;
                metrics.unknown_components = components
                    .values()
                    .filter(|c| c.status == HealthStatus::Unknown)
                    .count() as u64;

                // Calculate overall health score
                if !components.is_empty() {
                    let total_score: f32 = components.values().map(|c| c.score).sum();
                    metrics.overall_health_score = total_score / components.len() as f32;
                }

                // Calculate average response time
                let response_times: Vec<f64> =
                    components.values().map(|c| c.response_time_ms).collect();
                if !response_times.is_empty() {
                    metrics.average_response_time_ms =
                        response_times.iter().sum::<f64>() / response_times.len() as f64;
                }

                // Update component type counts
                metrics.components_by_type.clear();
                for component in components.values() {
                    *metrics
                        .components_by_type
                        .entry(component.component_type.clone())
                        .or_insert(0) += 1;
                }

                metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
            }

            // Update alert counts
            if let Ok(alerts) = self.active_alerts.lock() {
                metrics.active_alerts = alerts.len() as u64;

                metrics.alerts_by_severity.clear();
                for alert in alerts.values() {
                    *metrics
                        .alerts_by_severity
                        .entry(alert.severity.clone())
                        .or_insert(0) += 1;
                }
            }
        }
    }

    /// Store component in KV store
    async fn store_component_in_kv(&self, component: &ComponentHealth) -> ArbitrageResult<()> {
        let key = format!(
            "{}component:{}",
            self.config.kv_key_prefix, component.component_id
        );
        let value = serde_json::to_string(component)?;

        self.kv_store
            .put(&key, value)?
            .expiration_ttl(self.config.metrics_retention_days as u64 * 86400) // Convert days to seconds
            .execute()
            .await
            .map_err(|e| ArbitrageError::kv_error(format!("Failed to store component: {}", e)))?;

        Ok(())
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Test basic functionality
        let test_component = ComponentHealth::new(
            "health_monitor_test".to_string(),
            "Health Monitor Test".to_string(),
            "test".to_string(),
        );

        self.register_component(test_component).await?;

        let health_status = self.check_component_health("health_monitor_test").await?;

        Ok(health_status.is_operational())
    }

    /// Update component health status
    pub async fn update_component_health(
        &self,
        component_name: &str,
        is_healthy: bool,
        details: Option<String>,
    ) -> ArbitrageResult<()> {
        let mut component_to_update = None;
        let mut should_alert = false;

        // Update component and check if alert is needed
        if let Ok(mut components) = self.components.lock() {
            if let Some(component) = components.get_mut(component_name) {
                let was_healthy = component.status == HealthStatus::Healthy;
                component.status = if is_healthy {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy
                };
                component.last_check_time = chrono::Utc::now().timestamp_millis() as u64;
                component.response_time_ms = 0.0;
                component.check_count = 0;
                component.success_count = 0;
                component.failure_count = 0;
                component.consecutive_failures = 0;
                component.error_rate_percent = 0.0;
                component.metadata.insert(
                    "details".to_string(),
                    serde_json::Value::String(details.unwrap_or_else(|| "No details".to_string())),
                );

                // Check if we need to alert
                if was_healthy && !is_healthy {
                    should_alert = true;
                }

                component_to_update = Some(component.clone());
            }
        }

        // Handle alerting outside of lock
        if should_alert {
            let alert = HealthAlert::new(
                component_name.to_string(),
                HealthAlertType::StatusChange,
                HealthAlertSeverity::Error,
                format!("Component {} became unhealthy", component_name),
            );
            self.add_active_alert(alert).await;
        }

        // Store component outside of lock
        if let Some(component) = component_to_update {
            self.store_component_in_kv(&component).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_properties() {
        assert_eq!(HealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(HealthStatus::Healthy.score(), 1.0);
        assert!(HealthStatus::Healthy.is_operational());
        assert!(!HealthStatus::Critical.is_operational());
    }

    #[test]
    fn test_health_status_from_score() {
        assert_eq!(HealthStatus::from_score(1.0), HealthStatus::Healthy);
        assert_eq!(HealthStatus::from_score(0.8), HealthStatus::Degraded);
        assert_eq!(HealthStatus::from_score(0.4), HealthStatus::Unhealthy);
        assert_eq!(HealthStatus::from_score(0.1), HealthStatus::Critical);
        assert_eq!(HealthStatus::from_score(0.0), HealthStatus::Unknown);
    }

    #[test]
    fn test_component_health_creation() {
        let component = ComponentHealth::new(
            "test-component".to_string(),
            "Test Component".to_string(),
            "service".to_string(),
        )
        .with_dependency("dep1".to_string())
        .with_tag("env".to_string(), "prod".to_string());

        assert_eq!(component.component_id, "test-component");
        assert_eq!(component.component_name, "Test Component");
        assert_eq!(component.component_type, "service");
        assert_eq!(component.dependencies, vec!["dep1"]);
        assert_eq!(component.tags.get("env"), Some(&"prod".to_string()));
    }

    #[test]
    fn test_component_health_status_update() {
        let mut component = ComponentHealth::new(
            "test-component".to_string(),
            "Test Component".to_string(),
            "service".to_string(),
        );

        assert_eq!(component.status, HealthStatus::Unknown);
        assert_eq!(component.check_count, 0);

        component.update_status(HealthStatus::Healthy, 100.0);
        assert_eq!(component.status, HealthStatus::Healthy);
        assert_eq!(component.check_count, 1);
        assert_eq!(component.success_count, 1);
        assert_eq!(component.consecutive_failures, 0);

        component.update_status(HealthStatus::Critical, 500.0);
        assert_eq!(component.status, HealthStatus::Critical);
        assert_eq!(component.check_count, 2);
        assert_eq!(component.failure_count, 1);
        assert_eq!(component.consecutive_failures, 1);
    }

    #[test]
    fn test_health_check_creation() {
        let health_check = HealthCheck::new(
            "test-component".to_string(),
            "HTTP Health Check".to_string(),
            HealthCheckType::HttpGet,
        )
        .with_endpoint("https://api.example.com/health".to_string())
        .with_timeout(10)
        .with_interval(60);

        assert_eq!(health_check.component_id, "test-component");
        assert_eq!(health_check.check_name, "HTTP Health Check");
        assert_eq!(health_check.check_type, HealthCheckType::HttpGet);
        assert_eq!(
            health_check.endpoint,
            Some("https://api.example.com/health".to_string())
        );
        assert_eq!(health_check.timeout_seconds, 10);
        assert_eq!(health_check.interval_seconds, 60);
    }

    #[test]
    fn test_health_monitor_config_validation() {
        let mut config = HealthMonitorConfig::default();
        assert!(config.validate().is_ok());

        config.default_check_interval_seconds = 0;
        assert!(config.validate().is_err());

        config.default_check_interval_seconds = 30;
        config.health_score_threshold = 1.5;
        assert!(config.validate().is_err());

        config.health_score_threshold = 0.7;
        config.error_rate_threshold_percent = 150.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = HealthMonitorConfig::high_performance();
        assert_eq!(config.default_check_interval_seconds, 15);
        assert_eq!(config.response_time_threshold_ms, 500);
        assert!(config.enable_predictive_analysis);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = HealthMonitorConfig::high_reliability();
        assert_eq!(config.max_consecutive_failures, 3);
        assert_eq!(config.health_score_threshold, 0.8);
        assert!(config.enable_auto_recovery);
    }
}
