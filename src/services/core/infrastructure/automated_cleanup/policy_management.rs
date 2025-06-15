use crate::utils::now_system_time;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::services::core::config::ServiceConfig;
use crate::services::core::health::HealthCheck;
use crate::services::core::persistence::connection_manager::ConnectionManager;
use crate::services::core::persistence::transaction_coordinator::TransactionCoordinator;

/// Configuration for policy management interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyManagementConfig {
    /// Enable the policy management interface
    pub enabled: bool,
    /// Maximum number of policies per tenant
    pub max_policies_per_tenant: usize,
    /// Policy validation timeout
    pub validation_timeout: Duration,
    /// Enable policy versioning
    pub enable_versioning: bool,
    /// Maximum policy versions to keep
    pub max_versions: usize,
    /// Enable audit logging
    pub enable_audit_logging: bool,
    /// API rate limiting settings
    pub rate_limit: PolicyRateLimit,
    /// Template settings
    pub template_config: PolicyTemplateConfig,
}

impl Default for PolicyManagementConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_policies_per_tenant: 1000,
            validation_timeout: Duration::from_secs(30),
            enable_versioning: true,
            max_versions: 10,
            enable_audit_logging: true,
            rate_limit: PolicyRateLimit::default(),
            template_config: PolicyTemplateConfig::default(),
        }
    }
}

/// Rate limiting configuration for policy API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRateLimit {
    /// Requests per minute for policy operations
    pub requests_per_minute: u32,
    /// Burst capacity
    pub burst_capacity: u32,
    /// Enable per-tenant rate limiting
    pub per_tenant_limiting: bool,
}

impl Default for PolicyRateLimit {
    fn default() -> Self {
        Self {
            requests_per_minute: 100,
            burst_capacity: 20,
            per_tenant_limiting: true,
        }
    }
}

/// Policy template configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyTemplateConfig {
    /// Enable policy templates
    pub enabled: bool,
    /// Template validation timeout
    pub template_validation_timeout: Duration,
    /// Maximum templates per tenant
    pub max_templates_per_tenant: usize,
}

impl Default for PolicyTemplateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            template_validation_timeout: Duration::from_secs(15),
            max_templates_per_tenant: 50,
        }
    }
}

/// Cleanup policy definition with full configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPolicy {
    /// Unique policy identifier
    pub id: Uuid,
    /// Policy name (user-friendly)
    pub name: String,
    /// Policy description
    pub description: Option<String>,
    /// Policy version
    pub version: u32,
    /// Policy status
    pub status: PolicyStatus,
    /// Policy type
    pub policy_type: PolicyType,
    /// Target resources for cleanup
    pub targets: Vec<PolicyTarget>,
    /// Cleanup rules and conditions
    pub rules: Vec<CleanupRule>,
    /// Policy metadata
    pub metadata: PolicyMetadata,
    /// Policy configuration
    pub config: PolicyConfiguration,
    /// Policy validation rules
    pub validation: PolicyValidation,
    /// Tenant/namespace information
    pub tenant_id: String,
    /// Created timestamp
    pub created_at: SystemTime,
    /// Updated timestamp
    pub updated_at: SystemTime,
    /// Created by user
    pub created_by: String,
    /// Last modified by user
    pub modified_by: String,
}

/// Policy status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PolicyStatus {
    /// Policy is being created/validated
    Draft,
    /// Policy is active and executing
    Active,
    /// Policy is temporarily disabled
    Paused,
    /// Policy is scheduled for deletion
    Deprecated,
    /// Policy has validation errors
    Invalid,
    /// Policy testing mode (dry-run only)
    Testing,
}

/// Policy type categorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PolicyType {
    /// Time-based cleanup (TTL)
    TimeToLive,
    /// Usage-based cleanup
    UsageBased,
    /// Size-based cleanup
    SizeBased,
    /// Custom policy with user-defined rules
    Custom,
    /// Template-based policy
    Template,
    /// Composite policy combining multiple strategies
    Composite,
}

/// Policy target specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyTarget {
    /// Target type (KV, D1, R2, etc.)
    pub target_type: String,
    /// Resource patterns to match
    pub resource_patterns: Vec<String>,
    /// Exclusion patterns
    pub exclusion_patterns: Vec<String>,
    /// Target-specific configuration
    pub configuration: HashMap<String, serde_json::Value>,
}

