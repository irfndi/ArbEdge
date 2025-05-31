// src/services/core/admin/monitoring.rs

use crate::utils::{ArbitrageResult, ArbitrageError};
use worker::{Env, kv::KvStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Monitoring service for system health and performance metrics
#[derive(Debug, Clone)]
pub struct MonitoringService {
    kv_store: KvStore,
    env: Env,
}

impl MonitoringService {
    pub fn new(env: Env, kv_store: KvStore) -> Self {
        Self { kv_store, env }
    }

    /// Get system health status
    pub async fn get_system_health(&self) -> ArbitrageResult<SystemHealth> {
        let mut health = SystemHealth::default();

        // Check KV store health
        health.kv_store_status = self.check_kv_store_health().await;

        // Check service availability
        health.services_status = self.check_services_health().await?;

        // Get performance metrics
        health.performance_metrics = self.get_performance_metrics().await?;

        // Calculate overall health
        health.overall_status = self.calculate_overall_health(&health);

        Ok(health)
    }

    /// Check KV store health
    async fn check_kv_store_health(&self) -> ServiceStatus {
        let test_key = "health_check_test";
        let test_value = "test";

        match self.kv_store.put(test_key, test_value) {
            Ok(put_builder) => {
                match put_builder.execute().await {
                    Ok(_) => {
                        // Try to read back
                        match self.kv_store.get(test_key).text().await {
                            Ok(Some(value)) if value == test_value => ServiceStatus::Healthy,
                            Ok(Some(_)) => ServiceStatus::Degraded,
                            Ok(None) => ServiceStatus::Degraded,
                            Err(_) => ServiceStatus::Unhealthy,
                        }
                    }
                    Err(_) => ServiceStatus::Unhealthy,
                }
            }
            Err(_) => ServiceStatus::Unhealthy,
        }
    }

    /// Check services health
    async fn check_services_health(&self) -> ArbitrageResult<HashMap<String, ServiceStatus>> {
        let mut services = HashMap::new();

        // Check core services (simplified - in production, ping actual services)
        services.insert("user_profile_service".to_string(), ServiceStatus::Healthy);
        services.insert("session_service".to_string(), ServiceStatus::Healthy);
        services.insert("opportunity_service".to_string(), ServiceStatus::Healthy);
        services.insert("exchange_service".to_string(), ServiceStatus::Healthy);
        services.insert("telegram_service".to_string(), ServiceStatus::Healthy);

        // In a real implementation, you would:
        // 1. Ping each service endpoint
        // 2. Check database connections
        // 3. Verify external API connectivity
        // 4. Check queue health
        // 5. Validate cache status

        Ok(services)
    }

    /// Get performance metrics
    async fn get_performance_metrics(&self) -> ArbitrageResult<PerformanceMetrics> {
        // In a real implementation, these would come from actual monitoring systems
        Ok(PerformanceMetrics {
            cpu_usage_percent: 25.5,
            memory_usage_percent: 45.2,
            disk_usage_percent: 30.1,
            network_latency_ms: 15.3,
            active_connections: 150,
            requests_per_minute: 1250,
            error_rate_percent: 0.1,
            average_response_time_ms: 85.2,
            uptime_seconds: 86400, // 24 hours
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Calculate overall health status
    fn calculate_overall_health(&self, health: &SystemHealth) -> ServiceStatus {
        // If KV store is unhealthy, system is unhealthy
        if matches!(health.kv_store_status, ServiceStatus::Unhealthy) {
            return ServiceStatus::Unhealthy;
        }

        // Check critical services
        let critical_services = ["user_profile_service", "session_service"];
        for service_name in &critical_services {
            if let Some(status) = health.services_status.get(*service_name) {
                if matches!(status, ServiceStatus::Unhealthy) {
                    return ServiceStatus::Unhealthy;
                }
            }
        }

        // Check performance thresholds
        let metrics = &health.performance_metrics;
        if metrics.cpu_usage_percent > 90.0 
            || metrics.memory_usage_percent > 90.0 
            || metrics.error_rate_percent > 5.0 {
            return ServiceStatus::Degraded;
        }

        ServiceStatus::Healthy
    }

    /// Get system metrics over time
    pub async fn get_metrics_history(&self, hours: u32) -> ArbitrageResult<Vec<MetricsSnapshot>> {
        let mut snapshots = Vec::new();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let hour_ms = 60 * 60 * 1000;

        // Get historical metrics (simplified - in production, use time-series database)
        for i in 0..hours {
            let timestamp = now - (i as u64 * hour_ms);
            let metrics_key = format!("metrics_snapshot:{}", timestamp / hour_ms);
            
            if let Some(metrics_data) = self.kv_store.get(&metrics_key).text().await? {
                if let Ok(snapshot) = serde_json::from_str::<MetricsSnapshot>(&metrics_data) {
                    snapshots.push(snapshot);
                }
            } else {
                // Generate mock data for demonstration
                snapshots.push(MetricsSnapshot {
                    timestamp,
                    cpu_usage: 20.0 + (i as f64 * 2.0),
                    memory_usage: 40.0 + (i as f64 * 1.5),
                    active_users: 100 + (i * 10),
                    requests_per_minute: 1000 + (i * 50),
                    error_rate: 0.1,
                    response_time_ms: 80.0 + (i as f64 * 2.0),
                });
            }
        }

        snapshots.reverse(); // Most recent first
        Ok(snapshots)
    }

    /// Record metrics snapshot
    pub async fn record_metrics_snapshot(&self, snapshot: MetricsSnapshot) -> ArbitrageResult<()> {
        let hour_ms = 60 * 60 * 1000;
        let metrics_key = format!("metrics_snapshot:{}", snapshot.timestamp / hour_ms);
        
        let snapshot_data = serde_json::to_string(&snapshot)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize metrics snapshot: {}", e)))?;

        self.kv_store.put(&metrics_key, &snapshot_data)?
            .expiration_ttl(7 * 24 * 60 * 60) // Keep for 7 days
            .execute()
            .await?;

        Ok(())
    }

    /// Get error logs
    pub async fn get_error_logs(&self, limit: Option<u32>) -> ArbitrageResult<Vec<ErrorLog>> {
        let limit = limit.unwrap_or(100).min(1000);
        let mut logs = Vec::new();

        // Get recent error logs (simplified implementation)
        for i in 0..limit {
            let log_key = format!("error_log:{}", i);
            if let Some(log_data) = self.kv_store.get(&log_key).text().await? {
                if let Ok(error_log) = serde_json::from_str::<ErrorLog>(&log_data) {
                    logs.push(error_log);
                }
            }
        }

        // Sort by timestamp (most recent first)
        logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        logs.truncate(limit as usize);

        Ok(logs)
    }

    /// Log an error
    pub async fn log_error(&self, error: ErrorLog) -> ArbitrageResult<()> {
        let log_key = format!("error_log:{}", error.timestamp);
        let log_data = serde_json::to_string(&error)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize error log: {}", e)))?;

        self.kv_store.put(&log_key, &log_data)?
            .expiration_ttl(30 * 24 * 60 * 60) // Keep for 30 days
            .execute()
            .await?;

        Ok(())
    }

    /// Get system alerts
    pub async fn get_active_alerts(&self) -> ArbitrageResult<Vec<SystemAlert>> {
        let mut alerts = Vec::new();

        // Check for active alerts (simplified implementation)
        for i in 0..100 {
            let alert_key = format!("system_alert:{}", i);
            if let Some(alert_data) = self.kv_store.get(&alert_key).text().await? {
                if let Ok(alert) = serde_json::from_str::<SystemAlert>(&alert_data) {
                    if alert.is_active {
                        alerts.push(alert);
                    }
                }
            }
        }

        // Sort by severity and timestamp
        alerts.sort_by(|a, b| {
            b.severity.cmp(&a.severity)
                .then(b.created_at.cmp(&a.created_at))
        });

        Ok(alerts)
    }

    /// Create system alert
    pub async fn create_alert(&self, alert: SystemAlert) -> ArbitrageResult<()> {
        let alert_key = format!("system_alert:{}", alert.id);
        let alert_data = serde_json::to_string(&alert)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize alert: {}", e)))?;

        self.kv_store.put(&alert_key, &alert_data)?
            .execute()
            .await?;

        Ok(())
    }

    /// Resolve system alert
    pub async fn resolve_alert(&self, alert_id: &str) -> ArbitrageResult<()> {
        let alert_key = format!("system_alert:{}", alert_id);
        
        if let Some(alert_data) = self.kv_store.get(&alert_key).text().await? {
            let mut alert = serde_json::from_str::<SystemAlert>(&alert_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse alert: {}", e)))?;
            
            alert.is_active = false;
            alert.resolved_at = Some(chrono::Utc::now().timestamp_millis() as u64);
            
            let updated_data = serde_json::to_string(&alert)
                .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize alert: {}", e)))?;

            self.kv_store.put(&alert_key, &updated_data)?
                .execute()
                .await?;
        }

        Ok(())
    }
}

/// System health status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemHealth {
    pub overall_status: ServiceStatus,
    pub kv_store_status: ServiceStatus,
    pub services_status: HashMap<String, ServiceStatus>,
    pub performance_metrics: PerformanceMetrics,
    pub last_checked: u64,
}

impl Default for SystemHealth {
    fn default() -> Self {
        Self {
            overall_status: ServiceStatus::Unknown,
            kv_store_status: ServiceStatus::Unknown,
            services_status: HashMap::new(),
            performance_metrics: PerformanceMetrics::default(),
            last_checked: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Service status enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub disk_usage_percent: f64,
    pub network_latency_ms: f64,
    pub active_connections: u32,
    pub requests_per_minute: u32,
    pub error_rate_percent: f64,
    pub average_response_time_ms: f64,
    pub uptime_seconds: u64,
    pub last_updated: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            cpu_usage_percent: 0.0,
            memory_usage_percent: 0.0,
            disk_usage_percent: 0.0,
            network_latency_ms: 0.0,
            active_connections: 0,
            requests_per_minute: 0,
            error_rate_percent: 0.0,
            average_response_time_ms: 0.0,
            uptime_seconds: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Metrics snapshot for historical data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSnapshot {
    pub timestamp: u64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub active_users: u32,
    pub requests_per_minute: u32,
    pub error_rate: f64,
    pub response_time_ms: f64,
}

/// Error log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorLog {
    pub id: String,
    pub timestamp: u64,
    pub level: String, // "error", "warn", "info"
    pub message: String,
    pub service: String,
    pub user_id: Option<String>,
    pub request_id: Option<String>,
    pub stack_trace: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// System alert
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAlert {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: AlertSeverity,
    pub category: String, // "performance", "security", "availability"
    pub is_active: bool,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
    pub metadata: HashMap<String, String>,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
} 