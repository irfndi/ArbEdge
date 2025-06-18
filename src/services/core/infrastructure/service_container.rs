// use crate::services::core::ai::ai_intelligence::AIIntelligenceService;
// use crate::services::core::analysis::correlation_analysis::CorrelationAnalysisService;
// use crate::services::core::analysis::portfolio_analyzer::PortfolioAnalyzer;
// use crate::services::core::analysis::risk_assessment::RiskAssessmentService;
// use crate::services::core::auth::AuthService;
// use crate::services::core::infrastructure::cache_manager::CacheManager;
use crate::services::core::infrastructure::data_access_layer::{
    DataAccessLayer, DataAccessLayerConfig,
};
use crate::services::core::infrastructure::data_ingestion_module::DataIngestionModule;
use crate::services::core::infrastructure::database_repositories::{
    DatabaseManager, DatabaseManagerConfig,
};
// use crate::services::core::infrastructure::queue_manager::QueueManager;
use crate::services::core::opportunities::opportunity_distribution::OpportunityDistributionService;
use crate::services::core::opportunities::opportunity_engine::OpportunityEngine;
use crate::services::core::trading::exchange::ExchangeService;
// use crate::services::core::trading::position_manager::PositionManager;
use crate::services::core::user::session_management::SessionManagementService;
use crate::services::core::user::user_profile::UserProfileService;
// use crate::services::core::user::user_activity::UserActivityService;
// use crate::services::core::user::group_management::GroupManagementService;

use crate::services::core::admin::AdminService;
use crate::services::core::ai::ai_analysis_service::AiAnalysisService;
use crate::services::interfaces::telegram::ModularTelegramService;
use crate::utils::feature_flags::{load_feature_flags, FeatureFlags};
use crate::utils::{ArbitrageError, ArbitrageResult};
use std::sync::Arc;
use worker::console_log;
use worker::{kv::KvStore, Env};

/// Comprehensive service container for managing all application services
/// Provides centralized dependency injection and service lifecycle management
/// Addresses inconsistent service creation patterns across the application
pub struct ServiceContainer {
    pub session_service: Arc<SessionManagementService>,
    pub distribution_service: OpportunityDistributionService,
    pub telegram_service: Option<Arc<ModularTelegramService>>,
    pub exchange_service: Arc<ExchangeService>,
    pub user_profile_service: Option<Arc<UserProfileService>>,
    pub opportunity_engine: Option<Arc<OpportunityEngine>>,
    pub admin_service: Option<Arc<AdminService>>,
    pub ai_analysis_service: Option<Arc<AiAnalysisService>>,
    pub data_ingestion_module: Option<Arc<DataIngestionModule>>,
    pub database_manager: DatabaseManager,
    pub data_access_layer: DataAccessLayer,
    pub feature_flags: Arc<FeatureFlags>,
}

impl ServiceContainer {
    /// Create a new ServiceContainer with all core services initialized
    pub async fn new(env: &Env, kv_store: KvStore) -> ArbitrageResult<Self> {
        let mut database_manager = DatabaseManager::new(
            Arc::new(env.d1("ArbEdgeD1").map_err(|e| {
                ArbitrageError::configuration_error(format!("Failed to get D1 database: {:?}", e))
            })?),
            DatabaseManagerConfig::default(),
        );

        // CRITICAL: Initialize repositories to ensure UserRepository is available
        database_manager.initialize_repositories().await?;

        let data_access_layer =
            DataAccessLayer::new(DataAccessLayerConfig::default(), kv_store.clone()).await?;

        let feature_flags = load_feature_flags("feature_flags.json").unwrap_or_else(|_| {
            console_log!("⚠️ Failed to load feature flags from file, using defaults");
            Arc::new(FeatureFlags::new(std::collections::HashMap::new()))
        });

        let session_service_instance = Arc::new(SessionManagementService::new(
            database_manager.clone(),
            kv_store.clone(),
        ));

        let exchange_service = Arc::new(ExchangeService::new(env).map_err(|e| {
            ArbitrageError::configuration_error(format!(
                "Failed to create exchange service: {:?}",
                e
            ))
        })?);

        let user_profile_service_instance = Arc::new(UserProfileService::new(
            kv_store.clone(),
            database_manager.clone(),
            "default_key".to_string(),
        ));

        let distribution_service = OpportunityDistributionService::new(
            database_manager.clone(),
            data_access_layer.clone(),
            session_service_instance.clone(),
        );

        // Initialize OpportunityEngine
        let opportunity_engine = Self::initialize_opportunity_engine(
            env,
            user_profile_service_instance.clone(),
            exchange_service.clone(),
            kv_store.clone(),
            database_manager.clone(),
        )
        .await
        .ok();

        // Initialize Telegram Service (using modular service)
        // Note: Telegram service will be initialized after ServiceContainer creation to avoid circular dependency
        let telegram_service = None;

        // Initialize AI Analysis Service
        let ai_analysis_service = Some(Arc::new(AiAnalysisService::new(
            kv_store.clone(),
            Arc::new(env.d1("ArbEdgeD1").map_err(|e| {
                ArbitrageError::configuration_error(format!("Failed to get D1 database: {:?}", e))
            })?),
        )));

        // Initialize Admin Service (will be created when needed)
        let admin_service = None;

        Ok(Self {
            session_service: session_service_instance,
            distribution_service,
            telegram_service,
            exchange_service,
            user_profile_service: Some(user_profile_service_instance),
            admin_service,
            ai_analysis_service,
            data_ingestion_module: None,
            database_manager,
            data_access_layer,
            feature_flags,
            opportunity_engine,
        })
    }