/// Cleanup rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupRule {
    /// Rule identifier
    pub id: Uuid,
    /// Rule name
    pub name: String,
    /// Rule condition
    pub condition: RuleCondition,
    /// Action to take when condition is met
    pub action: CleanupAction,
    /// Rule priority (higher = executed first)
    pub priority: u32,
    /// Rule enabled status
    pub enabled: bool,
}

/// Rule condition types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    /// Age-based condition
    Age {
        /// Maximum age before cleanup
        max_age: Duration,
        /// Date field to check
        date_field: String,
    },
    /// Size-based condition
    Size {
        /// Maximum size threshold
        max_size: u64,
        /// Size unit (bytes, KB, MB, GB)
        unit: String,
    },
    /// Access-based condition
    LastAccessed {
        /// Maximum time since last access
        max_idle_time: Duration,
        /// Access tracking field
        access_field: String,
    },
    /// Custom condition with expression
    Custom {
        /// Custom expression/query
        expression: String,
        /// Expression language/type
        language: String,
    },
}

/// Cleanup action specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CleanupAction {
    /// Delete the resource
    Delete,
    /// Archive the resource
    Archive {
        /// Archive destination
        destination: String,
        /// Archive format
        format: String,
    },
    /// Move to different storage tier
    MoveTier {
        /// Target storage tier
        target_tier: String,
    },
    /// Custom action
    Custom {
        /// Action handler
        handler: String,
        /// Action parameters
        parameters: HashMap<String, serde_json::Value>,
    },
}

/// Policy metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMetadata {
    /// Policy tags for organization
    pub tags: Vec<String>,
    /// Policy category
    pub category: String,
    /// Business owner
    pub owner: String,
    /// Contact information
    pub contact: String,
    /// Documentation URL
    pub documentation_url: Option<String>,
    /// Policy cost estimate
    pub cost_estimate: Option<f64>,
    /// Expected data reduction
    pub expected_reduction: Option<f64>,
}

/// Policy configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfiguration {
    /// Enable dry-run mode
    pub dry_run: bool,
    /// Policy execution schedule
    pub schedule: PolicySchedule,
    /// Retry configuration
    pub retry_config: PolicyRetryConfig,
    /// Notification settings
    pub notifications: PolicyNotifications,
    /// Safety settings
    pub safety: PolicySafetyConfig,
}

/// Policy execution schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySchedule {
    /// Schedule type
    pub schedule_type: ScheduleType,
    /// Cron expression (if applicable)
    pub cron_expression: Option<String>,
    /// Interval duration (if applicable)
    pub interval: Option<Duration>,
    /// Timezone for schedule
    pub timezone: String,
    /// Enable jitter to spread load
    pub enable_jitter: bool,
}

/// Schedule type options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScheduleType {
    /// One-time execution
    Once,
    /// Interval-based execution
    Interval,
    /// Cron-based execution
    Cron,
    /// Event-triggered execution
    Event,
    /// Manual execution only
    Manual,
}

/// Policy retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRetryConfig {
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry delay strategy
    pub retry_delay: RetryDelay,
    /// Retry on specific errors
    pub retry_on_errors: Vec<String>,
    /// Stop retrying on specific errors
    pub stop_on_errors: Vec<String>,
}

/// Retry delay strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryDelay {
    /// Fixed delay between retries
    Fixed(Duration),
    /// Exponential backoff
    Exponential {
        /// Initial delay
        initial: Duration,
        /// Multiplier for each retry
        multiplier: f64,
        /// Maximum delay
        max_delay: Duration,
    },
    /// Linear backoff
    Linear {
        /// Initial delay
        initial: Duration,
        /// Increment per retry
        increment: Duration,
    },
}

/// Policy notification settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyNotifications {
    /// Enable notifications
    pub enabled: bool,
    /// Notification channels
    pub channels: Vec<NotificationChannel>,
    /// Notify on policy events
    pub events: Vec<PolicyEvent>,
    /// Notification templates
    pub templates: HashMap<PolicyEvent, String>,
}

/// Notification channel types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationChannel {
    /// Email notification
    Email(String),
    /// Slack webhook
    Slack(String),
    /// Webhook URL
    Webhook(String),
    /// SNS topic
    Sns(String),
}

