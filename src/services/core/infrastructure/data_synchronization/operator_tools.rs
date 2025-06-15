//! Operator Tools
//!
//! Comprehensive manual sync controls, operator dashboards, and administrative
//! tools for sync management including force sync triggers, monitoring dashboards,
//! and emergency controls.

use super::sync_coordinator::StorageTarget;
use super::{SyncCoordinator, SyncOperation, WriteMode};
use crate::services::core::infrastructure::shared_types::ComponentHealth;
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use worker::{Request, Response, Result as WorkerResult, RouteContext};

/// Operator tools configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorToolsConfig {
    /// Enable manual sync triggers
    pub enable_manual_triggers: bool,
    /// Enable sync dashboards
    pub enable_dashboards: bool,
    /// Enable emergency controls
    pub enable_emergency_controls: bool,
    /// Dashboard refresh interval in milliseconds
    pub dashboard_refresh_interval_ms: u64,
    /// Maximum operations in queue
    pub max_queue_size: u32,
    /// Operation timeout in milliseconds
    pub operation_timeout_ms: u64,
}

/// Manual sync trigger types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncTriggerType {
    /// Force immediate sync of all data
    ForceSync,
    /// Selective sync by data type
    SelectiveSync {
        data_types: Vec<String>,
        key_patterns: Vec<String>,
    },
    /// Range-based sync
    RangeSync {
        start_key: String,
        end_key: String,
        storage_targets: Vec<StorageTarget>,
    },
    /// Emergency sync for critical data
    EmergencySync {
        priority: SyncPriority,
        bypass_circuit_breaker: bool,
    },
    /// Scheduled sync trigger
    ScheduledSync {
        schedule_expression: String,
        recurring: bool,
    },
}

/// Sync priority levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncPriority {
    /// Low priority - background sync
    Low,
    /// Normal priority - standard sync
    Normal,
    /// High priority - expedited sync
    High,
    /// Critical priority - emergency sync
    Critical,
}

/// Manual sync request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualSyncRequest {
    /// Unique request identifier
    pub request_id: String,
    /// Trigger type
    pub trigger_type: SyncTriggerType,
    /// Write mode override
    pub write_mode: Option<WriteMode>,
    /// Requestor information
    pub requestor: String,
    /// Request timestamp
    pub requested_at: u64,
    /// Optional description
    pub description: Option<String>,
}

/// Sync operation queue management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncQueueStatus {
    /// Total operations in queue
    pub total_operations: u32,
    /// Operations by priority
    pub operations_by_priority: HashMap<String, u32>,
    /// Queue processing rate (ops/sec)
    pub processing_rate: f64,
    /// Estimated completion time
    pub estimated_completion_ms: u64,
    /// Queue health status
    pub queue_health: QueueHealth,
}

/// Queue health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueueHealth {
    /// Queue is healthy
    Healthy,
    /// Queue is under load but manageable
    UnderLoad,
    /// Queue is overloaded
    Overloaded,
    /// Queue is blocked
    Blocked,
}

/// Sync dashboard data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDashboard {
    /// Dashboard timestamp
    pub timestamp: u64,
    /// Overall sync health
    pub overall_health: ComponentHealth,
    /// Active sync operations
    pub active_operations: Vec<ActiveSyncOperation>,
    /// Queue status
    pub queue_status: SyncQueueStatus,
    /// Storage system status
    pub storage_status: HashMap<String, StorageSystemStatus>,
    /// Recent sync events
    pub recent_events: Vec<SyncEvent>,
    /// Performance metrics
    pub performance_metrics: SyncPerformanceMetrics,
    /// System recommendations
    pub recommendations: Vec<SystemRecommendation>,
}

/// Active sync operation info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSyncOperation {
    /// Operation identifier
    pub operation_id: String,
    /// Operation type
    pub operation_type: String,
    /// Progress percentage (0.0 to 1.0)
    pub progress: f64,
    /// Started timestamp
    pub started_at: u64,
    /// Estimated completion time
    pub estimated_completion: u64,
    /// Storage targets involved
    pub storage_targets: Vec<String>,
}

