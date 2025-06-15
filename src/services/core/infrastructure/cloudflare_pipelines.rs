//! Cloudflare Pipelines Service Module
//!
//! Provides pipeline orchestration and workflow management for the ArbEdge platform.
//! Integrates with Cloudflare Workers and other pipeline services.

use crate::services::core::infrastructure::UnifiedRetryConfig;
use crate::utils::now_system_time;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Pipeline service for workflow orchestration
#[derive(Debug, Clone)]
pub struct CloudflarePipelinesService {
    pipelines: HashMap<String, Pipeline>,
    executions: HashMap<String, PipelineExecution>,
    config: PipelineConfig,
}

/// Pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<PipelineStep>,
    pub triggers: Vec<PipelineTrigger>,
    pub config: PipelineStepConfig,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub enabled: bool,
}

/// Pipeline step definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    pub id: String,
    pub name: String,
    pub step_type: PipelineStepType,
    pub config: PipelineStepConfig,
    pub dependencies: Vec<String>,
    pub retry_config: RetryConfig,
    pub timeout: Duration,
}

/// Pipeline step types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipelineStepType {
    DataIngestion,
    DataProcessing,
    MarketAnalysis,
    OpportunityDetection,
    RiskAssessment,
    TradeExecution,
    Notification,
    Analytics,
    Custom(String),
}

/// Pipeline trigger types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipelineTrigger {
    Schedule(String), // Cron expression
    Event(String),    // Event name
    Webhook(String),  // Webhook URL
    Manual,
}

/// Pipeline step configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStepConfig {
    pub parameters: HashMap<String, serde_json::Value>,
    pub environment: HashMap<String, String>,
    pub resources: ResourceConfig,
}

/// Resource configuration for pipeline steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<u64>,
    pub timeout: Duration,
    pub max_retries: u32,
}

// Use unified retry configuration
pub type RetryConfig = UnifiedRetryConfig;

/// Pipeline execution state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineExecution {
    pub id: String,
    pub pipeline_id: String,
    pub status: ExecutionStatus,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub steps: HashMap<String, StepExecution>,
    pub context: ExecutionContext,
    pub error: Option<String>,
}

/// Step execution state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecution {
    pub step_id: String,
    pub status: ExecutionStatus,
    pub started_at: Option<SystemTime>,
    pub completed_at: Option<SystemTime>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub retry_count: u32,
}

/// Execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Retrying,
}

/// Execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub variables: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, String>,
    pub trigger_data: Option<serde_json::Value>,
}

/// Pipeline service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub max_concurrent_executions: u32,
    pub default_timeout: Duration,
    pub enable_metrics: bool,
    pub enable_logging: bool,
    pub worker_endpoint: Option<String>,
}

/// Pipeline operation result
pub type PipelineResult<T> = Result<T, PipelineError>;

/// Pipeline service errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipelineError {
    PipelineNotFound(String),
    ExecutionNotFound(String),
    StepNotFound(String),
    InvalidConfiguration(String),
    ExecutionFailed(String),
    TimeoutError(String),
    ResourceLimitExceeded(String),
    DependencyError(String),
    SerializationError(String),
    NetworkError(String),
    ServiceUnavailable(String),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::PipelineNotFound(id) => write!(f, "Pipeline not found: {}", id),
            PipelineError::ExecutionNotFound(id) => write!(f, "Execution not found: {}", id),
            PipelineError::StepNotFound(id) => write!(f, "Step not found: {}", id),
            PipelineError::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {}", msg),
            PipelineError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            PipelineError::TimeoutError(msg) => write!(f, "Timeout error: {}", msg),
            PipelineError::ResourceLimitExceeded(msg) => {
                write!(f, "Resource limit exceeded: {}", msg)
            }
            PipelineError::DependencyError(msg) => write!(f, "Dependency error: {}", msg),
            PipelineError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            PipelineError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            PipelineError::ServiceUnavailable(msg) => write!(f, "Service unavailable: {}", msg),
        }
    }
}

impl std::error::Error for PipelineError {}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_concurrent_executions: 10,
            default_timeout: Duration::from_secs(300), // 5 minutes
            enable_metrics: true,
            enable_logging: true,
            worker_endpoint: None,
        }
    }
}

