// Unified Alert System - Consolidates alert functionality to eliminate duplication
// Combines features from alert_manager.rs and real_time_alerting_system.rs

use crate::utils::now_system_time;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::UNIX_EPOCH;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Unified alert severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info = 5,      // P5 - Informational
    Low = 4,       // P4 - Low priority
    Medium = 3,    // P3 - Medium priority
    High = 2,      // P2 - High priority
    Critical = 1,  // P1 - Immediate action required
    Emergency = 0, // P0 - Emergency
}

impl AlertSeverity {
    pub fn as_str(&self) -> &str {
        match self {
            AlertSeverity::Info => "info",
            AlertSeverity::Low => "low",
            AlertSeverity::Medium => "medium",
            AlertSeverity::High => "high",
            AlertSeverity::Critical => "critical",
            AlertSeverity::Emergency => "emergency",
        }
    }

    pub fn priority_score(&self) -> u8 {
        self.clone() as u8
    }

    pub fn escalation_timeout_seconds(&self) -> u64 {
        match self {
            AlertSeverity::Emergency => 60, // 1 minute
            AlertSeverity::Critical => 300, // 5 minutes
            AlertSeverity::High => 900,     // 15 minutes
            AlertSeverity::Medium => 1800,  // 30 minutes
            AlertSeverity::Low => 3600,     // 1 hour
            AlertSeverity::Info => 7200,    // 2 hours
        }
    }
}

/// Unified alert status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    Triggered,
    Active,
    Acknowledged,
    Escalated,
    Resolved,
    Suppressed,
    Expired,
}

impl AlertStatus {
    pub fn as_str(&self) -> &str {
        match self {
            AlertStatus::Triggered => "triggered",
            AlertStatus::Active => "active",
            AlertStatus::Acknowledged => "acknowledged",
            AlertStatus::Escalated => "escalated",
            AlertStatus::Resolved => "resolved",
            AlertStatus::Suppressed => "suppressed",
            AlertStatus::Expired => "expired",
        }
    }

    pub fn is_actionable(&self) -> bool {
        matches!(
            self,
            AlertStatus::Triggered | AlertStatus::Active | AlertStatus::Escalated
        )
    }
}

/// Alert condition for rule evaluation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertCondition {
    GreaterThan,
    LessThan,
    Equal,
    NotEqual,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

impl AlertCondition {
    pub fn evaluate(&self, value: f64, threshold: f64) -> bool {
        match self {
            AlertCondition::GreaterThan => value > threshold,
            AlertCondition::LessThan => value < threshold,
            AlertCondition::Equal => (value - threshold).abs() < f64::EPSILON,
            AlertCondition::NotEqual => (value - threshold).abs() >= f64::EPSILON,
            AlertCondition::GreaterThanOrEqual => value >= threshold,
            AlertCondition::LessThanOrEqual => value <= threshold,
        }
    }
}

/// Correlation key for grouping related alerts
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationKey {
    pub service: String,
    pub component: String,
    pub metric_type: String,
    pub fingerprint: String,
}

impl CorrelationKey {
    pub fn new(service: String, component: String, metric_type: String) -> Self {
        let fingerprint = format!("{}:{}:{}", service, component, metric_type);
        Self {
            service,
            component,
            metric_type,
            fingerprint,
        }
    }
}

/// Unified alert structure combining features from both alert systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedAlert {
    // Core identification
    pub id: Uuid,
    pub rule_id: String,
    pub rule_name: String,
    pub correlation_key: CorrelationKey,

    // Alert content
    pub title: String,
    pub description: String,
    pub message: String,

    // Classification
    pub severity: AlertSeverity,
    pub status: AlertStatus,
    pub component: String,
    pub metric_name: String,

    // Values and conditions
    pub current_value: f64,
    pub threshold: f64,
    pub condition: AlertCondition,

    // Timing
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub started_at: u64, // Unix timestamp in milliseconds
    pub acknowledged_at: Option<SystemTime>,
    pub resolved_at: Option<SystemTime>,
    pub escalated_at: Option<SystemTime>,
    pub last_notification_at: Option<u64>,
    pub suppressed_until: Option<SystemTime>,

    // Metadata and context
    pub tags: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub context: HashMap<String, serde_json::Value>,

    // Escalation and notifications
    pub escalation_level: u8,
    pub notification_count: u32,
    pub fire_count: u32,
    pub acknowledged_by: Option<String>,

    // URLs and references
    pub runbook_url: Option<String>,
    pub dashboard_url: Option<String>,
}