/// Policy events for notifications
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PolicyEvent {
    /// Policy created
    Created,
    /// Policy activated
    Activated,
    /// Policy paused
    Paused,
    /// Policy execution started
    ExecutionStarted,
    /// Policy execution completed
    ExecutionCompleted,
    /// Policy execution failed
    ExecutionFailed,
    /// Policy validation failed
    ValidationFailed,
    /// Policy deprecated
    Deprecated,
}

/// Policy safety configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySafetyConfig {
    /// Maximum items to process per execution
    pub max_items_per_execution: Option<u64>,
    /// Maximum execution time
    pub max_execution_time: Option<Duration>,
    /// Require approval for large operations
    pub require_approval_threshold: Option<u64>,
    /// Enable circuit breaker
    pub enable_circuit_breaker: bool,
    /// Backup requirements
    pub backup_requirements: Vec<BackupRequirement>,
}

/// Backup requirement specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRequirement {
    /// Backup type
    pub backup_type: String,
    /// Retention period for backup
    pub retention_period: Duration,
    /// Backup validation required
    pub validation_required: bool,
}

/// Policy validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyValidation {
    /// Validation rules
    pub rules: Vec<ValidationRule>,
    /// Require impact analysis
    pub require_impact_analysis: bool,
    /// Testing requirements
    pub testing_requirements: TestingRequirements,
}

/// Validation rule types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    /// Rule name
    pub name: String,
    /// Rule type
    pub rule_type: ValidationRuleType,
    /// Rule parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Error message
    pub error_message: String,
}

/// Validation rule types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationRuleType {
    /// Schema validation
    Schema,
    /// Resource existence check
    ResourceExists,
    /// Permission check
    Permissions,
    /// Dependency validation
    Dependencies,
    /// Custom validation
    Custom,
}

/// Testing requirements for policies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestingRequirements {
    /// Require dry-run before activation
    pub require_dry_run: bool,
    /// Minimum test duration
    pub min_test_duration: Option<Duration>,
    /// Required test scenarios
    pub required_scenarios: Vec<String>,
    /// Performance thresholds
    pub performance_thresholds: HashMap<String, f64>,
}

/// Policy template for reusable policy configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyTemplate {
    /// Template identifier
    pub id: Uuid,
    /// Template name
    pub name: String,
    /// Template description
    pub description: String,
    /// Template category
    pub category: String,
    /// Template parameters
    pub parameters: Vec<TemplateParameter>,
    /// Base policy configuration
    pub base_policy: CleanupPolicy,
    /// Template metadata
    pub metadata: TemplateMetadata,
    /// Created timestamp
    pub created_at: SystemTime,
    /// Updated timestamp
    pub updated_at: SystemTime,
}

/// Template parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub parameter_type: ParameterType,
    /// Default value
    pub default_value: Option<serde_json::Value>,
    /// Parameter validation
    pub validation: ParameterValidation,
    /// Parameter description
    pub description: String,
    /// Required parameter
    pub required: bool,
}

/// Template parameter types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterType {
    String,
    Number,
    Boolean,
    Duration,
    Array,
    Object,
}

/// Parameter validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterValidation {
    /// Minimum value (for numbers)
    pub min: Option<f64>,
    /// Maximum value (for numbers)
    pub max: Option<f64>,
    /// Pattern validation (for strings)
    pub pattern: Option<String>,
    /// Allowed values
    pub allowed_values: Option<Vec<serde_json::Value>>,
}

/// Template metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    /// Template author
    pub author: String,
    /// Template version
    pub version: String,
    /// Compatible target types
    pub compatible_targets: Vec<String>,
    /// Use case examples
    pub use_cases: Vec<String>,
    /// Documentation links
    pub documentation: Vec<String>,
}

/// Policy execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyExecutionResult {
    /// Execution identifier
    pub execution_id: Uuid,
    /// Policy identifier
    pub policy_id: Uuid,
    /// Execution status
    pub status: ExecutionStatus,
    /// Execution start time
    pub started_at: SystemTime,
    /// Execution end time
    pub completed_at: Option<SystemTime>,
    /// Items processed
    pub items_processed: u64,
    /// Items cleaned up
    pub items_cleaned: u64,
    /// Data size cleaned
    pub data_size_cleaned: u64,
    /// Execution duration
    pub duration: Option<Duration>,
    /// Error information
    pub error: Option<ExecutionError>,
    /// Execution metrics
    pub metrics: ExecutionMetrics,
}