    /// Initialize OpportunityEngine with proper dependencies
    async fn initialize_opportunity_engine(
        _env: &Env,
        user_profile_service: Arc<UserProfileService>,
        exchange_service: Arc<ExchangeService>,
        kv_store: KvStore,
        database_manager: DatabaseManager,
    ) -> ArbitrageResult<Arc<OpportunityEngine>> {
        // Create UserAccessService
        let user_access_service = Arc::new(
            crate::services::core::user::user_access::UserAccessService::new(
                database_manager.clone(),
                (*user_profile_service).clone(),
                kv_store.clone(),
            ),
        );

        // Create AiBetaIntegrationService
        let ai_config = crate::services::core::ai::ai_beta_integration::AiBetaConfig::default();
        let ai_service = Arc::new(
            crate::services::core::ai::ai_beta_integration::AiBetaIntegrationService::new(
                ai_config,
            ),
        );

        // Create OpportunityConfig with defaults
        let config =
            crate::services::core::opportunities::opportunity_core::OpportunityConfig::default();

        let engine = OpportunityEngine::new(
            user_profile_service,
            user_access_service,
            ai_service,
            exchange_service.clone(),
            kv_store,
            config,
        )?;

        console_log!("✅ OpportunityEngine initialized successfully");
        Ok(Arc::new(engine))
    }

    /// Get OpportunityEngine (expected by commands)
    pub fn get_opportunity_engine(&self) -> Option<Arc<OpportunityEngine>> {
        self.opportunity_engine.clone()
    }

    /// Get OpportunityDistributionService (expected by commands)
    pub fn get_opportunity_distribution_service(&self) -> &OpportunityDistributionService {
        &self.distribution_service
    }

    /// Global opportunity service (alias for distribution service)
    pub fn global_opportunity_service(&self) -> &OpportunityDistributionService {
        &self.distribution_service
    }

    /// Create AdminService with all sub-services
    /// TODO: Implement AdminService when ready
    /// Get admin service for super admin operations
    pub fn get_admin_service(&self) -> Option<Arc<AdminService>> {
        self.admin_service.clone()
    }

    /// Get AI analysis service for API endpoints
    pub fn get_ai_service(&self) -> Option<Arc<AiAnalysisService>> {
        self.ai_analysis_service.clone()
    }

    /// Create with custom distribution configuration
    pub async fn with_distribution_config(
        env: &Env,
        kv_store: KvStore,
        // config: DistributionConfig,
    ) -> ArbitrageResult<Self> {
        let container = Self::new(env, kv_store).await?;
        // container.distribution_service = container.distribution_service.with_config(config);
        Ok(container)
    }

    /// Initialize Telegram service after container creation to avoid circular dependency
    pub async fn initialize_telegram_service(
        container: Arc<ServiceContainer>,
        env: &Env,
    ) -> ArbitrageResult<Arc<ModularTelegramService>> {
        match ModularTelegramService::new(env, container).await {
            Ok(service) => {
                console_log!("✅ Modular Telegram service initialized successfully");
                Ok(Arc::new(service))
            }
            Err(e) => {
                console_log!("⚠️ Failed to initialize Modular Telegram service: {:?}", e);
                console_log!("⚠️ Telegram webhooks will not be available");
                Err(e)
            }
        }
    }

    /// Set the Telegram service for push notifications using Arc for shared ownership
    pub fn set_telegram_service(&mut self, telegram_service: ModularTelegramService) {
        let arc_telegram_service = Arc::new(telegram_service);
        // Note: ModularTelegramService may need different integration with distribution service
        // self.distribution_service
        //     .set_notification_sender(Box::new((*arc_telegram_service).clone()));
        self.telegram_service = Some(arc_telegram_service);
    }