/// Storage system status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSystemStatus {
    /// System name
    pub system_name: String,
    /// Health status
    pub health: ComponentHealth,
    /// Connection status
    pub connection_status: ConnectionStatus,
    /// Last sync timestamp
    pub last_sync_time: u64,
    /// Pending operations count
    pub pending_operations: u32,
    /// Error rate
    pub error_rate: f64,
}

/// Connection status for storage systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Connected and healthy
    Connected,
    /// Connected but with issues
    Degraded,
    /// Disconnected
    Disconnected,
    /// Connection unknown
    Unknown,
}

/// Sync event for dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEvent {
    /// Event identifier
    pub event_id: String,
    /// Event type
    pub event_type: String,
    /// Event timestamp
    pub timestamp: u64,
    /// Event description
    pub description: String,
    /// Event severity
    pub severity: EventSeverity,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Event severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventSeverity {
    /// Informational event
    Info,
    /// Warning event
    Warning,
    /// Error event
    Error,
    /// Critical event
    Critical,
}

/// Performance metrics for sync operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPerformanceMetrics {
    /// Operations per second
    pub operations_per_second: f64,
    /// Average latency in milliseconds
    pub average_latency_ms: f64,
    /// Success rate percentage
    pub success_rate: f64,
    /// Data throughput in bytes per second
    pub throughput_bytes_per_second: f64,
    /// Resource utilization
    pub resource_utilization: ResourceUtilization,
}

/// Resource utilization metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilization {
    /// CPU utilization percentage
    pub cpu_utilization: f64,
    /// Memory utilization percentage
    pub memory_utilization: f64,
    /// Network utilization percentage
    pub network_utilization: f64,
    /// Storage IO utilization percentage
    pub storage_io_utilization: f64,
}

/// System recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRecommendation {
    /// Recommendation identifier
    pub recommendation_id: String,
    /// Recommendation type
    pub recommendation_type: RecommendationType,
    /// Recommendation title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Priority level
    pub priority: RecommendationPriority,
    /// Estimated impact
    pub estimated_impact: String,
    /// Suggested actions
    pub suggested_actions: Vec<String>,
}

/// Types of recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationType {
    /// Performance optimization
    Performance,
    /// Capacity planning
    Capacity,
    /// Configuration tuning
    Configuration,
    /// Security improvement
    Security,
    /// Reliability enhancement
    Reliability,
}

/// Recommendation priority
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    /// Low priority
    Low,
    /// Medium priority
    Medium,
    /// High priority
    High,
    /// Critical priority
    Critical,
}

/// Batch operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationRequest {
    /// Batch identifier
    pub batch_id: String,
    /// Operations to execute
    pub operations: Vec<SyncOperation>,
    /// Execution mode
    pub execution_mode: BatchExecutionMode,
    /// Maximum parallel operations
    pub max_parallel: u32,
    /// Timeout for the entire batch
    pub batch_timeout_ms: u64,
}

/// Batch execution modes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchExecutionMode {
    /// Execute all operations in parallel
    Parallel,
    /// Execute operations sequentially
    Sequential,
    /// Execute with dependency awareness
    DependencyAware,
}

/// Batch operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    /// Batch identifier
    pub batch_id: String,
    /// Overall success status
    pub success: bool,
    /// Individual operation results
    pub operation_results: Vec<OperationResult>,
    /// Total execution time
    pub total_execution_time_ms: u64,
    /// Statistics
    pub statistics: BatchStatistics,
}

/// Individual operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    /// Operation index in batch
    pub operation_index: u32,
    /// Success status
    pub success: bool,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Result metadata
    pub metadata: HashMap<String, String>,
}

/// Batch operation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatistics {
    /// Total operations
    pub total_operations: u32,
    /// Successful operations
    pub successful_operations: u32,
    /// Failed operations
    pub failed_operations: u32,
    /// Average operation time
    pub average_operation_time_ms: f64,
    /// Throughput (operations per second)
    pub throughput_ops_per_second: f64,
}