// Default implementation provided by UnifiedRetryConfig

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            cpu_limit: Some(1.0),
            memory_limit: Some(512 * 1024 * 1024), // 512MB
            timeout: Duration::from_secs(300),
            max_retries: 3,
        }
    }
}

impl CloudflarePipelinesService {
    /// Create a new pipeline service
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            pipelines: HashMap::new(),
            executions: HashMap::new(),
            config,
        }
    }

    /// Create a new pipeline
    pub async fn create_pipeline(&mut self, pipeline: Pipeline) -> PipelineResult<String> {
        let id = pipeline.id.clone();

        // Validate pipeline configuration
        self.validate_pipeline(&pipeline)?;

        self.pipelines.insert(id.clone(), pipeline);
        Ok(id)
    }

    /// Get a pipeline by ID
    pub async fn get_pipeline(&self, pipeline_id: &str) -> PipelineResult<Option<Pipeline>> {
        Ok(self.pipelines.get(pipeline_id).cloned())
    }

    /// List all pipelines
    pub async fn list_pipelines(&self) -> PipelineResult<Vec<Pipeline>> {
        Ok(self.pipelines.values().cloned().collect())
    }

    /// Update a pipeline
    pub async fn update_pipeline(&mut self, pipeline: Pipeline) -> PipelineResult<()> {
        let id = pipeline.id.clone();

        if !self.pipelines.contains_key(&id) {
            return Err(PipelineError::PipelineNotFound(id));
        }

        self.validate_pipeline(&pipeline)?;
        self.pipelines.insert(id, pipeline);
        Ok(())
    }

    /// Delete a pipeline
    pub async fn delete_pipeline(&mut self, pipeline_id: &str) -> PipelineResult<bool> {
        Ok(self.pipelines.remove(pipeline_id).is_some())
    }

    /// Execute a pipeline
    pub async fn execute_pipeline(
        &mut self,
        pipeline_id: &str,
        context: Option<ExecutionContext>,
    ) -> PipelineResult<String> {
        let pipeline = self
            .pipelines
            .get(pipeline_id)
            .ok_or_else(|| PipelineError::PipelineNotFound(pipeline_id.to_string()))?;

        if !pipeline.enabled {
            return Err(PipelineError::InvalidConfiguration(
                "Pipeline is disabled".to_string(),
            ));
        }

        let execution_id = Uuid::new_v4().to_string();
        let execution = PipelineExecution {
            id: execution_id.clone(),
            pipeline_id: pipeline_id.to_string(),
            status: ExecutionStatus::Pending,
            started_at: now_system_time(),
            completed_at: None,
            steps: HashMap::new(),
            context: context.unwrap_or_else(|| ExecutionContext {
                variables: HashMap::new(),
                metadata: HashMap::new(),
                trigger_data: None,
            }),
            error: None,
        };

        self.executions.insert(execution_id.clone(), execution);

        // Start execution asynchronously
        self.start_execution(&execution_id).await?;

        Ok(execution_id)
    }

    /// Get execution status
    pub async fn get_execution(
        &self,
        execution_id: &str,
    ) -> PipelineResult<Option<PipelineExecution>> {
        Ok(self.executions.get(execution_id).cloned())
    }

    /// List executions for a pipeline
    pub async fn list_executions(
        &self,
        pipeline_id: &str,
    ) -> PipelineResult<Vec<PipelineExecution>> {
        let executions: Vec<PipelineExecution> = self
            .executions
            .values()
            .filter(|e| e.pipeline_id == pipeline_id)
            .cloned()
            .collect();
        Ok(executions)
    }

    /// Cancel an execution
    pub async fn cancel_execution(&mut self, execution_id: &str) -> PipelineResult<()> {
        if let Some(execution) = self.executions.get_mut(execution_id) {
            if execution.status == ExecutionStatus::Running
                || execution.status == ExecutionStatus::Pending
            {
                execution.status = ExecutionStatus::Cancelled;
                execution.completed_at = Some(now_system_time());
            }
            Ok(())
        } else {
            Err(PipelineError::ExecutionNotFound(execution_id.to_string()))
        }
    }

    /// Get execution metrics
    pub async fn get_execution_metrics(
        &self,
        execution_id: &str,
    ) -> PipelineResult<ExecutionMetrics> {
        let execution = self
            .executions
            .get(execution_id)
            .ok_or_else(|| PipelineError::ExecutionNotFound(execution_id.to_string()))?;

        let duration = if let Some(completed_at) = execution.completed_at {
            completed_at
                .duration_since(execution.started_at)
                .unwrap_or_default()
        } else {
            now_system_time()
                .duration_since(execution.started_at)
                .unwrap_or_default()
        };

        let total_steps = execution.steps.len();
        let completed_steps = execution
            .steps
            .values()
            .filter(|s| s.status == ExecutionStatus::Completed)
            .count();
        let failed_steps = execution
            .steps
            .values()
            .filter(|s| s.status == ExecutionStatus::Failed)
            .count();

        Ok(ExecutionMetrics {
            execution_id: execution_id.to_string(),
            duration,
            total_steps,
            completed_steps,
            failed_steps,
            success_rate: if total_steps > 0 {
                completed_steps as f64 / total_steps as f64
            } else {
                0.0
            },
        })
    }

    /// Start pipeline execution
    async fn start_execution(&mut self, execution_id: &str) -> PipelineResult<()> {
        if let Some(execution) = self.executions.get_mut(execution_id) {
            execution.status = ExecutionStatus::Running;

            // TODO: Implement actual step execution logic
            // For now, mark as completed
            execution.status = ExecutionStatus::Completed;
            execution.completed_at = Some(now_system_time());
        }
        Ok(())
    }

    /// Validate pipeline configuration
    fn validate_pipeline(&self, pipeline: &Pipeline) -> PipelineResult<()> {
        if pipeline.name.is_empty() {
            return Err(PipelineError::InvalidConfiguration(
                "Pipeline name cannot be empty".to_string(),
            ));
        }

        if pipeline.steps.is_empty() {
            return Err(PipelineError::InvalidConfiguration(
                "Pipeline must have at least one step".to_string(),
            ));
        }

        // Validate step dependencies
        let step_ids: std::collections::HashSet<String> =
            pipeline.steps.iter().map(|s| s.id.clone()).collect();

        for step in &pipeline.steps {
            for dep in &step.dependencies {
                if !step_ids.contains(dep) {
                    return Err(PipelineError::DependencyError(format!(
                        "Step {} depends on non-existent step {}",
                        step.id, dep
                    )));
                }
            }
        }

        Ok(())
    }

    /// Get service health status
    pub async fn health_check(&self) -> PipelineResult<HealthStatus> {
        let active_executions = self
            .executions
            .values()
            .filter(|e| e.status == ExecutionStatus::Running)
            .count();

        Ok(HealthStatus {
            service: "CloudflarePipelines".to_string(),
            status: if active_executions < self.config.max_concurrent_executions as usize {
                "healthy".to_string()
            } else {
                "degraded".to_string()
            },
            active_executions,
            total_pipelines: self.pipelines.len(),
            total_executions: self.executions.len(),
        })
    }
}

