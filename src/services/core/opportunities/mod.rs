// src/services/core/opportunities/mod.rs

// Core modular components (new unified architecture)
pub mod access_manager;
pub mod ai_enhancer;
pub mod cache_manager;
pub mod market_analyzer;
pub mod opportunity_builders;
pub mod opportunity_categorization;
pub mod opportunity_core;
pub mod opportunity_engine; // Renamed from opportunity_models
pub mod opportunity_validity_engine;
pub mod trade_target_calculator;

// Strategic enhancement modules
pub mod funding_rate_manager;
pub mod historical_data_manager;
pub mod opportunity_scoring_engine;

// Legacy services (still needed)
pub mod opportunity_distribution;

// Re-export core components for easy access
pub use access_manager::AccessManager;
pub use ai_enhancer::AIEnhancer;
pub use cache_manager::{CachePrefixes, OpportunityDataCache};
pub use market_analyzer::MarketAnalyzer;
pub use opportunity_builders::OpportunityBuilder;
pub use opportunity_categorization::*;
pub use opportunity_core::*;
pub use opportunity_core::{OpportunityConfig, OpportunityContext, OpportunityUtils};
pub use opportunity_engine::OpportunityEngine; // Renamed from opportunity_models
pub use opportunity_validity_engine::OpportunityValidityEngine;
pub use trade_target_calculator::TradeTargetCalculator;

// Re-export strategic enhancement modules
pub use funding_rate_manager::{
    FundingRate, FundingRateArbitrage, FundingRateManager, FundingRateNotification,
};
pub use historical_data_manager::{
    HistoricalDataManager, HistoricalOpportunity, OpportunityAnalytics, PerformanceMetrics,
};
pub use opportunity_scoring_engine::{
    OpportunitiesSummary, OpportunityScore, OpportunityScoringEngine, ScoringWeights, SymbolSummary,
};

// Re-export remaining legacy service for backward compatibility
pub use opportunity_distribution::OpportunityDistributionService;
