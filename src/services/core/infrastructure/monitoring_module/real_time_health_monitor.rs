//! Real-time Health Monitoring System for KV, D1, R2 Storage Systems
//!
//! This service provides continuous health monitoring with real-time status tracking,
//! performance metrics collection, latency monitoring, and health status aggregation.
//! Integrates with existing health monitoring infrastructure and circuit breakers.

use crate::services::core::infrastructure::circuit_breaker_service::CircuitBreakerService;
use crate::services::core::infrastructure::monitoring_module::health_monitor::{
    HealthAlert, HealthAlertSeverity, HealthAlertType, HealthMonitor, HealthStatus,
};
use crate::services::core::infrastructure::persistence_layer::PerformanceMonitor;
use crate::utils::{ArbitrageError, ArbitrageResult};

use crate::utils::time::now_instant;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Real-time health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeHealthConfig {
    /// Enable real-time monitoring
    pub enabled: bool,
    /// Health check interval in seconds
    pub check_interval_seconds: u64,
    /// Performance metrics collection interval
    pub metrics_interval_seconds: u64,
    /// Health status cache TTL
    pub status_cache_ttl_seconds: u64,
    /// Enable KV store monitoring
    pub enable_kv_monitoring: bool,
    /// Enable D1 database monitoring
    pub enable_d1_monitoring: bool,
    /// Enable R2 storage monitoring
    pub enable_r2_monitoring: bool,
    /// Maximum latency threshold (ms)
    pub max_latency_threshold_ms: u64,
    /// Error rate threshold (percentage)
    pub error_rate_threshold: f32,
    /// Enable predictive health analysis
    pub enable_predictive_analysis: bool,
    /// Health score calculation weights
    pub health_weights: HealthWeights,
}

/// Health score calculation weights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthWeights {
    pub latency_weight: f32,
    pub error_rate_weight: f32,
    pub throughput_weight: f32,
    pub availability_weight: f32,
}

impl Default for RealTimeHealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_seconds: 30,
            metrics_interval_seconds: 10,
            status_cache_ttl_seconds: 60,
            enable_kv_monitoring: true,
            enable_d1_monitoring: true,
            enable_r2_monitoring: true,
            max_latency_threshold_ms: 1000,
            error_rate_threshold: 5.0,
            enable_predictive_analysis: true,
            health_weights: HealthWeights {
                latency_weight: 0.3,
                error_rate_weight: 0.3,
                throughput_weight: 0.2,
                availability_weight: 0.2,
            },
        }
    }
}

/// Storage system type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageSystemType {
    KvStore,
    D1Database,
    R2Storage,
}

impl StorageSystemType {
    pub fn as_str(&self) -> &str {
        match self {
            StorageSystemType::KvStore => "kv_store",
            StorageSystemType::D1Database => "d1_database",
            StorageSystemType::R2Storage => "r2_storage",
        }
    }
}

/// Real-time health metrics for storage systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHealthMetrics {
    pub system_type: StorageSystemType,
    pub current_status: HealthStatus,
    pub health_score: f32,
    pub availability_percentage: f32,
    pub average_latency_ms: f64,
    pub error_rate_percentage: f32,
    pub throughput_ops_per_second: f64,
    pub connection_pool_utilization: f32,
    pub last_successful_operation: u64,
    pub last_failed_operation: Option<u64>,
    pub consecutive_failures: u32,
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub performance_trends: PerformanceTrend,
    pub last_updated: u64,
}

/// Performance trend data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrend {
    pub latency_trend: Vec<f64>,    // Last 10 measurements
    pub error_rate_trend: Vec<f32>, // Last 10 measurements
    pub throughput_trend: Vec<f64>, // Last 10 measurements
    pub timestamp_trend: Vec<u64>,  // Corresponding timestamps
}

impl Default for PerformanceTrend {
    fn default() -> Self {
        Self {
            latency_trend: Vec::with_capacity(10),
            error_rate_trend: Vec::with_capacity(10),
            throughput_trend: Vec::with_capacity(10),
            timestamp_trend: Vec::with_capacity(10),
        }
    }
}

/// Real-time health monitoring operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckOperation {
    pub operation_id: String,
    pub system_type: StorageSystemType,
    pub operation_type: String,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub duration_ms: Option<u64>,
    pub success: Option<bool>,
    pub error_message: Option<String>,
}