/// Policy execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    /// Execution is queued
    Queued,
    /// Execution is running
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed
    Failed,
    /// Execution was cancelled
    Cancelled,
    /// Execution timed out
    TimedOut,
}

/// Execution error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Error details
    pub details: Option<serde_json::Value>,
    /// Retry information
    pub retry_info: Option<RetryInfo>,
}

/// Retry information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryInfo {
    /// Current retry attempt
    pub attempt: u32,
    /// Maximum retries allowed
    pub max_retries: u32,
    /// Next retry time
    pub next_retry_at: Option<SystemTime>,
    /// Retry delay
    pub retry_delay: Duration,
}

/// Execution metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    /// Processing rate (items per second)
    pub processing_rate: f64,
    /// Memory usage during execution
    pub memory_usage: u64,
    /// CPU usage during execution
    pub cpu_usage: f64,
    /// Network I/O metrics
    pub network_io: NetworkMetrics,
    /// Storage I/O metrics
    pub storage_io: StorageMetrics,
}

/// Network I/O metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Request count
    pub request_count: u64,
    /// Average response time
    pub avg_response_time: Duration,
}

/// Storage I/O metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetrics {
    /// Bytes read
    pub bytes_read: u64,
    /// Bytes written
    pub bytes_written: u64,
    /// Read operations
    pub read_operations: u64,
    /// Write operations
    pub write_operations: u64,
}

/// Policy audit entry for compliance and tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyAuditEntry {
    /// Audit entry identifier
    pub id: Uuid,
    /// Timestamp of the audit event
    pub timestamp: SystemTime,
    /// Policy identifier
    pub policy_id: Uuid,
    /// Audit event type
    pub event_type: AuditEventType,
    /// User who performed the action
    pub user_id: String,
    /// Action details
    pub details: AuditDetails,
    /// Request metadata
    pub request_metadata: RequestMetadata,
}

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    /// Policy created
    PolicyCreated,
    /// Policy updated
    PolicyUpdated,
    /// Policy deleted
    PolicyDeleted,
    /// Policy activated
    PolicyActivated,
    /// Policy deactivated
    PolicyDeactivated,
    /// Policy executed
    PolicyExecuted,
    /// Policy validation performed
    PolicyValidated,
    /// Template created/updated
    TemplateModified,
}

/// Audit details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditDetails {
    /// Previous state (for updates)
    pub previous_state: Option<serde_json::Value>,
    /// New state
    pub new_state: serde_json::Value,
    /// Change summary
    pub change_summary: String,
    /// Additional context
    pub context: HashMap<String, serde_json::Value>,
}

/// Request metadata for audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetadata {
    /// Client IP address
    pub client_ip: String,
    /// User agent
    pub user_agent: String,
    /// Request ID
    pub request_id: String,
    /// API endpoint
    pub endpoint: String,
    /// HTTP method
    pub method: String,
}

/// Main policy management interface
pub struct PolicyManagementInterface {
    config: PolicyManagementConfig,
    connection_manager: Arc<ConnectionManager>,
    transaction_coordinator: Arc<TransactionCoordinator>,
    policies: Arc<RwLock<HashMap<Uuid, CleanupPolicy>>>,
    templates: Arc<RwLock<HashMap<Uuid, PolicyTemplate>>>,
    executions: Arc<RwLock<HashMap<Uuid, PolicyExecutionResult>>>,
    audit_log: Arc<RwLock<Vec<PolicyAuditEntry>>>,
    rate_limiter: Arc<RwLock<HashMap<String, RateLimitState>>>,
}

/// Rate limiting state
#[derive(Debug, Clone)]
pub struct RateLimitState {
    /// Current token count
    pub tokens: u32,
    /// Last refill time
    pub last_refill: SystemTime,
    /// Request history for sliding window
    pub request_history: Vec<SystemTime>,
}