/// Execution metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub execution_id: String,
    pub duration: Duration,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub failed_steps: usize,
    pub success_rate: f64,
}

/// Health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub service: String,
    pub status: String,
    pub active_executions: usize,
    pub total_pipelines: usize,
    pub total_executions: usize,
}

/// Convenience functions for common pipeline operations
pub async fn create_data_ingestion_pipeline(
    service: &mut CloudflarePipelinesService,
    name: &str,
    sources: Vec<String>,
) -> PipelineResult<String> {
    let pipeline = Pipeline {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        description: "Data ingestion pipeline".to_string(),
        steps: sources
            .into_iter()
            .enumerate()
            .map(|(i, source)| PipelineStep {
                id: format!("step_{}", i),
                name: format!("Ingest from {}", source),
                step_type: PipelineStepType::DataIngestion,
                config: PipelineStepConfig {
                    parameters: {
                        let mut params = HashMap::new();
                        params.insert("source".to_string(), serde_json::Value::String(source));
                        params
                    },
                    environment: HashMap::new(),
                    resources: ResourceConfig::default(),
                },
                dependencies: vec![],
                retry_config: RetryConfig::default(),
                timeout: Duration::from_secs(300),
            })
            .collect(),
        triggers: vec![PipelineTrigger::Manual],
        config: PipelineStepConfig {
            parameters: HashMap::new(),
            environment: HashMap::new(),
            resources: ResourceConfig::default(),
        },
        created_at: now_system_time(),
        updated_at: now_system_time(),
        enabled: true,
    };

    service.create_pipeline(pipeline).await
}

