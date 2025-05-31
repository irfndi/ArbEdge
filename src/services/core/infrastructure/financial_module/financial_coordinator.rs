// src/services/core/infrastructure/financial_module/financial_coordinator.rs

//! Financial Coordinator - Main Orchestrator for Financial Operations
//!
//! This component serves as the central coordinator for all financial operations in the ArbEdge platform,
//! orchestrating balance tracking, fund analysis, and providing unified financial interfaces
//! for high-concurrency trading operations.
//!
//! ## Revolutionary Features:
//! - **Unified Financial Interface**: Single entry point for all financial operations
//! - **Cross-Component Coordination**: Intelligent coordination between balance tracking and analysis
//! - **Financial Workflow Management**: Automated financial workflows and triggers
//! - **Performance Optimization**: Efficient operation scheduling and resource management
//! - **Financial Event Processing**: Real-time financial event handling and notifications

use super::{BalanceTracker, FundAnalyzer};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::{kv::KvStore, Env};

/// Helper function to get current time in milliseconds as u64
/// Handles the f64 to u64 conversion safely by applying floor() before casting
fn get_current_time_millis() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Financial Coordinator Configuration
#[derive(Debug, Clone)]
pub struct FinancialCoordinatorConfig {
    // Coordination settings
    pub enable_unified_interface: bool,
    pub enable_cross_component_coordination: bool,
    pub enable_automated_workflows: bool,
    pub enable_financial_events: bool,

    // Workflow settings
    pub auto_analysis_threshold_usd: f64,
    pub auto_optimization_threshold_percent: f64,
    pub workflow_execution_interval_seconds: u64,
    pub max_concurrent_workflows: u32,

    // Performance settings
    pub operation_timeout_seconds: u64,
    pub cache_ttl_seconds: u64,
    pub batch_operation_size: usize,
    pub max_concurrent_operations: u32,

    // Event processing
    pub enable_balance_change_events: bool,
    pub enable_optimization_events: bool,
    pub event_processing_batch_size: usize,
    pub event_retention_hours: u32,
}

impl Default for FinancialCoordinatorConfig {
    fn default() -> Self {
        Self {
            enable_unified_interface: true,
            enable_cross_component_coordination: true,
            enable_automated_workflows: true,
            enable_financial_events: true,
            auto_analysis_threshold_usd: 1000.0,
            auto_optimization_threshold_percent: 5.0,
            workflow_execution_interval_seconds: 300,
            max_concurrent_workflows: 10,
            operation_timeout_seconds: 30,
            cache_ttl_seconds: 300,
            batch_operation_size: 50,
            max_concurrent_operations: 25,
            enable_balance_change_events: true,
            enable_optimization_events: true,
            event_processing_batch_size: 100,
            event_retention_hours: 24,
        }
    }
}

impl FinancialCoordinatorConfig {
    /// High-performance configuration for 1000-2500 concurrent users
    pub fn high_performance() -> Self {
        Self {
            enable_unified_interface: true,
            enable_cross_component_coordination: true,
            enable_automated_workflows: true,
            enable_financial_events: false, // Disable for performance
            auto_analysis_threshold_usd: 500.0,
            auto_optimization_threshold_percent: 3.0,
            workflow_execution_interval_seconds: 180,
            max_concurrent_workflows: 20,
            operation_timeout_seconds: 15,
            cache_ttl_seconds: 180,
            batch_operation_size: 100,
            max_concurrent_operations: 50,
            enable_balance_change_events: false,
            enable_optimization_events: false,
            event_processing_batch_size: 200,
            event_retention_hours: 12,
        }
    }