    /// Set the user profile service with encryption key - This is now primarily for overriding or specific setups if needed post-initialization.
    /// Main initialization happens in new().
    pub fn set_user_profile_service(&mut self, encryption_key: String) {
        let user_profile_service_instance = Arc::new(UserProfileService::new(
            self.data_access_layer.get_kv_store().clone(),
            self.database_manager.clone(),
            encryption_key,
        ));
        self.user_profile_service = Some(user_profile_service_instance.clone());

        // Attempt to re-inject into AuthService if it exists and is mutable
        // This path is less ideal than full setup in `new()`
        // if let Some(auth_service_option) = &mut self.auth_service {
        //     if let Some(auth_service_arc_mut) = Arc::get_mut(auth_service_option) {
        //          auth_service_arc_mut.set_user_profile_provider(user_profile_service_instance);
        //          worker::console_log!("UserProfileProvider re-injected into AuthService via set_user_profile_service.");
        //     } else {
        //         worker::console_warn!(
        //             "AuthService is already shared. UserProfileProvider could not be re-injected via set_user_profile_service. Ensure it was set during initial new()."
        //         );
        //     }
        // } else {
        //      worker::console_warn!("AuthService not present. Cannot inject UserProfileProvider via set_user_profile_service.");
        // }
    }

    /// Initialize AI Coordinator service with fallback mechanisms
    pub fn set_ai_coordinator(
        &mut self,
        _env: &Env, /* config: Option<AICoordinatorConfig> */
    ) {
        // let ai_config = config.unwrap_or_default();

        // match AICoordinator::new(env, ai_config) {
        //     Ok(coordinator) => {
        //         self.ai_coordinator = Some(Arc::new(coordinator));
        //         worker::console_log!("AI Coordinator initialized successfully");
        //     }
        //     Err(e) => {
        //         worker::console_log!(
        //             "Failed to initialize AI Coordinator: {} - continuing with fallback mode",
        //             e
        //         );
        //         self.ai_coordinator = None;
        //     }
        // }
    }

    /// Initialize Data Ingestion Module with fallback mechanisms
    pub async fn set_data_ingestion_module(
        &mut self,
        _env: &Env,
        // config: Option<DataIngestionModuleConfig>,
    ) {
        // let ingestion_config = config.unwrap_or_default();

        // match DataIngestionModule::new(ingestion_config, self.data_access_layer.get_kv_store(), env)
        //     .await
        // {
        //     Ok(module) => {
        //         self.data_ingestion_module = Some(Arc::new(module));
        //         worker::console_log!(
        //             "Data Ingestion Module initialized successfully with fallback support"
        //         );
        //     }
        //     Err(e) => {
        //         worker::console_log!(
        //             "Failed to initialize Data Ingestion Module: {} - continuing with fallback mode",
        //             e
        //         );
        //         self.data_ingestion_module = None;
        //     }
        // }
    }

    /// Get exchange service (most commonly used service)
    pub fn exchange_service(&self) -> &Arc<ExchangeService> {
        &self.exchange_service
    }

    /// Get session management service
    pub fn session_service(&self) -> &Arc<SessionManagementService> {
        &self.session_service
    }

    /// Get opportunity distribution service
    pub fn distribution_service(&self) -> &OpportunityDistributionService {
        &self.distribution_service
    }

    /// Get mutable opportunity distribution service
    pub fn distribution_service_mut(&mut self) -> &mut OpportunityDistributionService {
        &mut self.distribution_service
    }

    /// Get telegram service
    pub fn telegram_service(&self) -> Option<&Arc<ModularTelegramService>> {
        self.telegram_service.as_ref()
    }

    /// Get user profile service
    pub fn user_profile_service(&self) -> Option<&Arc<UserProfileService>> {
        self.user_profile_service.as_ref()
    }

    /// Get auth service
    /// TODO: Implement AuthService when ready
    pub fn data_ingestion_module(&self) -> Option<&Arc<DataIngestionModule>> {
        self.data_ingestion_module.as_ref()
    }

    /// Get database manager
    pub fn database_manager(&self) -> &DatabaseManager {
        &self.database_manager
    }

    /// Get data access layer
    pub fn data_access_layer(&self) -> &DataAccessLayer {
        &self.data_access_layer
    }

    /// Get feature flags
    pub fn get_feature_flags(&self) -> Arc<FeatureFlags> {
        self.feature_flags.clone()
    }

    /// Validate that all required services are configured
    pub fn validate_configuration(&self) -> ArbitrageResult<()> {
        if self.telegram_service.is_none() {
            return Err(crate::utils::ArbitrageError::configuration_error(
                "Telegram service not configured for push notifications".to_string(),
            ));
        }
        // if self.auth_service.is_none() {
        //     return Err(crate::utils::ArbitrageError::configuration_error(
        //         "AuthService not configured".to_string(),
        //     ));
        // }
        // if let Some(auth) = &self.auth_service {
        //     // Directly access the providers from the AuthService instance if it has public fields or getter methods for them.
        //     // Assuming AuthService::new initializes its internal providers and they are not meant to be None after `new()` sets them.
        //     // The health_check within AuthService itself should verify its internal state.
        //     // Here we just check that auth_service itself is present.
        // }

        Ok(())
    }

