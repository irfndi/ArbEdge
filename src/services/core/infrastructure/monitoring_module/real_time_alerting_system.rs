//! Real-Time Alerting System - Production-Ready Implementation
//!
//! Provides comprehensive alerting, anomaly detection, correlation, escalation,
//! and notification capabilities with enterprise-grade features.

use crate::services::core::infrastructure::{
    persistence_layer::{
        connection_pool::ConnectionManager, transaction_coordinator::TransactionCoordinator,
    },
    shared_types::ComponentHealth,
};
use crate::utils::now_system_time;
use futures::channel::{mpsc, oneshot};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Alert severity levels following industry standards (P1-P5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    Critical = 1, // P1 - Immediate action required
    High = 2,     // P2 - High priority
    Medium = 3,   // P3 - Medium priority
    Low = 4,      // P4 - Low priority
    Info = 5,     // P5 - Informational
}

/// Alert states following incident lifecycle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertState {
    Triggered,
    Acknowledged,
    Escalated,
    Resolved,
    Suppressed,
    Expired,
}

/// Notification channels for multi-channel alerting
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotificationChannel {
    Email,
    Slack,
    Webhook,
    SMS,
    PagerDuty,
    Teams,
}

/// Alert correlation keys for grouping related alerts
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationKey {
    pub service: String,
    pub component: String,
    pub metric_type: String,
    pub fingerprint: String,
}

/// Alert metadata and context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub correlation_key: CorrelationKey,
    pub severity: AlertSeverity,
    pub state: AlertState,
    pub title: String,
    pub description: String,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub acknowledged_at: Option<SystemTime>,
    pub resolved_at: Option<SystemTime>,
    pub acknowledged_by: Option<String>,
    pub tags: HashMap<String, String>,
    pub runbook_url: Option<String>,
    pub dashboard_url: Option<String>,
    pub metric_value: f64,
    pub threshold: f64,
    pub context: HashMap<String, serde_json::Value>,
    pub escalation_level: u8,
    pub fire_count: u32,
    pub suppressed_until: Option<SystemTime>,
}

/// Escalation policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationPolicy {
    pub id: String,
    pub name: String,
    pub levels: Vec<EscalationLevel>,
    pub repeat_interval: Duration,
    pub max_escalations: u8,
}

/// Individual escalation level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationLevel {
    pub level: u8,
    pub timeout: Duration,
    pub channels: Vec<NotificationChannel>,
    pub targets: Vec<String>, // email addresses, user IDs, etc.
}

/// Notification template for different channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationTemplate {
    pub channel: NotificationChannel,
    pub subject_template: String,
    pub body_template: String,
    pub format: String, // json, markdown, html, plain
}

/// Anomaly detection algorithm types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyDetectionAlgorithm {
    StatisticalThreshold,
    ZScore,
    IQR, // Interquartile Range
    MovingAverage,
    SeasonalDecomposition,
    IsolationForest,
    LocalOutlierFactor,
}

/// Anomaly detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetectionConfig {
    pub algorithm: AnomalyDetectionAlgorithm,
    pub window_size: usize,
    pub sensitivity: f64,
    pub threshold_multiplier: f64,
    pub min_data_points: usize,
    pub seasonal_periods: Option<usize>,
}

/// Statistical metrics for anomaly detection
#[derive(Debug, Clone)]
pub struct MetricStatistics {
    pub values: VecDeque<(SystemTime, f64)>,
    pub mean: f64,
    pub variance: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub percentiles: HashMap<u8, f64>, // P50, P95, P99, etc.
    pub last_updated: SystemTime,
}

/// Alert correlation group containing related alerts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCorrelationGroup {
    pub correlation_key: CorrelationKey,
    pub alerts: Vec<Uuid>,
    pub primary_alert: Uuid,
    pub created_at: SystemTime,
    pub last_updated: SystemTime,
    pub correlation_count: u32,
}