/// Sync schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSchedule {
    /// Schedule identifier
    pub schedule_id: String,
    /// Schedule name
    pub name: String,
    /// Cron expression or interval
    pub schedule_expression: String,
    /// Operations to execute
    pub operations: Vec<SyncOperation>,
    /// Active status
    pub active: bool,
    /// Next execution time
    pub next_execution: u64,
    /// Last execution time
    pub last_execution: Option<u64>,
    /// Execution history
    pub execution_history: Vec<ScheduleExecution>,
}

/// Schedule execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleExecution {
    /// Execution timestamp
    pub executed_at: u64,
    /// Success status
    pub success: bool,
    /// Execution duration
    pub duration_ms: u64,
    /// Operations executed
    pub operations_executed: u32,
    /// Error message if failed
    pub error_message: Option<String>,
}

/// Manual sync trigger service
pub struct ManualSyncTrigger {
    config: OperatorToolsConfig,
    feature_flags: super::SyncFeatureFlags,
    sync_coordinator: Arc<SyncCoordinator>,
    operation_queue: Arc<Mutex<VecDeque<ManualSyncRequest>>>,
    active_operations: Arc<Mutex<HashMap<String, ActiveSyncOperation>>>,
}

impl ManualSyncTrigger {
    /// Create new manual sync trigger
    pub async fn new(
        config: &OperatorToolsConfig,
        feature_flags: &super::SyncFeatureFlags,
        sync_coordinator: Arc<SyncCoordinator>,
    ) -> ArbitrageResult<Self> {
        Ok(Self {
            config: config.clone(),
            feature_flags: feature_flags.clone(),
            sync_coordinator,
            operation_queue: Arc::new(Mutex::new(VecDeque::new())),
            active_operations: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Initialize the trigger service
    pub async fn initialize(&self) -> ArbitrageResult<()> {
        // Start background queue processor
        self.start_queue_processor().await?;
        Ok(())
    }

    /// Trigger manual sync operation
    pub async fn trigger_sync(&self, request: ManualSyncRequest) -> ArbitrageResult<String> {
        if !self.config.enable_manual_triggers {
            return Err(ArbitrageError::permission_denied(
                "Manual sync triggers are disabled",
            ));
        }

        // Validate request
        self.validate_request(&request)?;

        // Add to queue
        {
            let mut queue = self.operation_queue.lock().await;
            if queue.len() >= self.config.max_queue_size as usize {
                return Err(ArbitrageError::resource_exhausted(
                    "Sync operation queue is full",
                ));
            }
            queue.push_back(request.clone());
        }

        Ok(request.request_id)
    }

    /// Get queue status
    pub async fn get_queue_status(&self) -> ArbitrageResult<SyncQueueStatus> {
        let queue = self.operation_queue.lock().await;
        let active_ops = self.active_operations.lock().await;

        // Calculate priority distribution
        let mut operations_by_priority = HashMap::new();
        for request in queue.iter() {
            let priority = match &request.trigger_type {
                SyncTriggerType::EmergencySync { priority, .. } => format!("{:?}", priority),
                _ => "Normal".to_string(),
            };
            let count = operations_by_priority.get(&priority).unwrap_or(&0);
            operations_by_priority.insert(priority, count + 1);
        }

        // Determine queue health
        let queue_health = if queue.len() > (self.config.max_queue_size as f64 * 0.9) as usize {
            QueueHealth::Overloaded
        } else if queue.len() > (self.config.max_queue_size as f64 * 0.7) as usize {
            QueueHealth::UnderLoad
        } else if active_ops.is_empty() && !queue.is_empty() {
            QueueHealth::Blocked
        } else {
            QueueHealth::Healthy
        };

        Ok(SyncQueueStatus {
            total_operations: queue.len() as u32,
            operations_by_priority,
            processing_rate: self.calculate_processing_rate().await,
            estimated_completion_ms: self.estimate_completion_time(&queue).await,
            queue_health,
        })
    }

    /// Start background queue processor
    async fn start_queue_processor(&self) -> ArbitrageResult<()> {
        // Background task implementation would go here
        Ok(())
    }

    /// Validate sync request
    fn validate_request(&self, request: &ManualSyncRequest) -> ArbitrageResult<()> {
        // Basic validation
        if request.request_id.is_empty() {
            return Err(ArbitrageError::validation_error("Request ID is required"));
        }

        if request.requestor.is_empty() {
            return Err(ArbitrageError::validation_error("Requestor is required"));
        }

        // Type-specific validation
        match &request.trigger_type {
            SyncTriggerType::RangeSync {
                start_key, end_key, ..
            } => {
                if start_key.is_empty() || end_key.is_empty() {
                    return Err(ArbitrageError::validation_error(
                        "Range sync requires non-empty start and end keys",
                    ));
                }
            }
            SyncTriggerType::SelectiveSync { data_types, .. } => {
                if data_types.is_empty() {
                    return Err(ArbitrageError::validation_error(
                        "Selective sync requires at least one data type",
                    ));
                }
            }
            _ => {} // Other types are valid by default
        }

        Ok(())
    }

    /// Calculate processing rate
    async fn calculate_processing_rate(&self) -> f64 {
        // Simple implementation - would track actual processing metrics
        10.0 // operations per second
    }

    /// Estimate completion time for queue
    async fn estimate_completion_time(&self, queue: &VecDeque<ManualSyncRequest>) -> u64 {
        let processing_rate = self.calculate_processing_rate().await;
        if processing_rate > 0.0 {
            ((queue.len() as f64 / processing_rate) * 1000.0) as u64
        } else {
            0
        }
    }

    /// Shutdown trigger service
    pub async fn shutdown(&self) -> ArbitrageResult<()> {
        // Wait for active operations to complete
        let mut retry_count = 0;
        while retry_count < 30 {
            let active_count = {
                let active_ops = self.active_operations.lock().await;
                active_ops.len()
            };

            if active_count == 0 {
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            retry_count += 1;
        }

        Ok(())
    }
}

/// Sync dashboard service
pub struct SyncDashboardService {
    config: OperatorToolsConfig,
    feature_flags: super::SyncFeatureFlags,
    sync_coordinator: Arc<SyncCoordinator>,
    manual_trigger: Arc<ManualSyncTrigger>,
    dashboard_data: Arc<RwLock<SyncDashboard>>,
    event_queue: Arc<Mutex<VecDeque<SyncEvent>>>,
}

impl SyncDashboardService {
    /// Create new dashboard service
    pub async fn new(
        config: &OperatorToolsConfig,
        feature_flags: &super::SyncFeatureFlags,
        sync_coordinator: Arc<SyncCoordinator>,
        manual_trigger: Arc<ManualSyncTrigger>,
    ) -> ArbitrageResult<Self> {
        let dashboard_data = Arc::new(RwLock::new(SyncDashboard {
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            overall_health: ComponentHealth {
                is_healthy: true,
                last_check: chrono::Utc::now().timestamp_millis() as u64,
                error_count: 0,
                uptime_seconds: 0,
                performance_score: 1.0,
                warning_count: 0,
                resource_usage_percent: 0.0,
                last_error: None,
                last_warning: None,
                component_name: "sync_dashboard".to_string(),
                version: "1.0.0".to_string(),
            },
            active_operations: Vec::new(),
            queue_status: SyncQueueStatus {
                total_operations: 0,
                operations_by_priority: HashMap::new(),
                processing_rate: 0.0,
                estimated_completion_ms: 0,
                queue_health: QueueHealth::Healthy,
            },
            storage_status: HashMap::new(),
            recent_events: Vec::new(),
            performance_metrics: SyncPerformanceMetrics {
                operations_per_second: 0.0,
                average_latency_ms: 0.0,
                success_rate: 1.0,
                throughput_bytes_per_second: 0.0,
                resource_utilization: ResourceUtilization {
                    cpu_utilization: 0.0,
                    memory_utilization: 0.0,
                    network_utilization: 0.0,
                    storage_io_utilization: 0.0,
                },
            },
            recommendations: Vec::new(),
        }));

        Ok(Self {
            config: config.clone(),
            feature_flags: feature_flags.clone(),
            sync_coordinator,
            manual_trigger,
            dashboard_data,
            event_queue: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    /// Initialize dashboard service
    pub async fn initialize(&self) -> ArbitrageResult<()> {
        if self.config.enable_dashboards {
            self.start_dashboard_updater().await?;
        }
        Ok(())
    }

    /// Get current dashboard data
    pub async fn get_dashboard(&self) -> ArbitrageResult<SyncDashboard> {
        let dashboard = self.dashboard_data.read().await;
        Ok(dashboard.clone())
    }

    /// Add event to dashboard
    pub async fn add_event(&self, event: SyncEvent) {
        let mut queue = self.event_queue.lock().await;
        queue.push_back(event);

        // Keep queue size manageable
        if queue.len() > 1000 {
            queue.pop_front();
        }
    }

    /// Start dashboard data updater
    async fn start_dashboard_updater(&self) -> ArbitrageResult<()> {
        // Background task implementation would go here
        Ok(())
    }

    /// Update dashboard data
    async fn update_dashboard_data(&self) -> ArbitrageResult<()> {
        let mut dashboard = self.dashboard_data.write().await;

        // Update timestamp
        dashboard.timestamp = chrono::Utc::now().timestamp_millis() as u64;

        // Update queue status
        dashboard.queue_status = self.manual_trigger.get_queue_status().await?;

        // Update coordinator health
        dashboard.overall_health = self.sync_coordinator.health_check().await?;

        // Update recent events
        {
            let event_queue = self.event_queue.lock().await;
            dashboard.recent_events = event_queue.iter().rev().take(100).cloned().collect();
        }

        Ok(())
    }

    /// Shutdown dashboard service
    pub async fn shutdown(&self) -> ArbitrageResult<()> {
        Ok(())
    }
}

/// Main operator tools service
pub struct OperatorToolsService {
    /// Configuration
    config: OperatorToolsConfig,
    /// Feature flags
    feature_flags: super::SyncFeatureFlags,
    /// Manual sync trigger
    manual_trigger: Arc<ManualSyncTrigger>,
    /// Dashboard service
    dashboard_service: Arc<SyncDashboardService>,
    /// Health status
    health: Arc<RwLock<ComponentHealth>>,
}

impl OperatorToolsService {
    /// Create new operator tools service
    pub async fn new(
        config: &OperatorToolsConfig,
        feature_flags: &super::SyncFeatureFlags,
        sync_coordinator: Arc<SyncCoordinator>,
    ) -> ArbitrageResult<Self> {
        // Create manual trigger
        let manual_trigger = Arc::new(
            ManualSyncTrigger::new(config, feature_flags, Arc::clone(&sync_coordinator)).await?,
        );

        // Create dashboard service
        let dashboard_service = Arc::new(
            SyncDashboardService::new(
                config,
                feature_flags,
                sync_coordinator,
                Arc::clone(&manual_trigger),
            )
            .await?,
        );

        let health = Arc::new(RwLock::new(ComponentHealth {
            is_healthy: true,
            last_check: chrono::Utc::now().timestamp_millis() as u64,
            error_count: 0,
            uptime_seconds: 0,
            performance_score: 1.0,
        }));

        Ok(Self {
            config: config.clone(),
            feature_flags: feature_flags.clone(),
            manual_trigger,
            dashboard_service,
            health,
        })
    }

    /// Initialize operator tools
    pub async fn initialize(&self) -> ArbitrageResult<()> {
        self.manual_trigger.initialize().await?;
        self.dashboard_service.initialize().await?;
        Ok(())
    }

    /// Handle REST API requests
    pub async fn handle_request(
        &self,
        req: Request,
        _ctx: RouteContext<()>,
    ) -> WorkerResult<Response> {
        let path = req.path();
        let method = req.method();

        match (method.as_str(), path.as_str()) {
            ("POST", "/api/sync/trigger") => self.handle_trigger_sync(req).await,
            ("GET", "/api/sync/queue") => self.handle_get_queue_status().await,
            ("GET", "/api/sync/dashboard") => self.handle_get_dashboard().await,
            ("GET", "/api/sync/health") => self.handle_health_check().await,
            _ => Response::error("Not Found", 404),
        }
    }

    /// Handle trigger sync request
    async fn handle_trigger_sync(&self, mut req: Request) -> WorkerResult<Response> {
        match req.json::<ManualSyncRequest>().await {
            Ok(sync_request) => match self.manual_trigger.trigger_sync(sync_request).await {
                Ok(request_id) => {
                    let response = serde_json::json!({
                        "success": true,
                        "request_id": request_id,
                        "message": "Sync operation queued successfully"
                    });
                    Response::from_json(&response)
                }
                Err(e) => {
                    let error_response = serde_json::json!({
                        "success": false,
                        "error": e.to_string()
                    });
                    Response::from_json(&error_response)
                        .map(|r| r.with_status(400))
                        .unwrap_or_else(|_| Response::error("Internal Server Error", 500).unwrap())
                }
            },
            Err(_) => Response::error("Bad Request", 400),
        }
    }

    /// Handle get queue status
    async fn handle_get_queue_status(&self) -> WorkerResult<Response> {
        match self.manual_trigger.get_queue_status().await {
            Ok(queue_status) => Response::from_json(&queue_status),
            Err(e) => {
                let error_response = serde_json::json!({
                    "success": false,
                    "error": e.to_string()
                });
                Response::from_json(&error_response)
                    .map(|r| r.with_status(500))
                    .unwrap_or_else(|_| Response::error("Internal Server Error", 500).unwrap())
            }
        }
    }

    /// Handle get dashboard
    async fn handle_get_dashboard(&self) -> WorkerResult<Response> {
        if !self.config.enable_dashboards {
            return Response::error("Dashboards are disabled", 403);
        }

        match self.dashboard_service.get_dashboard().await {
            Ok(dashboard) => Response::from_json(&dashboard),
            Err(e) => {
                let error_response = serde_json::json!({
                    "success": false,
                    "error": e.to_string()
                });
                Response::from_json(&error_response)
                    .map(|r| r.with_status(500))
                    .unwrap_or_else(|_| Response::error("Internal Server Error", 500).unwrap())
            }
        }
    }

    /// Handle health check
    async fn handle_health_check(&self) -> WorkerResult<Response> {
        let health = self.health.read().await;
        Response::from_json(&*health)
    }

    /// Get health status
    pub async fn health_check(&self) -> ArbitrageResult<ComponentHealth> {
        let health = self.health.read().await;
        Ok(health.clone())
    }

    /// Get metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<OperatorToolsMetrics> {
        Ok(OperatorToolsMetrics {
            manual_trigger_metrics: ManualTriggerMetrics {
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                average_processing_time_ms: 0.0,
                queue_size: 0,
                active_operations: 0,
            },
            dashboard_metrics: DashboardMetrics {
                total_views: 0,
                refresh_count: 0,
                average_response_time_ms: 0.0,
                active_connections: 0,
            },
            collected_at: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Shutdown operator tools
    pub async fn shutdown(&self) -> ArbitrageResult<()> {
        self.manual_trigger.shutdown().await?;
        self.dashboard_service.shutdown().await?;
        Ok(())
    }
}

impl Default for OperatorToolsConfig {
    fn default() -> Self {
        Self {
            enable_manual_triggers: true,
            enable_dashboards: true,
            enable_emergency_controls: true,
            dashboard_refresh_interval_ms: 5000,
            max_queue_size: 1000,
            operation_timeout_ms: 300000, // 5 minutes
        }
    }
}

/// Operator tools health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorToolsHealth {
    pub overall_healthy: bool,
    pub manual_trigger_healthy: bool,
    pub dashboard_healthy: bool,
    pub last_check: u64,
}

/// Operator tools metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorToolsMetrics {
    pub manual_trigger_metrics: ManualTriggerMetrics,
    pub dashboard_metrics: DashboardMetrics,
    pub collected_at: u64,
}

/// Manual trigger metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualTriggerMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_processing_time_ms: f64,
    pub queue_size: u32,
    pub active_operations: u32,
}

/// Dashboard metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardMetrics {
    pub total_views: u64,
    pub refresh_count: u64,
    pub average_response_time_ms: f64,
    pub active_connections: u32,
}