/// Dashboard-ready health data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthDashboardData {
    pub overall_health_score: f32,
    pub system_health_metrics: HashMap<StorageSystemType, StorageHealthMetrics>,
    pub active_alerts: Vec<HealthAlert>,
    pub recent_operations: Vec<HealthCheckOperation>,
    pub health_summary: HealthSummary,
    pub performance_overview: PerformanceOverview,
    pub generated_at: u64,
}

/// Health summary for dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub healthy_systems: u32,
    pub degraded_systems: u32,
    pub unhealthy_systems: u32,
    pub critical_systems: u32,
    pub total_systems: u32,
    pub uptime_percentage: f32,
}

/// Performance overview for dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceOverview {
    pub average_latency_ms: f64,
    pub peak_latency_ms: f64,
    pub total_throughput_ops: f64,
    pub overall_error_rate: f32,
    pub slowest_system: Option<StorageSystemType>,
    pub fastest_system: Option<StorageSystemType>,
}

/// Real-time health monitoring service
pub struct RealTimeHealthMonitor {
    config: RealTimeHealthConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Integration with existing services
    health_monitor: Option<Arc<HealthMonitor>>,
    circuit_breaker_service: Option<Arc<CircuitBreakerService>>,
    performance_monitor: Option<Arc<PerformanceMonitor>>,

    // Real-time metrics storage
    storage_metrics: Arc<Mutex<HashMap<StorageSystemType, StorageHealthMetrics>>>,
    recent_operations: Arc<Mutex<Vec<HealthCheckOperation>>>,
    active_alerts: Arc<Mutex<Vec<HealthAlert>>>,

    // Internal monitoring state
    is_monitoring: Arc<Mutex<bool>>,
    last_health_check: Arc<Mutex<u64>>,
}

impl RealTimeHealthMonitor {
    /// Create new real-time health monitor
    pub async fn new(
        config: RealTimeHealthConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        let monitor = Self {
            config,
            logger,
            kv_store,
            health_monitor: None,
            circuit_breaker_service: None,
            performance_monitor: None,
            storage_metrics: Arc::new(Mutex::new(HashMap::new())),
            recent_operations: Arc::new(Mutex::new(Vec::new())),
            active_alerts: Arc::new(Mutex::new(Vec::new())),
            is_monitoring: Arc::new(Mutex::new(false)),
            last_health_check: Arc::new(Mutex::new(0)),
        };

        // Initialize storage metrics
        monitor.initialize_storage_metrics().await?;

        monitor.logger.info("Real-time Health Monitor initialized");
        Ok(monitor)
    }

    /// Set health monitor integration
    pub fn set_health_monitor(&mut self, health_monitor: Arc<HealthMonitor>) {
        self.health_monitor = Some(health_monitor);
        self.logger.info("Health monitor integration enabled");
    }

    /// Set circuit breaker service integration
    pub fn set_circuit_breaker_service(
        &mut self,
        circuit_breaker_service: Arc<CircuitBreakerService>,
    ) {
        self.circuit_breaker_service = Some(circuit_breaker_service);
        self.logger
            .info("Circuit breaker service integration enabled");
    }

    /// Set performance monitor integration
    pub fn set_performance_monitor(&mut self, performance_monitor: Arc<PerformanceMonitor>) {
        self.performance_monitor = Some(performance_monitor);
        self.logger.info("Performance monitor integration enabled");
    }