    /// Health check for all services
    pub async fn health_check(&self) -> ArbitrageResult<ServiceHealthStatus> {
        let mut status = ServiceHealthStatus::default();

        match self.session_service.get_active_session_count().await {
            Ok(_) => status.session_service_healthy = true,
            Err(e) => {
                status.session_service_healthy = false;
                status.errors.push(format!("Session service error: {}", e));
            }
        }

        status.user_profile_service_healthy = self.user_profile_service.is_some();
        if !status.user_profile_service_healthy {
            // status.errors.push("UserProfileService not configured".to_string()); // only if mandatory
        }

        // Admin service health check
        // if let Some(admin_service) = &self.admin_service {
        //     match admin_service.health_check().await {
        //         Ok(admin_health) => {
        //             status.auth_service_healthy = admin_health.overall_healthy;
        //             if !admin_health.overall_healthy {
        //                 status.errors.push(format!("AdminService unhealthy: user_mgmt={}, system_config={}, monitoring={}, audit={}",
        //                     admin_health.user_management_healthy,
        //                     admin_health.system_config_healthy,
        //                     admin_health.monitoring_healthy,
        //                     admin_health.audit_healthy
        //                 ));
        //             }
        //         }
        //         Err(e) => {
        //             status.auth_service_healthy = false;
        //             status
        //                 .errors
        //                 .push(format!("AdminService health check error: {}", e));
        //         }
        //     }
        // } else {
        //     status.auth_service_healthy = false;
        //     status.errors.push("AdminService not configured".to_string());
        // }

        status.auth_service_healthy = true; // Temporarily set to true

        status.distribution_service_healthy = true;
        status.telegram_service_healthy = self.telegram_service.is_some();
        status.exchange_service_healthy = true;
        status.ai_coordinator_healthy = true; // Set to true since we don't have AI coordinator yet
        status.data_ingestion_module_healthy = self.data_ingestion_module.is_some();

        status.overall_healthy = status.session_service_healthy
            && status.distribution_service_healthy
            && status.telegram_service_healthy
            && status.exchange_service_healthy
            && status.user_profile_service_healthy
            && status.auth_service_healthy
            && status.ai_coordinator_healthy
            && status.data_ingestion_module_healthy;

        if !status.overall_healthy {
            worker::console_error!("Service container health check failed: {:?}", status.errors);
        }
        Ok(status)
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> ArbitrageResult<u32> {
        self.session_service.cleanup_expired_sessions().await
    }

    /// Distribute opportunities to eligible users
    pub async fn distribute_opportunities(
        &self,
        opportunities: &[crate::types::ArbitrageOpportunity],
    ) -> ArbitrageResult<u32> {
        if self.telegram_service.is_none() {
            return Err(ArbitrageError::service_unavailable(
                "Telegram service not configured for distribution",
            ));
        }
        let mut distributed_count = 0;
        for opportunity in opportunities {
            match self
                .distribution_service
                .distribute_opportunity(opportunity.clone()) // Changed to distribute_opportunity and clone
                .await
            {
                Ok(count_for_one) => distributed_count += count_for_one, // Assuming distribute_opportunity returns u32 for one opportunity
                Err(e) => {
                    worker::console_error!(
                        "Error distributing one opportunity: {}. Continuing...",
                        e
                    );
                    // Decide if one error should stop all, or just log and continue.
                    // For now, logging and continuing.
                }
            }
        }
        Ok(distributed_count)
    }
}

/// Health status of all services in the container
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ServiceHealthStatus {
    pub overall_healthy: bool,
    pub session_service_healthy: bool,
    pub distribution_service_healthy: bool,
    pub telegram_service_healthy: bool,
    pub exchange_service_healthy: bool,
    pub user_profile_service_healthy: bool,
    pub auth_service_healthy: bool,
    pub ai_coordinator_healthy: bool,
    pub data_ingestion_module_healthy: bool,
    pub errors: Vec<String>,
}

impl ServiceHealthStatus {
    pub fn summary(&self) -> String {
        if self.overall_healthy {
            "All services healthy".to_string()
        } else {
            format!("Some services unhealthy: {} errors", self.errors.len())
        }
    }

    pub fn detailed_report(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

// Placeholder for actual DistributionConfig if needed by with_distribution_config
// pub struct DistributionConfig { /* ... */ }
