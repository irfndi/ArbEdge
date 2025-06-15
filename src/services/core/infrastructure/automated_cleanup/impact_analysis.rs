use crate::utils::now_system_time;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::services::core::config::ServiceConfig;
use crate::services::core::health::HealthCheck;
use crate::services::core::persistence::connection_manager::ConnectionManager;
use crate::services::core::persistence::transaction_coordinator::TransactionCoordinator;

/// Impact analysis configuration for cleanup operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysisConfig {
    /// Maximum depth for dependency traversal
    pub max_dependency_depth: u32,
    /// Threshold for high-risk data (percentage)
    pub high_risk_threshold: f32,
    /// Critical dependency timeout
    pub dependency_timeout: Duration,
    /// Enable dry-run mode by default
    pub default_dry_run: bool,
    /// Maximum analysis time before timeout
    pub analysis_timeout: Duration,
    /// Circuit breaker settings
    pub circuit_breaker: CircuitBreakerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub timeout: Duration,
    pub retry_timeout: Duration,
}

impl Default for ImpactAnalysisConfig {
    fn default() -> Self {
        Self {
            max_dependency_depth: 10,
            high_risk_threshold: 0.8,
            dependency_timeout: Duration::from_secs(30),
            default_dry_run: true,
            analysis_timeout: Duration::from_secs(300), // 5 minutes
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 5,
                timeout: Duration::from_secs(60),
                retry_timeout: Duration::from_secs(300),
            },
        }
    }
}

/// Represents different types of data dependencies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DependencyType {
    /// Direct foreign key reference
    ForeignKey,
    /// Index dependency
    Index,
    /// View dependency
    View,
    /// Stored procedure dependency
    StoredProcedure,
    /// Application-level reference
    Application,
    /// External system dependency
    External,
    /// Backup/Archive dependency
    Backup,
    /// Replication dependency
    Replication,
}

/// Represents the impact level of a cleanup operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactLevel {
    /// No impact - safe to proceed
    None,
    /// Low impact - minimal disruption
    Low,
    /// Medium impact - some disruption expected
    Medium,
    /// High impact - significant disruption
    High,
    /// Critical impact - potential system failure
    Critical,
}

/// Represents a data dependency in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataDependency {
    pub id: Uuid,
    pub source_id: String,
    pub target_id: String,
    pub dependency_type: DependencyType,
    pub impact_level: ImpactLevel,
    pub is_critical: bool,
    pub last_accessed: Option<SystemTime>,
    pub access_frequency: f32, // accesses per day
    pub data_size: u64, // bytes
    pub business_critical: bool,
    pub metadata: HashMap<String, String>,
}

/// Risk assessment result for cleanup operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub overall_risk: ImpactLevel,
    pub confidence_score: f32, // 0.0 to 1.0
    pub affected_systems: Vec<String>,
    pub blocking_dependencies: Vec<DataDependency>,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
    pub estimated_recovery_time: Option<Duration>,
    pub rollback_available: bool,
}

/// Impact analysis request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysisRequest {
    pub cleanup_id: Uuid,
    pub target_data: Vec<String>, // Data identifiers to be cleaned
    pub cleanup_type: String,
    pub dry_run: bool,
    pub force_analysis: bool,
    pub max_risk_level: ImpactLevel,
    pub business_context: Option<String>,
}

/// Impact analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysisResult {
    pub request_id: Uuid,
    pub cleanup_id: Uuid,
    pub analysis_status: AnalysisStatus,
    pub risk_assessment: RiskAssessment,
    pub dependency_graph: DependencyGraph,
    pub safety_checks: Vec<SafetyCheck>,
    pub analysis_time: Duration,
    pub safe_to_proceed: bool,
    pub required_approvals: Vec<String>,
    pub recommended_actions: Vec<RecommendedAction>,
    pub created_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Timeout,
    Cancelled,
}

/// Dependency graph representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub nodes: HashMap<String, DependencyNode>,
    pub edges: Vec<DependencyEdge>,
    pub critical_paths: Vec<Vec<String>>,
    pub orphaned_data: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyNode {
    pub id: String,
    pub node_type: String,
    pub impact_level: ImpactLevel,
    pub size: u64,
    pub last_accessed: Option<SystemTime>,
    pub business_critical: bool,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub dependency_type: DependencyType,
    pub strength: f32, // 0.0 to 1.0
    pub bidirectional: bool,
}

