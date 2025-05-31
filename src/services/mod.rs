// src/services/mod.rs

// Core services organized by domain
pub mod core {
    pub mod user {
        pub mod dynamic_config;
        pub mod session_management;
        pub mod user_access;
        pub mod user_profile;
        pub mod user_trading_preferences;

        pub use dynamic_config::DynamicConfigService;
        pub use session_management::SessionManagementService;
        pub use user_profile::UserProfileService;
        pub use user_trading_preferences::UserTradingPreferencesService;
    }

    pub mod trading {
        pub mod ai_exchange_router;
        pub mod exchange;
        pub mod exchange_availability;
        pub mod positions;

        pub use ai_exchange_router::AiExchangeRouterService;
        pub use exchange::ExchangeService;
        pub use exchange_availability::ExchangeAvailabilityService;
        pub use positions::PositionsService;
    }

    pub mod opportunities {
        pub mod opportunity_categorization;
        pub mod opportunity_distribution;

        // New modular architecture
        pub mod access_manager;
        pub mod ai_enhancer;
        pub mod cache_manager;
        pub mod market_analyzer;
        pub mod opportunity_builders;
        pub mod opportunity_core;
        pub mod opportunity_engine;

        pub use opportunity_categorization::OpportunityCategorizationService;
        pub use opportunity_distribution::{
            DistributionConfig, DistributionStats, OpportunityDistributionService,
        };

        // Re-export new modular components
        pub use access_manager::AccessManager;
        pub use ai_enhancer::AIEnhancer;
        pub use cache_manager::{CacheManager, CachePrefixes};
        pub use market_analyzer::MarketAnalyzer;
        pub use opportunity_builders::OpportunityBuilder;
        pub use opportunity_core::{OpportunityConfig, OpportunityContext, OpportunityUtils};
        pub use opportunity_engine::OpportunityEngine;
    }

    pub mod analysis {
        pub mod correlation_analysis;
        pub mod market_analysis;
        pub mod technical_analysis;

        pub use correlation_analysis::CorrelationAnalysisService;
        pub use market_analysis::MarketAnalysisService;
        pub use technical_analysis::TechnicalAnalysisService;
    }

    pub mod ai {
        pub mod ai_beta_integration;
        pub mod ai_integration;
        pub mod ai_intelligence;

        pub use ai_beta_integration::AiBetaIntegrationService;
        pub use ai_integration::AiIntegrationService;
        pub use ai_intelligence::AiIntelligenceService;
    }

    pub mod invitation {
        pub mod affiliation_service;
        pub mod invitation_service;
        pub mod referral_service;

        pub use affiliation_service::AffiliationService;
        pub use invitation_service::InvitationService;
        pub use referral_service::ReferralService;
    }

    pub mod infrastructure {
        // Legacy files - REMOVED (replaced by modular architecture)
        // pub mod ai_gateway; -> replaced by ai_services module
        // pub mod cloudflare_pipelines; -> replaced by data_ingestion_module
        // pub mod cloudflare_queues; -> replaced by data_ingestion_module
        // pub mod d1_database; -> replaced by database_repositories module
        // pub mod hybrid_data_access; -> replaced by data_access_layer module
        // pub mod kv_service; -> replaced by data_access_layer module
        // pub mod monitoring_observability; -> replaced by monitoring_module
        // pub mod notifications; -> replaced by notification_module
        // pub mod vectorize_service; -> replaced by ai_services module

        // New Modular Infrastructure Architecture (7 Modules + 1 Legacy)
        pub mod ai_services;
        pub mod analytics_module;
        pub mod data_access_layer;
        pub mod data_ingestion_module;
        pub mod database_repositories;
        pub mod financial_module;
        pub mod monitoring_module;
        pub mod notification_module;

        // Legacy files still in use
        pub mod analytics_engine;
        pub mod durable_objects;
        pub mod service_container;