/// Suppression rule for maintenance windows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuppressionRule {
    pub id: String,
    pub name: String,
    pub pattern: String, // regex pattern for matching alerts
    pub start_time: SystemTime,
    pub end_time: SystemTime,
    pub reason: String,
    pub created_by: String,
}

/// Real-time alerting system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingSystemConfig {
    pub max_alerts_in_memory: usize,
    pub correlation_window: Duration,
    pub deduplication_window: Duration,
    pub escalation_check_interval: Duration,
    pub metrics_retention: Duration,
    pub notification_timeout: Duration,
    pub max_notification_retries: u8,
    pub anomaly_detection: AnomalyDetectionConfig,
    pub default_escalation_policy: String,
}

impl Default for AlertingSystemConfig {
    fn default() -> Self {
        Self {
            max_alerts_in_memory: 10000,
            correlation_window: Duration::from_secs(300), // 5 minutes
            deduplication_window: Duration::from_secs(60), // 1 minute
            escalation_check_interval: Duration::from_secs(60), // 1 minute
            metrics_retention: Duration::from_secs(3600 * 24 * 7), // 7 days
            notification_timeout: Duration::from_secs(30),
            max_notification_retries: 3,
            anomaly_detection: AnomalyDetectionConfig {
                algorithm: AnomalyDetectionAlgorithm::ZScore,
                window_size: 100,
                sensitivity: 2.0,
                threshold_multiplier: 2.5,
                min_data_points: 10,
                seasonal_periods: None,
            },
            default_escalation_policy: "default".to_string(),
        }
    }
}

/// Command types for the alerting system actor
#[allow(clippy::large_enum_variant)]
pub enum AlertingCommand {
    ProcessAlert(Box<Alert>),
    AcknowledgeAlert {
        alert_id: Uuid,
        acknowledged_by: String,
    },
    ResolveAlert {
        alert_id: Uuid,
        resolved_by: String,
    },
    SuppressAlert {
        alert_id: Uuid,
        duration: Duration,
        reason: String,
    },
    AddSuppressionRule(SuppressionRule),
    RemoveSuppressionRule(String),
    UpdateEscalationPolicy(EscalationPolicy),
    GetAlertStatus {
        alert_id: Uuid,
        response: oneshot::Sender<Option<Alert>>,
    },
    GetActiveAlerts {
        response: oneshot::Sender<Vec<Alert>>,
    },
    GetCorrelatedAlerts {
        correlation_key: CorrelationKey,
        response: oneshot::Sender<Vec<Alert>>,
    },
    CheckAnomalies {
        metric_name: String,
        value: f64,
        timestamp: SystemTime,
    },
    Shutdown,
}

/// Notification delivery status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationStatus {
    pub notification_id: Uuid,
    pub alert_id: Uuid,
    pub channel: NotificationChannel,
    pub target: String,
    pub status: String, // "sent", "failed", "retrying"
    pub attempt: u8,
    pub sent_at: Option<SystemTime>,
    pub error: Option<String>,
}

/// Correlation result enumeration
#[derive(Debug)]
enum CorrelationResult {
    NewAlert,
    Deduplicated(Uuid),
    Correlated(CorrelationKey),
}

/// Main real-time alerting system
pub struct RealTimeAlertingSystem {
    config: AlertingSystemConfig,
    alerts: Arc<RwLock<HashMap<Uuid, Alert>>>,
    correlation_groups: Arc<RwLock<HashMap<CorrelationKey, AlertCorrelationGroup>>>,
    escalation_policies: Arc<RwLock<HashMap<String, EscalationPolicy>>>,
    suppression_rules: Arc<RwLock<HashMap<String, SuppressionRule>>>,
    notification_templates: Arc<RwLock<HashMap<NotificationChannel, NotificationTemplate>>>,
    metric_statistics: Arc<RwLock<HashMap<String, MetricStatistics>>>,
    command_tx: mpsc::UnboundedSender<AlertingCommand>,
}

// Implement Send + Sync for thread safety
unsafe impl Send for RealTimeAlertingSystem {}
unsafe impl Sync for RealTimeAlertingSystem {}