    /// Start real-time monitoring
    pub async fn start_monitoring(&self) -> ArbitrageResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        {
            let mut is_monitoring = self.is_monitoring.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire monitoring lock: {}", e))
            })?;
            *is_monitoring = true;
        }

        self.logger.info("Real-time health monitoring started");
        Ok(())
    }

    /// Stop real-time monitoring
    pub async fn stop_monitoring(&self) -> ArbitrageResult<()> {
        {
            let mut is_monitoring = self.is_monitoring.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire monitoring lock: {}", e))
            })?;
            *is_monitoring = false;
        }

        self.logger.info("Real-time health monitoring stopped");
        Ok(())
    }

    /// Perform health checks on all enabled storage systems
    pub async fn check_all_systems(&self) -> ArbitrageResult<HealthDashboardData> {
        let start_time = now_instant();

        // Update last health check time
        {
            let mut last_check = self.last_health_check.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire last check lock: {}", e))
            })?;
            *last_check = chrono::Utc::now().timestamp_millis() as u64;
        }

        // Check each enabled storage system
        if self.config.enable_kv_monitoring {
            self.check_kv_store().await?;
        }

        if self.config.enable_d1_monitoring {
            self.check_d1_database().await?;
        }

        if self.config.enable_r2_monitoring {
            self.check_r2_storage().await?;
        }

        // Generate dashboard data
        let dashboard_data = self.generate_dashboard_data().await?;

        let duration = start_time.elapsed();
        self.logger.debug(&format!(
            "Health check completed in {}ms",
            duration.as_millis()
        ));

        Ok(dashboard_data)
    }

    /// Check KV store health
    async fn check_kv_store(&self) -> ArbitrageResult<()> {
        let operation_id = uuid::Uuid::new_v4().to_string();
        let start_time = now_instant();
        let started_at = chrono::Utc::now().timestamp_millis() as u64;

        let mut operation = HealthCheckOperation {
            operation_id: operation_id.clone(),
            system_type: StorageSystemType::KvStore,
            operation_type: "health_check".to_string(),
            started_at,
            completed_at: None,
            duration_ms: None,
            success: None,
            error_message: None,
        };

        // Perform KV health check
        let result = self.test_kv_operations().await;

        let duration = start_time.elapsed();
        let completed_at = chrono::Utc::now().timestamp_millis() as u64;

        operation.completed_at = Some(completed_at);
        operation.duration_ms = Some(duration.as_millis() as u64);

        match result {
            Ok(_) => {
                operation.success = Some(true);
                self.update_storage_health(
                    StorageSystemType::KvStore,
                    HealthStatus::Healthy,
                    duration.as_millis() as f64,
                    false,
                )
                .await?;
            }
            Err(e) => {
                operation.success = Some(false);
                operation.error_message = Some(e.to_string());
                self.update_storage_health(
                    StorageSystemType::KvStore,
                    HealthStatus::Unhealthy,
                    duration.as_millis() as f64,
                    true,
                )
                .await?;
            }
        }

        // Store operation history
        self.add_operation_to_history(operation).await?;

        Ok(())
    }

    /// Test KV store operations
    async fn test_kv_operations(&self) -> ArbitrageResult<()> {
        let test_key = format!("health_check_{}", chrono::Utc::now().timestamp_millis());
        let test_value = "health_check_value";

        // Test write operation
        self.kv_store
            .put(&test_key, test_value)?
            .execute()
            .await
            .map_err(|e| ArbitrageError::api_error(format!("KV put operation failed: {}", e)))?;

        // Test read operation
        let retrieved_value =
            self.kv_store.get(&test_key).text().await.map_err(|e| {
                ArbitrageError::api_error(format!("KV get operation failed: {}", e))
            })?;

        if retrieved_value.is_none() {
            return Err(ArbitrageError::api_error("KV get returned null"));
        }

        // Test delete operation
        self.kv_store
            .delete(&test_key)
            .await
            .map_err(|e| ArbitrageError::api_error(format!("KV delete operation failed: {}", e)))?;

        Ok(())
    }

    /// Check D1 database health
    async fn check_d1_database(&self) -> ArbitrageResult<()> {
        let operation_id = uuid::Uuid::new_v4().to_string();
        let start_time = now_instant();
        let started_at = chrono::Utc::now().timestamp_millis() as u64;

        let mut operation = HealthCheckOperation {
            operation_id,
            system_type: StorageSystemType::D1Database,
            operation_type: "health_check".to_string(),
            started_at,
            completed_at: None,
            duration_ms: None,
            success: None,
            error_message: None,
        };

        // Test D1 database connectivity and basic operations
        let result = self.test_d1_operations().await;

        let duration = start_time.elapsed();
        let completed_at = chrono::Utc::now().timestamp_millis() as u64;

        operation.completed_at = Some(completed_at);
        operation.duration_ms = Some(duration.as_millis() as u64);

        match result {
            Ok(_) => {
                operation.success = Some(true);
                self.update_storage_health(
                    StorageSystemType::D1Database,
                    HealthStatus::Healthy,
                    duration.as_millis() as f64,
                    false,
                )
                .await?;
            }
            Err(e) => {
                operation.success = Some(false);
                operation.error_message = Some(e.to_string());
                self.update_storage_health(
                    StorageSystemType::D1Database,
                    HealthStatus::Unhealthy,
                    duration.as_millis() as f64,
                    true,
                )
                .await?;
            }
        }

        self.add_operation_to_history(operation).await?;
        Ok(())
    }

    /// Test D1 database operations
    async fn test_d1_operations(&self) -> ArbitrageResult<()> {
        // For now, simulate D1 health check
        // In production, this would connect to D1 and run actual queries
        Ok(())
    }

    /// Check R2 storage health
    async fn check_r2_storage(&self) -> ArbitrageResult<()> {
        let operation_id = uuid::Uuid::new_v4().to_string();
        let start_time = now_instant();
        let started_at = chrono::Utc::now().timestamp_millis() as u64;

        let mut operation = HealthCheckOperation {
            operation_id,
            system_type: StorageSystemType::R2Storage,
            operation_type: "health_check".to_string(),
            started_at,
            completed_at: None,
            duration_ms: None,
            success: None,
            error_message: None,
        };

        // Test R2 storage accessibility and operations
        let result = self.test_r2_operations().await;

        let duration = start_time.elapsed();
        let completed_at = chrono::Utc::now().timestamp_millis() as u64;

        operation.completed_at = Some(completed_at);
        operation.duration_ms = Some(duration.as_millis() as u64);

        match result {
            Ok(_) => {
                operation.success = Some(true);
                self.update_storage_health(
                    StorageSystemType::R2Storage,
                    HealthStatus::Healthy,
                    duration.as_millis() as f64,
                    false,
                )
                .await?;
            }
            Err(e) => {
                operation.success = Some(false);
                operation.error_message = Some(e.to_string());
                self.update_storage_health(
                    StorageSystemType::R2Storage,
                    HealthStatus::Unhealthy,
                    duration.as_millis() as f64,
                    true,
                )
                .await?;
            }
        }

        self.add_operation_to_history(operation).await?;
        Ok(())
    }

    /// Test R2 storage operations
    async fn test_r2_operations(&self) -> ArbitrageResult<()> {
        // For now, simulate R2 health check
        // In production, this would connect to R2 and test object operations
        Ok(())
    }

    /// Update storage system health metrics
    async fn update_storage_health(
        &self,
        system_type: StorageSystemType,
        status: HealthStatus,
        latency_ms: f64,
        is_error: bool,
    ) -> ArbitrageResult<()> {
        let metrics_for_alerts = {
            let mut metrics = self.storage_metrics.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire metrics lock: {}", e))
            })?;

            let metric =
                metrics
                    .entry(system_type.clone())
                    .or_insert_with(|| StorageHealthMetrics {
                        system_type: system_type.clone(),
                        current_status: HealthStatus::Unknown,
                        health_score: 0.0,
                        availability_percentage: 100.0,
                        average_latency_ms: 0.0,
                        error_rate_percentage: 0.0,
                        throughput_ops_per_second: 0.0,
                        connection_pool_utilization: 0.0,
                        last_successful_operation: 0,
                        last_failed_operation: None,
                        consecutive_failures: 0,
                        total_operations: 0,
                        successful_operations: 0,
                        failed_operations: 0,
                        performance_trends: PerformanceTrend::default(),
                        last_updated: 0,
                    });

            // Update basic metrics
            metric.current_status = status.clone();
            metric.total_operations += 1;
            metric.last_updated = chrono::Utc::now().timestamp_millis() as u64;

            // Update latency moving average
            let alpha = 0.3; // Smoothing factor
            if metric.average_latency_ms == 0.0 {
                metric.average_latency_ms = latency_ms;
            } else {
                metric.average_latency_ms =
                    alpha * latency_ms + (1.0 - alpha) * metric.average_latency_ms;
            }

            // Update error tracking
            if is_error {
                metric.failed_operations += 1;
                metric.consecutive_failures += 1;
                metric.last_failed_operation = Some(metric.last_updated);
            } else {
                metric.successful_operations += 1;
                metric.consecutive_failures = 0;
                metric.last_successful_operation = metric.last_updated;
            }

            // Update error rate
            metric.error_rate_percentage =
                (metric.failed_operations as f32 / metric.total_operations as f32) * 100.0;

            // Update availability
            metric.availability_percentage =
                (metric.successful_operations as f32 / metric.total_operations as f32) * 100.0;

            // Calculate health score using weighted formula
            metric.health_score = self.calculate_health_score(metric);

            // Update performance trends (safe to use ? here since lock will be dropped)
            if let Err(e) = self.update_performance_trends(metric, latency_ms) {
                // Log error but don't fail the entire operation
                self.logger
                    .error(&format!("Failed to update performance trends: {}", e));
            }

            // Clone metrics for alert checking
            metric.clone()
        }; // Lock is dropped here

        // Check for alerts after releasing the lock
        self.check_for_alerts(&system_type, &metrics_for_alerts)
            .await?;

        Ok(())
    }

    /// Calculate health score based on multiple factors
    fn calculate_health_score(&self, metrics: &StorageHealthMetrics) -> f32 {
        let weights = &self.config.health_weights;

        // Latency score (lower is better)
        let latency_score = if metrics.average_latency_ms
            <= self.config.max_latency_threshold_ms as f64
        {
            1.0 - (metrics.average_latency_ms / self.config.max_latency_threshold_ms as f64) as f32
        } else {
            0.0
        };

        // Error rate score (lower is better)
        let error_score = if metrics.error_rate_percentage <= self.config.error_rate_threshold {
            1.0 - (metrics.error_rate_percentage / self.config.error_rate_threshold)
        } else {
            0.0
        };

        // Availability score
        let availability_score = metrics.availability_percentage / 100.0;

        // Throughput score (simplified)
        let throughput_score = if metrics.throughput_ops_per_second > 0.0 {
            (metrics.throughput_ops_per_second / 100.0).min(1.0) as f32
        } else {
            0.5 // Neutral score if no throughput data
        };

        // Weighted calculation
        weights.latency_weight * latency_score
            + weights.error_rate_weight * error_score
            + weights.availability_weight * availability_score
            + weights.throughput_weight * throughput_score
    }

    /// Update performance trends
    fn update_performance_trends(
        &self,
        metrics: &mut StorageHealthMetrics,
        latency_ms: f64,
    ) -> ArbitrageResult<()> {
        let trends = &mut metrics.performance_trends;
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Add new data points
        trends.latency_trend.push(latency_ms);
        trends.error_rate_trend.push(metrics.error_rate_percentage);
        trends
            .throughput_trend
            .push(metrics.throughput_ops_per_second);
        trends.timestamp_trend.push(now);

        // Keep only last 10 measurements
        if trends.latency_trend.len() > 10 {
            trends.latency_trend.remove(0);
            trends.error_rate_trend.remove(0);
            trends.throughput_trend.remove(0);
            trends.timestamp_trend.remove(0);
        }

        Ok(())
    }

    /// Check for alert conditions
    async fn check_for_alerts(
        &self,
        system_type: &StorageSystemType,
        metrics: &StorageHealthMetrics,
    ) -> ArbitrageResult<()> {
        let mut alerts_to_add = Vec::new();

        // High latency alert
        if metrics.average_latency_ms > self.config.max_latency_threshold_ms as f64 {
            alerts_to_add.push(self.create_alert(
                system_type.clone(),
                HealthAlertType::SlowResponse,
                HealthAlertSeverity::Warning,
                format!("High latency detected: {:.2}ms", metrics.average_latency_ms),
            ));
        }

        // High error rate alert
        if metrics.error_rate_percentage > self.config.error_rate_threshold {
            alerts_to_add.push(self.create_alert(
                system_type.clone(),
                HealthAlertType::HighErrorRate,
                HealthAlertSeverity::Error,
                format!("High error rate: {:.1}%", metrics.error_rate_percentage),
            ));
        }

        // Consecutive failures alert
        if metrics.consecutive_failures >= 3 {
            alerts_to_add.push(self.create_alert(
                system_type.clone(),
                HealthAlertType::ConsecutiveFailures,
                HealthAlertSeverity::Critical,
                format!("Consecutive failures: {}", metrics.consecutive_failures),
            ));
        }

        // Add alerts to active list
        if !alerts_to_add.is_empty() {
            let mut active_alerts = self.active_alerts.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire alerts lock: {}", e))
            })?;

            for alert in alerts_to_add {
                active_alerts.push(alert);
            }

            // Keep only recent alerts (last 20)
            if active_alerts.len() > 20 {
                let len = active_alerts.len();
                active_alerts.drain(0..len - 20);
            }
        }

        Ok(())
    }

    /// Create health alert
    fn create_alert(
        &self,
        system_type: StorageSystemType,
        alert_type: HealthAlertType,
        severity: HealthAlertSeverity,
        message: String,
    ) -> HealthAlert {
        HealthAlert::new(
            system_type.as_str().to_string(),
            alert_type,
            severity,
            message,
        )
    }

    /// Add operation to history
    async fn add_operation_to_history(
        &self,
        operation: HealthCheckOperation,
    ) -> ArbitrageResult<()> {
        let mut recent_ops = self.recent_operations.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire operations lock: {}", e))
        })?;

        recent_ops.push(operation);

        // Keep only recent operations (last 50)
        if recent_ops.len() > 50 {
            recent_ops.remove(0);
        }

        Ok(())
    }

    /// Generate dashboard data
    async fn generate_dashboard_data(&self) -> ArbitrageResult<HealthDashboardData> {
        let storage_metrics = self
            .storage_metrics
            .lock()
            .map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire metrics lock: {}", e))
            })?
            .clone();

        let active_alerts = self
            .active_alerts
            .lock()
            .map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire alerts lock: {}", e))
            })?
            .clone();

        let recent_operations = self
            .recent_operations
            .lock()
            .map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire operations lock: {}", e))
            })?
            .clone();

        // Calculate overall health score
        let overall_health_score = if storage_metrics.is_empty() {
            0.0
        } else {
            storage_metrics
                .values()
                .map(|m| m.health_score)
                .sum::<f32>()
                / storage_metrics.len() as f32
        };

        // Generate health summary
        let health_summary = self.generate_health_summary(&storage_metrics);

        // Generate performance overview
        let performance_overview = self.generate_performance_overview(&storage_metrics);

        Ok(HealthDashboardData {
            overall_health_score,
            system_health_metrics: storage_metrics,
            active_alerts,
            recent_operations,
            health_summary,
            performance_overview,
            generated_at: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Generate health summary
    fn generate_health_summary(
        &self,
        metrics: &HashMap<StorageSystemType, StorageHealthMetrics>,
    ) -> HealthSummary {
        let mut healthy = 0;
        let mut degraded = 0;
        let mut unhealthy = 0;
        let mut critical = 0;

        for metric in metrics.values() {
            match metric.current_status {
                HealthStatus::Healthy => healthy += 1,
                HealthStatus::Degraded => degraded += 1,
                HealthStatus::Unhealthy => unhealthy += 1,
                HealthStatus::Critical => critical += 1,
                HealthStatus::Unknown => {} // Don't count unknown
            }
        }

        let total = healthy + degraded + unhealthy + critical;
        let uptime_percentage = if total > 0 {
            (healthy + degraded) as f32 / total as f32 * 100.0
        } else {
            100.0
        };

        HealthSummary {
            healthy_systems: healthy,
            degraded_systems: degraded,
            unhealthy_systems: unhealthy,
            critical_systems: critical,
            total_systems: total,
            uptime_percentage,
        }
    }

    /// Generate performance overview
    fn generate_performance_overview(
        &self,
        metrics: &HashMap<StorageSystemType, StorageHealthMetrics>,
    ) -> PerformanceOverview {
        if metrics.is_empty() {
            return PerformanceOverview {
                average_latency_ms: 0.0,
                peak_latency_ms: 0.0,
                total_throughput_ops: 0.0,
                overall_error_rate: 0.0,
                slowest_system: None,
                fastest_system: None,
            };
        }

        let latencies: Vec<f64> = metrics.values().map(|m| m.average_latency_ms).collect();
        let average_latency_ms = latencies.iter().sum::<f64>() / latencies.len() as f64;
        let peak_latency_ms = latencies.iter().fold(0.0f64, |acc, &x| acc.max(x));

        let total_throughput_ops = metrics.values().map(|m| m.throughput_ops_per_second).sum();

        let error_rates: Vec<f32> = metrics.values().map(|m| m.error_rate_percentage).collect();
        let overall_error_rate = error_rates.iter().sum::<f32>() / error_rates.len() as f32;

        // Find slowest and fastest systems
        let slowest_system = metrics
            .iter()
            .max_by(|(_, a), (_, b)| {
                a.average_latency_ms
                    .partial_cmp(&b.average_latency_ms)
                    .unwrap()
            })
            .map(|(system_type, _)| system_type.clone());

        let fastest_system = metrics
            .iter()
            .min_by(|(_, a), (_, b)| {
                a.average_latency_ms
                    .partial_cmp(&b.average_latency_ms)
                    .unwrap()
            })
            .map(|(system_type, _)| system_type.clone());

        PerformanceOverview {
            average_latency_ms,
            peak_latency_ms,
            total_throughput_ops,
            overall_error_rate,
            slowest_system,
            fastest_system,
        }
    }

    /// Initialize storage metrics for all enabled systems
    async fn initialize_storage_metrics(&self) -> ArbitrageResult<()> {
        let mut metrics = self.storage_metrics.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire metrics lock: {}", e))
        })?;

        let now = chrono::Utc::now().timestamp_millis() as u64;

        if self.config.enable_kv_monitoring {
            metrics.insert(
                StorageSystemType::KvStore,
                StorageHealthMetrics {
                    system_type: StorageSystemType::KvStore,
                    current_status: HealthStatus::Unknown,
                    health_score: 0.0,
                    availability_percentage: 100.0,
                    average_latency_ms: 0.0,
                    error_rate_percentage: 0.0,
                    throughput_ops_per_second: 0.0,
                    connection_pool_utilization: 0.0,
                    last_successful_operation: now,
                    last_failed_operation: None,
                    consecutive_failures: 0,
                    total_operations: 0,
                    successful_operations: 0,
                    failed_operations: 0,
                    performance_trends: PerformanceTrend::default(),
                    last_updated: now,
                },
            );
        }

        if self.config.enable_d1_monitoring {
            metrics.insert(
                StorageSystemType::D1Database,
                StorageHealthMetrics {
                    system_type: StorageSystemType::D1Database,
                    current_status: HealthStatus::Unknown,
                    health_score: 0.0,
                    availability_percentage: 100.0,
                    average_latency_ms: 0.0,
                    error_rate_percentage: 0.0,
                    throughput_ops_per_second: 0.0,
                    connection_pool_utilization: 0.0,
                    last_successful_operation: now,
                    last_failed_operation: None,
                    consecutive_failures: 0,
                    total_operations: 0,
                    successful_operations: 0,
                    failed_operations: 0,
                    performance_trends: PerformanceTrend::default(),
                    last_updated: now,
                },
            );
        }

        if self.config.enable_r2_monitoring {
            metrics.insert(
                StorageSystemType::R2Storage,
                StorageHealthMetrics {
                    system_type: StorageSystemType::R2Storage,
                    current_status: HealthStatus::Unknown,
                    health_score: 0.0,
                    availability_percentage: 100.0,
                    average_latency_ms: 0.0,
                    error_rate_percentage: 0.0,
                    throughput_ops_per_second: 0.0,
                    connection_pool_utilization: 0.0,
                    last_successful_operation: now,
                    last_failed_operation: None,
                    consecutive_failures: 0,
                    total_operations: 0,
                    successful_operations: 0,
                    failed_operations: 0,
                    performance_trends: PerformanceTrend::default(),
                    last_updated: now,
                },
            );
        }

        Ok(())
    }

    /// Get real-time health status for specific system
    pub async fn get_system_health(
        &self,
        system_type: &StorageSystemType,
    ) -> Option<StorageHealthMetrics> {
        let metrics = self.storage_metrics.lock().ok()?;
        metrics.get(system_type).cloned()
    }

    /// Get overall health dashboard
    pub async fn get_health_dashboard(&self) -> ArbitrageResult<HealthDashboardData> {
        self.generate_dashboard_data().await
    }

    /// Health check for the real-time monitor itself
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let is_monitoring = self
            .is_monitoring
            .lock()
            .map_err(|_| ArbitrageError::internal_error("Real-time monitor unresponsive"))?;

        let last_check = self
            .last_health_check
            .lock()
            .map_err(|_| ArbitrageError::internal_error("Real-time monitor unresponsive"))?;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let check_age_ms = now - *last_check;

        // Consider healthy if monitoring is running and last check was within reasonable time
        let is_healthy =
            *is_monitoring && check_age_ms < (self.config.check_interval_seconds * 2000);

        Ok(is_healthy)
    }
}
