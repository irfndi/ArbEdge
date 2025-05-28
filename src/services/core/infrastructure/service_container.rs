use crate::services::core::infrastructure::cloudflare_pipelines::{
    CloudflarePipelinesService, PipelinesConfig,
};
use crate::services::core::infrastructure::vectorize_service::{VectorizeConfig, VectorizeService};
use crate::services::core::infrastructure::{D1Service, KVService};
use crate::services::core::opportunities::opportunity_distribution::{
    DistributionConfig, OpportunityDistributionService,
};
use crate::services::core::trading::exchange::{ExchangeInterface, ExchangeService};
use crate::services::core::user::session_management::SessionManagementService;
use crate::services::core::user::user_profile::UserProfileService;
use crate::services::interfaces::telegram::telegram::TelegramService;
use crate::types;
use crate::utils::ArbitrageResult;
use std::sync::Arc;
use worker::{kv::KvStore, Env};

/// Comprehensive service container for managing all application services
/// Provides centralized dependency injection and service lifecycle management
/// Addresses inconsistent service creation patterns across the application
pub struct ServiceContainer {
    pub session_service: SessionManagementService,
    pub distribution_service: OpportunityDistributionService,
    pub telegram_service: Option<Arc<TelegramService>>,
    pub exchange_service: Arc<ExchangeService>,
    pub user_profile_service: Option<Arc<UserProfileService>>,
    pub vectorize_service: Option<Arc<VectorizeService>>,
    pub pipelines_service: Option<Arc<CloudflarePipelinesService>>,
    pub d1_service: D1Service,
    pub kv_service: KVService,
}

impl ServiceContainer {
    /// Create a new comprehensive service container with all dependencies
    pub fn new(env: &Env, kv_store: KvStore) -> ArbitrageResult<Self> {
        // Create custom environment wrapper
        let custom_env = types::Env::new(env.clone());

        // Create infrastructure services
        let d1_service = D1Service::new(env)?;
        let kv_service = KVService::new(kv_store);

        // Create exchange service (core service used by many endpoints)
        let exchange_service = Arc::new(ExchangeService::new(&custom_env)?);

        // Create session management service
        let session_service = SessionManagementService::new(d1_service.clone(), kv_service.clone());

        // Create opportunity distribution service
        let distribution_service = OpportunityDistributionService::new(
            d1_service.clone(),
            kv_service.clone(),
            session_service.clone(),
        );

        Ok(Self {
            session_service,
            distribution_service,
            telegram_service: None,
            exchange_service,
            user_profile_service: None,
            vectorize_service: None,
            pipelines_service: None,
            d1_service,
            kv_service,
        })
    }

    /// Create with custom distribution configuration
    pub fn with_distribution_config(
        env: &Env,
        kv_store: KvStore,
        config: DistributionConfig,
    ) -> ArbitrageResult<Self> {
        let mut container = Self::new(env, kv_store)?;
        container.distribution_service = container.distribution_service.with_config(config);
        Ok(container)
    }

    /// Set the Telegram service for push notifications using Arc for shared ownership
    pub fn set_telegram_service(&mut self, telegram_service: TelegramService) {
        let arc_telegram_service = Arc::new(telegram_service);

        // Set telegram service in distribution service
        self.distribution_service
            .set_notification_sender(Box::new(arc_telegram_service.clone()));

        // Store the Arc for container access
        self.telegram_service = Some(arc_telegram_service);
    }

    /// Set the user profile service with encryption key
    pub fn set_user_profile_service(&mut self, encryption_key: String) {
        // UserProfileService expects raw KvStore, not KVService
        let kv_store = self.kv_service.get_kv_store();
        let user_profile_service =
            UserProfileService::new(kv_store, self.d1_service.clone(), encryption_key);
        self.user_profile_service = Some(Arc::new(user_profile_service));
    }