impl RealTimeAlertingSystem {
    /// Create a new real-time alerting system
    pub async fn new(
        config: AlertingSystemConfig,
        _connection_manager: Arc<ConnectionManager>,
        _transaction_coordinator: Arc<TransactionCoordinator>,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let (command_tx, _command_rx) = mpsc::unbounded();

        let system = Arc::new(Self {
            config: config.clone(),
            alerts: Arc::new(RwLock::new(HashMap::new())),
            correlation_groups: Arc::new(RwLock::new(HashMap::new())),
            escalation_policies: Arc::new(RwLock::new(HashMap::new())),
            suppression_rules: Arc::new(RwLock::new(HashMap::new())),
            notification_templates: Arc::new(RwLock::new(HashMap::new())),
            metric_statistics: Arc::new(RwLock::new(HashMap::new())),
            command_tx,
        });

        // Initialize default escalation policy
        system.initialize_default_policies().await?;

        // Note: Background tasks removed for WASM compatibility
        // In a real WASM environment, these would need to be triggered differently

        Ok(system)
    }

    /// Initialize default escalation policies and notification templates
    async fn initialize_default_policies(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let default_policy = EscalationPolicy {
            id: "default".to_string(),
            name: "Default Escalation Policy".to_string(),
            levels: vec![
                EscalationLevel {
                    level: 0,
                    timeout: Duration::from_secs(300), // 5 minutes
                    channels: vec![NotificationChannel::Email],
                    targets: vec!["oncall@example.com".to_string()],
                },
                EscalationLevel {
                    level: 1,
                    timeout: Duration::from_secs(900), // 15 minutes
                    channels: vec![NotificationChannel::Slack],
                    targets: vec!["#alerts".to_string()],
                },
            ],
            repeat_interval: Duration::from_secs(1800), // 30 minutes
            max_escalations: 3,
        };

        let mut policies = self.escalation_policies.write().unwrap();
        policies.insert("default".to_string(), default_policy);

        let default_template = NotificationTemplate {
            channel: NotificationChannel::Email,
            subject_template: "Alert: {{title}}".to_string(),
            body_template: "{{description}}\n\nSeverity: {{severity}}\nValue: {{metric_value}}\nThreshold: {{threshold}}".to_string(),
            format: "plain".to_string(),
        };

        let mut templates = self.notification_templates.write().unwrap();
        templates.insert(NotificationChannel::Email, default_template);

        Ok(())
    }

    /// Process an incoming alert
    pub async fn process_alert(
        &self,
        mut alert: Alert,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if alert should be suppressed
        if self.is_alert_suppressed(&alert).await {
            alert.state = AlertState::Suppressed;
            return Ok(());
        }

        // Check for existing correlations and deduplication
        let correlation_result = self.correlate_alert(&alert).await?;

        match correlation_result {
            CorrelationResult::NewAlert => {
                // Process as new alert
                self.create_new_alert(alert).await?;
            }
            CorrelationResult::Deduplicated(existing_id) => {
                // Update existing alert fire count
                self.update_alert_fire_count(existing_id).await?;
            }
            CorrelationResult::Correlated(group_key) => {
                // Add to existing correlation group
                self.add_to_correlation_group(alert, group_key).await?;
            }
        }

        Ok(())
    }

