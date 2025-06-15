use crate::utils::now_system_time;
use crate::utils::time::{now_instant, WasmInstant};
use std::collections::HashMap;
use std::time::{Duration, UNIX_EPOCH};

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

use worker::Env;

use crate::services::core::infrastructure::chaos_engineering::{
    ChaosEngineeringConfig, RecoveryVerifier,
};
use crate::utils::error::{ArbitrageError, ArbitrageResult};
#[cfg(target_arch = "wasm32")]
use gloo_timers;

/// Status of a chaos experiment campaign
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CampaignStatus {
    Pending,
    Scheduled,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Aborted,
}

/// Priority level for experiment campaigns
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CampaignPriority {
    Critical = 4,
    High = 3,
    Medium = 2,
    Low = 1,
}

/// Types of chaos experiments that can be orchestrated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExperimentType {
    StorageFaultInjection {
        storage_type: String,
        fault_type: String,
        duration_ms: u64,
        intensity: f64,
    },
    NetworkChaos {
        chaos_type: String,
        target_service: String,
        parameters: HashMap<String, String>,
    },
    ResourceExhaustion {
        resource_type: String,
        exhaustion_level: f64,
        duration_ms: u64,
    },
    CombinedExperiment {
        experiments: Vec<ExperimentType>,
        sequential: bool,
    },
}

/// Configuration for experiment safety controls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub max_concurrent_experiments: u32,
    pub max_traffic_impact_percentage: f64,
    pub business_hours_only: bool,
    pub allowed_time_windows: Vec<TimeWindow>,
    pub excluded_services: Vec<String>,
    pub automatic_halt_threshold: f64,
    pub statistical_significance_level: f64,
    pub minimum_experiment_duration_seconds: u64,
    pub maximum_experiment_duration_seconds: u64,
    pub failover_exclusion_enabled: bool,
    pub circuit_breaker_threshold: u32,
}

/// Time window configuration for experiment scheduling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start_hour: u8,
    pub end_hour: u8,
    pub days_of_week: Vec<u8>, // 0 = Monday, 6 = Sunday
    pub timezone: String,
}

/// Blast radius configuration for controlling experiment impact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusConfig {
    pub initial_percentage: f64,
    pub maximum_percentage: f64,
    pub escalation_threshold: f64,
    pub escalation_step: f64,
    pub automatic_escalation: bool,
    pub target_services: Vec<String>,
    pub excluded_services: Vec<String>,
}

/// Metrics for experiment monitoring and analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentMetrics {
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub success_rate_baseline: f64,
    pub success_rate_experiment: f64,
    pub error_rate_baseline: f64,
    pub error_rate_experiment: f64,
    pub latency_p50_baseline: f64,
    pub latency_p50_experiment: f64,
    pub latency_p99_baseline: f64,
    pub latency_p99_experiment: f64,
    pub throughput_baseline: f64,
    pub throughput_experiment: f64,
    pub statistical_significance: f64,
    pub impact_detected: bool,
    pub halt_triggered: bool,
    pub custom_metrics: HashMap<String, f64>,
}

/// Comprehensive chaos experiment campaign
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentCampaign {
    pub id: String,
    pub name: String,
    pub description: String,
    pub priority: CampaignPriority,
    pub status: CampaignStatus,
    pub experiment_type: ExperimentType,
    pub safety_config: SafetyConfig,
    pub blast_radius_config: BlastRadiusConfig,
    pub scheduled_start_time: Option<u64>,
    pub actual_start_time: Option<u64>,
    pub expected_duration_seconds: u64,
    pub actual_end_time: Option<u64>,
    pub metrics: Option<ExperimentMetrics>,
    pub created_by: String,
    pub created_at: u64,
    pub tags: HashMap<String, String>,
    pub dependencies: Vec<String>,
    pub auto_recovery_enabled: bool,
    pub recovery_verification_config: Option<String>,
}

/// Statistics for experiment orchestration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationStats {
    pub total_campaigns: u64,
    pub active_campaigns: u64,
    pub completed_campaigns: u64,
    pub failed_campaigns: u64,
    pub aborted_campaigns: u64,
    pub average_success_rate: f64,
    pub total_experiment_hours: f64,
    pub vulnerabilities_discovered: u64,
    pub uptime_percentage: f64,
    pub last_updated: u64,
}

/// Main experiment orchestration engine
#[derive(Debug)]
pub struct ExperimentOrchestrator {
    #[allow(dead_code)]
    config: ChaosEngineeringConfig,
    active_campaigns: HashMap<String, ExperimentCampaign>,
    campaign_queue: Vec<ExperimentCampaign>,
    safety_controller: SafetyController,
    scheduler: ExperimentScheduler,
    metrics_collector: MetricsCollector,
    blast_radius_controller: BlastRadiusController,
    recovery_verifier: Option<RecoveryVerifier>,
    stats: OrchestrationStats,
}

/// Safety controller for experiment protection
#[derive(Debug)]
pub struct SafetyController {
    config: SafetyConfig,
    #[allow(dead_code)]
    current_traffic_impact: f64,
    circuit_breaker_count: u32,
    last_safety_check: WasmInstant,
    safety_violations: Vec<SafetyViolation>,
}

/// Safety violation tracking
#[derive(Debug, Clone)]
pub struct SafetyViolation {
    pub violation_type: String,
    pub severity: String,
    pub timestamp: u64,
    pub campaign_id: String,
    pub description: String,
    pub auto_resolved: bool,
}

/// Experiment scheduler for intelligent campaign ordering
#[derive(Debug)]
pub struct ExperimentScheduler {
    safety_config: SafetyConfig,
    #[allow(dead_code)]
    scheduling_algorithm: SchedulingAlgorithm,
    #[allow(dead_code)]
    next_available_slot: Option<u64>,
    scheduled_campaigns: Vec<(String, u64)>,
}

