// src/services/core/opportunities/mod.rs

// Core modular components (new unified architecture)
pub mod opportunity_core;
pub mod market_analyzer;
pub mod access_manager;
pub mod ai_enhancer;
pub mod cache_manager;
pub mod opportunity_builders;
pub mod opportunity_engine;

// Legacy services (still needed)
pub mod opportunity_distribution;

// Re-export core components for easy access
pub use opportunity_core::{OpportunityContext, OpportunityConfig, OpportunityUtils};
pub use market_analyzer::MarketAnalyzer;
pub use access_manager::AccessManager;
pub use ai_enhancer::AIEnhancer;
pub use cache_manager::{CacheManager, CachePrefixes};
pub use opportunity_builders::OpportunityBuilder;
pub use opportunity_engine::OpportunityEngine;

// Re-export remaining legacy service for backward compatibility
pub use opportunity_distribution::OpportunityDistributionService; 