pub async fn create_market_analysis_pipeline(
    service: &mut CloudflarePipelinesService,
    name: &str,
    exchanges: Vec<String>,
) -> PipelineResult<String> {
    let pipeline = Pipeline {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        description: "Market analysis pipeline".to_string(),
        steps: vec![
            PipelineStep {
                id: "data_collection".to_string(),
                name: "Collect Market Data".to_string(),
                step_type: PipelineStepType::DataIngestion,
                config: PipelineStepConfig {
                    parameters: {
                        let mut params = HashMap::new();
                        params.insert(
                            "exchanges".to_string(),
                            serde_json::Value::Array(
                                exchanges
                                    .into_iter()
                                    .map(serde_json::Value::String)
                                    .collect(),
                            ),
                        );
                        params
                    },
                    environment: HashMap::new(),
                    resources: ResourceConfig::default(),
                },
                dependencies: vec![],
                retry_config: RetryConfig::default(),
                timeout: Duration::from_secs(180),
            },
            PipelineStep {
                id: "market_analysis".to_string(),
                name: "Analyze Market Data".to_string(),
                step_type: PipelineStepType::MarketAnalysis,
                config: PipelineStepConfig {
                    parameters: HashMap::new(),
                    environment: HashMap::new(),
                    resources: ResourceConfig::default(),
                },
                dependencies: vec!["data_collection".to_string()],
                retry_config: RetryConfig::default(),
                timeout: Duration::from_secs(300),
            },
            PipelineStep {
                id: "opportunity_detection".to_string(),
                name: "Detect Opportunities".to_string(),
                step_type: PipelineStepType::OpportunityDetection,
                config: PipelineStepConfig {
                    parameters: HashMap::new(),
                    environment: HashMap::new(),
                    resources: ResourceConfig::default(),
                },
                dependencies: vec!["market_analysis".to_string()],
                retry_config: RetryConfig::default(),
                timeout: Duration::from_secs(120),
            },
        ],
        triggers: vec![
            PipelineTrigger::Schedule("0 */5 * * * *".to_string()), // Every 5 minutes
            PipelineTrigger::Event("market_data_updated".to_string()),
        ],
        config: PipelineStepConfig {
            parameters: HashMap::new(),
            environment: HashMap::new(),
            resources: ResourceConfig::default(),
        },
        created_at: now_system_time(),
        updated_at: now_system_time(),
        enabled: true,
    };

    service.create_pipeline(pipeline).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_creation() {
        let config = PipelineConfig::default();
        let mut service = CloudflarePipelinesService::new(config);

        let pipeline_id = create_data_ingestion_pipeline(
            &mut service,
            "Test Pipeline",
            vec!["binance".to_string(), "coinbase".to_string()],
        )
        .await
        .unwrap();

        let pipeline = service.get_pipeline(&pipeline_id).await.unwrap();
        assert!(pipeline.is_some());
        assert_eq!(pipeline.unwrap().name, "Test Pipeline");
    }

    #[tokio::test]
    async fn test_pipeline_execution() {
        let config = PipelineConfig::default();
        let mut service = CloudflarePipelinesService::new(config);

        let pipeline_id = create_market_analysis_pipeline(
            &mut service,
            "Market Analysis",
            vec!["binance".to_string()],
        )
        .await
        .unwrap();

        let execution_id = service.execute_pipeline(&pipeline_id, None).await.unwrap();
        let execution = service.get_execution(&execution_id).await.unwrap();
        assert!(execution.is_some());
    }

    #[tokio::test]
    async fn test_health_check() {
        let config = PipelineConfig::default();
        let service = CloudflarePipelinesService::new(config);

        let health = service.health_check().await.unwrap();
        assert_eq!(health.service, "CloudflarePipelines");
        assert_eq!(health.status, "healthy");
    }
}