/// Scheduling algorithms for experiment ordering
#[derive(Debug, Clone)]
pub enum SchedulingAlgorithm {
    PriorityBased,
    TimeOptimal,
    RiskMinimized,
    ResourceBalanced,
}

/// Metrics collection and analysis
#[derive(Debug)]
pub struct MetricsCollector {
    baseline_metrics: HashMap<String, f64>,
    #[allow(dead_code)]
    experiment_metrics: HashMap<String, f64>,
    #[allow(dead_code)]
    collection_interval_seconds: u64,
    #[allow(dead_code)]
    statistical_analyzer: StatisticalAnalyzer,
    #[allow(dead_code)]
    alert_thresholds: HashMap<String, f64>,
}

/// Statistical analysis for experiment evaluation
#[derive(Debug)]
pub struct StatisticalAnalyzer {
    #[allow(dead_code)]
    significance_level: f64,
    #[allow(dead_code)]
    minimum_sample_size: u64,
    #[allow(dead_code)]
    analysis_methods: Vec<AnalysisMethod>,
}

/// Statistical analysis methods
#[derive(Debug, Clone)]
pub enum AnalysisMethod {
    TTest,
    MannWhitneyU,
    KolmogorovSmirnov,
    ChiSquare,
    BayesianAnalysis,
}

/// Blast radius controller for impact management
#[derive(Debug)]
pub struct BlastRadiusController {
    #[allow(dead_code)]
    config: BlastRadiusConfig,
    current_impact: f64,
    #[allow(dead_code)]
    escalation_history: Vec<EscalationEvent>,
    impact_monitors: HashMap<String, f64>,
}

/// Blast radius escalation event
#[derive(Debug, Clone)]
pub struct EscalationEvent {
    pub timestamp: u64,
    pub from_percentage: f64,
    pub to_percentage: f64,
    pub trigger_reason: String,
    pub campaign_id: String,
    pub auto_triggered: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            max_concurrent_experiments: 3,
            max_traffic_impact_percentage: 5.0,
            business_hours_only: true,
            allowed_time_windows: vec![TimeWindow {
                start_hour: 9,
                end_hour: 17,
                days_of_week: vec![0, 1, 2, 3, 4], // Monday to Friday
                timezone: "UTC".to_string(),
            }],
            excluded_services: vec![],
            automatic_halt_threshold: 2.0, // 2% impact threshold
            statistical_significance_level: 0.05,
            minimum_experiment_duration_seconds: 300, // 5 minutes
            maximum_experiment_duration_seconds: 3600, // 1 hour
            failover_exclusion_enabled: true,
            circuit_breaker_threshold: 5,
        }
    }
}

impl Default for BlastRadiusConfig {
    fn default() -> Self {
        Self {
            initial_percentage: 1.0,
            maximum_percentage: 5.0,
            escalation_threshold: 1.5, // 1.5% impact before escalation
            escalation_step: 0.5,      // Increase by 0.5% each step
            automatic_escalation: false,
            target_services: vec![],
            excluded_services: vec![],
        }
    }
}

impl ExperimentOrchestrator {
    /// Create a new experiment orchestrator
    pub fn new(config: ChaosEngineeringConfig) -> Self {
        let safety_config = SafetyConfig::default();
        let scheduler = ExperimentScheduler::new(safety_config.clone());
        let safety_controller = SafetyController::new(safety_config.clone());
        let metrics_collector = MetricsCollector::new();
        let blast_radius_controller = BlastRadiusController::new(BlastRadiusConfig::default());

        Self {
            config,
            active_campaigns: HashMap::new(),
            campaign_queue: Vec::new(),
            safety_controller,
            scheduler,
            metrics_collector,
            blast_radius_controller,
            recovery_verifier: None,
            stats: OrchestrationStats::default(),
        }
    }

    /// Initialize the orchestrator with recovery verifier
    pub fn with_recovery_verifier(mut self, verifier: RecoveryVerifier) -> Self {
        self.recovery_verifier = Some(verifier);
        self
    }