impl PolicyManagementInterface {
    /// Create a new policy management interface
    pub async fn new(
        config: PolicyManagementConfig,
        connection_manager: Arc<ConnectionManager>,
        transaction_coordinator: Arc<TransactionCoordinator>,
    ) -> crate::utils::error::ArbitrageResult<Self> {
        Ok(Self {
            config,
            connection_manager,
            transaction_coordinator,
            policies: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            rate_limiter: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new cleanup policy
    pub async fn create_policy(
        &self,
        request: CreatePolicyRequest,
        user_context: &UserContext,
    ) -> crate::utils::error::ArbitrageResult<CleanupPolicy> {
        // Check rate limiting
        self.check_rate_limit(&user_context.tenant_id).await?;

        // Validate policy
        self.validate_policy(&request.policy).await?;

        // Check tenant policy count
        self.check_tenant_policy_limit(&user_context.tenant_id).await?;

        let policy_id = Uuid::new_v4();
        let now = now_system_time();
        
        let mut policy = request.policy;
        policy.id = policy_id;
        policy.created_at = now;
        policy.updated_at = now;
        policy.created_by = user_context.user_id.clone();
        policy.modified_by = user_context.user_id.clone();
        policy.tenant_id = user_context.tenant_id.clone();
        policy.version = 1;
        policy.status = PolicyStatus::Draft;

        // Store policy
        {
            let mut policies = self.policies.write().await;
            policies.insert(policy_id, policy.clone());
        }

        // Audit log
        self.log_audit_event(
            policy_id,
            AuditEventType::PolicyCreated,
            &user_context.user_id,
            AuditDetails {
                previous_state: None,
                new_state: serde_json::to_value(&policy)?,
                change_summary: "Policy created".to_string(),
                context: HashMap::new(),
            },
            request.request_metadata,
        ).await;

        Ok(policy)
    }

    /// Update an existing policy
    pub async fn update_policy(
        &self,
        policy_id: Uuid,
        request: UpdatePolicyRequest,
        user_context: &UserContext,
    ) -> crate::utils::error::ArbitrageResult<CleanupPolicy> {
        // Check rate limiting
        self.check_rate_limit(&user_context.tenant_id).await?;

        let mut policies = self.policies.write().await;
        let policy = policies.get_mut(&policy_id)
            .ok_or_else(|| crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::NotFound,
                format!("Policy not found: {}", policy_id),
            ))?;

        // Check ownership
        if policy.tenant_id != user_context.tenant_id {
            return Err(crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::Forbidden,
                "Access denied".to_string(),
            ));
        }

        let previous_state = serde_json::to_value(&policy)?;

        // Update policy
        if let Some(name) = request.name {
            policy.name = name;
        }
        if let Some(description) = request.description {
            policy.description = description;
        }
        if let Some(status) = request.status {
            policy.status = status;
        }
        if let Some(rules) = request.rules {
            policy.rules = rules;
        }
        if let Some(config) = request.config {
            policy.config = config;
        }

        policy.updated_at = now_system_time();
        policy.modified_by = user_context.user_id.clone();

        // Increment version if enabled
        if self.config.enable_versioning {
            policy.version += 1;
        }

        // Validate updated policy
        self.validate_policy(policy).await?;

        let updated_policy = policy.clone();

        // Audit log
        self.log_audit_event(
            policy_id,
            AuditEventType::PolicyUpdated,
            &user_context.user_id,
            AuditDetails {
                previous_state: Some(previous_state),
                new_state: serde_json::to_value(&updated_policy)?,
                change_summary: "Policy updated".to_string(),
                context: HashMap::new(),
            },
            request.request_metadata,
        ).await;

        Ok(updated_policy)
    }

    /// Delete a policy
    pub async fn delete_policy(
        &self,
        policy_id: Uuid,
        user_context: &UserContext,
        request_metadata: RequestMetadata,
    ) -> crate::utils::error::ArbitrageResult<()> {
        // Check rate limiting
        self.check_rate_limit(&user_context.tenant_id).await?;

        let mut policies = self.policies.write().await;
        let policy = policies.get(&policy_id)
            .ok_or_else(|| crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::NotFound,
                format!("Policy not found: {}", policy_id),
            ))?;

        // Check ownership
        if policy.tenant_id != user_context.tenant_id {
            return Err(crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::Forbidden,
                "Access denied".to_string(),
            ));
        }

        let previous_state = serde_json::to_value(&policy)?;
        policies.remove(&policy_id);