impl UnifiedAlert {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rule_id: String,
        rule_name: String,
        correlation_key: CorrelationKey,
        title: String,
        description: String,
        severity: AlertSeverity,
        component: String,
        metric_name: String,
        current_value: f64,
        threshold: f64,
        condition: AlertCondition,
    ) -> Self {
        let now = now_system_time();
        let now_millis = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            id: Uuid::new_v4(),
            rule_id,
            rule_name,
            correlation_key,
            title,
            description: description.clone(),
            message: description,
            severity,
            status: AlertStatus::Triggered,
            component,
            metric_name,
            current_value,
            threshold,
            condition,
            created_at: now,
            updated_at: now,
            started_at: now_millis,
            acknowledged_at: None,
            resolved_at: None,
            escalated_at: None,
            last_notification_at: None,
            suppressed_until: None,
            tags: HashMap::new(),
            annotations: HashMap::new(),
            context: HashMap::new(),
            escalation_level: 0,
            notification_count: 0,
            fire_count: 1,
            acknowledged_by: None,
            runbook_url: None,
            dashboard_url: None,
        }
    }

    pub fn acknowledge(&mut self, acknowledged_by: String) {
        self.status = AlertStatus::Acknowledged;
        self.acknowledged_at = Some(now_system_time());
        self.acknowledged_by = Some(acknowledged_by);
        self.updated_at = now_system_time();
    }

    pub fn resolve(&mut self) {
        self.status = AlertStatus::Resolved;
        self.resolved_at = Some(now_system_time());
        self.updated_at = now_system_time();
    }

    pub fn escalate(&mut self) {
        self.status = AlertStatus::Escalated;
        self.escalated_at = Some(now_system_time());
        self.escalation_level += 1;
        self.updated_at = now_system_time();
    }

    pub fn suppress(&mut self, duration: Duration) {
        self.status = AlertStatus::Suppressed;
        self.suppressed_until = Some(now_system_time() + duration);
        self.updated_at = now_system_time();
    }

    pub fn add_tag(&mut self, key: String, value: String) {
        self.tags.insert(key, value);
        self.updated_at = now_system_time();
    }

    pub fn add_annotation(&mut self, key: String, value: String) {
        self.annotations.insert(key, value);
        self.updated_at = now_system_time();
    }

    pub fn add_context(&mut self, key: String, value: serde_json::Value) {
        self.context.insert(key, value);
        self.updated_at = now_system_time();
    }

    pub fn increment_fire_count(&mut self) {
        self.fire_count += 1;
        self.updated_at = now_system_time();
    }

    pub fn increment_notification_count(&mut self) {
        self.notification_count += 1;
        self.last_notification_at = Some(
            now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        );
        self.updated_at = now_system_time();
    }

    pub fn duration_seconds(&self) -> u64 {
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        (now - self.started_at) / 1000
    }

    pub fn should_escalate(&self, escalation_timeout: u64) -> bool {
        if !self.status.is_actionable() {
            return false;
        }

        let time_since_start = self.duration_seconds();
        time_since_start >= escalation_timeout
    }

    pub fn is_suppressed(&self) -> bool {
        if let Some(suppressed_until) = self.suppressed_until {
            now_system_time() < suppressed_until
        } else {
            self.status == AlertStatus::Suppressed
        }
    }

    pub fn is_expired(&self, max_age_seconds: u64) -> bool {
        self.duration_seconds() > max_age_seconds
    }

    pub fn with_runbook_url(mut self, url: String) -> Self {
        self.runbook_url = Some(url);
        self
    }

    pub fn with_dashboard_url(mut self, url: String) -> Self {
        self.dashboard_url = Some(url);
        self
    }
}

/// Alert rule for automated alert generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub component: String,
    pub metric_name: String,
    pub condition: AlertCondition,
    pub severity: AlertSeverity,
    pub threshold: f64,
    pub duration_seconds: u64,
    pub evaluation_interval_seconds: u64,
    pub enabled: bool,
    pub tags: HashMap<String, String>,
    pub notification_channels: Vec<String>,
    pub escalation_policy: Option<String>,
    pub suppression_rules: Vec<String>,
    pub runbook_url: Option<String>,
    pub dashboard_url: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl AlertRule {
    pub fn new(
        name: String,
        component: String,
        metric_name: String,
        condition: AlertCondition,
        severity: AlertSeverity,
        threshold: f64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            description: String::new(),
            component,
            metric_name,
            condition,
            severity,
            threshold,
            duration_seconds: 60,            // Default 1 minute
            evaluation_interval_seconds: 60, // Default 1 minute
            enabled: true,
            tags: HashMap::new(),
            notification_channels: Vec::new(),
            escalation_policy: None,
            suppression_rules: Vec::new(),
            runbook_url: None,
            dashboard_url: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = description;
        self
    }

    pub fn with_duration(mut self, duration_seconds: u64) -> Self {
        self.duration_seconds = duration_seconds;
        self
    }

    pub fn with_notification_channel(mut self, channel: String) -> Self {
        self.notification_channels.push(channel);
        self
    }

    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }

    pub fn evaluate(&self, metric_value: f64) -> bool {
        if !self.enabled {
            return false;
        }
        self.condition.evaluate(metric_value, self.threshold)
    }

    pub fn create_alert(&self, current_value: f64, message: String) -> UnifiedAlert {
        let correlation_key = CorrelationKey::new(
            self.component.clone(),
            self.component.clone(),
            self.metric_name.clone(),
        );

        let mut alert = UnifiedAlert::new(
            self.id.clone(),
            self.name.clone(),
            correlation_key,
            self.name.clone(),
            message.clone(),
            self.severity.clone(),
            self.component.clone(),
            self.metric_name.clone(),
            current_value,
            self.threshold,
            self.condition.clone(),
        );

        // Copy rule metadata to alert
        alert.tags = self.tags.clone();
        if let Some(runbook_url) = &self.runbook_url {
            alert.runbook_url = Some(runbook_url.clone());
        }
        if let Some(dashboard_url) = &self.dashboard_url {
            alert.dashboard_url = Some(dashboard_url.clone());
        }

        alert
    }
}