        // Re-export new modular components
        pub use ai_services::{
            AICache, AICoordinator, AIServicesConfig, AIServicesHealth, AIServicesMetrics,
            EmbeddingEngine, ModelRouter, PersonalizationEngine,
        };
        pub use analytics_module::{
            AnalyticsCoordinator, AnalyticsModule, AnalyticsModuleConfig, AnalyticsModuleHealth,
            AnalyticsModuleMetrics, DataProcessor, MetricsAggregator, ReportGenerator,
        };
        pub use data_access_layer::{
            APIConnector, CacheLayer, DataAccessLayer, DataAccessLayerConfig,
            DataAccessLayerHealth, DataCoordinator, DataSourceManager, DataValidator,
        };
        pub use data_ingestion_module::{
            DataIngestionModule, DataIngestionModuleConfig, DataTransformer, IngestionCoordinator,
            PipelineManager, QueueManager,
        };
        pub use database_repositories::{
            AIDataRepository, AnalyticsRepository, ConfigRepository, DatabaseManager,
            InvitationRepository, UserRepository,
        };
        pub use financial_module::{
            BalanceTracker, ExchangeBalanceSnapshot, FinancialCoordinator, FinancialModule,
            FinancialModuleConfig, FinancialModuleHealth, FinancialModuleMetrics, FundAnalyzer,
            FundOptimizationResult, PortfolioAnalytics,
        };
        pub use monitoring_module::{
            AlertManager, HealthMonitor, MetricsCollector, MonitoringModule,
            MonitoringModuleConfig, MonitoringModuleHealth, MonitoringModuleMetrics,
            ObservabilityCoordinator, TraceCollector,
        };
        pub use notification_module::{
            ChannelManager, DeliveryManager, NotificationCoordinator, NotificationModule,
            NotificationModuleConfig, NotificationModuleHealth, NotificationModuleMetrics,
            TemplateEngine,
        };

        // Legacy exports (still in use)
        pub use analytics_engine::{
            AnalyticsEngineConfig, AnalyticsEngineService, RealTimeMetrics, UserAnalytics,
        };
        pub use durable_objects::{
            GlobalRateLimiterDO, MarketDataCoordinatorDO, OpportunityCoordinatorDO,
            UserOpportunityQueueDO,
        };
        pub use service_container::{ServiceContainer, ServiceHealthStatus};
    }
}

// Interface services for different platforms
pub mod interfaces {
    pub mod telegram {
        #[allow(clippy::module_inception)]
        pub mod telegram;
        pub mod telegram_keyboard;

        pub use telegram::TelegramService;
        pub use telegram_keyboard::{InlineKeyboard, InlineKeyboardButton};
    }

    pub mod api {
        // TODO: Implement REST API interface modules
        // - api_service.rs: Core API service for HTTP endpoints
        // - api_middleware.rs: Authentication and rate limiting middleware
        // - api_routes.rs: Route definitions and handlers
    }

    pub mod discord {
        // TODO: Implement Discord bot interface modules
        // - discord_service.rs: Core Discord bot service
        // - discord_commands.rs: Discord slash commands and interactions
        // - discord_embeds.rs: Rich embed formatting for Discord messages
    }
}

// Re-export commonly used services for backward compatibility
pub use core::ai::{AiBetaIntegrationService, AiIntegrationService, AiIntelligenceService};
pub use core::analysis::{
    CorrelationAnalysisService, MarketAnalysisService, TechnicalAnalysisService,
};
pub use core::infrastructure::{
    // Legacy services still in use
    AnalyticsEngineService,
    AnalyticsModule,
    DataAccessLayer,
    DataIngestionModule,
    DatabaseManager,
    FinancialModule,

    GlobalRateLimiterDO,
    MarketDataCoordinatorDO,
    MonitoringModule,
    // New modular infrastructure components
    NotificationModule,
    OpportunityCoordinatorDO,
    ServiceContainer,
    UserOpportunityQueueDO,
};
pub use core::invitation::{AffiliationService, InvitationService, ReferralService};
pub use core::opportunities::{
    AIEnhancer,
    AccessManager,
    CacheManager,
    MarketAnalyzer,
    OpportunityBuilder,
    OpportunityCategorizationService,
    OpportunityConfig,
    OpportunityContext,
    OpportunityDistributionService,
    // New modular components
    OpportunityEngine,
};
pub use core::trading::{
    AiExchangeRouterService, ExchangeAvailabilityService, ExchangeService, PositionsService,
};
pub use core::user::{
    DynamicConfigService, SessionManagementService, UserProfileService,
    UserTradingPreferencesService,
};
pub use interfaces::telegram::{InlineKeyboard, InlineKeyboardButton, TelegramService};