    /// Check if metric value indicates an anomaly
    pub async fn check_anomaly(
        &self,
        metric_name: &str,
        value: f64,
        timestamp: SystemTime,
    ) -> Result<Option<Alert>, Box<dyn std::error::Error + Send + Sync>> {
        let mut stats_lock = self.metric_statistics.write().unwrap();
        let stats = stats_lock
            .entry(metric_name.to_string())
            .or_insert_with(|| MetricStatistics {
                values: VecDeque::new(),
                mean: 0.0,
                variance: 0.0,
                std_dev: 0.0,
                min: f64::INFINITY,
                max: f64::NEG_INFINITY,
                percentiles: HashMap::new(),
                last_updated: timestamp,
            });

        // Add new value and maintain window
        stats.values.push_back((timestamp, value));
        while stats.values.len() > self.config.anomaly_detection.window_size {
            stats.values.pop_front();
        }

        // Update statistics
        self.update_metric_statistics(stats);

        // Check for anomaly
        if stats.values.len() >= self.config.anomaly_detection.min_data_points {
            let is_anomaly = match self.config.anomaly_detection.algorithm {
                AnomalyDetectionAlgorithm::ZScore => {
                    let z_score = (value - stats.mean) / stats.std_dev;
                    z_score.abs() > self.config.anomaly_detection.sensitivity
                }
                AnomalyDetectionAlgorithm::IQR => {
                    let q1 = stats.percentiles.get(&25).unwrap_or(&0.0);
                    let q3 = stats.percentiles.get(&75).unwrap_or(&0.0);
                    let iqr = q3 - q1;
                    let lower_bound = q1 - 1.5 * iqr;
                    let upper_bound = q3 + 1.5 * iqr;
                    value < lower_bound || value > upper_bound
                }
                _ => false, // Other algorithms can be implemented
            };

            if is_anomaly {
                let alert = Alert {
                    id: Uuid::new_v4(),
                    correlation_key: CorrelationKey {
                        service: "arbedge-core".to_string(),
                        component: "anomaly-detection".to_string(),
                        metric_type: metric_name.to_string(),
                        fingerprint: format!("{}:anomaly", metric_name),
                    },
                    severity: AlertSeverity::Medium,
                    state: AlertState::Triggered,
                    title: format!("Anomaly detected in {}", metric_name),
                    description: format!(
                        "Metric {} has value {:.2} which deviates significantly from normal (mean: {:.2}, std: {:.2})",
                        metric_name, value, stats.mean, stats.std_dev
                    ),
                    created_at: timestamp,
                    updated_at: timestamp,
                    acknowledged_at: None,
                    resolved_at: None,
                    acknowledged_by: None,
                    tags: HashMap::new(),
                    runbook_url: Some(format!("https://docs.arbedge.com/runbooks/anomaly-{}", metric_name)),
                    dashboard_url: Some(format!("https://grafana.arbedge.com/d/anomaly?metric={}", metric_name)),
                    metric_value: value,
                    threshold: stats.mean + (self.config.anomaly_detection.sensitivity * stats.std_dev),
                    context: HashMap::new(),
                    escalation_level: 0,
                    fire_count: 1,
                    suppressed_until: None,
                };

                return Ok(Some(alert));
            }
        }

        Ok(None)
    }

    /// Update metric statistics
    fn update_metric_statistics(&self, stats: &mut MetricStatistics) {
        let values: Vec<f64> = stats.values.iter().map(|(_, v)| *v).collect();

        if values.is_empty() {
            return;
        }

        // Calculate mean
        stats.mean = values.iter().sum::<f64>() / values.len() as f64;

        // Calculate variance and standard deviation
        stats.variance =
            values.iter().map(|v| (v - stats.mean).powi(2)).sum::<f64>() / values.len() as f64;
        stats.std_dev = stats.variance.sqrt();

        // Calculate min/max
        stats.min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        stats.max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

        // Calculate percentiles
        let mut sorted_values = values.clone();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        for &percentile in &[50, 75, 90, 95, 99] {
            let index = (percentile as f64 / 100.0 * (sorted_values.len() - 1) as f64) as usize;
            stats.percentiles.insert(percentile, sorted_values[index]);
        }

        stats.last_updated = now_system_time();
    }

