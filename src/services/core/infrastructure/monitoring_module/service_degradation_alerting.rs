//! Service Degradation Alerting System
//!
//! Intelligent alerting system for detecting and notifying about service degradation
//! with configurable thresholds, escalation paths, and pattern recognition.
//! Integrates with existing AlertManager and RealTimeHealthMonitor.

use crate::services::core::infrastructure::monitoring_module::{
    alert_manager::{AlertCondition, AlertManager, AlertRule, AlertSeverity},
    real_time_health_monitor::{RealTimeHealthMonitor, StorageHealthMetrics, StorageSystemType},
};
use crate::utils::time::now_instant;
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Service degradation alerting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDegradationConfig {
    /// Enable service degradation alerting
    pub enabled: bool,
    /// Pattern detection window in seconds
    pub pattern_detection_window_seconds: u64,
    /// Minimum samples required for pattern detection
    pub min_samples_for_pattern: usize,
    /// Degradation threshold percentage (0.0 to 1.0)
    pub degradation_threshold: f32,
    /// Enable trend-based warnings
    pub enable_trend_warnings: bool,
    /// Trend analysis window in seconds
    pub trend_analysis_window_seconds: u64,
    /// Enable escalation policies
    pub enable_escalation: bool,
    /// Auto-escalation timeout in seconds
    pub auto_escalation_timeout_seconds: u64,
    /// Enable alert deduplication
    pub enable_deduplication: bool,
    /// Deduplication window in seconds
    pub deduplication_window_seconds: u64,
    /// Enable predictive alerting
    pub enable_predictive_alerting: bool,
    /// Notification channels for degradation alerts
    pub notification_channels: Vec<String>,
    /// Feature flags for different alert types
    pub feature_flags: DegradationFeatureFlags,
}

/// Feature flags for degradation alerting capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationFeatureFlags {
    pub enable_latency_degradation: bool,
    pub enable_error_rate_degradation: bool,
    pub enable_throughput_degradation: bool,
    pub enable_availability_degradation: bool,
    pub enable_cascading_failure_detection: bool,
    pub enable_resource_exhaustion_alerts: bool,
    pub enable_dependency_failure_alerts: bool,
    pub enable_recovery_notifications: bool,
}

impl Default for ServiceDegradationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pattern_detection_window_seconds: 300, // 5 minutes
            min_samples_for_pattern: 5,
            degradation_threshold: 0.2, // 20% degradation
            enable_trend_warnings: true,
            trend_analysis_window_seconds: 900, // 15 minutes
            enable_escalation: true,
            auto_escalation_timeout_seconds: 600, // 10 minutes
            enable_deduplication: true,
            deduplication_window_seconds: 300, // 5 minutes
            enable_predictive_alerting: true,
            notification_channels: vec!["email".to_string(), "slack".to_string()],
            feature_flags: DegradationFeatureFlags {
                enable_latency_degradation: true,
                enable_error_rate_degradation: true,
                enable_throughput_degradation: true,
                enable_availability_degradation: true,
                enable_cascading_failure_detection: true,
                enable_resource_exhaustion_alerts: true,
                enable_dependency_failure_alerts: true,
                enable_recovery_notifications: true,
            },
        }
    }
}

/// Degradation pattern types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DegradationPattern {
    /// Gradual performance decline
    GradualDegradation,
    /// Sudden performance drop
    SuddenDegradation,
    /// Intermittent issues
    IntermittentDegradation,
    /// Cascading failure across services
    CascadingFailure,
    /// Resource exhaustion pattern
    ResourceExhaustion,
    /// Dependency failure impact
    DependencyFailure,
    /// Recovery pattern
    ServiceRecovery,
}

impl DegradationPattern {
    pub fn as_str(&self) -> &str {
        match self {
            DegradationPattern::GradualDegradation => "gradual_degradation",
            DegradationPattern::SuddenDegradation => "sudden_degradation",
            DegradationPattern::IntermittentDegradation => "intermittent_degradation",
            DegradationPattern::CascadingFailure => "cascading_failure",
            DegradationPattern::ResourceExhaustion => "resource_exhaustion",
            DegradationPattern::DependencyFailure => "dependency_failure",
            DegradationPattern::ServiceRecovery => "service_recovery",
        }
    }