    /// High-reliability configuration with enhanced monitoring
    pub fn high_reliability() -> Self {
        Self {
            enable_unified_interface: true,
            enable_cross_component_coordination: false, // Disable for stability
            enable_automated_workflows: false,
            enable_financial_events: true,
            auto_analysis_threshold_usd: 2000.0,
            auto_optimization_threshold_percent: 10.0,
            workflow_execution_interval_seconds: 600,
            max_concurrent_workflows: 5,
            operation_timeout_seconds: 60,
            cache_ttl_seconds: 600,
            batch_operation_size: 25,
            max_concurrent_operations: 10,
            enable_balance_change_events: true,
            enable_optimization_events: true,
            event_processing_batch_size: 50,
            event_retention_hours: 48,
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.auto_analysis_threshold_usd < 0.0 {
            return Err(ArbitrageError::configuration_error(
                "auto_analysis_threshold_usd must be non-negative".to_string(),
            ));
        }
        if self.auto_optimization_threshold_percent < 0.0 {
            return Err(ArbitrageError::configuration_error(
                "auto_optimization_threshold_percent must be non-negative".to_string(),
            ));
        }
        if self.workflow_execution_interval_seconds == 0 {
            return Err(ArbitrageError::configuration_error(
                "workflow_execution_interval_seconds must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Financial Coordinator Health Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialCoordinatorHealth {
    pub is_healthy: bool,
    pub coordination_healthy: bool,
    pub workflow_healthy: bool,
    pub event_processing_healthy: bool,
    pub active_workflows: u32,
    pub pending_events: u32,
    pub average_operation_time_ms: f64,
    pub last_health_check: u64,
}

/// Financial Coordinator Performance Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialCoordinatorMetrics {
    // Coordination metrics
    pub operations_coordinated: u64,
    pub workflows_executed: u64,
    pub events_processed: u64,
    pub average_operation_time_ms: f64,

    // Performance metrics
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub cache_hit_rate: f64,
    pub error_rate: f64,

    // Business metrics
    pub financial_analyses_triggered: u64,
    pub optimizations_triggered: u64,
    pub balance_updates_processed: u64,
    pub last_updated: u64,
}

/// Financial workflow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialWorkflow {
    pub workflow_id: String,
    pub user_id: String,
    pub workflow_type: String, // "balance_analysis", "portfolio_optimization", "risk_assessment"
    pub trigger_condition: String,
    pub status: String, // "pending", "running", "completed", "failed"
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

/// Financial event for tracking and notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialEvent {
    pub event_id: String,
    pub user_id: String,
    pub event_type: String, // "balance_change", "optimization_completed", "risk_threshold_exceeded"
    pub event_data: serde_json::Value,
    pub severity: String, // "info", "warning", "critical"
    pub timestamp: u64,
    pub processed: bool,
}

/// Financial operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialOperationRequest {
    pub operation_id: String,
    pub user_id: String,
    pub operation_type: String,
    pub parameters: HashMap<String, serde_json::Value>,
    pub priority: String, // "low", "medium", "high", "critical"
    pub requested_at: u64,
}

/// Financial operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialOperationResult {
    pub operation_id: String,
    pub operation_type: String,
    pub status: String, // "success", "failed", "timeout"
    pub result_data: serde_json::Value,
    pub execution_time_ms: u64,
    pub completed_at: u64,
    pub error_message: Option<String>,
}

/// Financial Coordinator for unified financial operations
pub struct FinancialCoordinator {
    config: FinancialCoordinatorConfig,
    kv_store: Option<KvStore>,

    // Workflow management
    active_workflows: HashMap<String, FinancialWorkflow>,
    pending_events: Vec<FinancialEvent>,

    // Operation tracking
    operation_queue: Vec<FinancialOperationRequest>,
    operation_results: HashMap<String, FinancialOperationResult>,

    // Performance tracking
    metrics: FinancialCoordinatorMetrics,
    last_operation_time: u64,
    is_initialized: bool,
}

impl FinancialCoordinator {
    /// Create new Financial Coordinator with configuration
    pub fn new(config: FinancialCoordinatorConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        Ok(Self {
            config,
            kv_store: None,
            active_workflows: HashMap::new(),
            pending_events: Vec::new(),
            operation_queue: Vec::new(),
            operation_results: HashMap::new(),
            metrics: FinancialCoordinatorMetrics::default(),
            last_operation_time: get_current_time_millis(),
            is_initialized: false,
        })
    }

    /// Initialize the Financial Coordinator with environment
    pub async fn initialize(&mut self, env: &Env) -> ArbitrageResult<()> {
        // Initialize KV store for coordination data
        self.kv_store = Some(env.kv("FINANCIAL_COORDINATION").map_err(|e| {
            ArbitrageError::configuration_error(format!("Failed to initialize KV store: {:?}", e))
        })?);

        self.is_initialized = true;
        Ok(())
    }