    /// Create and schedule a new experiment campaign
    pub async fn create_campaign(
        &mut self,
        campaign: ExperimentCampaign,
    ) -> ArbitrageResult<String> {
        // Validate campaign safety
        self.safety_controller.validate_campaign(&campaign)?;

        // Check dependencies
        self.validate_dependencies(&campaign)?;

        // Schedule the campaign
        let scheduled_time = self.scheduler.schedule_campaign(&campaign).await?;

        let mut scheduled_campaign = campaign;
        scheduled_campaign.scheduled_start_time = Some(scheduled_time);
        scheduled_campaign.status = CampaignStatus::Scheduled;

        let campaign_id = scheduled_campaign.id.clone();
        self.campaign_queue.push(scheduled_campaign);

        // Sort queue by priority and scheduled time
        self.campaign_queue.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.scheduled_start_time.cmp(&b.scheduled_start_time))
        });

        self.stats.total_campaigns += 1;
        Ok(campaign_id)
    }

    /// Start orchestration loop
    pub async fn start_orchestration(&mut self, env: &Env) -> ArbitrageResult<()> {
        loop {
            // Check for campaigns to start
            if let Some(campaign) = self.get_next_ready_campaign() {
                self.start_campaign(campaign, env).await?;
            }

            // Monitor active campaigns
            self.monitor_active_campaigns(env).await?;

            // Perform safety checks
            self.safety_controller.perform_safety_check().await?;

            // Update statistics
            self.update_statistics().await;

            // Wait before next iteration
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::sleep(Duration::from_secs(1)).await;
            #[cfg(not(target_arch = "wasm32"))]
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Get the next campaign ready to start
    fn get_next_ready_campaign(&mut self) -> Option<ExperimentCampaign> {
        let current_time = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Find campaigns ready to start
        for i in 0..self.campaign_queue.len() {
            let campaign = &self.campaign_queue[i];

            if campaign.status == CampaignStatus::Scheduled {
                if let Some(scheduled_time) = campaign.scheduled_start_time {
                    if current_time >= scheduled_time {
                        // Check if we can start this campaign safely
                        if self.can_start_campaign(campaign) {
                            return Some(self.campaign_queue.remove(i));
                        }
                    }
                }
            }
        }
        None
    }

    /// Check if a campaign can be started safely
    fn can_start_campaign(&self, campaign: &ExperimentCampaign) -> bool {
        // Check concurrent experiment limit
        if self.active_campaigns.len() >= campaign.safety_config.max_concurrent_experiments as usize
        {
            return false;
        }

        // Check traffic impact limit
        let potential_impact = self.blast_radius_controller.current_impact
            + campaign.blast_radius_config.initial_percentage;
        if potential_impact > campaign.safety_config.max_traffic_impact_percentage {
            return false;
        }

        // Check business hours if required
        if campaign.safety_config.business_hours_only && !self.scheduler.is_business_hours() {
            return false;
        }

        // Check circuit breaker
        if self.safety_controller.circuit_breaker_count
            >= campaign.safety_config.circuit_breaker_threshold
        {
            return false;
        }

        true
    }

    /// Start a specific campaign
    async fn start_campaign(
        &mut self,
        mut campaign: ExperimentCampaign,
        env: &Env,
    ) -> ArbitrageResult<()> {
        let current_time = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        campaign.actual_start_time = Some(current_time);
        campaign.status = CampaignStatus::Running;

        // Initialize metrics collection
        self.metrics_collector
            .start_collection(&campaign.id)
            .await?;

        // Start blast radius monitoring
        self.blast_radius_controller
            .start_monitoring(&campaign)
            .await?;

        // Execute the experiment based on type
        self.execute_experiment(&campaign, env).await?;

        // Add to active campaigns
        let campaign_id = campaign.id.clone();
        self.active_campaigns.insert(campaign_id, campaign);
        self.stats.active_campaigns += 1;

        Ok(())
    }

    /// Execute experiment based on its type
    fn execute_experiment<'a>(
        &'a self,
        campaign: &'a ExperimentCampaign,
        env: &'a Env,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ArbitrageResult<()>> + 'a>> {
        Box::pin(async move {
            match &campaign.experiment_type {
                ExperimentType::StorageFaultInjection {
                    storage_type,
                    fault_type,
                    duration_ms,
                    intensity,
                } => {
                    // Execute storage fault injection
                    self.execute_storage_fault(
                        storage_type,
                        fault_type,
                        *duration_ms,
                        *intensity,
                        env,
                    )
                    .await?;
                }
                ExperimentType::NetworkChaos {
                    chaos_type,
                    target_service,
                    parameters,
                } => {
                    // Execute network chaos
                    self.execute_network_chaos(chaos_type, target_service, parameters, env)
                        .await?;
                }
                ExperimentType::ResourceExhaustion {
                    resource_type,
                    exhaustion_level,
                    duration_ms,
                } => {
                    // Execute resource exhaustion
                    self.execute_resource_exhaustion(
                        resource_type,
                        *exhaustion_level,
                        *duration_ms,
                        env,
                    )
                    .await?;
                }
                ExperimentType::CombinedExperiment {
                    experiments,
                    sequential,
                } => {
                    // Execute combined experiments
                    self.execute_combined_experiments(experiments, *sequential, env)
                        .await?;
                }
            }
            Ok(())
        })
    }

    /// Execute storage fault injection
    async fn execute_storage_fault(
        &self,
        storage_type: &str,
        fault_type: &str,
        _duration_ms: u64,
        _intensity: f64,
        _env: &Env,
    ) -> ArbitrageResult<()> {
        // Implementation would integrate with existing storage fault injection
        // For now, log the execution
        log::info!(
            "Executing storage fault injection: type={}, fault={}",
            storage_type,
            fault_type
        );
        Ok(())
    }

    /// Execute network chaos
    async fn execute_network_chaos(
        &self,
        chaos_type: &str,
        target_service: &str,
        _parameters: &HashMap<String, String>,
        _env: &Env,
    ) -> ArbitrageResult<()> {
        // Implementation would integrate with existing network chaos
        log::info!(
            "Executing network chaos: type={}, target={}",
            chaos_type,
            target_service
        );
        Ok(())
    }

    /// Execute resource exhaustion
    async fn execute_resource_exhaustion(
        &self,
        resource_type: &str,
        exhaustion_level: f64,
        _duration_ms: u64,
        _env: &Env,
    ) -> ArbitrageResult<()> {
        // Implementation would integrate with existing resource chaos
        log::info!(
            "Executing resource exhaustion: type={}, level={}",
            resource_type,
            exhaustion_level
        );
        Ok(())
    }

    /// Execute combined experiments
    async fn execute_combined_experiments(
        &self,
        experiments: &[ExperimentType],
        sequential: bool,
        env: &Env,
    ) -> ArbitrageResult<()> {
        if sequential {
            // Execute experiments one by one
            for experiment in experiments {
                let temp_campaign = self.create_temp_campaign(experiment.clone());
                self.execute_experiment(&temp_campaign, env).await?;
            }
        } else {
            // Execute experiments in parallel (sequential for WASM compatibility)
            for experiment in experiments {
                let temp_campaign = self.create_temp_campaign(experiment.clone());
                // Execute experiment directly for cross-platform compatibility
                let _ = self.execute_experiment(&temp_campaign, env).await;
            }
        }
        Ok(())
    }

    /// Create a temporary campaign for combined experiments
    fn create_temp_campaign(&self, experiment_type: ExperimentType) -> ExperimentCampaign {
        ExperimentCampaign {
            id: format!(
                "temp_{}",
                now_system_time()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            name: "Temporary Combined Experiment".to_string(),
            description: "Part of combined experiment".to_string(),
            priority: CampaignPriority::Medium,
            status: CampaignStatus::Running,
            experiment_type,
            safety_config: SafetyConfig::default(),
            blast_radius_config: BlastRadiusConfig::default(),
            scheduled_start_time: None,
            actual_start_time: None,
            expected_duration_seconds: 300,
            actual_end_time: None,
            metrics: None,
            created_by: "system".to_string(),
            created_at: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            tags: HashMap::new(),
            dependencies: vec![],
            auto_recovery_enabled: true,
            recovery_verification_config: None,
        }
    }

    /// Monitor active campaigns
    async fn monitor_active_campaigns(&mut self, env: &Env) -> ArbitrageResult<()> {
        let mut campaigns_to_complete = Vec::new();
        let mut campaigns_to_halt = Vec::new();
        let current_time = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // First pass: collect campaign IDs and check completion/halt conditions
        {
            let campaign_ids: Vec<String> = self.active_campaigns.keys().cloned().collect();

            for campaign_id in campaign_ids {
                if let Some(campaign) = self.active_campaigns.get(&campaign_id).cloned() {
                    // Check if campaign should be completed
                    if let Some(start_time) = campaign.actual_start_time {
                        let elapsed = current_time - start_time;
                        if elapsed >= campaign.expected_duration_seconds {
                            campaigns_to_complete.push(campaign_id.clone());
                            continue;
                        }
                    }

                    // Collect metrics
                    let metrics = self
                        .metrics_collector
                        .collect_campaign_metrics(&campaign_id)
                        .await?;

                    // Update campaign metrics
                    if let Some(campaign_mut) = self.active_campaigns.get_mut(&campaign_id) {
                        campaign_mut.metrics = Some(metrics.clone());
                    }

                    // Check for automatic halt conditions
                    if self.should_halt_campaign(&metrics, &campaign) {
                        campaigns_to_halt.push(campaign_id.clone());
                    }
                }
            }
        }

        // Second pass: halt campaigns that need halting
        for campaign_id in campaigns_to_halt {
            self.halt_campaign(&campaign_id, "Automatic halt triggered", env)
                .await?;
        }

        // Third pass: complete campaigns that are finished
        for campaign_id in campaigns_to_complete {
            self.complete_campaign(&campaign_id, env).await?;
        }

        Ok(())
    }

    /// Check if a campaign should be halted
    fn should_halt_campaign(
        &self,
        metrics: &ExperimentMetrics,
        campaign: &ExperimentCampaign,
    ) -> bool {
        // Check impact threshold
        let impact = (metrics.success_rate_baseline - metrics.success_rate_experiment).abs();
        if impact > campaign.safety_config.automatic_halt_threshold {
            return true;
        }

        // Check error rate increase
        let error_increase = metrics.error_rate_experiment - metrics.error_rate_baseline;
        if error_increase > campaign.safety_config.automatic_halt_threshold {
            return true;
        }

        // Check statistical significance of negative impact
        if metrics.statistical_significance < campaign.safety_config.statistical_significance_level
            && metrics.impact_detected
        {
            return true;
        }

        false
    }

    /// Halt a specific campaign
    async fn halt_campaign(
        &mut self,
        campaign_id: &str,
        reason: &str,
        env: &Env,
    ) -> ArbitrageResult<()> {
        if let Some(mut campaign) = self.active_campaigns.remove(campaign_id) {
            campaign.status = CampaignStatus::Aborted;

            let current_time = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            campaign.actual_end_time = Some(current_time);

            // Stop metrics collection
            self.metrics_collector.stop_collection(campaign_id).await?;

            // Stop blast radius monitoring
            self.blast_radius_controller
                .stop_monitoring(campaign_id)
                .await?;

            // Trigger recovery if enabled
            if campaign.auto_recovery_enabled {
                if let Some(ref mut recovery_verifier) = self.recovery_verifier {
                    recovery_verifier.verify_recovery(campaign_id, env).await?;
                }
            }

            // Log the halt
            log::warn!("Campaign {} halted: {}", campaign_id, reason);

            self.stats.aborted_campaigns += 1;
            self.stats.active_campaigns -= 1;
        }

        Ok(())
    }

    /// Complete a campaign
    async fn complete_campaign(&mut self, campaign_id: &str, env: &Env) -> ArbitrageResult<()> {
        if let Some(mut campaign) = self.active_campaigns.remove(campaign_id) {
            let current_time = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            campaign.actual_end_time = Some(current_time);
            campaign.status = CampaignStatus::Completed;

            // Stop metrics collection
            self.metrics_collector.stop_collection(campaign_id).await?;

            // Stop blast radius monitoring
            self.blast_radius_controller
                .stop_monitoring(campaign_id)
                .await?;

            // Perform final analysis
            if let Some(ref metrics) = campaign.metrics {
                self.perform_final_analysis(metrics, &campaign).await?;
            }

            // Trigger recovery verification if enabled
            if campaign.auto_recovery_enabled {
                if let Some(ref mut recovery_verifier) = self.recovery_verifier {
                    recovery_verifier.verify_recovery(campaign_id, env).await?;
                }
            }

            log::info!("Campaign {} completed successfully", campaign_id);

            self.stats.completed_campaigns += 1;
            self.stats.active_campaigns -= 1;
        }

        Ok(())
    }

    /// Perform final analysis of experiment results
    async fn perform_final_analysis(
        &self,
        metrics: &ExperimentMetrics,
        _campaign: &ExperimentCampaign,
    ) -> ArbitrageResult<()> {
        // Analyze results and determine if vulnerabilities were discovered
        let has_vulnerability = metrics.impact_detected
            || metrics.statistical_significance < 0.05
            || metrics.success_rate_experiment < 0.95;

        if has_vulnerability {
            log::warn!("Potential vulnerability discovered in experiment");
            // In a real implementation, this would trigger alerts and reports
        }

        Ok(())
    }

    /// Validate campaign dependencies
    fn validate_dependencies(&self, campaign: &ExperimentCampaign) -> ArbitrageResult<()> {
        for dependency_id in &campaign.dependencies {
            // Check if dependency campaign exists and is completed
            if let Some(dependency) = self.active_campaigns.get(dependency_id) {
                if dependency.status != CampaignStatus::Completed {
                    return Err(ArbitrageError::configuration_error(format!(
                        "Dependency campaign {} is not completed",
                        dependency_id
                    )));
                }
            }
        }
        Ok(())
    }

    /// Update orchestration statistics
    async fn update_statistics(&mut self) {
        let current_time = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.stats.last_updated = current_time;
        self.stats.active_campaigns = self.active_campaigns.len() as u64;

        // Calculate average success rate from completed campaigns
        let mut total_success_rate = 0.0;
        let mut campaign_count = 0;

        for campaign in self.active_campaigns.values() {
            if let Some(ref metrics) = campaign.metrics {
                total_success_rate += metrics.success_rate_experiment;
                campaign_count += 1;
            }
        }

        if campaign_count > 0 {
            self.stats.average_success_rate = total_success_rate / campaign_count as f64;
        }

        // Calculate uptime percentage
        self.stats.uptime_percentage = 99.9; // Placeholder - would be calculated from actual metrics
    }

    /// Get current orchestration statistics
    pub fn get_statistics(&self) -> &OrchestrationStats {
        &self.stats
    }

    /// Get active campaigns
    pub fn get_active_campaigns(&self) -> Vec<&ExperimentCampaign> {
        self.active_campaigns.values().collect()
    }

    /// Get queued campaigns
    pub fn get_queued_campaigns(&self) -> &[ExperimentCampaign] {
        &self.campaign_queue
    }
}

impl SafetyController {
    fn new(config: SafetyConfig) -> Self {
        Self {
            config,
            current_traffic_impact: 0.0,
            circuit_breaker_count: 0,
            last_safety_check: now_instant(),
            safety_violations: Vec::new(),
        }
    }

    fn validate_campaign(&self, campaign: &ExperimentCampaign) -> ArbitrageResult<()> {
        // Validate safety configuration
        if campaign.blast_radius_config.initial_percentage
            > self.config.max_traffic_impact_percentage
        {
            return Err(ArbitrageError::configuration_error(
                "Initial blast radius exceeds maximum allowed traffic impact",
            ));
        }

        if campaign.expected_duration_seconds < self.config.minimum_experiment_duration_seconds
            || campaign.expected_duration_seconds > self.config.maximum_experiment_duration_seconds
        {
            return Err(ArbitrageError::configuration_error(
                "Experiment duration outside allowed range",
            ));
        }

        // Check excluded services
        if let ExperimentType::NetworkChaos { target_service, .. } = &campaign.experiment_type {
            if self.config.excluded_services.contains(target_service) {
                return Err(ArbitrageError::configuration_error(format!(
                    "Target service {} is in excluded list",
                    target_service
                )));
            }
        }

        Ok(())
    }

    async fn perform_safety_check(&mut self) -> ArbitrageResult<()> {
        let now = now_instant();
        if now.duration_since(self.last_safety_check) < Duration::from_secs(10) {
            return Ok(()); // Only check every 10 seconds
        }

        // Reset circuit breaker count if enough time has passed
        if now.duration_since(self.last_safety_check) > Duration::from_secs(300) {
            self.circuit_breaker_count = 0;
        }

        // Check for failover conditions
        if self.config.failover_exclusion_enabled {
            // In a real implementation, this would check actual failover status
            let is_failover_active = false; // Placeholder
            if is_failover_active {
                self.record_safety_violation(
                    "failover_active".to_string(),
                    "high".to_string(),
                    "Failover detected - halting experiments".to_string(),
                    "system".to_string(),
                );
            }
        }

        self.last_safety_check = now;
        Ok(())
    }

    fn record_safety_violation(
        &mut self,
        violation_type: String,
        severity: String,
        description: String,
        campaign_id: String,
    ) {
        let violation = SafetyViolation {
            violation_type,
            severity,
            timestamp: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            campaign_id,
            description,
            auto_resolved: false,
        };

        self.safety_violations.push(violation);
        self.circuit_breaker_count += 1;
    }
}

impl ExperimentScheduler {
    fn new(safety_config: SafetyConfig) -> Self {
        Self {
            safety_config,
            scheduling_algorithm: SchedulingAlgorithm::PriorityBased,
            next_available_slot: None,
            scheduled_campaigns: Vec::new(),
        }
    }

    async fn schedule_campaign(&mut self, campaign: &ExperimentCampaign) -> ArbitrageResult<u64> {
        let current_time = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Find next available time slot
        let mut scheduled_time = current_time + 60; // Start at least 1 minute from now

        // Check business hours constraint
        if campaign.safety_config.business_hours_only {
            scheduled_time = self.find_next_business_hours_slot(scheduled_time);
        }

        // Account for other scheduled campaigns
        while self.is_slot_occupied(scheduled_time, campaign.expected_duration_seconds) {
            scheduled_time += 300; // Try next 5-minute slot
        }

        // Record the scheduled time
        self.scheduled_campaigns
            .push((campaign.id.clone(), scheduled_time));
        self.scheduled_campaigns.sort_by_key(|(_, time)| *time);

        Ok(scheduled_time)
    }

    fn find_next_business_hours_slot(&self, start_time: u64) -> u64 {
        // Convert to DateTime for easier manipulation
        let datetime =
            DateTime::<Utc>::from_timestamp(start_time as i64, 0).unwrap_or_else(Utc::now);

        // For simplicity, assume first time window
        let time_window = &self.safety_config.allowed_time_windows[0];

        let hour = datetime.hour();
        let weekday = datetime.weekday().number_from_monday() - 1; // 0 = Monday

        // Check if current time is within business hours
        if time_window.days_of_week.contains(&(weekday as u8))
            && hour >= time_window.start_hour as u32
            && hour < time_window.end_hour as u32
        {
            return start_time; // Current time is fine
        }

        // Find next business day
        let mut next_datetime = datetime;
        loop {
            next_datetime += chrono::Duration::days(1);
            let next_weekday = next_datetime.weekday().number_from_monday() - 1;

            if time_window.days_of_week.contains(&(next_weekday as u8)) {
                // Set to start of business hours
                let start_of_day = next_datetime
                    .with_hour(time_window.start_hour as u32)
                    .unwrap()
                    .with_minute(0)
                    .unwrap()
                    .with_second(0)
                    .unwrap();
                return start_of_day.timestamp() as u64;
            }
        }
    }

    fn is_slot_occupied(&self, start_time: u64, duration: u64) -> bool {
        let end_time = start_time + duration;

        for (_, scheduled_time) in &self.scheduled_campaigns {
            let scheduled_end = scheduled_time + 3600; // Assume 1 hour duration for existing

            // Check for overlap
            if start_time < scheduled_end && end_time > *scheduled_time {
                return true;
            }
        }
        false
    }

    fn is_business_hours(&self) -> bool {
        let now = Utc::now();
        let hour = now.hour();
        let weekday = now.weekday().number_from_monday() - 1;

        // Check first time window (simplified)
        if let Some(time_window) = self.safety_config.allowed_time_windows.first() {
            return time_window.days_of_week.contains(&(weekday as u8))
                && hour >= time_window.start_hour as u32
                && hour < time_window.end_hour as u32;
        }
        false
    }
}

impl MetricsCollector {
    fn new() -> Self {
        Self {
            baseline_metrics: HashMap::new(),
            experiment_metrics: HashMap::new(),
            collection_interval_seconds: 30,
            statistical_analyzer: StatisticalAnalyzer::new(),
            alert_thresholds: HashMap::new(),
        }
    }

    async fn start_collection(&mut self, campaign_id: &str) -> ArbitrageResult<()> {
        log::info!("Starting metrics collection for campaign {}", campaign_id);

        // Initialize baseline metrics
        self.baseline_metrics
            .insert("success_rate".to_string(), 99.5);
        self.baseline_metrics.insert("error_rate".to_string(), 0.5);
        self.baseline_metrics
            .insert("latency_p50".to_string(), 150.0);
        self.baseline_metrics
            .insert("latency_p99".to_string(), 800.0);
        self.baseline_metrics
            .insert("throughput".to_string(), 1000.0);

        Ok(())
    }

    async fn collect_campaign_metrics(
        &mut self,
        campaign_id: &str,
    ) -> ArbitrageResult<ExperimentMetrics> {
        // Simulate metrics collection
        let current_time = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Simulate slight degradation in experiment group
        let success_rate_baseline = 99.5;
        let success_rate_experiment = 99.2; // Slight decrease
        let error_rate_baseline = 0.5;
        let error_rate_experiment = 0.8; // Slight increase

        let metrics = ExperimentMetrics {
            start_time: current_time - 300, // Started 5 minutes ago
            end_time: None,
            success_rate_baseline,
            success_rate_experiment,
            error_rate_baseline,
            error_rate_experiment,
            latency_p50_baseline: 150.0,
            latency_p50_experiment: 155.0,
            latency_p99_baseline: 800.0,
            latency_p99_experiment: 820.0,
            throughput_baseline: 1000.0,
            throughput_experiment: 980.0,
            statistical_significance: 0.15, // Not significant
            impact_detected: false,
            halt_triggered: false,
            custom_metrics: HashMap::new(),
        };

        log::debug!(
            "Collected metrics for campaign {}: {:?}",
            campaign_id,
            metrics
        );
        Ok(metrics)
    }

    async fn stop_collection(&mut self, campaign_id: &str) -> ArbitrageResult<()> {
        log::info!("Stopping metrics collection for campaign {}", campaign_id);
        Ok(())
    }
}

impl StatisticalAnalyzer {
    fn new() -> Self {
        Self {
            significance_level: 0.05,
            minimum_sample_size: 100,
            analysis_methods: vec![AnalysisMethod::TTest, AnalysisMethod::MannWhitneyU],
        }
    }
}

impl BlastRadiusController {
    fn new(config: BlastRadiusConfig) -> Self {
        Self {
            config,
            current_impact: 0.0,
            escalation_history: Vec::new(),
            impact_monitors: HashMap::new(),
        }
    }

    async fn start_monitoring(&mut self, campaign: &ExperimentCampaign) -> ArbitrageResult<()> {
        self.current_impact += campaign.blast_radius_config.initial_percentage;
        self.impact_monitors.insert(
            campaign.id.clone(),
            campaign.blast_radius_config.initial_percentage,
        );

        log::info!(
            "Started blast radius monitoring for campaign {}: {}%",
            campaign.id,
            campaign.blast_radius_config.initial_percentage
        );
        Ok(())
    }

    async fn stop_monitoring(&mut self, campaign_id: &str) -> ArbitrageResult<()> {
        if let Some(impact) = self.impact_monitors.remove(campaign_id) {
            self.current_impact -= impact;
            log::info!(
                "Stopped blast radius monitoring for campaign {}",
                campaign_id
            );
        }
        Ok(())
    }
}

impl Default for OrchestrationStats {
    fn default() -> Self {
        Self {
            total_campaigns: 0,
            active_campaigns: 0,
            completed_campaigns: 0,
            failed_campaigns: 0,
            aborted_campaigns: 0,
            average_success_rate: 0.0,
            total_experiment_hours: 0.0,
            vulnerabilities_discovered: 0,
            uptime_percentage: 100.0,
            last_updated: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_config() -> ChaosEngineeringConfig {
        ChaosEngineeringConfig {
            enabled: true,
            max_concurrent_experiments: 3,
            default_experiment_timeout_seconds: 300,
            safety_check_interval_seconds: 30,
            max_fault_intensity: 0.8,
            enable_automated_recovery: true,
            enable_metrics_collection: true,
            feature_flags:
                crate::services::core::infrastructure::chaos_engineering::ChaosFeatureFlags {
                    storage_fault_injection: true,
                    network_chaos_simulation: true,
                    resource_exhaustion_testing: true,
                    automated_orchestration: true,
                    realtime_recovery_verification: true,
                    chaos_metrics_dashboard: true,
                },
            recovery_verification: None,
        }
    }

    fn create_test_campaign() -> ExperimentCampaign {
        ExperimentCampaign {
            id: "test_campaign_001".to_string(),
            name: "Test Storage Fault Campaign".to_string(),
            description: "Testing storage fault injection with orchestration".to_string(),
            priority: CampaignPriority::High,
            status: CampaignStatus::Pending,
            experiment_type: ExperimentType::StorageFaultInjection {
                storage_type: "kv".to_string(),
                fault_type: "timeout".to_string(),
                duration_ms: 5000,
                intensity: 0.5,
            },
            safety_config: SafetyConfig::default(),
            blast_radius_config: BlastRadiusConfig::default(),
            scheduled_start_time: None,
            actual_start_time: None,
            expected_duration_seconds: 300,
            actual_end_time: None,
            metrics: None,
            created_by: "test_user".to_string(),
            created_at: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            tags: HashMap::new(),
            dependencies: vec![],
            auto_recovery_enabled: true,
            recovery_verification_config: None,
        }
    }

    #[test]
    fn test_orchestrator_creation() {
        let config = create_test_config();
        let orchestrator = ExperimentOrchestrator::new(config);

        assert!(orchestrator.active_campaigns.is_empty());
        assert!(orchestrator.campaign_queue.is_empty());
        assert_eq!(orchestrator.stats.total_campaigns, 0);
        assert_eq!(orchestrator.stats.active_campaigns, 0);
    }

    #[test]
    fn test_safety_config_defaults() {
        let safety_config = SafetyConfig::default();

        assert_eq!(safety_config.max_concurrent_experiments, 3);
        assert_eq!(safety_config.max_traffic_impact_percentage, 5.0);
        assert!(safety_config.business_hours_only);
        assert_eq!(safety_config.automatic_halt_threshold, 2.0);
        assert_eq!(safety_config.statistical_significance_level, 0.05);
        assert_eq!(safety_config.minimum_experiment_duration_seconds, 300);
        assert_eq!(safety_config.maximum_experiment_duration_seconds, 3600);
        assert!(safety_config.failover_exclusion_enabled);
        assert_eq!(safety_config.circuit_breaker_threshold, 5);
    }

    #[test]
    fn test_blast_radius_config_defaults() {
        let blast_config = BlastRadiusConfig::default();

        assert_eq!(blast_config.initial_percentage, 1.0);
        assert_eq!(blast_config.maximum_percentage, 5.0);
        assert_eq!(blast_config.escalation_threshold, 1.5);
        assert_eq!(blast_config.escalation_step, 0.5);
        assert!(!blast_config.automatic_escalation);
        assert!(blast_config.target_services.is_empty());
        assert!(blast_config.excluded_services.is_empty());
    }

    #[test]
    fn test_campaign_creation() {
        let campaign = create_test_campaign();

        assert_eq!(campaign.id, "test_campaign_001");
        assert_eq!(campaign.priority, CampaignPriority::High);
        assert_eq!(campaign.status, CampaignStatus::Pending);
        assert_eq!(campaign.expected_duration_seconds, 300);
        assert!(campaign.auto_recovery_enabled);

        match campaign.experiment_type {
            ExperimentType::StorageFaultInjection {
                storage_type,
                fault_type,
                duration_ms,
                intensity,
            } => {
                assert_eq!(storage_type, "kv");
                assert_eq!(fault_type, "timeout");
                assert_eq!(duration_ms, 5000);
                assert_eq!(intensity, 0.5);
            }
            _ => panic!("Wrong experiment type"),
        }
    }

    #[tokio::test]
    async fn test_campaign_scheduling() {
        let config = create_test_config();
        let mut orchestrator = ExperimentOrchestrator::new(config);
        let campaign = create_test_campaign();

        let result = orchestrator.create_campaign(campaign).await;
        assert!(result.is_ok());

        let campaign_id = result.unwrap();
        assert_eq!(campaign_id, "test_campaign_001");
        assert_eq!(orchestrator.campaign_queue.len(), 1);
        assert_eq!(orchestrator.stats.total_campaigns, 1);

        let scheduled_campaign = &orchestrator.campaign_queue[0];
        assert_eq!(scheduled_campaign.status, CampaignStatus::Scheduled);
        assert!(scheduled_campaign.scheduled_start_time.is_some());
    }

    #[test]
    fn test_safety_controller_validation() {
        let safety_config = SafetyConfig::default();
        let safety_controller = SafetyController::new(safety_config);

        // Test valid campaign
        let valid_campaign = create_test_campaign();
        assert!(safety_controller.validate_campaign(&valid_campaign).is_ok());

        // Test campaign with invalid blast radius
        let mut invalid_campaign = create_test_campaign();
        invalid_campaign.blast_radius_config.initial_percentage = 10.0; // Exceeds 5% limit
        assert!(safety_controller
            .validate_campaign(&invalid_campaign)
            .is_err());

        // Test campaign with invalid duration
        let mut invalid_duration_campaign = create_test_campaign();
        invalid_duration_campaign.expected_duration_seconds = 10; // Too short
        assert!(safety_controller
            .validate_campaign(&invalid_duration_campaign)
            .is_err());
    }

    #[test]
    fn test_experiment_scheduler() {
        let safety_config = SafetyConfig::default();
        let scheduler = ExperimentScheduler::new(safety_config);

        // Test business hours detection
        let _is_business_hours = scheduler.is_business_hours();
        // Result depends on when test is run, just ensure it doesn't panic
    }

    #[test]
    fn test_metrics_collector() {
        let metrics_collector = MetricsCollector::new();

        assert_eq!(metrics_collector.collection_interval_seconds, 30);
        assert!(metrics_collector.baseline_metrics.is_empty());
        assert!(metrics_collector.experiment_metrics.is_empty());
    }

    #[tokio::test]
    async fn test_blast_radius_controller() {
        let config = BlastRadiusConfig::default();
        let mut controller = BlastRadiusController::new(config);

        let campaign = create_test_campaign();

        // Test monitoring start
        let result = controller.start_monitoring(&campaign).await;
        assert!(result.is_ok());
        assert_eq!(controller.current_impact, 1.0);
        assert!(controller.impact_monitors.contains_key(&campaign.id));

        // Test monitoring stop
        let result = controller.stop_monitoring(&campaign.id).await;
        assert!(result.is_ok());
        assert_eq!(controller.current_impact, 0.0);
        assert!(!controller.impact_monitors.contains_key(&campaign.id));
    }

    #[test]
    fn test_orchestration_stats() {
        let stats = OrchestrationStats::default();

        assert_eq!(stats.total_campaigns, 0);
        assert_eq!(stats.active_campaigns, 0);
        assert_eq!(stats.completed_campaigns, 0);
        assert_eq!(stats.failed_campaigns, 0);
        assert_eq!(stats.aborted_campaigns, 0);
        assert_eq!(stats.average_success_rate, 0.0);
        assert_eq!(stats.total_experiment_hours, 0.0);
        assert_eq!(stats.vulnerabilities_discovered, 0);
        assert_eq!(stats.uptime_percentage, 100.0);
        assert!(stats.last_updated > 0);
    }

    #[test]
    fn test_combined_experiment_creation() {
        let storage_experiment = ExperimentType::StorageFaultInjection {
            storage_type: "kv".to_string(),
            fault_type: "timeout".to_string(),
            duration_ms: 5000,
            intensity: 0.5,
        };

        let network_experiment = ExperimentType::NetworkChaos {
            chaos_type: "latency".to_string(),
            target_service: "api_service".to_string(),
            parameters: HashMap::from([
                ("latency_ms".to_string(), "100".to_string()),
                ("jitter_ms".to_string(), "10".to_string()),
            ]),
        };

        let combined_experiment = ExperimentType::CombinedExperiment {
            experiments: vec![storage_experiment, network_experiment],
            sequential: true,
        };

        match combined_experiment {
            ExperimentType::CombinedExperiment {
                experiments,
                sequential,
            } => {
                assert_eq!(experiments.len(), 2);
                assert!(sequential);
            }
            _ => panic!("Wrong experiment type"),
        }
    }

    #[test]
    fn test_priority_ordering() {
        let mut priorities = [
            CampaignPriority::Low,
            CampaignPriority::Critical,
            CampaignPriority::Medium,
            CampaignPriority::High,
        ];

        priorities.sort();

        assert_eq!(priorities[0], CampaignPriority::Low);
        assert_eq!(priorities[1], CampaignPriority::Medium);
        assert_eq!(priorities[2], CampaignPriority::High);
        assert_eq!(priorities[3], CampaignPriority::Critical);
    }

    #[test]
    fn test_time_window_configuration() {
        let time_window = TimeWindow {
            start_hour: 9,
            end_hour: 17,
            days_of_week: vec![0, 1, 2, 3, 4], // Monday to Friday
            timezone: "UTC".to_string(),
        };

        assert_eq!(time_window.start_hour, 9);
        assert_eq!(time_window.end_hour, 17);
        assert_eq!(time_window.days_of_week.len(), 5);
        assert_eq!(time_window.timezone, "UTC");
    }

    #[test]
    fn test_experiment_metrics_structure() {
        let current_time = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let metrics = ExperimentMetrics {
            start_time: current_time,
            end_time: None,
            success_rate_baseline: 99.5,
            success_rate_experiment: 99.2,
            error_rate_baseline: 0.5,
            error_rate_experiment: 0.8,
            latency_p50_baseline: 150.0,
            latency_p50_experiment: 155.0,
            latency_p99_baseline: 800.0,
            latency_p99_experiment: 820.0,
            throughput_baseline: 1000.0,
            throughput_experiment: 980.0,
            statistical_significance: 0.15,
            impact_detected: false,
            halt_triggered: false,
            custom_metrics: HashMap::new(),
        };

        assert!(metrics.success_rate_baseline > metrics.success_rate_experiment);
        assert!(metrics.error_rate_experiment > metrics.error_rate_baseline);
        assert!(metrics.latency_p50_experiment > metrics.latency_p50_baseline);
        assert!(metrics.throughput_baseline > metrics.throughput_experiment);
        assert!(!metrics.impact_detected);
        assert!(!metrics.halt_triggered);
    }
}