    pub fn severity(&self) -> AlertSeverity {
        match self {
            DegradationPattern::GradualDegradation => AlertSeverity::Warning,
            DegradationPattern::SuddenDegradation => AlertSeverity::Error,
            DegradationPattern::IntermittentDegradation => AlertSeverity::Warning,
            DegradationPattern::CascadingFailure => AlertSeverity::Critical,
            DegradationPattern::ResourceExhaustion => AlertSeverity::Critical,
            DegradationPattern::DependencyFailure => AlertSeverity::Error,
            DegradationPattern::ServiceRecovery => AlertSeverity::Info,
        }
    }
}

/// Degradation alert with context and recommended actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationAlert {
    pub alert_id: String,
    pub service_type: StorageSystemType,
    pub pattern: DegradationPattern,
    pub severity: AlertSeverity,
    pub title: String,
    pub description: String,
    pub current_metrics: DegradationMetrics,
    pub baseline_metrics: Option<DegradationMetrics>,
    pub degradation_percentage: f32,
    pub impact_assessment: ImpactAssessment,
    pub recommended_actions: Vec<String>,
    pub escalation_path: Vec<String>,
    pub notification_channels: Vec<String>,
    pub created_at: u64,
    pub last_updated: u64,
    pub status: DegradationAlertStatus,
    pub correlation_id: Option<String>,
    pub tags: HashMap<String, String>,
}

/// Metrics snapshot for degradation analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationMetrics {
    pub timestamp: u64,
    pub health_score: f32,
    pub availability_percentage: f32,
    pub average_latency_ms: f64,
    pub error_rate_percentage: f32,
    pub throughput_ops_per_second: f64,
    pub consecutive_failures: u32,
}

impl From<&StorageHealthMetrics> for DegradationMetrics {
    fn from(metrics: &StorageHealthMetrics) -> Self {
        Self {
            timestamp: metrics.last_updated,
            health_score: metrics.health_score,
            availability_percentage: metrics.availability_percentage,
            average_latency_ms: metrics.average_latency_ms,
            error_rate_percentage: metrics.error_rate_percentage,
            throughput_ops_per_second: metrics.throughput_ops_per_second,
            consecutive_failures: metrics.consecutive_failures,
        }
    }
}

/// Impact assessment for degradation alerts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAssessment {
    pub affected_services: Vec<String>,
    pub estimated_user_impact: UserImpactLevel,
    pub business_impact: BusinessImpactLevel,
    pub recovery_time_estimate: Option<u64>, // seconds
    pub affected_operations: Vec<String>,
    pub cascading_risk: CascadingRiskLevel,
}

/// User impact levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserImpactLevel {
    None,
    Low,
    Medium,
    High,
    Critical,
}

/// Business impact levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BusinessImpactLevel {
    None,
    Low,
    Medium,
    High,
    Critical,
}

/// Cascading risk levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CascadingRiskLevel {
    None,
    Low,
    Medium,
    High,
    Critical,
}

/// Degradation alert status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DegradationAlertStatus {
    Active,
    Investigating,
    Mitigating,
    Resolved,
    Suppressed,
    Escalated,
}

/// Historical pattern data for trend analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternHistory {
    pub service_type: StorageSystemType,
    pub metric_snapshots: VecDeque<DegradationMetrics>,
    pub detected_patterns: Vec<(DegradationPattern, u64)>, // pattern, timestamp
    pub baseline_metrics: DegradationMetrics,
    pub last_updated: u64,
}