    /// Initialize Vectorize service with fallback mechanisms
    pub fn set_vectorize_service(&mut self, env: &Env, config: Option<VectorizeConfig>) {
        let vectorize_config = config.unwrap_or_default();

        match VectorizeService::new(env, vectorize_config.clone()) {
            Ok(service) => {
                // Create a separate instance for distribution service since it needs ownership
                if let Ok(dist_service) = VectorizeService::new(env, vectorize_config) {
                    self.distribution_service
                        .set_vectorize_service(dist_service);
                }

                // Store the Arc for container access
                self.vectorize_service = Some(Arc::new(service));
            }
            Err(e) => {
                worker::console_log!(
                    "Failed to initialize Vectorize service: {} - continuing with fallback mode",
                    e
                );
                self.vectorize_service = None;
            }
        }
    }

    /// Initialize Pipelines service with fallback mechanisms
    pub fn set_pipelines_service(&mut self, env: &Env, config: Option<PipelinesConfig>) {
        let pipelines_config = config.unwrap_or_default();

        match CloudflarePipelinesService::new(env, pipelines_config) {
            Ok(mut service) => {
                // Set KV service for fallback data persistence
                service.set_kv_service(self.kv_service.clone());

                self.pipelines_service = Some(Arc::new(service));
                worker::console_log!(
                    "Pipelines service initialized successfully with fallback support and KV persistence"
                );
            }
            Err(e) => {
                worker::console_log!(
                    "Failed to initialize Pipelines service: {} - continuing with fallback mode",
                    e
                );
                self.pipelines_service = None;
            }
        }
    }

    /// Get exchange service (most commonly used service)
    pub fn exchange_service(&self) -> &Arc<ExchangeService> {
        &self.exchange_service
    }