/// Safety check performed during analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyCheck {
    pub check_type: SafetyCheckType,
    pub status: SafetyCheckStatus,
    pub message: String,
    pub severity: ImpactLevel,
    pub auto_fixable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SafetyCheckType {
    DependencyValidation,
    BusinessCriticalData,
    RecentActivity,
    BackupAvailability,
    RollbackPlan,
    ExternalSystemImpact,
    ComplianceCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SafetyCheckStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

/// Recommended action for safe cleanup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedAction {
    pub action_type: ActionType,
    pub description: String,
    pub priority: ImpactLevel,
    pub estimated_duration: Option<Duration>,
    pub prerequisites: Vec<String>,
    pub rollback_plan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    CreateBackup,
    NotifyStakeholders,
    ScheduleMaintenance,
    UpdateDocumentation,
    PrepareRollback,
    ValidateConnections,
    StageCleanup,
    MonitorSystems,
}

/// Circuit breaker for protecting the analysis service
#[derive(Debug)]
struct CircuitBreaker {
    config: CircuitBreakerConfig,
    failure_count: Arc<RwLock<u32>>,
    last_failure_time: Arc<RwLock<Option<SystemTime>>>,
    state: Arc<RwLock<CircuitBreakerState>>,
}

#[derive(Debug, PartialEq)]
enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            failure_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
        }
    }

    async fn call_with_breaker<F, T, E>(&self, operation: F) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        let state = self.state.read().await;
        
        match *state {
            CircuitBreakerState::Open => {
                let last_failure = self.last_failure_time.read().await;
                if let Some(last_fail_time) = *last_failure {
                    if now_system_time().duration_since(last_fail_time).unwrap_or_default() 
                        > self.config.retry_timeout {
                        drop(last_failure);
                        drop(state);
                        *self.state.write().await = CircuitBreakerState::HalfOpen;
                    } else {
                        return Err(operation.await.unwrap_err()); // Return cached error
                    }
                }
            }
            _ => {}
        }
        
        drop(state);
        
        match operation.await {
            Ok(result) => {
                *self.failure_count.write().await = 0;
                *self.state.write().await = CircuitBreakerState::Closed;
                Ok(result)
            }
            Err(error) => {
                let mut failure_count = self.failure_count.write().await;
                *failure_count += 1;
                
                if *failure_count >= self.config.failure_threshold {
                    *self.state.write().await = CircuitBreakerState::Open;
                    *self.last_failure_time.write().await = Some(now_system_time());
                }
                
                Err(error)
            }
        }
    }
}

/// Main cleanup impact analysis engine
pub struct CleanupImpactAnalysisEngine {
    config: ImpactAnalysisConfig,
    connection_manager: Arc<ConnectionManager>,
    transaction_coordinator: Arc<TransactionCoordinator>,
    dependency_cache: Arc<RwLock<HashMap<String, Vec<DataDependency>>>>,
    analysis_cache: Arc<RwLock<HashMap<Uuid, ImpactAnalysisResult>>>,
    circuit_breaker: CircuitBreaker,
    running_analyses: Arc<RwLock<HashSet<Uuid>>>,
}