impl PatternHistory {
    pub fn new(service_type: StorageSystemType, baseline: DegradationMetrics) -> Self {
        Self {
            service_type,
            metric_snapshots: VecDeque::with_capacity(100), // Keep last 100 snapshots
            detected_patterns: Vec::new(),
            baseline_metrics: baseline,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn add_snapshot(&mut self, metrics: DegradationMetrics) {
        self.metric_snapshots.push_back(metrics);
        if self.metric_snapshots.len() > 100 {
            self.metric_snapshots.pop_front();
        }
        self.last_updated = chrono::Utc::now().timestamp_millis() as u64;
    }

    pub fn add_pattern(&mut self, pattern: DegradationPattern) {
        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        self.detected_patterns.push((pattern, timestamp));

        // Keep only recent patterns (last 50)
        if self.detected_patterns.len() > 50 {
            self.detected_patterns.remove(0);
        }
    }

    pub fn calculate_trend(&self, window_seconds: u64) -> Option<f32> {
        if self.metric_snapshots.len() < 2 {
            return None;
        }

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let cutoff_time = now - (window_seconds * 1000);

        let recent_snapshots: Vec<&DegradationMetrics> = self
            .metric_snapshots
            .iter()
            .filter(|snapshot| snapshot.timestamp >= cutoff_time)
            .collect();

        if recent_snapshots.len() < 2 {
            return None;
        }

        // Calculate trend in health score
        let first_score = recent_snapshots.first()?.health_score;
        let last_score = recent_snapshots.last()?.health_score;

        Some((last_score - first_score) / first_score)
    }
}

/// Service degradation alerting engine
pub struct ServiceDegradationAlerting {
    config: ServiceDegradationConfig,
    logger: crate::utils::logger::Logger,
    #[allow(dead_code)]
    kv_store: KvStore,

    // Integration with existing services
    alert_manager: Option<Arc<AlertManager>>,
    real_time_monitor: Option<Arc<RealTimeHealthMonitor>>,

    // Pattern detection and history
    pattern_history: Arc<Mutex<HashMap<StorageSystemType, PatternHistory>>>,
    active_alerts: Arc<Mutex<HashMap<String, DegradationAlert>>>,
    alert_deduplication: Arc<Mutex<HashMap<String, u64>>>, // alert_key -> last_timestamp

    // Performance tracking
    startup_time: u64,
    last_analysis: Arc<Mutex<u64>>,
}

impl ServiceDegradationAlerting {
    /// Create new service degradation alerting system
    pub async fn new(
        config: ServiceDegradationConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        let system = Self {
            config,
            logger,
            kv_store,
            alert_manager: None,
            real_time_monitor: None,
            pattern_history: Arc::new(Mutex::new(HashMap::new())),
            active_alerts: Arc::new(Mutex::new(HashMap::new())),
            alert_deduplication: Arc::new(Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
            last_analysis: Arc::new(Mutex::new(0)),
        };

        system
            .logger
            .info("Service Degradation Alerting System initialized");
        Ok(system)
    }

    /// Set alert manager integration
    pub fn set_alert_manager(&mut self, alert_manager: Arc<AlertManager>) {
        self.alert_manager = Some(alert_manager);
        self.logger.info("Alert manager integration enabled");
    }

    /// Set real-time health monitor integration
    pub fn set_real_time_monitor(&mut self, monitor: Arc<RealTimeHealthMonitor>) {
        self.real_time_monitor = Some(monitor);
        self.logger
            .info("Real-time health monitor integration enabled");
    }

    /// Analyze service health and detect degradation patterns
    pub async fn analyze_service_health(&self) -> ArbitrageResult<Vec<DegradationAlert>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let start_time = now_instant();
        let mut generated_alerts = Vec::new();

        // Update analysis timestamp
        {
            let mut last_analysis = self.last_analysis.lock().map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to acquire analysis lock: {}", e))
            })?;
            *last_analysis = chrono::Utc::now().timestamp_millis() as u64;
        }

        // Get real-time health data from monitor
        if let Some(monitor) = &self.real_time_monitor {
            let dashboard_data = monitor.get_health_dashboard().await?;

            for (system_type, health_metrics) in &dashboard_data.system_health_metrics {
                // Convert health metrics to degradation metrics
                let current_metrics = DegradationMetrics::from(health_metrics);

                // Update pattern history
                self.update_pattern_history(system_type.clone(), current_metrics.clone())
                    .await?;

                // Detect degradation patterns
                let detected_patterns = self
                    .detect_degradation_patterns(system_type, &current_metrics)
                    .await?;

                // Generate alerts for detected patterns
                for pattern in detected_patterns {
                    if let Some(alert) = self
                        .create_degradation_alert(system_type.clone(), pattern, &current_metrics)
                        .await?
                    {
                        generated_alerts.push(alert);
                    }
                }
            }
        }

        // Process and deduplicate alerts
        let processed_alerts = self
            .process_and_deduplicate_alerts(generated_alerts)
            .await?;

        // Send notifications for new alerts
        for alert in &processed_alerts {
            self.send_degradation_notification(alert).await?;
        }

        let duration = start_time.elapsed();
        self.logger.debug(&format!(
            "Service degradation analysis completed in {}ms, generated {} alerts",
            duration.as_millis(),
            processed_alerts.len()
        ));

        Ok(processed_alerts)
    }