/// Notification channel configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    Email,
    Slack,
    Webhook,
    SMS,
    PagerDuty,
    Teams,
    Telegram,
}

impl NotificationChannel {
    pub fn as_str(&self) -> &str {
        match self {
            NotificationChannel::Email => "email",
            NotificationChannel::Slack => "slack",
            NotificationChannel::Webhook => "webhook",
            NotificationChannel::SMS => "sms",
            NotificationChannel::PagerDuty => "pagerduty",
            NotificationChannel::Teams => "teams",
            NotificationChannel::Telegram => "telegram",
        }
    }
}

/// Escalation policy for alert escalation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationPolicy {
    pub id: String,
    pub name: String,
    pub levels: Vec<EscalationLevel>,
    pub repeat_interval: Duration,
    pub max_escalations: u8,
    pub enabled: bool,
}

/// Individual escalation level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationLevel {
    pub level: u8,
    pub timeout: Duration,
    pub channels: Vec<NotificationChannel>,
    pub targets: Vec<String>, // email addresses, user IDs, etc.
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
    pub enabled: bool,
}

impl SuppressionRule {
    pub fn is_active(&self) -> bool {
        if !self.enabled {
            return false;
        }
        let now = now_system_time();
        now >= self.start_time && now <= self.end_time
    }

    pub fn matches_alert(&self, alert: &UnifiedAlert) -> bool {
        if !self.is_active() {
            return false;
        }

        // Simple pattern matching - in production, use regex
        alert.component.contains(&self.pattern)
            || alert.metric_name.contains(&self.pattern)
            || alert.title.contains(&self.pattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_severity_ordering() {
        assert!(AlertSeverity::Emergency < AlertSeverity::Critical);
        assert!(AlertSeverity::Critical < AlertSeverity::High);
        assert!(AlertSeverity::High < AlertSeverity::Medium);
        assert!(AlertSeverity::Medium < AlertSeverity::Low);
        assert!(AlertSeverity::Low < AlertSeverity::Info);
    }

    #[test]
    fn test_alert_condition_evaluation() {
        let condition = AlertCondition::GreaterThan;
        assert!(condition.evaluate(10.0, 5.0));
        assert!(!condition.evaluate(3.0, 5.0));
    }

    #[test]
    fn test_unified_alert_creation() {
        let correlation_key = CorrelationKey::new(
            "test-service".to_string(),
            "test-component".to_string(),
            "cpu_usage".to_string(),
        );

        let alert = UnifiedAlert::new(
            "rule-1".to_string(),
            "High CPU Usage".to_string(),
            correlation_key,
            "CPU Usage Alert".to_string(),
            "CPU usage is above threshold".to_string(),
            AlertSeverity::High,
            "test-component".to_string(),
            "cpu_usage".to_string(),
            85.0,
            80.0,
            AlertCondition::GreaterThan,
        );

        assert_eq!(alert.rule_id, "rule-1");
        assert_eq!(alert.severity, AlertSeverity::High);
        assert_eq!(alert.status, AlertStatus::Triggered);
        assert_eq!(alert.current_value, 85.0);
        assert_eq!(alert.threshold, 80.0);
    }

    #[test]
    fn test_alert_rule_evaluation() {
        let rule = AlertRule::new(
            "CPU High".to_string(),
            "server".to_string(),
            "cpu_usage".to_string(),
            AlertCondition::GreaterThan,
            AlertSeverity::High,
            80.0,
        );

        assert!(rule.evaluate(85.0));
        assert!(!rule.evaluate(75.0));
    }

    #[test]
    fn test_suppression_rule() {
        let start_time = now_system_time();
        let end_time = start_time + Duration::from_secs(3600); // 1 hour

        let suppression = SuppressionRule {
            id: "maint-1".to_string(),
            name: "Maintenance Window".to_string(),
            pattern: "server".to_string(),
            start_time,
            end_time,
            reason: "Scheduled maintenance".to_string(),
            created_by: "admin".to_string(),
            enabled: true,
        };

        assert!(suppression.is_active());

        let correlation_key = CorrelationKey::new(
            "test-service".to_string(),
            "server".to_string(),
            "cpu_usage".to_string(),
        );

        let alert = UnifiedAlert::new(
            "rule-1".to_string(),
            "Server Alert".to_string(),
            correlation_key,
            "Server Issue".to_string(),
            "Server has an issue".to_string(),
            AlertSeverity::High,
            "server".to_string(),
            "cpu_usage".to_string(),
            85.0,
            80.0,
            AlertCondition::GreaterThan,
        );

        assert!(suppression.matches_alert(&alert));
    }
}
