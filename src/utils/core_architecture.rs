use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::RwLock;

#[cfg(target_arch = "wasm32")]
use std::sync::RwLock;

// Helper macros for conditional async/sync operations
#[cfg(not(target_arch = "wasm32"))]
macro_rules! read_lock {
    ($lock:expr) => {
        $lock.read().await
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! write_lock {
    ($lock:expr) => {
        $lock.write().await
    };
}

#[cfg(target_arch = "wasm32")]
macro_rules! read_lock {
    ($lock:expr) => {
        $lock.read().unwrap()
    };
}

#[cfg(target_arch = "wasm32")]
macro_rules! write_lock {
    ($lock:expr) => {
        $lock.write().unwrap()
    };
}

/// Service Health Status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Starting,
    Stopping,
    Stopped,
}

/// Service Type Identifiers
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    TelegramService,
    GlobalOpportunityService,
    TechnicalAnalysisService,
    AiBetaIntegrationService,
    NotificationService,
    ExchangeService,
    UserProfileService,
    MarketAnalysisService,
    OpportunityCategorizationService,
    D1DatabaseService,
    CorrelationAnalysisService,
    FundMonitoringService,
}

/// Service Health Check Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub service_type: ServiceType,
    pub status: ServiceStatus,
    pub response_time_ms: Option<u64>,
    pub last_check: u64,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl HealthCheckResult {
    pub fn healthy(service_type: ServiceType, response_time_ms: u64) -> Self {
        Self {
            service_type,
            status: ServiceStatus::Healthy,
            response_time_ms: Some(response_time_ms),
            last_check: chrono::Utc::now().timestamp_millis() as u64,
            error_message: None,
            metadata: HashMap::new(),
        }
    }

    pub fn unhealthy(service_type: ServiceType, error: String) -> Self {
        Self {
            service_type,
            status: ServiceStatus::Unhealthy,
            response_time_ms: None,
            last_check: chrono::Utc::now().timestamp_millis() as u64,
            error_message: Some(error),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Service Health Check Trait
#[async_trait::async_trait]
pub trait HealthCheckable {
    async fn health_check(&self) -> HealthCheckResult;
    fn service_type(&self) -> ServiceType;
}

/// Service Lifecycle Management Trait
#[async_trait::async_trait]
pub trait ServiceLifecycle {
    async fn start(&mut self) -> ArbitrageResult<()>;
    async fn stop(&mut self) -> ArbitrageResult<()>;
    async fn restart(&mut self) -> ArbitrageResult<()> {
        self.stop().await?;
        self.start().await
    }
    fn is_running(&self) -> bool;
}

/// Service Dependencies Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDependency {
    pub service_type: ServiceType,
    pub required: bool, // If true, this service cannot start without this dependency
    pub start_order: u32, // Lower numbers start first
}

/// Service Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub service_type: ServiceType,
    pub enabled: bool,
    pub dependencies: Vec<ServiceDependency>,
    pub health_check_interval_seconds: u64,
    pub restart_on_failure: bool,
    pub max_restart_attempts: u32,
    pub configuration: serde_json::Value,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            service_type: ServiceType::TelegramService,
            enabled: true,
            dependencies: Vec::new(),
            health_check_interval_seconds: 30,
            restart_on_failure: true,
            max_restart_attempts: 3,
            configuration: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

/// Service Registry Entry
#[derive(Debug)]
pub struct ServiceRegistryEntry {
    pub config: ServiceConfig,
    pub status: ServiceStatus,
    pub last_health_check: Option<HealthCheckResult>,
    pub restart_attempts: u32,
    pub started_at: Option<u64>,
    pub stopped_at: Option<u64>,
}

impl ServiceRegistryEntry {
    pub fn new(config: ServiceConfig) -> Self {
        Self {
            config,
            status: ServiceStatus::Stopped,
            last_health_check: None,
            restart_attempts: 0,
            started_at: None,
            stopped_at: None,
        }
    }
}

/// Core Service Architecture Manager
pub struct CoreServiceArchitecture {
    service_registry: Arc<RwLock<HashMap<ServiceType, ServiceRegistryEntry>>>,
    #[cfg(not(target_arch = "wasm32"))]
    health_check_tasks: HashMap<ServiceType, tokio::task::JoinHandle<()>>,
    #[cfg(target_arch = "wasm32")]
    health_check_tasks: HashMap<ServiceType, ()>, // Placeholder for WASM
    system_status: ServiceStatus,
    startup_order: Vec<ServiceType>,
}

impl CoreServiceArchitecture {
    pub fn new() -> Self {
        Self {
            service_registry: Arc::new(RwLock::new(HashMap::new())),
            health_check_tasks: HashMap::new(),
            system_status: ServiceStatus::Stopped,
            startup_order: Vec::new(),
        }
    }

    /// Initialize the service architecture with default configurations
    pub async fn initialize(&mut self) -> ArbitrageResult<()> {
        let default_configs = self.create_default_service_configs();

        for config in default_configs {
            self.register_service(config).await?;
        }

        self.calculate_startup_order().await?;
        Ok(())
    }

    /// Register a service in the architecture
    pub async fn register_service(&mut self, config: ServiceConfig) -> ArbitrageResult<()> {
        let service_type = config.service_type.clone();
        let entry = ServiceRegistryEntry::new(config);

        {
            let mut registry = write_lock!(self.service_registry);
            registry.insert(service_type, entry);
        }

        Ok(())
    }

    /// Start all services in dependency order
    pub async fn start_all_services(&mut self) -> ArbitrageResult<()> {
        self.system_status = ServiceStatus::Starting;

        let startup_order = self.startup_order.clone();
        for service_type in &startup_order {
            if let Err(e) = self.start_service(service_type.clone()).await {
                log::error!("Failed to start service {:?}: {}", service_type, e);
                self.system_status = ServiceStatus::Degraded;
                // Continue starting other services
            }
        }

        // Start health check tasks
        self.start_health_checks().await?;

        self.system_status = ServiceStatus::Healthy;
        Ok(())
    }

    /// Start a specific service
    pub async fn start_service(&mut self, service_type: ServiceType) -> ArbitrageResult<()> {
        // First, collect dependencies and check them
        let (enabled, dependencies) = {
            let registry = read_lock!(self.service_registry);
            if let Some(entry) = registry.get(&service_type) {
                (entry.config.enabled, entry.config.dependencies.clone())
            } else {
                return Ok(()); // Service not found
            }
        };

        if !enabled {
            return Ok(()); // Service is disabled
        }

        // Check dependencies
        {
            let registry = read_lock!(self.service_registry);
            for dep in &dependencies {
                if dep.required {
                    if let Some(dep_entry) = registry.get(&dep.service_type) {
                        if dep_entry.status != ServiceStatus::Healthy {
                            return Err(ArbitrageError::internal_error(format!(
                                "Required dependency {:?} is not healthy for service {:?}",
                                dep.service_type, service_type
                            )));
                        }
                    } else {
                        return Err(ArbitrageError::internal_error(format!(
                            "Required dependency {:?} not found for service {:?}",
                            dep.service_type, service_type
                        )));
                    }
                }
            }
        }

        // Update service status
        {
            let mut registry = write_lock!(self.service_registry);
            if let Some(entry) = registry.get_mut(&service_type) {
                entry.status = ServiceStatus::Starting;
                entry.started_at = Some(chrono::Utc::now().timestamp_millis() as u64);
                entry.restart_attempts = 0;
            }
        }

        match service_type {
            ServiceType::TelegramService => {
                log::info!("Initializing Telegram service instance");
            }
            ServiceType::D1DatabaseService => {
                log::info!("Initializing D1 database service instance");
            }
            ServiceType::ExchangeService => {
                log::info!("Initializing exchange service instance");
            }
            ServiceType::GlobalOpportunityService => {
                log::info!("Initializing global opportunity service instance");
            }
            ServiceType::NotificationService => {
                log::info!("Initializing notification service instance");
            }
            ServiceType::TechnicalAnalysisService => {
                log::info!("Initializing technical analysis service instance");
            }
            ServiceType::AiBetaIntegrationService => {
                log::info!("Initializing AI beta integration service instance");
            }
            ServiceType::UserProfileService => {
                log::info!("Initializing user profile service instance");
            }
            ServiceType::MarketAnalysisService => {
                log::info!("Initializing market analysis service instance");
            }
            ServiceType::OpportunityCategorizationService => {
                log::info!("Initializing opportunity categorization service instance");
            }
            ServiceType::CorrelationAnalysisService => {
                log::info!("Initializing correlation analysis service instance");
            }
            ServiceType::FundMonitoringService => {
                log::info!("Initializing fund monitoring service instance");
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        #[cfg(target_arch = "wasm32")]
        {
            // Use browser-compatible sleep for WASM with proper await
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                web_sys::window()
                    .unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 100)
                    .unwrap();
            });
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        }

        {
            let mut registry = write_lock!(self.service_registry);
            if let Some(entry) = registry.get_mut(&service_type) {
                entry.status = ServiceStatus::Healthy;
            }
        }

        log::info!("Service {:?} started successfully", service_type);
        Ok(())
    }

    /// Stop a specific service
    pub async fn stop_service(&mut self, service_type: ServiceType) -> ArbitrageResult<()> {
        {
            let mut registry = write_lock!(self.service_registry);
            if let Some(entry) = registry.get_mut(&service_type) {
                entry.status = ServiceStatus::Stopping;
            }
        }

        // Stop health check task if running
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(task) = self.health_check_tasks.remove(&service_type) {
            task.abort();
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.health_check_tasks.remove(&service_type);
        }

        match service_type {
            ServiceType::TelegramService => {
                log::info!("Shutting down Telegram service instance");
            }
            ServiceType::D1DatabaseService => {
                log::info!("Shutting down D1 database service instance");
            }
            ServiceType::ExchangeService => {
                log::info!("Shutting down exchange service instance");
            }
            ServiceType::GlobalOpportunityService => {
                log::info!("Shutting down global opportunity service instance");
            }
            ServiceType::NotificationService => {
                log::info!("Shutting down notification service instance");
            }
            ServiceType::TechnicalAnalysisService => {
                log::info!("Shutting down technical analysis service instance");
            }
            ServiceType::AiBetaIntegrationService => {
                log::info!("Shutting down AI beta integration service instance");
            }
            ServiceType::UserProfileService => {
                log::info!("Shutting down user profile service instance");
            }
            ServiceType::MarketAnalysisService => {
                log::info!("Shutting down market analysis service instance");
            }
            ServiceType::OpportunityCategorizationService => {
                log::info!("Shutting down opportunity categorization service instance");
            }
            ServiceType::CorrelationAnalysisService => {
                log::info!("Shutting down correlation analysis service instance");
            }
            ServiceType::FundMonitoringService => {
                log::info!("Shutting down fund monitoring service instance");
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(tokio::time::Duration::from_millis(25)).await;

        #[cfg(target_arch = "wasm32")]
        {
            // Use browser-compatible sleep for WASM with proper await
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                web_sys::window()
                    .unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50)
                    .unwrap();
            });
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        }

        {
            let mut registry = write_lock!(self.service_registry);
            if let Some(entry) = registry.get_mut(&service_type) {
                entry.status = ServiceStatus::Stopped;
                entry.stopped_at = Some(chrono::Utc::now().timestamp_millis() as u64);
            }
        }

        log::info!("Service {:?} stopped successfully", service_type);
        Ok(())
    }

    /// Stop all services in reverse dependency order
    pub async fn stop_all_services(&mut self) -> ArbitrageResult<()> {
        self.system_status = ServiceStatus::Stopping;

        // Stop health checks first
        #[cfg(not(target_arch = "wasm32"))]
        for (_, task) in self.health_check_tasks.drain() {
            task.abort();
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.health_check_tasks.clear();
        }

        // Stop services in reverse order
        let startup_order = self.startup_order.clone();
        for service_type in startup_order.iter().rev() {
            if let Err(e) = self.stop_service(service_type.clone()).await {
                log::error!("Failed to stop service {:?}: {}", service_type, e);
            }
        }

        self.system_status = ServiceStatus::Stopped;
        Ok(())
    }

    /// Restart a specific service
    pub async fn restart_service(&mut self, service_type: ServiceType) -> ArbitrageResult<()> {
        self.stop_service(service_type.clone()).await?;

        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        #[cfg(target_arch = "wasm32")]
        {
            // Use browser-compatible sleep for WASM with proper await
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                web_sys::window()
                    .unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 100)
                    .unwrap();
            });
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        }

        self.start_service(service_type).await
    }

    /// Get system health overview
    pub async fn get_system_health(&self) -> SystemHealthOverview {
        let registry = read_lock!(self.service_registry);
        let mut healthy_count = 0;
        let mut unhealthy_count = 0;
        let mut degraded_count = 0;
        let mut stopped_count = 0;
        let mut service_statuses = HashMap::new();

        for (service_type, entry) in registry.iter() {
            match entry.status {
                ServiceStatus::Healthy => healthy_count += 1,
                ServiceStatus::Unhealthy => unhealthy_count += 1,
                ServiceStatus::Degraded => degraded_count += 1,
                ServiceStatus::Stopped => stopped_count += 1,
                _ => degraded_count += 1,
            }
            service_statuses.insert(service_type.clone(), entry.status.clone());
        }

        let overall_status = if unhealthy_count > 0 {
            ServiceStatus::Unhealthy
        } else if degraded_count > 0 {
            ServiceStatus::Degraded
        } else if healthy_count > 0 {
            ServiceStatus::Healthy
        } else {
            ServiceStatus::Stopped
        };

        SystemHealthOverview {
            overall_status,
            total_services: registry.len(),
            healthy_count,
            unhealthy_count,
            degraded_count,
            stopped_count,
            service_statuses,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    /// Get detailed service information
    pub async fn get_service_info(&self, service_type: &ServiceType) -> Option<ServiceInfo> {
        let registry = read_lock!(self.service_registry);
        registry.get(service_type).map(|entry| ServiceInfo {
            service_type: entry.config.service_type.clone(),
            status: entry.status.clone(),
            enabled: entry.config.enabled,
            dependencies: entry.config.dependencies.clone(),
            last_health_check: entry.last_health_check.clone(),
            restart_attempts: entry.restart_attempts,
            started_at: entry.started_at,
            stopped_at: entry.stopped_at,
            uptime_seconds: entry
                .started_at
                .map(|start| (chrono::Utc::now().timestamp_millis() as u64 - start) / 1000),
        })
    }

    /// Start health check monitoring for all services
    async fn start_health_checks(&mut self) -> ArbitrageResult<()> {
        #[cfg(target_arch = "wasm32")]
        {
            // Health checks not available in WASM
            return Ok(());
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let registry = self.service_registry.clone();

            // Get enabled services
            let services_to_monitor: Vec<ServiceType> = {
                let reg = registry.read().await;
                reg.iter()
                    .filter(|(_, entry)| entry.config.enabled)
                    .map(|(service_type, _)| service_type.clone())
                    .collect()
            };

            for service_type in services_to_monitor {
                let registry_clone = registry.clone();
                let service_type_clone = service_type.clone();

                let task = tokio::spawn(async move {
                    loop {
                        let interval = {
                            let reg = registry_clone.read().await;
                            if let Some(entry) = reg.get(&service_type_clone) {
                                entry.config.health_check_interval_seconds
                            } else {
                                break; // Service removed
                            }
                        };

                        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;

                        // Perform health check
                        let health_result = {
                            let start_time = crate::utils::time::now_instant();

                            let is_healthy = match service_type_clone {
                                ServiceType::TelegramService => true,
                                ServiceType::GlobalOpportunityService => true,
                                ServiceType::D1DatabaseService => {
                                    // Simulate occasional database issues
                                    (chrono::Utc::now().timestamp() % 10) != 0
                                }
                                _ => true,
                            };

                            let response_time = start_time.elapsed().as_millis() as u64;

                            if is_healthy {
                                HealthCheckResult::healthy(
                                    service_type_clone.clone(),
                                    response_time,
                                )
                                .with_metadata(
                                    "last_check_type".to_string(),
                                    serde_json::json!("automated"),
                                )
                            } else {
                                HealthCheckResult::unhealthy(
                                    service_type_clone.clone(),
                                    "Service failed health check".to_string(),
                                )
                            }
                        };

                        // Update registry
                        {
                            let mut reg = registry_clone.write().await;
                            if let Some(entry) = reg.get_mut(&service_type_clone) {
                                entry.last_health_check = Some(health_result.clone());

                                // Update status based on health check
                                match health_result.status {
                                    ServiceStatus::Healthy => {
                                        if entry.status == ServiceStatus::Degraded
                                            || entry.status == ServiceStatus::Unhealthy
                                        {
                                            entry.status = ServiceStatus::Healthy;
                                            entry.restart_attempts = 0;
                                        }
                                    }
                                    ServiceStatus::Unhealthy => {
                                        entry.status = ServiceStatus::Unhealthy;
                                        entry.restart_attempts += 1;

                                        // Check if restart is enabled and we haven't exceeded max attempts
                                        if entry.config.restart_on_failure
                                            && entry.restart_attempts
                                                <= entry.config.max_restart_attempts
                                        {
                                            log::warn!(
                                                "Service {:?} is unhealthy, initiating restart (attempt {}/{})",
                                                service_type_clone,
                                                entry.restart_attempts,
                                                entry.config.max_restart_attempts
                                            );

                                            // Trigger restart in background
                                            let service_type_for_restart =
                                                service_type_clone.clone();
                                            let registry_for_restart = registry_clone.clone();

                                            tokio::spawn(async move {
                                                // Simulate restart process - in production this would call actual service restart
                                                log::info!(
                                                    "Starting restart process for service {:?}",
                                                    service_type_for_restart
                                                );

                                                // Mark service as restarting
                                                {
                                                    let mut reg =
                                                        registry_for_restart.write().await;
                                                    if let Some(entry) =
                                                        reg.get_mut(&service_type_for_restart)
                                                    {
                                                        entry.status = ServiceStatus::Starting;
                                                    }
                                                }

                                                // Simulate restart delay
                                                tokio::time::sleep(
                                                    tokio::time::Duration::from_secs(2),
                                                )
                                                .await;

                                                let restart_successful = {
                                                    let reg = registry_for_restart.read().await;
                                                    if let Some(entry) =
                                                        reg.get(&service_type_for_restart)
                                                    {
                                                        if entry.restart_attempts >= 3 {
                                                            false
                                                        } else {
                                                            // Attempt service restart
                                                            log::info!("Attempting to restart service: {:?}", service_type_for_restart);
                                                            true // Simulate successful restart for now
                                                        }
                                                    } else {
                                                        false
                                                    }
                                                };

                                                {
                                                    let mut reg =
                                                        registry_for_restart.write().await;
                                                    if let Some(entry) =
                                                        reg.get_mut(&service_type_for_restart)
                                                    {
                                                        if restart_successful {
                                                            entry.status = ServiceStatus::Healthy;
                                                            entry.restart_attempts = 0;
                                                            entry.started_at = Some(
                                                                chrono::Utc::now()
                                                                    .timestamp_millis()
                                                                    as u64,
                                                            );
                                                            log::info!("Successfully restarted service {:?}", service_type_for_restart);
                                                        } else {
                                                            entry.status = ServiceStatus::Unhealthy;
                                                            log::error!(
                                                                "Failed to restart service {:?}",
                                                                service_type_for_restart
                                                            );
                                                        }
                                                    }
                                                }
                                            });
                                        } else if entry.restart_attempts
                                            > entry.config.max_restart_attempts
                                        {
                                            log::error!(
                                                "Service {:?} exceeded maximum restart attempts ({}), service will remain unhealthy",
                                                service_type_clone,
                                                entry.config.max_restart_attempts
                                            );
                                        }
                                    }
                                    ServiceStatus::Degraded => {
                                        if entry.status == ServiceStatus::Healthy {
                                            entry.status = ServiceStatus::Degraded;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                });

                self.health_check_tasks.insert(service_type, task);
            }

            Ok(())
        }
    }

    /// Calculate service startup order based on dependencies using iterative topological sort
    async fn calculate_startup_order(&mut self) -> ArbitrageResult<()> {
        let registry = read_lock!(self.service_registry);
        let mut order = Vec::new();
        let mut in_degree = std::collections::HashMap::new();
        let mut graph = std::collections::HashMap::new();

        // Build dependency graph and calculate in-degrees
        for (service_type, _entry) in registry.iter() {
            in_degree.insert(service_type.clone(), 0);
            graph.insert(service_type.clone(), Vec::new());
        }

        // Calculate in-degrees and build adjacency list
        for (_service_type, entry) in registry.iter() {
            let mut deps = entry.config.dependencies.clone();
            deps.sort_by_key(|d| d.start_order);

            for dep in deps {
                // Dependency -> Current service edge
                if let Some(dependents) = graph.get_mut(&dep.service_type) {
                    dependents.push(entry.config.service_type.clone());
                }
                if let Some(degree) = in_degree.get_mut(&entry.config.service_type) {
                    *degree += 1;
                }
            }
        }

        // Kahn's algorithm for topological sorting
        let mut queue = std::collections::VecDeque::new();

        // Start with services that have no dependencies
        for (service_type, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(service_type.clone());
            }
        }

        while let Some(current) = queue.pop_front() {
            order.push(current.clone());

            // Process all services that depend on the current service
            if let Some(dependents) = graph.get(&current) {
                for dependent in dependents {
                    if let Some(degree) = in_degree.get_mut(dependent) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent.clone());
                        }
                    }
                }
            }
        }

        // Check for circular dependencies
        if order.len() != registry.len() {
            return Err(ArbitrageError::internal_error(
                "Circular dependency detected in service configuration".to_string(),
            ));
        }

        self.startup_order = order;
        Ok(())
    }

    /// Validate service configurations for logical consistency
    pub fn validate_service_configs(configs: &[ServiceConfig]) -> ArbitrageResult<()> {
        use std::collections::HashSet;

        // Track existing service types
        let existing_services: HashSet<ServiceType> = configs
            .iter()
            .map(|config| config.service_type.clone())
            .collect();

        for config in configs {
            // Check for self-dependency
            for dependency in &config.dependencies {
                if dependency.service_type == config.service_type {
                    return Err(ArbitrageError::validation_error(format!(
                        "Service {:?} cannot depend on itself",
                        config.service_type
                    )));
                }

                // Check that dependency exists in the configuration set
                if !existing_services.contains(&dependency.service_type) {
                    return Err(ArbitrageError::validation_error(format!(
                        "Service {:?} depends on {:?} which is not defined in the configuration",
                        config.service_type, dependency.service_type
                    )));
                }
            }
        }

        Ok(())
    }

    /// Create default service configurations
    fn create_default_service_configs(&self) -> Vec<ServiceConfig> {
        vec![
            // Database service - no dependencies, starts first
            ServiceConfig {
                service_type: ServiceType::D1DatabaseService,
                enabled: true,
                dependencies: vec![],
                health_check_interval_seconds: 15,
                restart_on_failure: true,
                max_restart_attempts: 5,
                configuration: serde_json::json!({"database_url": "memory"}),
            },
            // Core services - depend on database
            ServiceConfig {
                service_type: ServiceType::UserProfileService,
                enabled: true,
                dependencies: vec![ServiceDependency {
                    service_type: ServiceType::D1DatabaseService,
                    required: true,
                    start_order: 1,
                }],
                health_check_interval_seconds: 30,
                restart_on_failure: true,
                max_restart_attempts: 3,
                configuration: serde_json::json!({}),
            },
            ServiceConfig {
                service_type: ServiceType::ExchangeService,
                enabled: true,
                dependencies: vec![ServiceDependency {
                    service_type: ServiceType::D1DatabaseService,
                    required: true,
                    start_order: 1,
                }],
                health_check_interval_seconds: 20,
                restart_on_failure: true,
                max_restart_attempts: 3,
                configuration: serde_json::json!({}),
            },
            // Analysis services
            ServiceConfig {
                service_type: ServiceType::MarketAnalysisService,
                enabled: true,
                dependencies: vec![ServiceDependency {
                    service_type: ServiceType::ExchangeService,
                    required: true,
                    start_order: 2,
                }],
                health_check_interval_seconds: 30,
                restart_on_failure: true,
                max_restart_attempts: 3,
                configuration: serde_json::json!({}),
            },
            ServiceConfig {
                service_type: ServiceType::TechnicalAnalysisService,
                enabled: true,
                dependencies: vec![ServiceDependency {
                    service_type: ServiceType::ExchangeService,
                    required: true,
                    start_order: 2,
                }],
                health_check_interval_seconds: 60,
                restart_on_failure: true,
                max_restart_attempts: 3,
                configuration: serde_json::json!({}),
            },
            // AI services
            ServiceConfig {
                service_type: ServiceType::AiBetaIntegrationService,
                enabled: true,
                dependencies: vec![
                    ServiceDependency {
                        service_type: ServiceType::MarketAnalysisService,
                        required: true,
                        start_order: 3,
                    },
                    ServiceDependency {
                        service_type: ServiceType::UserProfileService,
                        required: true,
                        start_order: 2,
                    },
                ],
                health_check_interval_seconds: 45,
                restart_on_failure: true,
                max_restart_attempts: 2,
                configuration: serde_json::json!({}),
            },
            // Opportunity services
            ServiceConfig {
                service_type: ServiceType::GlobalOpportunityService,
                enabled: true,
                dependencies: vec![
                    ServiceDependency {
                        service_type: ServiceType::MarketAnalysisService,
                        required: true,
                        start_order: 3,
                    },
                    ServiceDependency {
                        service_type: ServiceType::TechnicalAnalysisService,
                        required: false,
                        start_order: 3,
                    },
                ],
                health_check_interval_seconds: 30,
                restart_on_failure: true,
                max_restart_attempts: 3,
                configuration: serde_json::json!({}),
            },
            // Notification and communication services
            ServiceConfig {
                service_type: ServiceType::NotificationService,
                enabled: true,
                dependencies: vec![],
                health_check_interval_seconds: 30,
                restart_on_failure: true,
                max_restart_attempts: 3,
                configuration: serde_json::json!({}),
            },
            ServiceConfig {
                service_type: ServiceType::TelegramService,
                enabled: true,
                dependencies: vec![
                    ServiceDependency {
                        service_type: ServiceType::NotificationService,
                        required: true,
                        start_order: 4,
                    },
                    ServiceDependency {
                        service_type: ServiceType::GlobalOpportunityService,
                        required: true,
                        start_order: 4,
                    },
                ],
                health_check_interval_seconds: 20,
                restart_on_failure: true,
                max_restart_attempts: 5,
                configuration: serde_json::json!({}),
            },
        ]
    }
}

impl Default for CoreServiceArchitecture {
    fn default() -> Self {
        Self::new()
    }
}

/// System Health Overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthOverview {
    pub overall_status: ServiceStatus,
    pub total_services: usize,
    pub healthy_count: usize,
    pub unhealthy_count: usize,
    pub degraded_count: usize,
    pub stopped_count: usize,
    pub service_statuses: HashMap<ServiceType, ServiceStatus>,
    pub last_updated: u64,
}

/// Detailed Service Information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub service_type: ServiceType,
    pub status: ServiceStatus,
    pub enabled: bool,
    pub dependencies: Vec<ServiceDependency>,
    pub last_health_check: Option<HealthCheckResult>,
    pub restart_attempts: u32,
    pub started_at: Option<u64>,
    pub stopped_at: Option<u64>,
    pub uptime_seconds: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_architecture_creation() {
        let mut architecture = CoreServiceArchitecture::new();
        assert_eq!(architecture.system_status, ServiceStatus::Stopped);

        architecture.initialize().await.unwrap();
        assert!(!architecture.startup_order.is_empty());
    }

    #[tokio::test]
    async fn test_service_registration() {
        let mut architecture = CoreServiceArchitecture::new();

        let config = ServiceConfig {
            service_type: ServiceType::TelegramService,
            enabled: true,
            dependencies: vec![],
            health_check_interval_seconds: 30,
            restart_on_failure: true,
            max_restart_attempts: 3,
            configuration: serde_json::json!({}),
        };

        architecture.register_service(config).await.unwrap();

        let info = architecture
            .get_service_info(&ServiceType::TelegramService)
            .await;
        assert!(info.is_some());
        assert_eq!(info.unwrap().status, ServiceStatus::Stopped);
    }

    #[tokio::test]
    async fn test_service_startup_order() {
        let mut architecture = CoreServiceArchitecture::new();
        architecture.initialize().await.unwrap();

        // Verify we have services in the startup order
        assert!(!architecture.startup_order.is_empty());

        // Find positions of key services
        let telegram_pos = architecture
            .startup_order
            .iter()
            .position(|s| s == &ServiceType::TelegramService);

        let notification_pos = architecture
            .startup_order
            .iter()
            .position(|s| s == &ServiceType::NotificationService);

        // Both services should exist in startup order
        assert!(
            telegram_pos.is_some(),
            "TelegramService should be in startup order"
        );
        assert!(
            notification_pos.is_some(),
            "NotificationService should be in startup order"
        );

        // Notification service should start before Telegram (since Telegram depends on it)
        assert!(notification_pos.unwrap() < telegram_pos.unwrap());
    }

    #[tokio::test]
    async fn test_health_check_result() {
        let healthy = HealthCheckResult::healthy(ServiceType::TelegramService, 150);
        assert_eq!(healthy.status, ServiceStatus::Healthy);
        assert_eq!(healthy.response_time_ms, Some(150));

        let unhealthy = HealthCheckResult::unhealthy(
            ServiceType::D1DatabaseService,
            "Connection failed".to_string(),
        );
        assert_eq!(unhealthy.status, ServiceStatus::Unhealthy);
        assert!(unhealthy.error_message.is_some());
    }

    #[tokio::test]
    async fn test_system_health_overview() {
        let mut architecture = CoreServiceArchitecture::new();
        architecture.initialize().await.unwrap();

        let health = architecture.get_system_health().await;
        assert!(health.total_services > 0);
        assert_eq!(health.overall_status, ServiceStatus::Stopped); // Not started yet
    }

    #[test]
    fn test_service_config_default() {
        let config = ServiceConfig::default();
        assert_eq!(config.service_type, ServiceType::TelegramService);
        assert!(config.enabled);
        assert_eq!(config.health_check_interval_seconds, 30);
        assert!(config.restart_on_failure);
        assert_eq!(config.max_restart_attempts, 3);
    }

    #[test]
    fn test_service_dependency() {
        let dep = ServiceDependency {
            service_type: ServiceType::D1DatabaseService,
            required: true,
            start_order: 1,
        };

        assert_eq!(dep.service_type, ServiceType::D1DatabaseService);
        assert!(dep.required);
        assert_eq!(dep.start_order, 1);
    }

    #[tokio::test]
    async fn test_circular_dependency_detection() {
        let mut architecture = CoreServiceArchitecture::new();

        // Create services with circular dependencies: A -> B -> C -> A
        let config_a = ServiceConfig {
            service_type: ServiceType::TelegramService,
            enabled: true,
            dependencies: vec![ServiceDependency {
                service_type: ServiceType::NotificationService,
                required: true,
                start_order: 1,
            }],
            health_check_interval_seconds: 30,
            restart_on_failure: true,
            max_restart_attempts: 3,
            configuration: serde_json::json!({}),
        };

        let config_b = ServiceConfig {
            service_type: ServiceType::NotificationService,
            enabled: true,
            dependencies: vec![ServiceDependency {
                service_type: ServiceType::ExchangeService,
                required: true,
                start_order: 1,
            }],
            health_check_interval_seconds: 30,
            restart_on_failure: true,
            max_restart_attempts: 3,
            configuration: serde_json::json!({}),
        };

        let config_c = ServiceConfig {
            service_type: ServiceType::ExchangeService,
            enabled: true,
            dependencies: vec![ServiceDependency {
                service_type: ServiceType::TelegramService, // Circular dependency!
                required: true,
                start_order: 1,
            }],
            health_check_interval_seconds: 30,
            restart_on_failure: true,
            max_restart_attempts: 3,
            configuration: serde_json::json!({}),
        };

        architecture.register_service(config_a).await.unwrap();
        architecture.register_service(config_b).await.unwrap();
        architecture.register_service(config_c).await.unwrap();

        // This should detect the circular dependency
        let result = architecture.calculate_startup_order().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Circular dependency detected"));
    }

    #[tokio::test]
    async fn test_service_validation() {
        // Test self-dependency validation
        let configs_with_self_dep = vec![ServiceConfig {
            service_type: ServiceType::TelegramService,
            enabled: true,
            dependencies: vec![ServiceDependency {
                service_type: ServiceType::TelegramService, // Self-dependency
                required: true,
                start_order: 1,
            }],
            health_check_interval_seconds: 30,
            restart_on_failure: true,
            max_restart_attempts: 3,
            configuration: serde_json::json!({}),
        }];

        let result = CoreServiceArchitecture::validate_service_configs(&configs_with_self_dep);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot depend on itself"));

        // Test missing dependency validation
        let configs_with_missing_dep = vec![ServiceConfig {
            service_type: ServiceType::TelegramService,
            enabled: true,
            dependencies: vec![ServiceDependency {
                service_type: ServiceType::NotificationService, // Missing service
                required: true,
                start_order: 1,
            }],
            health_check_interval_seconds: 30,
            restart_on_failure: true,
            max_restart_attempts: 3,
            configuration: serde_json::json!({}),
        }];

        let result = CoreServiceArchitecture::validate_service_configs(&configs_with_missing_dep);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not defined in the configuration"));

        // Test valid configuration
        let valid_configs = vec![
            ServiceConfig {
                service_type: ServiceType::D1DatabaseService,
                enabled: true,
                dependencies: vec![],
                health_check_interval_seconds: 30,
                restart_on_failure: true,
                max_restart_attempts: 3,
                configuration: serde_json::json!({}),
            },
            ServiceConfig {
                service_type: ServiceType::TelegramService,
                enabled: true,
                dependencies: vec![ServiceDependency {
                    service_type: ServiceType::D1DatabaseService,
                    required: true,
                    start_order: 1,
                }],
                health_check_interval_seconds: 30,
                restart_on_failure: true,
                max_restart_attempts: 3,
                configuration: serde_json::json!({}),
            },
        ];

        let result = CoreServiceArchitecture::validate_service_configs(&valid_configs);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_service_operations() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let architecture = Arc::new(Mutex::new(CoreServiceArchitecture::new()));
        let mut handles = vec![];

        // Test concurrent service registration
        for i in 0..5 {
            let arch_clone = Arc::clone(&architecture);
            let handle = tokio::spawn(async move {
                let mut arch = arch_clone.lock().await;
                let config = ServiceConfig {
                    service_type: match i {
                        0 => ServiceType::D1DatabaseService,
                        1 => ServiceType::ExchangeService,
                        2 => ServiceType::TelegramService,
                        3 => ServiceType::NotificationService,
                        _ => ServiceType::UserProfileService,
                    },
                    enabled: true,
                    dependencies: vec![],
                    health_check_interval_seconds: 30,
                    restart_on_failure: true,
                    max_restart_attempts: 3,
                    configuration: serde_json::json!({}),
                };
                arch.register_service(config).await
            });
            handles.push(handle);
        }

        // Wait for all concurrent operations to complete
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        // Verify all services were registered
        let arch = architecture.lock().await;
        let health = arch.get_system_health().await;
        assert_eq!(health.total_services, 5);
    }
}