        // Audit log
        self.log_audit_event(
            policy_id,
            AuditEventType::PolicyDeleted,
            &user_context.user_id,
            AuditDetails {
                previous_state: Some(previous_state),
                new_state: serde_json::Value::Null,
                change_summary: "Policy deleted".to_string(),
                context: HashMap::new(),
            },
            request_metadata,
        ).await;

        Ok(())
    }

    /// List policies with filtering and pagination
    pub async fn list_policies(
        &self,
        request: ListPoliciesRequest,
        user_context: &UserContext,
    ) -> crate::utils::error::ArbitrageResult<ListPoliciesResponse> {
        let policies = self.policies.read().await;
        
        let mut filtered_policies: Vec<CleanupPolicy> = policies
            .values()
            .filter(|p| p.tenant_id == user_context.tenant_id)
            .filter(|p| {
                // Apply filters
                if let Some(status) = &request.filter.status {
                    if &p.status != status {
                        return false;
                    }
                }
                if let Some(policy_type) = &request.filter.policy_type {
                    if &p.policy_type != policy_type {
                        return false;
                    }
                }
                if let Some(search) = &request.filter.search {
                    if !p.name.contains(search) && 
                       !p.description.as_ref().unwrap_or(&String::new()).contains(search) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort
        match request.sort.field.as_str() {
            "name" => filtered_policies.sort_by(|a, b| a.name.cmp(&b.name)),
            "created_at" => filtered_policies.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
            "updated_at" => filtered_policies.sort_by(|a, b| a.updated_at.cmp(&b.updated_at)),
            _ => {} // Default order
        }

        if request.sort.direction == SortDirection::Desc {
            filtered_policies.reverse();
        }

        // Pagination
        let total = filtered_policies.len();
        let start = request.pagination.offset as usize;
        let end = std::cmp::min(start + request.pagination.limit as usize, total);
        
        let page_policies = if start < total {
            filtered_policies[start..end].to_vec()
        } else {
            Vec::new()
        };

        Ok(ListPoliciesResponse {
            policies: page_policies,
            pagination: PaginationResponse {
                offset: request.pagination.offset,
                limit: request.pagination.limit,
                total: total as u64,
                has_next: end < total,
            },
        })
    }

    /// Validate a policy configuration
    async fn validate_policy(&self, policy: &CleanupPolicy) -> crate::utils::error::ArbitrageResult<()> {
        // Basic validation
        if policy.name.is_empty() {
            return Err(crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::ValidationError,
                "Policy name cannot be empty".to_string(),
            ));
        }

        if policy.rules.is_empty() {
            return Err(crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::ValidationError,
                "Policy must have at least one rule".to_string(),
            ));
        }

        if policy.targets.is_empty() {
            return Err(crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::ValidationError,
                "Policy must have at least one target".to_string(),
            ));
        }

        // Validate rules
        for rule in &policy.rules {
            self.validate_rule(rule).await?;
        }

        // Validate targets
        for target in &policy.targets {
            self.validate_target(target).await?;
        }

        Ok(())
    }

    /// Validate a cleanup rule
    async fn validate_rule(&self, _rule: &CleanupRule) -> crate::utils::error::ArbitrageResult<()> {
        // TODO: Implement rule-specific validation
        // - Validate condition expressions
        // - Check action parameters
        // - Verify resource access
        Ok(())
    }

    /// Validate a policy target
    async fn validate_target(&self, _target: &PolicyTarget) -> crate::utils::error::ArbitrageResult<()> {
        // TODO: Implement target validation
        // - Check target type exists
        // - Validate resource patterns
        // - Verify access permissions
        Ok(())
    }

    /// Check rate limiting for a tenant
    async fn check_rate_limit(&self, tenant_id: &str) -> crate::utils::error::ArbitrageResult<()> {
        if !self.config.rate_limit.per_tenant_limiting {
            return Ok(());
        }

        let mut limiter = self.rate_limiter.write().await;
        let now = now_system_time();
        
        let state = limiter.entry(tenant_id.to_string()).or_insert(RateLimitState {
            tokens: self.config.rate_limit.burst_capacity,
            last_refill: now,
            request_history: Vec::new(),
        });

        // Token bucket implementation
        let elapsed = now.duration_since(state.last_refill).unwrap_or(Duration::ZERO);
        let tokens_to_add = (elapsed.as_secs() * self.config.rate_limit.requests_per_minute as u64 / 60) as u32;
        
        state.tokens = std::cmp::min(
            self.config.rate_limit.burst_capacity,
            state.tokens + tokens_to_add
        );
        state.last_refill = now;

        if state.tokens == 0 {
            return Err(crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::RateLimited,
                "Rate limit exceeded".to_string(),
            ));
        }

        state.tokens -= 1;
        Ok(())
    }

    /// Check tenant policy limit
    async fn check_tenant_policy_limit(&self, tenant_id: &str) -> crate::utils::error::ArbitrageResult<()> {
        let policies = self.policies.read().await;
        let tenant_policy_count = policies.values()
            .filter(|p| p.tenant_id == tenant_id)
            .count();

        if tenant_policy_count >= self.config.max_policies_per_tenant {
            return Err(crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::QuotaExceeded,
                format!("Maximum policies per tenant exceeded: {}", self.config.max_policies_per_tenant),
            ));
        }

        Ok(())
    }

    /// Log an audit event
    async fn log_audit_event(
        &self,
        policy_id: Uuid,
        event_type: AuditEventType,
        user_id: &str,
        details: AuditDetails,
        request_metadata: RequestMetadata,
    ) {
        if !self.config.enable_audit_logging {
            return;
        }

        let audit_entry = PolicyAuditEntry {
            id: Uuid::new_v4(),
            timestamp: now_system_time(),
            policy_id,
            event_type,
            user_id: user_id.to_string(),
            details,
            request_metadata,
        };

        let mut audit_log = self.audit_log.write().await;
        audit_log.push(audit_entry);
    }

    /// Start the policy management interface
    pub async fn start(&self, _env: &worker::Env) -> crate::utils::error::ArbitrageResult<()> {
        // TODO: Initialize REST API endpoints
        // TODO: Start background tasks for policy execution
        // TODO: Load existing policies from storage
        Ok(())
    }

    /// Stop the policy management interface
    pub async fn stop(&self) -> crate::utils::error::ArbitrageResult<()> {
        // TODO: Gracefully stop background tasks
        // TODO: Save pending changes
        Ok(())
    }
}