    /// Update pattern history for a service
    async fn update_pattern_history(
        &self,
        service_type: StorageSystemType,
        metrics: DegradationMetrics,
    ) -> ArbitrageResult<()> {
        let mut pattern_history = self.pattern_history.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire pattern history lock: {}", e))
        })?;

        let history = pattern_history
            .entry(service_type.clone())
            .or_insert_with(|| PatternHistory::new(service_type, metrics.clone()));

        history.add_snapshot(metrics);
        Ok(())
    }

    /// Detect degradation patterns for a service
    async fn detect_degradation_patterns(
        &self,
        service_type: &StorageSystemType,
        current_metrics: &DegradationMetrics,
    ) -> ArbitrageResult<Vec<DegradationPattern>> {
        let mut detected_patterns = Vec::new();

        let pattern_history = self.pattern_history.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire pattern history lock: {}", e))
        })?;

        if let Some(history) = pattern_history.get(service_type) {
            // Check for various degradation patterns
            if self.config.feature_flags.enable_latency_degradation {
                if let Some(pattern) = self.detect_latency_degradation(history, current_metrics) {
                    detected_patterns.push(pattern);
                }
            }

            if self.config.feature_flags.enable_error_rate_degradation {
                if let Some(pattern) = self.detect_error_rate_degradation(history, current_metrics)
                {
                    detected_patterns.push(pattern);
                }
            }

            if self.config.feature_flags.enable_throughput_degradation {
                if let Some(pattern) = self.detect_throughput_degradation(history, current_metrics)
                {
                    detected_patterns.push(pattern);
                }
            }

            if self.config.feature_flags.enable_availability_degradation {
                if let Some(pattern) =
                    self.detect_availability_degradation(history, current_metrics)
                {
                    detected_patterns.push(pattern);
                }
            }

            if self.config.feature_flags.enable_cascading_failure_detection {
                if let Some(pattern) = self.detect_cascading_failure(history, current_metrics) {
                    detected_patterns.push(pattern);
                }
            }

            // Check for recovery patterns
            if self.config.feature_flags.enable_recovery_notifications {
                if let Some(pattern) = self.detect_service_recovery(history, current_metrics) {
                    detected_patterns.push(pattern);
                }
            }
        }

        Ok(detected_patterns)
    }

    /// Detect latency degradation patterns
    fn detect_latency_degradation(
        &self,
        history: &PatternHistory,
        current_metrics: &DegradationMetrics,
    ) -> Option<DegradationPattern> {
        if history.metric_snapshots.len() < self.config.min_samples_for_pattern {
            return None;
        }

        let baseline_latency = history.baseline_metrics.average_latency_ms;
        let current_latency = current_metrics.average_latency_ms;
        let latency_increase = (current_latency - baseline_latency) / baseline_latency;

        if latency_increase > self.config.degradation_threshold as f64 {
            // Check if it's sudden or gradual
            if let Some(recent_snapshots) = history
                .metric_snapshots
                .iter()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .get(0..5)
            {
                let mut sudden_change = false;
                for window in recent_snapshots.windows(2) {
                    let increase = (window[1].average_latency_ms - window[0].average_latency_ms)
                        / window[0].average_latency_ms;
                    if increase > 0.5 {
                        // 50% sudden increase
                        sudden_change = true;
                        break;
                    }
                }

                if sudden_change {
                    Some(DegradationPattern::SuddenDegradation)
                } else {
                    Some(DegradationPattern::GradualDegradation)
                }
            } else {
                Some(DegradationPattern::GradualDegradation)
            }
        } else {
            None
        }
    }

    /// Detect error rate degradation patterns
    fn detect_error_rate_degradation(
        &self,
        history: &PatternHistory,
        current_metrics: &DegradationMetrics,
    ) -> Option<DegradationPattern> {
        if history.metric_snapshots.len() < self.config.min_samples_for_pattern {
            return None;
        }

        let baseline_error_rate = history.baseline_metrics.error_rate_percentage;
        let current_error_rate = current_metrics.error_rate_percentage;

        // Check for significant error rate increase
        if current_error_rate > baseline_error_rate + 5.0 && current_error_rate > 10.0 {
            // Check pattern of recent errors
            let recent_errors: Vec<f32> = history
                .metric_snapshots
                .iter()
                .rev()
                .take(5)
                .map(|m| m.error_rate_percentage)
                .collect();

            if recent_errors.len() >= 3 {
                let high_error_count = recent_errors
                    .iter()
                    .filter(|&&rate| rate > baseline_error_rate + 5.0)
                    .count();
                if high_error_count == recent_errors.len() {
                    Some(DegradationPattern::SuddenDegradation)
                } else if high_error_count >= recent_errors.len() / 2 {
                    Some(DegradationPattern::IntermittentDegradation)
                } else {
                    None
                }
            } else {
                Some(DegradationPattern::SuddenDegradation)
            }
        } else {
            None
        }
    }

    /// Detect throughput degradation patterns
    fn detect_throughput_degradation(
        &self,
        history: &PatternHistory,
        current_metrics: &DegradationMetrics,
    ) -> Option<DegradationPattern> {
        if history.metric_snapshots.len() < self.config.min_samples_for_pattern {
            return None;
        }

        let baseline_throughput = history.baseline_metrics.throughput_ops_per_second;
        let current_throughput = current_metrics.throughput_ops_per_second;

        if baseline_throughput > 0.0 {
            let throughput_decrease =
                (baseline_throughput - current_throughput) / baseline_throughput;

            if throughput_decrease > self.config.degradation_threshold as f64 {
                Some(DegradationPattern::GradualDegradation)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Detect availability degradation patterns
    fn detect_availability_degradation(
        &self,
        history: &PatternHistory,
        current_metrics: &DegradationMetrics,
    ) -> Option<DegradationPattern> {
        let baseline_availability = history.baseline_metrics.availability_percentage;
        let current_availability = current_metrics.availability_percentage;
        let availability_decrease = baseline_availability - current_availability;

        if availability_decrease > self.config.degradation_threshold * 100.0 {
            if current_metrics.consecutive_failures > 0 {
                Some(DegradationPattern::SuddenDegradation)
            } else {
                Some(DegradationPattern::GradualDegradation)
            }
        } else {
            None
        }
    }

    /// Detect cascading failure patterns
    fn detect_cascading_failure(
        &self,
        _history: &PatternHistory,
        current_metrics: &DegradationMetrics,
    ) -> Option<DegradationPattern> {
        // Simple heuristic: multiple severe degradations indicate cascading failure
        let severe_conditions = [
            current_metrics.error_rate_percentage > 20.0,
            current_metrics.availability_percentage < 80.0,
            current_metrics.consecutive_failures > 3,
            current_metrics.health_score < 0.3,
        ]
        .iter()
        .filter(|&&condition| condition)
        .count();

        if severe_conditions >= 3 {
            Some(DegradationPattern::CascadingFailure)
        } else {
            None
        }
    }

    /// Detect service recovery patterns
    fn detect_service_recovery(
        &self,
        history: &PatternHistory,
        current_metrics: &DegradationMetrics,
    ) -> Option<DegradationPattern> {
        if history.metric_snapshots.len() < 3 {
            return None;
        }

        // Check if service is recovering from a degraded state
        let recent_health_scores: Vec<f32> = history
            .metric_snapshots
            .iter()
            .rev()
            .take(5)
            .map(|m| m.health_score)
            .collect();

        if recent_health_scores.len() >= 3 {
            let improving_trend = recent_health_scores
                .windows(2)
                .all(|window| window[0] <= window[1]); // Health improving

            let was_degraded = recent_health_scores.iter().any(|&score| score < 0.7);
            let now_healthy = current_metrics.health_score > 0.8;

            if improving_trend && was_degraded && now_healthy {
                Some(DegradationPattern::ServiceRecovery)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Create degradation alert from detected pattern
    async fn create_degradation_alert(
        &self,
        service_type: StorageSystemType,
        pattern: DegradationPattern,
        current_metrics: &DegradationMetrics,
    ) -> ArbitrageResult<Option<DegradationAlert>> {
        let pattern_history = self.pattern_history.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire pattern history lock: {}", e))
        })?;

        let baseline_metrics = pattern_history
            .get(&service_type)
            .map(|h| h.baseline_metrics.clone());

        // Calculate degradation percentage
        let degradation_percentage = if let Some(baseline) = &baseline_metrics {
            let health_degradation =
                (baseline.health_score - current_metrics.health_score) / baseline.health_score;
            health_degradation.max(0.0) * 100.0
        } else {
            0.0
        };

        // Generate alert content
        let (title, description) =
            self.generate_alert_content(&service_type, &pattern, degradation_percentage);

        // Assess impact
        let impact_assessment = self.assess_impact(&service_type, &pattern, current_metrics);

        // Generate recommended actions
        let recommended_actions = self.generate_recommended_actions(&pattern, &impact_assessment);

        // Create alert
        let severity = pattern.severity();
        let alert = DegradationAlert {
            alert_id: uuid::Uuid::new_v4().to_string(),
            service_type,
            pattern,
            severity,
            title,
            description,
            current_metrics: current_metrics.clone(),
            baseline_metrics,
            degradation_percentage,
            impact_assessment,
            recommended_actions,
            escalation_path: self.config.notification_channels.clone(),
            notification_channels: self.config.notification_channels.clone(),
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
            status: DegradationAlertStatus::Active,
            correlation_id: None,
            tags: HashMap::new(),
        };

        Ok(Some(alert))
    }

    /// Generate alert title and description
    fn generate_alert_content(
        &self,
        service_type: &StorageSystemType,
        pattern: &DegradationPattern,
        degradation_percentage: f32,
    ) -> (String, String) {
        let service_name = service_type.as_str();

        let title = match pattern {
            DegradationPattern::GradualDegradation => {
                format!("Gradual Performance Degradation - {}", service_name)
            }
            DegradationPattern::SuddenDegradation => {
                format!("Sudden Performance Drop - {}", service_name)
            }
            DegradationPattern::IntermittentDegradation => {
                format!("Intermittent Issues Detected - {}", service_name)
            }
            DegradationPattern::CascadingFailure => {
                format!("Cascading Failure Alert - {}", service_name)
            }
            DegradationPattern::ResourceExhaustion => {
                format!("Resource Exhaustion Alert - {}", service_name)
            }
            DegradationPattern::DependencyFailure => {
                format!("Dependency Failure Impact - {}", service_name)
            }
            DegradationPattern::ServiceRecovery => {
                format!("Service Recovery Detected - {}", service_name)
            }
        };

        let description = match pattern {
            DegradationPattern::GradualDegradation => {
                format!(
                    "The {} service is experiencing gradual performance degradation. \
                    Health score has decreased by {:.1}% over the monitoring window. \
                    This pattern suggests underlying resource constraints or increasing load.",
                    service_name, degradation_percentage
                )
            }
            DegradationPattern::SuddenDegradation => {
                format!(
                    "The {} service has experienced a sudden performance drop. \
                    Health score decreased by {:.1}% in a short time period. \
                    Immediate investigation required to identify the root cause.",
                    service_name, degradation_percentage
                )
            }
            DegradationPattern::CascadingFailure => {
                format!(
                    "Critical: Cascading failure detected in {} service. \
                    Multiple severe conditions present including high error rates, \
                    low availability, and consecutive failures. Immediate action required.",
                    service_name
                )
            }
            DegradationPattern::ServiceRecovery => {
                format!(
                    "Good news: The {} service is recovering from previous degradation. \
                    Health score is improving and availability is being restored. \
                    Continue monitoring to ensure full recovery.",
                    service_name
                )
            }
            _ => {
                format!(
                    "Service degradation pattern '{}' detected in {} service. \
                    Health score affected by {:.1}%. Investigation recommended.",
                    pattern.as_str(),
                    service_name,
                    degradation_percentage
                )
            }
        };

        (title, description)
    }

    /// Assess impact of degradation
    fn assess_impact(
        &self,
        service_type: &StorageSystemType,
        pattern: &DegradationPattern,
        _current_metrics: &DegradationMetrics,
    ) -> ImpactAssessment {
        let user_impact = match pattern {
            DegradationPattern::CascadingFailure => UserImpactLevel::Critical,
            DegradationPattern::SuddenDegradation => UserImpactLevel::High,
            DegradationPattern::ResourceExhaustion => UserImpactLevel::High,
            DegradationPattern::GradualDegradation => UserImpactLevel::Medium,
            DegradationPattern::IntermittentDegradation => UserImpactLevel::Medium,
            DegradationPattern::DependencyFailure => UserImpactLevel::Medium,
            DegradationPattern::ServiceRecovery => UserImpactLevel::None,
        };

        let business_impact = match pattern {
            DegradationPattern::CascadingFailure => BusinessImpactLevel::Critical,
            DegradationPattern::SuddenDegradation => BusinessImpactLevel::High,
            DegradationPattern::ResourceExhaustion => BusinessImpactLevel::High,
            _ => BusinessImpactLevel::Medium,
        };

        let cascading_risk = match (service_type, pattern) {
            (StorageSystemType::D1Database, DegradationPattern::CascadingFailure) => {
                CascadingRiskLevel::Critical
            }
            (StorageSystemType::D1Database, _) => CascadingRiskLevel::High,
            (_, DegradationPattern::CascadingFailure) => CascadingRiskLevel::High,
            _ => CascadingRiskLevel::Medium,
        };

        ImpactAssessment {
            affected_services: vec![service_type.as_str().to_string()],
            estimated_user_impact: user_impact,
            business_impact,
            recovery_time_estimate: None,
            affected_operations: vec!["read".to_string(), "write".to_string()],
            cascading_risk,
        }
    }

    /// Generate recommended actions
    fn generate_recommended_actions(
        &self,
        pattern: &DegradationPattern,
        impact: &ImpactAssessment,
    ) -> Vec<String> {
        let mut actions = Vec::new();

        match pattern {
            DegradationPattern::GradualDegradation => {
                actions.push("Monitor resource utilization trends".to_string());
                actions.push("Review recent configuration changes".to_string());
                actions.push("Consider scaling resources if needed".to_string());
            }
            DegradationPattern::SuddenDegradation => {
                actions.push("Immediately investigate recent deployments".to_string());
                actions.push("Check for infrastructure changes".to_string());
                actions.push("Review error logs for anomalies".to_string());
                actions.push("Consider rolling back recent changes".to_string());
            }
            DegradationPattern::CascadingFailure => {
                actions.push("IMMEDIATE: Activate incident response team".to_string());
                actions.push("Implement circuit breakers if not already active".to_string());
                actions.push("Consider failover to backup systems".to_string());
                actions.push("Isolate affected components to prevent spread".to_string());
            }
            DegradationPattern::IntermittentDegradation => {
                actions.push("Enable detailed logging and monitoring".to_string());
                actions.push("Look for correlation with external events".to_string());
                actions.push("Check for resource contention issues".to_string());
            }
            DegradationPattern::ResourceExhaustion => {
                actions.push("Immediately scale resources".to_string());
                actions.push("Implement rate limiting if applicable".to_string());
                actions.push("Review capacity planning".to_string());
            }
            DegradationPattern::DependencyFailure => {
                actions.push("Check health of dependent services".to_string());
                actions.push("Implement fallback mechanisms".to_string());
                actions.push("Review dependency timeout settings".to_string());
            }
            DegradationPattern::ServiceRecovery => {
                actions.push("Continue monitoring for stability".to_string());
                actions.push("Document lessons learned".to_string());
                actions.push("Update runbooks based on incident".to_string());
            }
        }

        // Add escalation actions for high impact
        match impact.estimated_user_impact {
            UserImpactLevel::Critical | UserImpactLevel::High => {
                actions.push("Notify stakeholders immediately".to_string());
                actions.push("Prepare customer communications".to_string());
            }
            _ => {}
        }

        actions
    }

    /// Process and deduplicate alerts
    async fn process_and_deduplicate_alerts(
        &self,
        alerts: Vec<DegradationAlert>,
    ) -> ArbitrageResult<Vec<DegradationAlert>> {
        if !self.config.enable_deduplication {
            return Ok(alerts);
        }

        let mut deduplication = self.alert_deduplication.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire deduplication lock: {}", e))
        })?;

        let mut active_alerts = self.active_alerts.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire active alerts lock: {}", e))
        })?;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let dedup_window_ms = self.config.deduplication_window_seconds * 1000;

        let mut processed_alerts = Vec::new();

        for alert in alerts {
            let dedup_key = format!("{}_{}", alert.service_type.as_str(), alert.pattern.as_str());

            // Check if we should deduplicate this alert
            let should_send = if let Some(&last_sent) = deduplication.get(&dedup_key) {
                (now - last_sent) > dedup_window_ms
            } else {
                true
            };

            if should_send {
                deduplication.insert(dedup_key, now);
                active_alerts.insert(alert.alert_id.clone(), alert.clone());
                processed_alerts.push(alert);
            }
        }

        // Clean up old deduplication entries
        deduplication.retain(|_, &mut timestamp| (now - timestamp) <= dedup_window_ms);

        Ok(processed_alerts)
    }

    /// Send degradation notification
    async fn send_degradation_notification(&self, alert: &DegradationAlert) -> ArbitrageResult<()> {
        if let Some(alert_manager) = &self.alert_manager {
            // Create AlertRule for integration with existing alert manager
            let _alert_rule = AlertRule::new(
                format!("degradation_{}", alert.service_type.as_str()),
                alert.service_type.as_str().to_string(),
                "health_score".to_string(),
                AlertCondition::LessThan,
                alert.severity.clone(),
                0.7, // Health score threshold
            )
            .with_description(alert.description.clone())
            .with_duration(self.config.pattern_detection_window_seconds);

            // Evaluate the alert using the alert manager
            let generated_alerts = alert_manager
                .evaluate_metric(
                    alert.service_type.as_str(),
                    "health_score",
                    alert.current_metrics.health_score as f64,
                )
                .await?;

            self.logger.info(&format!(
                "Sent degradation notification: {} - {} (Severity: {})",
                alert.title,
                alert.pattern.as_str(),
                alert.severity.as_str()
            ));

            self.logger.debug(&format!(
                "Generated {} alerts via alert manager",
                generated_alerts.len()
            ));
        } else {
            self.logger
                .warn("Alert manager not configured - degradation notification not sent");
        }

        Ok(())
    }

    /// Get active degradation alerts
    pub async fn get_active_alerts(&self) -> Vec<DegradationAlert> {
        if let Ok(active_alerts) = self.active_alerts.lock() {
            active_alerts.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Resolve degradation alert
    pub async fn resolve_alert(&self, alert_id: &str) -> ArbitrageResult<bool> {
        let mut active_alerts = self.active_alerts.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to acquire active alerts lock: {}", e))
        })?;

        if let Some(alert) = active_alerts.get_mut(alert_id) {
            alert.status = DegradationAlertStatus::Resolved;
            alert.last_updated = chrono::Utc::now().timestamp_millis() as u64;
            self.logger
                .info(&format!("Resolved degradation alert: {}", alert_id));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Health check for the degradation alerting system
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let last_analysis = self.last_analysis.lock().map_err(|_| {
            ArbitrageError::internal_error("Degradation alerting system unresponsive")
        })?;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let analysis_age_ms = now - *last_analysis;

        // Consider healthy if analysis ran recently or system just started
        let is_healthy = analysis_age_ms < (self.config.pattern_detection_window_seconds * 2000)
            || (now - self.startup_time) < 300000; // 5 minute grace period after startup

        Ok(is_healthy)
    }
}