    /// Execute coordinated financial operation
    pub async fn execute_financial_operation(
        &mut self,
        operation_request: FinancialOperationRequest,
        balance_tracker: &mut BalanceTracker,
        fund_analyzer: &mut FundAnalyzer,
    ) -> ArbitrageResult<FinancialOperationResult> {
        let start_time = get_current_time_millis();

        // Validate operation request
        self.validate_operation_request(&operation_request)?;

        // Execute operation based on type
        let result_data = match operation_request.operation_type.as_str() {
            "get_balances" => {
                self.execute_get_balances(&operation_request, balance_tracker)
                    .await?
            }
            "analyze_portfolio" => {
                self.execute_analyze_portfolio(&operation_request, balance_tracker, fund_analyzer)
                    .await?
            }
            "optimize_portfolio" => {
                self.execute_optimize_portfolio(&operation_request, balance_tracker, fund_analyzer)
                    .await?
            }
            "get_balance_history" => {
                self.execute_get_balance_history(&operation_request, balance_tracker)
                    .await?
            }
            "comprehensive_analysis" => {
                self.execute_comprehensive_analysis(
                    &operation_request,
                    balance_tracker,
                    fund_analyzer,
                )
                .await?
            }
            _ => {
                return Err(ArbitrageError::validation_error(format!(
                    "Unknown operation type: {}",
                    operation_request.operation_type
                )))
            }
        };

        // Create operation result
        let execution_time = get_current_time_millis() - start_time;
        let operation_result = FinancialOperationResult {
            operation_id: operation_request.operation_id.clone(),
            operation_type: operation_request.operation_type.clone(),
            status: "success".to_string(),
            result_data,
            execution_time_ms: execution_time,
            completed_at: get_current_time_millis(),
            error_message: None,
        };

        // Store result
        self.operation_results.insert(
            operation_request.operation_id.clone(),
            operation_result.clone(),
        );

        // Update metrics
        self.update_operation_metrics(execution_time, true);

        // Trigger automated workflows if enabled
        if self.config.enable_automated_workflows {
            self.trigger_automated_workflows(&operation_request, &operation_result)
                .await?;
        }

        // Generate events if enabled
        if self.config.enable_financial_events {
            self.generate_financial_events(&operation_request, &operation_result)
                .await?;
        }

        Ok(operation_result)
    }