#[async_trait::async_trait]
impl HealthCheck for PolicyManagementInterface {
    async fn health_check(&self) -> crate::utils::error::ArbitrageResult<()> {
        // Check connection to storage
        self.connection_manager.health_check().await?;
        
        // Check if configuration is valid
        if !self.config.enabled {
            return Err(crate::utils::error::ArbitrageError::new(
                crate::utils::error::ErrorKind::ServiceUnavailable,
                "Policy management interface is disabled".to_string(),
            ));
        }

        Ok(())
    }
}

/// Request structures for API operations

/// User context for request authorization
#[derive(Debug, Clone)]
pub struct UserContext {
    /// User identifier
    pub user_id: String,
    /// Tenant identifier
    pub tenant_id: String,
    /// User roles
    pub roles: Vec<String>,
    /// User permissions
    pub permissions: HashSet<String>,
}

/// Create policy request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePolicyRequest {
    /// Policy to create
    pub policy: CleanupPolicy,
    /// Request metadata
    pub request_metadata: RequestMetadata,
}

/// Update policy request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePolicyRequest {
    /// Updated policy name
    pub name: Option<String>,
    /// Updated policy description
    pub description: Option<String>,
    /// Updated policy status
    pub status: Option<PolicyStatus>,
    /// Updated policy rules
    pub rules: Option<Vec<CleanupRule>>,
    /// Updated policy configuration
    pub config: Option<PolicyConfiguration>,
    /// Request metadata
    pub request_metadata: RequestMetadata,
}

/// List policies request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPoliciesRequest {
    /// Filtering options
    pub filter: PolicyFilter,
    /// Sorting options
    pub sort: SortOptions,
    /// Pagination options
    pub pagination: PaginationRequest,
}

/// Policy filtering options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyFilter {
    /// Filter by status
    pub status: Option<PolicyStatus>,
    /// Filter by policy type
    pub policy_type: Option<PolicyType>,
    /// Search in name and description
    pub search: Option<String>,
    /// Filter by tags
    pub tags: Option<Vec<String>>,
    /// Filter by owner
    pub owner: Option<String>,
}