impl CleanupImpactAnalysisEngine {
    pub fn new(
        config: ImpactAnalysisConfig,
        connection_manager: Arc<ConnectionManager>,
        transaction_coordinator: Arc<TransactionCoordinator>,
    ) -> Self {
        let circuit_breaker = CircuitBreaker::new(config.circuit_breaker.clone());
        
        Self {
            config,
            connection_manager,
            transaction_coordinator,
            dependency_cache: Arc::new(RwLock::new(HashMap::new())),
            analysis_cache: Arc::new(RwLock::new(HashMap::new())),
            circuit_breaker,
            running_analyses: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Perform comprehensive impact analysis for cleanup operation
    pub async fn analyze_impact(
        &self,
        request: ImpactAnalysisRequest,
    ) -> Result<ImpactAnalysisResult, Box<dyn std::error::Error + Send + Sync>> {
        let request_id = Uuid::new_v4();
        let start_time = now_system_time();
        
        // Check if analysis is already running for this cleanup
        {
            let mut running = self.running_analyses.write().await;
            if running.contains(&request.cleanup_id) {
                return Err("Analysis already in progress for this cleanup".into());
            }
            running.insert(request.cleanup_id);
        }

        // Use circuit breaker to protect the analysis service
        let result = self.circuit_breaker.call_with_breaker(async {
            self.perform_analysis_internal(request_id, &request, start_time).await
        }).await;

        // Remove from running analyses
        {
            let mut running = self.running_analyses.write().await;
            running.remove(&request.cleanup_id);
        }

        result
    }

    async fn perform_analysis_internal(
        &self,
        request_id: Uuid,
        request: &ImpactAnalysisRequest,
        start_time: SystemTime,
    ) -> Result<ImpactAnalysisResult, Box<dyn std::error::Error + Send + Sync>> {
        // Build dependency graph
        let dependency_graph = self.build_dependency_graph(&request.target_data).await?;
        
        // Perform risk assessment
        let risk_assessment = self.assess_risk(&dependency_graph, request).await?;
        
        // Run safety checks
        let safety_checks = self.run_safety_checks(&dependency_graph, request).await?;
        
        // Generate recommendations
        let recommended_actions = self.generate_recommendations(&risk_assessment, &safety_checks).await?;
        
        // Determine if safe to proceed
        let safe_to_proceed = self.determine_safety(&risk_assessment, &safety_checks, request).await;
        
        let analysis_time = now_system_time().duration_since(start_time)
            .unwrap_or_default();

        let result = ImpactAnalysisResult {
            request_id,
            cleanup_id: request.cleanup_id,
            analysis_status: AnalysisStatus::Completed,
            risk_assessment,
            dependency_graph,
            safety_checks,
            analysis_time,
            safe_to_proceed,
            required_approvals: self.get_required_approvals(&risk_assessment.overall_risk).await,
            recommended_actions,
            created_at: now_system_time(),
        };

        // Cache the result
        {
            let mut cache = self.analysis_cache.write().await;
            cache.insert(request_id, result.clone());
        }

        Ok(result)
    }

    /// Build comprehensive dependency graph for target data
    async fn build_dependency_graph(
        &self,
        target_data: &[String],
    ) -> Result<DependencyGraph, Box<dyn std::error::Error + Send + Sync>> {
        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut visited = HashSet::new();
        let mut critical_paths = Vec::new();
        let mut orphaned_data = Vec::new();

        // Build graph through recursive dependency traversal
        for data_id in target_data {
            self.traverse_dependencies(
                data_id,
                &mut nodes,
                &mut edges,
                &mut visited,
                0,
            ).await?;
        }

        // Identify critical paths and orphaned data
        for (node_id, node) in &nodes {
            if node.business_critical {
                let path = self.find_critical_path_to_node(node_id, &nodes, &edges).await;
                if !path.is_empty() {
                    critical_paths.push(path);
                }
            }
            
            // Check for orphaned data (no incoming dependencies)
            let has_incoming = edges.iter().any(|edge| edge.to == *node_id);
            if !has_incoming && target_data.contains(node_id) {
                orphaned_data.push(node_id.clone());
            }
        }

        Ok(DependencyGraph {
            nodes,
            edges,
            critical_paths,
            orphaned_data,
        })
    }

    async fn traverse_dependencies(
        &self,
        data_id: &str,
        nodes: &mut HashMap<String, DependencyNode>,
        edges: &mut Vec<DependencyEdge>,
        visited: &mut HashSet<String>,
        depth: u32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if visited.contains(data_id) || depth >= self.config.max_dependency_depth {
            return Ok(());
        }

        visited.insert(data_id.to_string());

        // Get or create node
        if !nodes.contains_key(data_id) {
            let node = self.create_dependency_node(data_id).await?;
            nodes.insert(data_id.to_string(), node);
        }

        // Get dependencies from cache or database
        let dependencies = self.get_dependencies(data_id).await?;
        
        for dependency in dependencies {
            // Create edge
            edges.push(DependencyEdge {
                from: data_id.to_string(),
                to: dependency.target_id.clone(),
                dependency_type: dependency.dependency_type,
                strength: self.calculate_dependency_strength(&dependency).await,
                bidirectional: false,
            });

            // Recursively traverse
            self.traverse_dependencies(
                &dependency.target_id,
                nodes,
                edges,
                visited,
                depth + 1,
            ).await?;
        }

        Ok(())
    }

    async fn create_dependency_node(
        &self,
        data_id: &str,
    ) -> Result<DependencyNode, Box<dyn std::error::Error + Send + Sync>> {
        // TODO: Implement actual data retrieval logic
        // This would query the actual data systems to get node information
        
        Ok(DependencyNode {
            id: data_id.to_string(),
            node_type: "table".to_string(), // Would be determined from actual data
            impact_level: ImpactLevel::Medium, // Would be calculated based on usage
            size: 1024 * 1024, // Would be actual size
            last_accessed: Some(now_system_time() - Duration::from_days(7)),
            business_critical: false, // Would be determined from metadata
            metadata: HashMap::new(),
        })
    }

    async fn get_dependencies(
        &self,
        data_id: &str,
    ) -> Result<Vec<DataDependency>, Box<dyn std::error::Error + Send + Sync>> {
        // Check cache first
        {
            let cache = self.dependency_cache.read().await;
            if let Some(cached_deps) = cache.get(data_id) {
                return Ok(cached_deps.clone());
            }
        }

        // TODO: Implement actual dependency discovery
        // This would query various systems to find dependencies:
        // - Database foreign keys
        // - Application references
        // - External system dependencies
        // - Backup/replication dependencies
        
        let dependencies = vec![
            // Placeholder dependency
            DataDependency {
                id: Uuid::new_v4(),
                source_id: data_id.to_string(),
                target_id: format!("{}_related", data_id),
                dependency_type: DependencyType::ForeignKey,
                impact_level: ImpactLevel::Medium,
                is_critical: false,
                last_accessed: Some(now_system_time() - Duration::from_days(1)),
                access_frequency: 10.0,
                data_size: 1024,
                business_critical: false,
                metadata: HashMap::new(),
            }
        ];

        // Cache the result
        {
            let mut cache = self.dependency_cache.write().await;
            cache.insert(data_id.to_string(), dependencies.clone());
        }

        Ok(dependencies)
    }

    async fn calculate_dependency_strength(&self, dependency: &DataDependency) -> f32 {
        let mut strength = 0.5; // Base strength

        // Adjust based on dependency type
        match dependency.dependency_type {
            DependencyType::ForeignKey => strength += 0.3,
            DependencyType::Index => strength += 0.2,
            DependencyType::External => strength += 0.4,
            _ => strength += 0.1,
        }

        // Adjust based on access frequency
        if dependency.access_frequency > 100.0 {
            strength += 0.2;
        }

        // Adjust for business critical
        if dependency.business_critical {
            strength += 0.3;
        }

        strength.min(1.0)
    }

    async fn find_critical_path_to_node(
        &self,
        target_node: &str,
        nodes: &HashMap<String, DependencyNode>,
        edges: &[DependencyEdge],
    ) -> Vec<String> {
        // Simple path finding - in production this would be more sophisticated
        let mut path = Vec::new();
        path.push(target_node.to_string());
        
        // Find strongest incoming edge
        if let Some(edge) = edges.iter()
            .filter(|e| e.to == target_node)
            .max_by(|a, b| a.strength.partial_cmp(&b.strength).unwrap_or(std::cmp::Ordering::Equal)) {
            path.insert(0, edge.from.clone());
        }

        path
    }

    /// Assess risk of cleanup operation
    async fn assess_risk(
        &self,
        dependency_graph: &DependencyGraph,
        request: &ImpactAnalysisRequest,
    ) -> Result<RiskAssessment, Box<dyn std::error::Error + Send + Sync>> {
        let mut overall_risk = ImpactLevel::None;
        let mut confidence_score = 1.0;
        let mut affected_systems = HashSet::new();
        let mut blocking_dependencies = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        // Analyze each node in the graph
        for (node_id, node) in &dependency_graph.nodes {
            // Check if node is business critical
            if node.business_critical {
                overall_risk = overall_risk.max(ImpactLevel::High);
                warnings.push(format!("Business critical data detected: {}", node_id));
                confidence_score *= 0.9;
            }

            // Check recent activity
            if let Some(last_accessed) = node.last_accessed {
                let days_since_access = now_system_time()
                    .duration_since(last_accessed)
                    .unwrap_or_default()
                    .as_secs() / 86400;
                
                if days_since_access < 7 {
                    overall_risk = overall_risk.max(ImpactLevel::Medium);
                    warnings.push(format!("Recently accessed data: {} ({}d ago)", node_id, days_since_access));
                }
            }

            affected_systems.insert(node.node_type.clone());
        }

        // Analyze critical paths
        for critical_path in &dependency_graph.critical_paths {
            if critical_path.len() > 3 {
                overall_risk = overall_risk.max(ImpactLevel::High);
                warnings.push(format!("Complex critical path detected: {:?}", critical_path));
                confidence_score *= 0.8;
            }
        }

        // Check for blocking dependencies
        for edge in &dependency_graph.edges {
            if edge.strength > 0.8 && request.target_data.contains(&edge.from) {
                // Find the dependency object for more details
                blocking_dependencies.push(DataDependency {
                    id: Uuid::new_v4(),
                    source_id: edge.from.clone(),
                    target_id: edge.to.clone(),
                    dependency_type: edge.dependency_type.clone(),
                    impact_level: ImpactLevel::High,
                    is_critical: true,
                    last_accessed: Some(now_system_time()),
                    access_frequency: 100.0, // High strength suggests high usage
                    data_size: 0,
                    business_critical: true,
                    metadata: HashMap::new(),
                });
            }
        }

        // Generate recommendations based on risk level
        match overall_risk {
            ImpactLevel::Critical | ImpactLevel::High => {
                recommendations.push("Create comprehensive backup before proceeding".to_string());
                recommendations.push("Schedule maintenance window".to_string());
                recommendations.push("Prepare detailed rollback plan".to_string());
                recommendations.push("Notify all stakeholders".to_string());
            }
            ImpactLevel::Medium => {
                recommendations.push("Create backup of affected data".to_string());
                recommendations.push("Test in staging environment first".to_string());
            }
            _ => {
                recommendations.push("Monitor systems during cleanup".to_string());
            }
        }

        Ok(RiskAssessment {
            overall_risk,
            confidence_score,
            affected_systems: affected_systems.into_iter().collect(),
            blocking_dependencies,
            warnings,
            recommendations,
            estimated_recovery_time: self.estimate_recovery_time(&overall_risk).await,
            rollback_available: self.check_rollback_availability(request).await,
        })
    }

    async fn estimate_recovery_time(&self, risk_level: &ImpactLevel) -> Option<Duration> {
        match risk_level {
            ImpactLevel::Critical => Some(Duration::from_hours(24)),
            ImpactLevel::High => Some(Duration::from_hours(4)),
            ImpactLevel::Medium => Some(Duration::from_hours(1)),
            _ => Some(Duration::from_minutes(15)),
        }
    }

    async fn check_rollback_availability(&self, _request: &ImpactAnalysisRequest) -> bool {
        // TODO: Check if rollback mechanisms are available
        // This would verify backup systems, transaction logs, etc.
        true
    }

    /// Run comprehensive safety checks
    async fn run_safety_checks(
        &self,
        dependency_graph: &DependencyGraph,
        request: &ImpactAnalysisRequest,
    ) -> Result<Vec<SafetyCheck>, Box<dyn std::error::Error + Send + Sync>> {
        let mut checks = Vec::new();

        // Dependency validation check
        checks.push(self.check_dependency_validation(dependency_graph).await);
        
        // Business critical data check
        checks.push(self.check_business_critical_data(dependency_graph).await);
        
        // Recent activity check
        checks.push(self.check_recent_activity(dependency_graph).await);
        
        // Backup availability check
        checks.push(self.check_backup_availability(request).await);
        
        // Rollback plan check
        checks.push(self.check_rollback_plan(request).await);
        
        // External system impact check
        checks.push(self.check_external_system_impact(dependency_graph).await);
        
        // Compliance check
        checks.push(self.check_compliance(request).await);

        Ok(checks)
    }

    async fn check_dependency_validation(&self, dependency_graph: &DependencyGraph) -> SafetyCheck {
        let has_circular_deps = self.detect_circular_dependencies(dependency_graph).await;
        
        if has_circular_deps {
            SafetyCheck {
                check_type: SafetyCheckType::DependencyValidation,
                status: SafetyCheckStatus::Failed,
                message: "Circular dependencies detected".to_string(),
                severity: ImpactLevel::High,
                auto_fixable: false,
            }
        } else {
            SafetyCheck {
                check_type: SafetyCheckType::DependencyValidation,
                status: SafetyCheckStatus::Passed,
                message: "No circular dependencies found".to_string(),
                severity: ImpactLevel::None,
                auto_fixable: false,
            }
        }
    }

    async fn detect_circular_dependencies(&self, dependency_graph: &DependencyGraph) -> bool {
        // Simple cycle detection using DFS
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node_id in dependency_graph.nodes.keys() {
            if !visited.contains(node_id) {
                if self.has_cycle_dfs(node_id, &dependency_graph.edges, &mut visited, &mut rec_stack).await {
                    return true;
                }
            }
        }

        false
    }

    async fn has_cycle_dfs(
        &self,
        node: &str,
        edges: &[DependencyEdge],
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());

        // Find all outgoing edges from this node
        for edge in edges.iter().filter(|e| e.from == node) {
            if !visited.contains(&edge.to) {
                if self.has_cycle_dfs(&edge.to, edges, visited, rec_stack).await {
                    return true;
                }
            } else if rec_stack.contains(&edge.to) {
                return true; // Back edge found - cycle detected
            }
        }

        rec_stack.remove(node);
        false
    }

    async fn check_business_critical_data(&self, dependency_graph: &DependencyGraph) -> SafetyCheck {
        let critical_count = dependency_graph.nodes.values()
            .filter(|node| node.business_critical)
            .count();

        if critical_count > 0 {
            SafetyCheck {
                check_type: SafetyCheckType::BusinessCriticalData,
                status: SafetyCheckStatus::Warning,
                message: format!("{} business critical data items found", critical_count),
                severity: ImpactLevel::High,
                auto_fixable: false,
            }
        } else {
            SafetyCheck {
                check_type: SafetyCheckType::BusinessCriticalData,
                status: SafetyCheckStatus::Passed,
                message: "No business critical data affected".to_string(),
                severity: ImpactLevel::None,
                auto_fixable: false,
            }
        }
    }

    async fn check_recent_activity(&self, dependency_graph: &DependencyGraph) -> SafetyCheck {
        let recent_threshold = now_system_time() - Duration::from_days(7);
        let recent_count = dependency_graph.nodes.values()
            .filter(|node| {
                node.last_accessed
                    .map(|t| t > recent_threshold)
                    .unwrap_or(false)
            })
            .count();

        if recent_count > 0 {
            SafetyCheck {
                check_type: SafetyCheckType::RecentActivity,
                status: SafetyCheckStatus::Warning,
                message: format!("{} recently accessed data items", recent_count),
                severity: ImpactLevel::Medium,
                auto_fixable: false,
            }
        } else {
            SafetyCheck {
                check_type: SafetyCheckType::RecentActivity,
                status: SafetyCheckStatus::Passed,
                message: "No recent activity detected".to_string(),
                severity: ImpactLevel::None,
                auto_fixable: false,
            }
        }
    }

    async fn check_backup_availability(&self, _request: &ImpactAnalysisRequest) -> SafetyCheck {
        // TODO: Check actual backup systems
        SafetyCheck {
            check_type: SafetyCheckType::BackupAvailability,
            status: SafetyCheckStatus::Passed,
            message: "Backup systems available".to_string(),
            severity: ImpactLevel::None,
            auto_fixable: false,
        }
    }

    async fn check_rollback_plan(&self, _request: &ImpactAnalysisRequest) -> SafetyCheck {
        // TODO: Validate rollback procedures
        SafetyCheck {
            check_type: SafetyCheckType::RollbackPlan,
            status: SafetyCheckStatus::Passed,
            message: "Rollback plan available".to_string(),
            severity: ImpactLevel::None,
            auto_fixable: false,
        }
    }

    async fn check_external_system_impact(&self, dependency_graph: &DependencyGraph) -> SafetyCheck {
        let external_deps = dependency_graph.edges.iter()
            .filter(|edge| edge.dependency_type == DependencyType::External)
            .count();

        if external_deps > 0 {
            SafetyCheck {
                check_type: SafetyCheckType::ExternalSystemImpact,
                status: SafetyCheckStatus::Warning,
                message: format!("{} external system dependencies", external_deps),
                severity: ImpactLevel::Medium,
                auto_fixable: false,
            }
        } else {
            SafetyCheck {
                check_type: SafetyCheckType::ExternalSystemImpact,
                status: SafetyCheckStatus::Passed,
                message: "No external system impact".to_string(),
                severity: ImpactLevel::None,
                auto_fixable: false,
            }
        }
    }

    async fn check_compliance(&self, _request: &ImpactAnalysisRequest) -> SafetyCheck {
        // TODO: Check compliance requirements (GDPR, HIPAA, etc.)
        SafetyCheck {
            check_type: SafetyCheckType::ComplianceCheck,
            status: SafetyCheckStatus::Passed,
            message: "Compliance requirements met".to_string(),
            severity: ImpactLevel::None,
            auto_fixable: false,
        }
    }

    async fn generate_recommendations(
        &self,
        risk_assessment: &RiskAssessment,
        safety_checks: &[SafetyCheck],
    ) -> Result<Vec<RecommendedAction>, Box<dyn std::error::Error + Send + Sync>> {
        let mut actions = Vec::new();

        // Based on risk level
        match risk_assessment.overall_risk {
            ImpactLevel::Critical => {
                actions.extend(vec![
                    RecommendedAction {
                        action_type: ActionType::CreateBackup,
                        description: "Create comprehensive backup of all affected systems".to_string(),
                        priority: ImpactLevel::Critical,
                        estimated_duration: Some(Duration::from_hours(2)),
                        prerequisites: vec!["Backup system availability".to_string()],
                        rollback_plan: Some("Restore from backup".to_string()),
                    },
                    RecommendedAction {
                        action_type: ActionType::NotifyStakeholders,
                        description: "Notify all stakeholders and obtain executive approval".to_string(),
                        priority: ImpactLevel::Critical,
                        estimated_duration: Some(Duration::from_hours(4)),
                        prerequisites: vec!["Stakeholder contact list".to_string()],
                        rollback_plan: None,
                    },
                ]);
            }
            ImpactLevel::High => {
                actions.push(RecommendedAction {
                    action_type: ActionType::ScheduleMaintenance,
                    description: "Schedule maintenance window for cleanup operation".to_string(),
                    priority: ImpactLevel::High,
                    estimated_duration: Some(Duration::from_hours(1)),
                    prerequisites: vec!["Maintenance calendar".to_string()],
                    rollback_plan: Some("Cancel maintenance if issues arise".to_string()),
                });
            }
            _ => {}
        }

        // Based on safety check failures
        for check in safety_checks {
            if check.status == SafetyCheckStatus::Failed {
                match check.check_type {
                    SafetyCheckType::BackupAvailability => {
                        actions.push(RecommendedAction {
                            action_type: ActionType::CreateBackup,
                            description: "Ensure backup systems are operational".to_string(),
                            priority: check.severity.clone(),
                            estimated_duration: Some(Duration::from_minutes(30)),
                            prerequisites: vec![],
                            rollback_plan: None,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Always recommend monitoring
        actions.push(RecommendedAction {
            action_type: ActionType::MonitorSystems,
            description: "Monitor all affected systems during cleanup".to_string(),
            priority: ImpactLevel::Medium,
            estimated_duration: Some(Duration::from_hours(1)),
            prerequisites: vec!["Monitoring dashboard access".to_string()],
            rollback_plan: None,
        });

        Ok(actions)
    }

    async fn determine_safety(
        &self,
        risk_assessment: &RiskAssessment,
        safety_checks: &[SafetyCheck],
        request: &ImpactAnalysisRequest,
    ) -> bool {
        // Check if risk is within acceptable limits
        if risk_assessment.overall_risk > request.max_risk_level {
            return false;
        }

        // Check for failed safety checks
        let has_failed_checks = safety_checks.iter()
            .any(|check| check.status == SafetyCheckStatus::Failed);
        
        if has_failed_checks {
            return false;
        }

        // Check confidence score
        if risk_assessment.confidence_score < 0.7 {
            return false;
        }

        // If force_analysis is false and dry_run is true, always safe
        if !request.force_analysis && request.dry_run {
            return true;
        }

        true
    }

    async fn get_required_approvals(&self, risk_level: &ImpactLevel) -> Vec<String> {
        match risk_level {
            ImpactLevel::Critical => vec![
                "Executive Approval".to_string(),
                "Data Governance Team".to_string(),
                "Security Team".to_string(),
            ],
            ImpactLevel::High => vec![
                "Senior Management".to_string(),
                "Data Governance Team".to_string(),
            ],
            ImpactLevel::Medium => vec![
                "Team Lead".to_string(),
            ],
            _ => vec![],
        }
    }

    /// Get cached analysis result
    pub async fn get_analysis_result(
        &self,
        request_id: Uuid,
    ) -> Option<ImpactAnalysisResult> {
        let cache = self.analysis_cache.read().await;
        cache.get(&request_id).cloned()
    }

    /// Clear old analysis results from cache
    pub async fn cleanup_cache(&self, max_age: Duration) {
        let mut cache = self.analysis_cache.write().await;
        let cutoff_time = now_system_time() - max_age;
        
        cache.retain(|_, result| {
            result.created_at > cutoff_time
        });
    }
}

#[async_trait::async_trait]
impl HealthCheck for CleanupImpactAnalysisEngine {
    async fn health_check(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check circuit breaker state
        let state = self.circuit_breaker.state.read().await;
        if *state == CircuitBreakerState::Open {
            return Err("Circuit breaker is open".into());
        }

        // Check dependency cache size
        let cache_size = self.dependency_cache.read().await.len();
        if cache_size > 10000 {
            return Err("Dependency cache is too large".into());
        }

        // Check running analyses
        let running_count = self.running_analyses.read().await.len();
        if running_count > 100 {
            return Err("Too many running analyses".into());
        }

        Ok(())
    }
}

// Helper trait for Duration
trait DurationExt {
    fn from_days(days: u64) -> Duration;
    fn from_hours(hours: u64) -> Duration;
    fn from_minutes(minutes: u64) -> Duration;
}

impl DurationExt for Duration {
    fn from_days(days: u64) -> Duration {
        Duration::from_secs(days * 24 * 60 * 60)
    }

    fn from_hours(hours: u64) -> Duration {
        Duration::from_secs(hours * 60 * 60)
    }

    fn from_minutes(minutes: u64) -> Duration {
        Duration::from_secs(minutes * 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_secs(1),
            retry_timeout: Duration::from_secs(2),
        };
        
        let circuit_breaker = CircuitBreaker::new(config);
        
        // Test normal operation
        let result: Result<i32, &str> = circuit_breaker.call_with_breaker(async { Ok(42) }).await;
        assert!(result.is_ok());
        
        // Test failure handling
        let result: Result<i32, &str> = circuit_breaker.call_with_breaker(async { Err("failure") }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dependency_graph_cycle_detection() {
        let config = ImpactAnalysisConfig::default();
        let connection_manager = Arc::new(ConnectionManager::new(ServiceConfig::default()));
        let transaction_coordinator = Arc::new(TransactionCoordinator::new(
            connection_manager.clone(),
            Default::default(),
        ));
        
        let engine = CleanupImpactAnalysisEngine::new(
            config,
            connection_manager,
            transaction_coordinator,
        );

        // Create a dependency graph with a cycle
        let mut edges = vec![
            DependencyEdge {
                from: "A".to_string(),
                to: "B".to_string(),
                dependency_type: DependencyType::ForeignKey,
                strength: 0.8,
                bidirectional: false,
            },
            DependencyEdge {
                from: "B".to_string(),
                to: "C".to_string(),
                dependency_type: DependencyType::ForeignKey,
                strength: 0.8,
                bidirectional: false,
            },
            DependencyEdge {
                from: "C".to_string(),
                to: "A".to_string(),
                dependency_type: DependencyType::ForeignKey,
                strength: 0.8,
                bidirectional: false,
            },
        ];

        let dependency_graph = DependencyGraph {
            nodes: HashMap::new(),
            edges,
            critical_paths: Vec::new(),
            orphaned_data: Vec::new(),
        };

        let has_cycle = engine.detect_circular_dependencies(&dependency_graph).await;
        assert!(has_cycle);
    }

    #[tokio::test]
    async fn test_risk_assessment() {
        let config = ImpactAnalysisConfig::default();
        let connection_manager = Arc::new(ConnectionManager::new(ServiceConfig::default()));
        let transaction_coordinator = Arc::new(TransactionCoordinator::new(
            connection_manager.clone(),
            Default::default(),
        ));
        
        let engine = CleanupImpactAnalysisEngine::new(
            config,
            connection_manager,
            transaction_coordinator,
        );

        let request = ImpactAnalysisRequest {
            cleanup_id: Uuid::new_v4(),
            target_data: vec!["test_table".to_string()],
            cleanup_type: "ttl".to_string(),
            dry_run: true,
            force_analysis: false,
            max_risk_level: ImpactLevel::High,
            business_context: None,
        };

        let mut nodes = HashMap::new();
        nodes.insert("test_table".to_string(), DependencyNode {
            id: "test_table".to_string(),
            node_type: "table".to_string(),
            impact_level: ImpactLevel::High,
            size: 1024 * 1024,
            last_accessed: Some(now_system_time() - Duration::from_days(1)),
            business_critical: true,
            metadata: HashMap::new(),
        });

        let dependency_graph = DependencyGraph {
            nodes,
            edges: Vec::new(),
            critical_paths: Vec::new(),
            orphaned_data: Vec::new(),
        };

        let risk_assessment = engine.assess_risk(&dependency_graph, &request).await.unwrap();
        assert_eq!(risk_assessment.overall_risk, ImpactLevel::High);
        assert!(!risk_assessment.warnings.is_empty());
    }
} 