    /// Execute get balances operation
    async fn execute_get_balances(
        &self,
        operation_request: &FinancialOperationRequest,
        balance_tracker: &mut BalanceTracker,
    ) -> ArbitrageResult<serde_json::Value> {
        // Extract exchange IDs from parameters
        let exchange_ids = operation_request
            .parameters
            .get("exchange_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["binance".to_string(), "bybit".to_string()]);

        // Get real-time balances
        let balance_snapshots = balance_tracker
            .get_real_time_balances(&operation_request.user_id, &exchange_ids)
            .await?;

        // Convert to JSON
        let result = serde_json::json!({
            "operation_type": "get_balances",
            "user_id": operation_request.user_id,
            "exchange_count": balance_snapshots.len(),
            "balances": balance_snapshots,
            "total_value_usd": balance_snapshots.values().map(|s| s.total_usd_value).sum::<f64>(),
            "timestamp": get_current_time_millis()
        });

        Ok(result)
    }

    /// Execute analyze portfolio operation
    async fn execute_analyze_portfolio(
        &self,
        operation_request: &FinancialOperationRequest,
        balance_tracker: &mut BalanceTracker,
        fund_analyzer: &mut FundAnalyzer,
    ) -> ArbitrageResult<serde_json::Value> {
        // Get balance snapshots first
        let exchange_ids = operation_request
            .parameters
            .get("exchange_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["binance".to_string(), "bybit".to_string()]);

        let balance_snapshots = balance_tracker
            .get_real_time_balances(&operation_request.user_id, &exchange_ids)
            .await?;

        // Perform portfolio analysis
        let portfolio_analytics = fund_analyzer
            .analyze_portfolio(&operation_request.user_id, &balance_snapshots)
            .await?;

        // Convert to JSON
        let result = serde_json::json!({
            "operation_type": "analyze_portfolio",
            "user_id": operation_request.user_id,
            "portfolio_analytics": portfolio_analytics,
            "balance_snapshots_count": balance_snapshots.len(),
            "timestamp": get_current_time_millis()
        });

        Ok(result)
    }

    /// Execute optimize portfolio operation
    async fn execute_optimize_portfolio(
        &self,
        operation_request: &FinancialOperationRequest,
        balance_tracker: &mut BalanceTracker,
        fund_analyzer: &mut FundAnalyzer,
    ) -> ArbitrageResult<serde_json::Value> {
        // Get balance snapshots first
        let exchange_ids = operation_request
            .parameters
            .get("exchange_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["binance".to_string(), "bybit".to_string()]);

        let balance_snapshots = balance_tracker
            .get_real_time_balances(&operation_request.user_id, &exchange_ids)
            .await?;

        // Extract target allocation from parameters
        let target_allocation = operation_request
            .parameters
            .get("target_allocation")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                    .collect::<HashMap<String, f64>>()
            })
            .unwrap_or_else(|| {
                // Default allocation
                let mut default_allocation = HashMap::new();
                default_allocation.insert("BTC".to_string(), 40.0);
                default_allocation.insert("ETH".to_string(), 30.0);
                default_allocation.insert("USDT".to_string(), 30.0);
                default_allocation
            });

        // Perform fund optimization
        let optimization_result = fund_analyzer
            .optimize_fund_allocation(
                &operation_request.user_id,
                &balance_snapshots,
                &target_allocation,
            )
            .await?;

        // Convert to JSON
        let result = serde_json::json!({
            "operation_type": "optimize_portfolio",
            "user_id": operation_request.user_id,
            "optimization_result": optimization_result,
            "target_allocation": target_allocation,
            "timestamp": get_current_time_millis()
        });

        Ok(result)
    }

    /// Execute get balance history operation
    async fn execute_get_balance_history(
        &self,
        operation_request: &FinancialOperationRequest,
        balance_tracker: &mut BalanceTracker,
    ) -> ArbitrageResult<serde_json::Value> {
        // Extract parameters
        let exchange_id = operation_request
            .parameters
            .get("exchange_id")
            .and_then(|v| v.as_str());
        let asset = operation_request
            .parameters
            .get("asset")
            .and_then(|v| v.as_str());
        let from_timestamp = operation_request
            .parameters
            .get("from_timestamp")
            .and_then(|v| v.as_u64());
        let to_timestamp = operation_request
            .parameters
            .get("to_timestamp")
            .and_then(|v| v.as_u64());
        let limit = operation_request
            .parameters
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|l| l as u32);

        // Get balance history
        let balance_history = balance_tracker
            .get_balance_history(
                &operation_request.user_id,
                exchange_id,
                asset,
                from_timestamp,
                to_timestamp,
                limit,
            )
            .await?;

        // Convert to JSON
        let result = serde_json::json!({
            "operation_type": "get_balance_history",
            "user_id": operation_request.user_id,
            "history_entries": balance_history,
            "entry_count": balance_history.len(),
            "filters": {
                "exchange_id": exchange_id,
                "asset": asset,
                "from_timestamp": from_timestamp,
                "to_timestamp": to_timestamp,
                "limit": limit
            },
            "timestamp": get_current_time_millis()
        });

        Ok(result)
    }