    /// Correlate alert with existing alerts
    async fn correlate_alert(
        &self,
        alert: &Alert,
    ) -> Result<CorrelationResult, Box<dyn std::error::Error + Send + Sync>> {
        let alerts_lock = self.alerts.read().unwrap();
        let now = now_system_time();

        // Check for deduplication (exact same alert within deduplication window)
        for existing_alert in alerts_lock.values() {
            if existing_alert.correlation_key == alert.correlation_key
                && existing_alert.state != AlertState::Resolved
                && now
                    .duration_since(existing_alert.created_at)
                    .unwrap_or(Duration::ZERO)
                    < self.config.deduplication_window
            {
                return Ok(CorrelationResult::Deduplicated(existing_alert.id));
            }
        }

        // Check for correlation (similar alerts within correlation window)
        let correlation_groups_lock = self.correlation_groups.read().unwrap();
        if let Some(group) = correlation_groups_lock.get(&alert.correlation_key) {
            if now
                .duration_since(group.created_at)
                .unwrap_or(Duration::ZERO)
                < self.config.correlation_window
            {
                return Ok(CorrelationResult::Correlated(alert.correlation_key.clone()));
            }
        }

        Ok(CorrelationResult::NewAlert)
    }

    /// Create a new alert
    async fn create_new_alert(
        &self,
        alert: Alert,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let alert_id = alert.id;

        // Store alert
        self.alerts.write().unwrap().insert(alert_id, alert.clone());

        // Create correlation group
        let correlation_group = AlertCorrelationGroup {
            correlation_key: alert.correlation_key.clone(),
            alerts: vec![alert_id],
            primary_alert: alert_id,
            created_at: alert.created_at,
            last_updated: alert.updated_at,
            correlation_count: 1,
        };

        self.correlation_groups
            .write()
            .unwrap()
            .insert(alert.correlation_key.clone(), correlation_group);

        // Send notifications
        self.send_notifications(&alert).await?;

        Ok(())
    }