    /// Get session management service
    pub fn session_service(&self) -> &SessionManagementService {
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
    pub fn telegram_service(&self) -> Option<&Arc<TelegramService>> {
        self.telegram_service.as_ref()
    }

    /// Get user profile service
    pub fn user_profile_service(&self) -> Option<&Arc<UserProfileService>> {
        self.user_profile_service.as_ref()
    }

    /// Get vectorize service
    pub fn vectorize_service(&self) -> Option<&Arc<VectorizeService>> {
        self.vectorize_service.as_ref()
    }

    /// Get pipelines service
    pub fn pipelines_service(&self) -> Option<&Arc<CloudflarePipelinesService>> {
        self.pipelines_service.as_ref()
    }

    /// Get D1 service
    pub fn d1_service(&self) -> &D1Service {
        &self.d1_service
    }

    /// Get KV service
    pub fn kv_service(&self) -> &KVService {
        &self.kv_service
    }

    /// Validate that all required services are configured
    pub fn validate_configuration(&self) -> ArbitrageResult<()> {
        // Check if Telegram service is set for push notifications
        if self.telegram_service.is_none() {
            return Err(crate::utils::ArbitrageError::configuration_error(
                "Telegram service not configured for push notifications".to_string(),
            ));
        }

        Ok(())
    }

    /// Health check for all services
    pub async fn health_check(&self) -> ArbitrageResult<ServiceHealthStatus> {
        let mut status = ServiceHealthStatus::default();

        // Check session service health
        match self.session_service.get_active_session_count().await {
            Ok(_) => status.session_service_healthy = true,
            Err(e) => {
                status.session_service_healthy = false;
                status.errors.push(format!("Session service error: {}", e));
            }
        }

        // Check distribution service health
        match self.distribution_service.get_distribution_stats().await {
            Ok(_) => status.distribution_service_healthy = true,
            Err(e) => {
                status.distribution_service_healthy = false;
                status
                    .errors
                    .push(format!("Distribution service error: {}", e));
            }
        }

        // Check exchange service health (try multiple exchanges for robustness)
        let supported_exchanges = ["binance", "bybit"];
        let mut exchange_healthy = false;
        let mut exchange_errors = Vec::new();

        for exchange in &supported_exchanges {
            match self.exchange_service.get_markets(exchange).await {
                Ok(_) => {
                    exchange_healthy = true;
                    break; // If any exchange works, consider service healthy
                }
                Err(e) => {
                    exchange_errors.push(format!("{}: {}", exchange, e));
                }
            }
        }

        status.exchange_service_healthy = exchange_healthy;
        if !exchange_healthy {
            status.errors.push(format!(
                "Exchange service error - all exchanges failed: [{}]",
                exchange_errors.join(", ")
            ));
        }

        // Check Telegram service health
        status.telegram_service_healthy = self.telegram_service.is_some();
        if !status.telegram_service_healthy {
            status
                .errors
                .push("Telegram service not configured".to_string());
        }

        // Check user profile service health
        status.user_profile_service_healthy = self.user_profile_service.is_some();
        if !status.user_profile_service_healthy {
            status
                .errors
                .push("User profile service not configured".to_string());
        }

        // Check vectorize service health
        if let Some(ref vectorize_service) = self.vectorize_service {
            match vectorize_service.health_check().await {
                Ok(true) => status.vectorize_service_healthy = true,
                Ok(false) => {
                    status.vectorize_service_healthy = false;
                    status
                        .errors
                        .push("Vectorize service unhealthy - using fallback mode".to_string());
                }
                Err(e) => {
                    status.vectorize_service_healthy = false;
                    status.errors.push(format!(
                        "Vectorize service error: {} - using fallback mode",
                        e
                    ));
                }
            }
        } else {
            status.vectorize_service_healthy = false;
            status
                .errors
                .push("Vectorize service not configured - using fallback mode".to_string());
        }

        // Check pipelines service health
        if let Some(ref pipelines_service) = self.pipelines_service {
            match pipelines_service.health_check().await {
                Ok(true) => status.pipelines_service_healthy = true,
                Ok(false) => {
                    status.pipelines_service_healthy = false;
                    status
                        .errors
                        .push("Pipelines service unhealthy - using fallback storage".to_string());
                }
                Err(e) => {
                    status.pipelines_service_healthy = false;
                    status.errors.push(format!(
                        "Pipelines service error: {} - using fallback storage",
                        e
                    ));
                }
            }
        } else {
            status.pipelines_service_healthy = false;
            status
                .errors
                .push("Pipelines service not configured - using fallback storage".to_string());
        }

        status.overall_healthy = status.session_service_healthy
            && status.distribution_service_healthy
            && status.exchange_service_healthy
            && status.telegram_service_healthy
            && status.user_profile_service_healthy;
        // Note: Vectorize and Pipelines are optional services with fallbacks,
        // so they don't affect overall health status

        Ok(status)
    }

    /// Cleanup expired sessions across all services
    pub async fn cleanup_expired_sessions(&self) -> ArbitrageResult<u32> {
        self.session_service.cleanup_expired_sessions().await
    }

    /// Distribute opportunities to eligible users
    pub async fn distribute_opportunities(
        &self,
        opportunities: &[crate::types::ArbitrageOpportunity],
    ) -> ArbitrageResult<u32> {
        let mut total_distributed = 0;

        for opportunity in opportunities {
            match self
                .distribution_service
                .distribute_opportunity(opportunity.clone())
                .await
            {
                Ok(count) => total_distributed += count,
                Err(e) => {
                    // Log error but continue with other opportunities
                    worker::console_log!("Failed to distribute opportunity: {}", e);
                }
            }
        }

        Ok(total_distributed)
    }
}

/// Health status for all services in the container
#[derive(Debug, Clone, Default)]
pub struct ServiceHealthStatus {
    pub overall_healthy: bool,
    pub session_service_healthy: bool,
    pub distribution_service_healthy: bool,
    pub telegram_service_healthy: bool,
    pub exchange_service_healthy: bool,
    pub user_profile_service_healthy: bool,
    pub vectorize_service_healthy: bool,
    pub pipelines_service_healthy: bool,
    pub errors: Vec<String>,
}

impl ServiceHealthStatus {
    /// Get a summary of the health status
    pub fn summary(&self) -> String {
        if self.overall_healthy {
            "All services healthy".to_string()
        } else {
            format!("Service issues: {}", self.errors.join(", "))
        }
    }

    /// Get detailed status report
    pub fn detailed_report(&self) -> serde_json::Value {
        serde_json::json!({
            "overall_healthy": self.overall_healthy,
            "services": {
                "session_service": self.session_service_healthy,
                "distribution_service": self.distribution_service_healthy,
                "telegram_service": self.telegram_service_healthy,
                "exchange_service": self.exchange_service_healthy,
                "user_profile_service": self.user_profile_service_healthy,
                "vectorize_service": self.vectorize_service_healthy,
                "pipelines_service": self.pipelines_service_healthy
            },
            "errors": self.errors,
            "summary": self.summary()
        })
    }
}