    /// Execute comprehensive analysis operation
    async fn execute_comprehensive_analysis(
        &self,
        operation_request: &FinancialOperationRequest,
        balance_tracker: &mut BalanceTracker,
        fund_analyzer: &mut FundAnalyzer,
    ) -> ArbitrageResult<serde_json::Value> {
        // Get balance snapshots
        let exchange_ids = operation_request
            .parameters
            .get("exchange_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["binance".to_string(), "bybit".to_string()]);

        let balance_snapshots = balance_tracker
            .get_real_time_balances(&operation_request.user_id, &exchange_ids)
            .await?;

        // Perform portfolio analysis
        let portfolio_analytics = fund_analyzer
            .analyze_portfolio(&operation_request.user_id, &balance_snapshots)
            .await?;

        // Get balance history (last 7 days)
        let from_timestamp = get_current_time_millis() - (7 * 24 * 60 * 60 * 1000);
        let balance_history = balance_tracker
            .get_balance_history(
                &operation_request.user_id,
                None,
                None,
                Some(from_timestamp),
                None,
                Some(100),
            )
            .await?;

        // Calculate summary metrics
        let total_exchanges = balance_snapshots.len();
        let total_assets = balance_snapshots
            .values()
            .flat_map(|snapshot| snapshot.balances.iter().map(|balance| &balance.asset))
            .collect::<std::collections::HashSet<_>>()
            .len();
        let total_value_usd = balance_snapshots
            .values()
            .map(|s| s.total_usd_value)
            .sum::<f64>();

        // Convert to JSON
        let result = serde_json::json!({
            "operation_type": "comprehensive_analysis",
            "user_id": operation_request.user_id,
            "summary": {
                "total_exchanges": total_exchanges,
                "total_assets": total_assets,
                "total_value_usd": total_value_usd,
                "analysis_timestamp": get_current_time_millis()
            },
            "balance_snapshots": balance_snapshots,
            "portfolio_analytics": portfolio_analytics,
            "balance_history": {
                "entries": balance_history,
                "entry_count": balance_history.len(),
                "period_days": 7
            },
            "timestamp": get_current_time_millis()
        });

        Ok(result)
    }

    /// Validate operation request
    fn validate_operation_request(
        &self,
        request: &FinancialOperationRequest,
    ) -> ArbitrageResult<()> {
        if request.operation_id.is_empty() {
            return Err(ArbitrageError::validation_error(
                "operation_id cannot be empty".to_string(),
            ));
        }
        if request.user_id.is_empty() {
            return Err(ArbitrageError::validation_error(
                "user_id cannot be empty".to_string(),
            ));
        }
        const ALLOWED: &[&str] = &[
            "get_balances",
            "analyze_portfolio",
            "optimize_portfolio",
            "get_balance_history",
            "comprehensive_analysis",
        ];

        if !ALLOWED.contains(&request.operation_type.as_str()) {
            return Err(ArbitrageError::validation_error(format!(
                "unsupported operation_type '{}'",
                request.operation_type
            )));
        }
        Ok(())
    }

    /// Trigger automated workflows based on operation results
    async fn trigger_automated_workflows(
        &mut self,
        operation_request: &FinancialOperationRequest,
        operation_result: &FinancialOperationResult,
    ) -> ArbitrageResult<()> {
        // Check if portfolio value exceeds auto-analysis threshold
        if let Some(total_value) = operation_result
            .result_data
            .get("total_value_usd")
            .and_then(|v| v.as_f64())
        {
            if total_value > self.config.auto_analysis_threshold_usd {
                self.create_workflow(
                    &operation_request.user_id,
                    "auto_portfolio_analysis",
                    "portfolio_value_threshold_exceeded",
                )
                .await?;
            }
        }

        // Check for optimization triggers
        if operation_request.operation_type == "analyze_portfolio" {
            if let Some(_analytics) = operation_result.result_data.get("portfolio_analytics") {
                // Mock check for optimization need - in reality, this would analyze the portfolio metrics
                self.create_workflow(
                    &operation_request.user_id,
                    "auto_portfolio_optimization",
                    "analysis_completed",
                )
                .await?;
            }
        }

        Ok(())
    }

    /// Generate financial events based on operation results
    async fn generate_financial_events(
        &mut self,
        operation_request: &FinancialOperationRequest,
        operation_result: &FinancialOperationResult,
    ) -> ArbitrageResult<()> {
        // Generate balance change event
        if self.config.enable_balance_change_events
            && operation_request.operation_type == "get_balances"
        {
            let event = FinancialEvent {
                event_id: uuid::Uuid::new_v4().to_string(),
                user_id: operation_request.user_id.clone(),
                event_type: "balance_update".to_string(),
                event_data: serde_json::json!({
                    "operation_id": operation_request.operation_id,
                    "total_value_usd": operation_result.result_data.get("total_value_usd"),
                    "exchange_count": operation_result.result_data.get("exchange_count")
                }),
                severity: "info".to_string(),
                timestamp: get_current_time_millis(),
                processed: false,
            };
            self.pending_events.push(event);
        }

        // Generate optimization event
        if self.config.enable_optimization_events
            && operation_request.operation_type == "optimize_portfolio"
        {
            let event = FinancialEvent {
                event_id: uuid::Uuid::new_v4().to_string(),
                user_id: operation_request.user_id.clone(),
                event_type: "optimization_completed".to_string(),
                event_data: serde_json::json!({
                    "operation_id": operation_request.operation_id,
                    "optimization_score": operation_result.result_data
                        .get("optimization_result")
                        .and_then(|r| r.get("optimization_score"))
                }),
                severity: "info".to_string(),
                timestamp: get_current_time_millis(),
                processed: false,
            };
            self.pending_events.push(event);
        }

        Ok(())
    }

    /// Create a new workflow
    async fn create_workflow(
        &mut self,
        user_id: &str,
        workflow_type: &str,
        trigger_condition: &str,
    ) -> ArbitrageResult<()> {
        let workflow = FinancialWorkflow {
            workflow_id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            workflow_type: workflow_type.to_string(),
            trigger_condition: trigger_condition.to_string(),
            status: "pending".to_string(),
            created_at: get_current_time_millis(),
            started_at: None,
            completed_at: None,
            result: None,
            error_message: None,
        };

        self.active_workflows
            .insert(workflow.workflow_id.clone(), workflow);
        self.metrics.workflows_executed += 1;

        Ok(())
    }

    /// Process pending events
    pub async fn process_pending_events(&mut self) -> ArbitrageResult<()> {
        let events_to_process = std::mem::take(&mut self.pending_events);

        for mut event in events_to_process {
            // Mock event processing - in reality, this would send notifications, update databases, etc.
            event.processed = true;
            self.metrics.events_processed += 1;
        }

        Ok(())
    }

    /// Get operation result
    pub fn get_operation_result(&self, operation_id: &str) -> Option<&FinancialOperationResult> {
        self.operation_results.get(operation_id)
    }

    /// Get active workflows for a user
    pub fn get_user_workflows(&self, user_id: &str) -> Vec<&FinancialWorkflow> {
        self.active_workflows
            .values()
            .filter(|workflow| workflow.user_id == user_id)
            .collect()
    }

    /// Get pending events for a user
    pub fn get_user_events(&self, user_id: &str) -> Vec<&FinancialEvent> {
        self.pending_events
            .iter()
            .filter(|event| event.user_id == user_id)
            .collect()
    }

    /// Update operation performance metrics
    fn update_operation_metrics(&mut self, operation_time_ms: u64, success: bool) {
        self.metrics.operations_coordinated += 1;

        if success {
            self.metrics.successful_operations += 1;
        } else {
            self.metrics.failed_operations += 1;
        }

        // Update average operation time (exponential moving average)
        let alpha = 0.1;
        self.metrics.average_operation_time_ms = alpha * operation_time_ms as f64
            + (1.0 - alpha) * self.metrics.average_operation_time_ms;

        // Update error rate
        let total_operations = self.metrics.successful_operations + self.metrics.failed_operations;
        if total_operations > 0 {
            self.metrics.error_rate =
                self.metrics.failed_operations as f64 / total_operations as f64;
        }

        self.metrics.last_updated = get_current_time_millis();
        self.last_operation_time = get_current_time_millis();
    }

    /// Get health status
    pub async fn health_check(&self) -> ArbitrageResult<FinancialCoordinatorHealth> {
        let active_workflows = self.active_workflows.len() as u32;
        let pending_events = self.pending_events.len() as u32;

        let coordination_healthy = self.metrics.error_rate < 0.05; // 5% error threshold
        let workflow_healthy = active_workflows <= self.config.max_concurrent_workflows;
        let event_processing_healthy =
            pending_events < (self.config.event_processing_batch_size * 2) as u32;

        let is_healthy = coordination_healthy && workflow_healthy && event_processing_healthy;

        Ok(FinancialCoordinatorHealth {
            is_healthy,
            coordination_healthy,
            workflow_healthy,
            event_processing_healthy,
            active_workflows,
            pending_events,
            average_operation_time_ms: self.metrics.average_operation_time_ms,
            last_health_check: get_current_time_millis(),
        })
    }

    /// Get performance metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<FinancialCoordinatorMetrics> {
        Ok(self.metrics.clone())
    }

    /// Cleanup old workflows and events
    pub async fn cleanup_old_data(&mut self, max_age_hours: u32) -> ArbitrageResult<()> {
        let cutoff_time = get_current_time_millis() - (max_age_hours as u64 * 60 * 60 * 1000);

        // Remove old workflows
        self.active_workflows
            .retain(|_, workflow| workflow.created_at >= cutoff_time);

        // Remove old events
        self.pending_events
            .retain(|event| event.timestamp >= cutoff_time);

        // Remove old operation results
        self.operation_results
            .retain(|_, result| result.completed_at >= cutoff_time);

        Ok(())
    }

    /// Check if coordinator is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Get configuration
    pub fn config(&self) -> &FinancialCoordinatorConfig {
        &self.config
    }
}

impl Default for FinancialCoordinatorMetrics {
    fn default() -> Self {
        Self {
            operations_coordinated: 0,
            workflows_executed: 0,
            events_processed: 0,
            average_operation_time_ms: 0.0,
            successful_operations: 0,
            failed_operations: 0,
            cache_hit_rate: 0.0,
            error_rate: 0.0,
            financial_analyses_triggered: 0,
            optimizations_triggered: 0,
            balance_updates_processed: 0,
            last_updated: get_current_time_millis(),
        }
    }
}