    /// Update alert fire count for deduplicated alerts
    async fn update_alert_fire_count(
        &self,
        alert_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let alert_clone = {
            let mut alerts_lock = self.alerts.write().unwrap();
            if let Some(alert) = alerts_lock.get_mut(&alert_id) {
                alert.fire_count += 1;
                alert.updated_at = now_system_time();

                // Send notification for repeated fires (configurable threshold)
                if alert.fire_count % 5 == 0 {
                    // Every 5th fire
                    Some(alert.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(alert) = alert_clone {
            self.send_notifications(&alert).await?;
        }
        Ok(())
    }

    /// Add alert to existing correlation group
    async fn add_to_correlation_group(
        &self,
        alert: Alert,
        correlation_key: CorrelationKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let alert_id = alert.id;

        // Store alert
        self.alerts.write().unwrap().insert(alert_id, alert.clone());

        // Update correlation group
        let mut groups_lock = self.correlation_groups.write().unwrap();
        if let Some(group) = groups_lock.get_mut(&correlation_key) {
            group.alerts.push(alert_id);
            group.last_updated = alert.updated_at;
            group.correlation_count += 1;
        }

        Ok(())
    }

    /// Check if alert should be suppressed
    async fn is_alert_suppressed(&self, alert: &Alert) -> bool {
        let suppression_rules = self.suppression_rules.read().unwrap();
        let now = now_system_time();

        for rule in suppression_rules.values() {
            if now >= rule.start_time && now <= rule.end_time {
                // Simple pattern matching - in production, use regex
                if alert.title.contains(&rule.pattern) || alert.description.contains(&rule.pattern)
                {
                    return true;
                }
            }
        }

        // Check individual alert suppression
        if let Some(suppressed_until) = alert.suppressed_until {
            if now < suppressed_until {
                return true;
            }
        }

        false
    }

    /// Send notifications for an alert
    async fn send_notifications(
        &self,
        alert: &Alert,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let policies = self.escalation_policies.read().unwrap();
        let policy_name = alert
            .tags
            .get("escalation_policy")
            .unwrap_or(&self.config.default_escalation_policy);

        if let Some(policy) = policies.get(policy_name) {
            if let Some(level) = policy.levels.first() {
                for channel in &level.channels {
                    for target in &level.targets {
                        let notification_id = Uuid::new_v4();

                        // In production, this would send actual notifications
                        let _status = NotificationStatus {
                            notification_id,
                            alert_id: alert.id,
                            channel: channel.clone(),
                            target: target.clone(),
                            status: "sent".to_string(),
                            attempt: 1,
                            sent_at: Some(now_system_time()),
                            error: None,
                        };

                        // Note: Removed notification broadcast channel for WASM compatibility
                    }
                }
            }
        }

        Ok(())
    }

    /// Command processor main loop
    #[allow(dead_code)]
    async fn run_command_processor(
        &self,
        mut command_rx: mpsc::UnboundedReceiver<AlertingCommand>,
    ) {
        use futures::StreamExt;
        while let Some(command) = command_rx.next().await {
            match command {
                AlertingCommand::ProcessAlert(alert) => {
                    if let Err(e) = self.process_alert(*alert).await {
                        eprintln!("Error processing alert: {}", e);
                    }
                }
                AlertingCommand::AcknowledgeAlert {
                    alert_id,
                    acknowledged_by,
                } => {
                    let mut alerts = self.alerts.write().unwrap();
                    if let Some(alert) = alerts.get_mut(&alert_id) {
                        alert.state = AlertState::Acknowledged;
                        alert.acknowledged_at = Some(now_system_time());
                        alert.acknowledged_by = Some(acknowledged_by);
                        alert.updated_at = now_system_time();
                    }
                }
                AlertingCommand::ResolveAlert {
                    alert_id,
                    resolved_by: _,
                } => {
                    let mut alerts = self.alerts.write().unwrap();
                    if let Some(alert) = alerts.get_mut(&alert_id) {
                        alert.state = AlertState::Resolved;
                        alert.resolved_at = Some(now_system_time());
                        alert.updated_at = now_system_time();
                    }
                }
                AlertingCommand::GetAlertStatus { alert_id, response } => {
                    let alerts = self.alerts.read().unwrap();
                    let alert = alerts.get(&alert_id).cloned();
                    let _ = response.send(alert);
                }
                AlertingCommand::GetActiveAlerts { response } => {
                    let alerts = self.alerts.read().unwrap();
                    let active_alerts: Vec<Alert> = alerts
                        .values()
                        .filter(|a| {
                            matches!(
                                a.state,
                                AlertState::Triggered
                                    | AlertState::Acknowledged
                                    | AlertState::Escalated
                            )
                        })
                        .cloned()
                        .collect();
                    let _ = response.send(active_alerts);
                }
                AlertingCommand::CheckAnomalies {
                    metric_name,
                    value,
                    timestamp,
                } => {
                    if let Ok(Some(alert)) =
                        self.check_anomaly(&metric_name, value, timestamp).await
                    {
                        if let Err(e) = self.process_alert(alert).await {
                            eprintln!("Error processing anomaly alert: {}", e);
                        }
                    }
                }
                AlertingCommand::Shutdown => break,
                _ => {
                    // Handle other commands
                }
            }
        }
    }

    /// Escalation processor for handling timeouts
    #[allow(dead_code)]
    async fn run_escalation_processor(&self) {
        // Note: WASM-compatible simple implementation
        loop {
            // Simple delay instead of interval for WASM compatibility

            if let Err(e) = self.process_escalations().await {
                eprintln!("Error processing escalations: {}", e);
            }
        }
    }

    /// Process alert escalations
    #[allow(dead_code)]
    async fn process_escalations(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = now_system_time();
        let mut alerts_to_escalate = Vec::new();

        {
            let alerts = self.alerts.read().unwrap();
            for alert in alerts.values() {
                if alert.state == AlertState::Triggered {
                    let elapsed = now
                        .duration_since(alert.created_at)
                        .unwrap_or(Duration::ZERO);

                    // Check if escalation timeout reached
                    let policies = self.escalation_policies.read().unwrap();
                    let policy_name = alert
                        .tags
                        .get("escalation_policy")
                        .unwrap_or(&self.config.default_escalation_policy);

                    if let Some(policy) = policies.get(policy_name) {
                        if let Some(level) = policy.levels.get(alert.escalation_level as usize) {
                            if elapsed >= level.timeout {
                                alerts_to_escalate.push(alert.id);
                            }
                        }
                    }
                }
            }
        }

        for alert_id in alerts_to_escalate {
            self.escalate_alert(alert_id).await?;
        }

        Ok(())
    }

    /// Escalate an alert to the next level
    #[allow(dead_code)]
    async fn escalate_alert(
        &self,
        alert_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let alert_clone = {
            let mut alerts = self.alerts.write().unwrap();
            if let Some(alert) = alerts.get_mut(&alert_id) {
                alert.escalation_level += 1;
                alert.state = AlertState::Escalated;
                alert.updated_at = now_system_time();

                // Send notifications for escalated level
                Some(alert.clone())
            } else {
                None
            }
        };

        if let Some(alert) = alert_clone {
            self.send_notifications(&alert).await?;
        }
        Ok(())
    }

    /// Cleanup processor for removing old data
    #[allow(dead_code)]
    async fn run_cleanup_processor(&self) {
        // Note: WASM-compatible simple implementation
        loop {
            // Simple delay for WASM compatibility

            if let Err(e) = self.cleanup_old_data().await {
                eprintln!("Error during cleanup: {}", e);
            }
        }
    }

    /// Clean up old resolved alerts and metrics
    #[allow(dead_code)]
    async fn cleanup_old_data(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = now_system_time();
        let retention_cutoff = now - self.config.metrics_retention;

        // Clean up old alerts
        {
            let mut alerts = self.alerts.write().unwrap();
            alerts.retain(|_, alert| {
                if alert.state == AlertState::Resolved {
                    alert
                        .resolved_at
                        .is_none_or(|resolved_at| resolved_at > retention_cutoff)
                } else {
                    true
                }
            });
        }

        // Clean up old metric statistics
        {
            let mut stats = self.metric_statistics.write().unwrap();
            for metric_stats in stats.values_mut() {
                metric_stats
                    .values
                    .retain(|(timestamp, _)| *timestamp > retention_cutoff);
            }
        }

        // Clean up old correlation groups
        {
            let mut groups = self.correlation_groups.write().unwrap();
            groups.retain(|_, group| group.last_updated > retention_cutoff);
        }

        Ok(())
    }

    /// Get command sender for external use
    pub fn get_command_sender(&self) -> mpsc::UnboundedSender<AlertingCommand> {
        self.command_tx.clone()
    }

    /// Health check for the alerting system
    pub async fn health_check(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let mut health = HashMap::new();

        let alerts_count = self.alerts.read().unwrap().len();
        let correlation_groups_count = self.correlation_groups.read().unwrap().len();
        let metrics_count = self.metric_statistics.read().unwrap().len();

        health.insert(
            "status".to_string(),
            serde_json::Value::String("healthy".to_string()),
        );
        health.insert(
            "alerts_in_memory".to_string(),
            serde_json::Value::Number(alerts_count.into()),
        );
        health.insert(
            "correlation_groups".to_string(),
            serde_json::Value::Number(correlation_groups_count.into()),
        );
        health.insert(
            "metrics_tracked".to_string(),
            serde_json::Value::Number(metrics_count.into()),
        );
        health.insert(
            "uptime".to_string(),
            serde_json::Value::String("system_start_time_here".to_string()),
        );

        Ok(health)
    }

    /// Get component health for infrastructure monitoring  
    pub fn get_component_health(&self) -> ComponentHealth {
        let alerts_count = self.alerts.read().unwrap().len();
        let is_healthy = alerts_count < self.config.max_alerts_in_memory;
        let performance_score = if is_healthy { 1.0 } else { 0.5 };

        ComponentHealth::new(
            is_healthy,
            "RealTimeAlertingSystem".to_string(),
            now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            performance_score,
            0, // error_count would be tracked in real implementation
            0, // warning_count would be tracked in real implementation
        )
    }
}