/// Sorting options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortOptions {
    /// Sort field
    pub field: String,
    /// Sort direction
    pub direction: SortDirection,
}

/// Sort direction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Pagination request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationRequest {
    /// Offset from start
    pub offset: u64,
    /// Maximum items to return
    pub limit: u64,
}

/// List policies response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPoliciesResponse {
    /// List of policies
    pub policies: Vec<CleanupPolicy>,
    /// Pagination information
    pub pagination: PaginationResponse,
}

/// Pagination response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationResponse {
    /// Current offset
    pub offset: u64,
    /// Current limit
    pub limit: u64,
    /// Total items available
    pub total: u64,
    /// Whether there are more items
    pub has_next: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_policy_management_interface_creation() {
        let config = PolicyManagementConfig::default();
        let connection_manager = Arc::new(
            ConnectionManager::new(Default::default()).await.unwrap()
        );
        let transaction_coordinator = Arc::new(
            TransactionCoordinator::new(Default::default()).await.unwrap()
        );

        let interface = PolicyManagementInterface::new(
            config,
            connection_manager,
            transaction_coordinator,
        ).await;

        assert!(interface.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let mut config = PolicyManagementConfig::default();
        config.rate_limit.requests_per_minute = 1;
        config.rate_limit.burst_capacity = 1;

        let connection_manager = Arc::new(
            ConnectionManager::new(Default::default()).await.unwrap()
        );
        let transaction_coordinator = Arc::new(
            TransactionCoordinator::new(Default::default()).await.unwrap()
        );

        let interface = PolicyManagementInterface::new(
            config,
            connection_manager,
            transaction_coordinator,
        ).await.unwrap();

        // First request should succeed
        assert!(interface.check_rate_limit("tenant1").await.is_ok());

        // Second request should fail (burst capacity exhausted)
        assert!(interface.check_rate_limit("tenant1").await.is_err());
    }

    #[test]
    fn test_policy_serialization() {
        let policy = CleanupPolicy {
            id: Uuid::new_v4(),
            name: "Test Policy".to_string(),
            description: Some("Test Description".to_string()),
            version: 1,
            status: PolicyStatus::Draft,
            policy_type: PolicyType::TimeToLive,
            targets: vec![],
            rules: vec![],
            metadata: PolicyMetadata {
                tags: vec![],
                category: "test".to_string(),
                owner: "test_user".to_string(),
                contact: "test@example.com".to_string(),
                documentation_url: None,
                cost_estimate: None,
                expected_reduction: None,
            },
            config: PolicyConfiguration {
                dry_run: true,
                schedule: PolicySchedule {
                    schedule_type: ScheduleType::Manual,
                    cron_expression: None,
                    interval: None,
                    timezone: "UTC".to_string(),
                    enable_jitter: false,
                },
                retry_config: PolicyRetryConfig {
                    max_retries: 3,
                    retry_delay: RetryDelay::Fixed(Duration::from_secs(60)),
                    retry_on_errors: vec![],
                    stop_on_errors: vec![],
                },
                notifications: PolicyNotifications {
                    enabled: false,
                    channels: vec![],
                    events: vec![],
                    templates: HashMap::new(),
                },
                safety: PolicySafetyConfig {
                    max_items_per_execution: None,
                    max_execution_time: None,
                    require_approval_threshold: None,
                    enable_circuit_breaker: true,
                    backup_requirements: vec![],
                },
            },
            validation: PolicyValidation {
                rules: vec![],
                require_impact_analysis: false,
                testing_requirements: TestingRequirements {
                    require_dry_run: true,
                    min_test_duration: None,
                    required_scenarios: vec![],
                    performance_thresholds: HashMap::new(),
                },
            },
            tenant_id: "test_tenant".to_string(),
            created_at: now_system_time(),
            updated_at: now_system_time(),
            created_by: "test_user".to_string(),
            modified_by: "test_user".to_string(),
        };

        let serialized = serde_json::to_string(&policy).unwrap();
        let deserialized: CleanupPolicy = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(policy.id, deserialized.id);
        assert_eq!(policy.name, deserialized.name);
        assert_eq!(policy.status, deserialized.status);
    }
} 