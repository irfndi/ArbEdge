// AI Services Module - Phase 2B Infrastructure Modularization
// Replaces vectorize_service.rs and ai_gateway.rs with specialized modular components

pub mod ai_cache;
pub mod ai_coordinator;
pub mod embedding_engine;
pub mod model_router;
pub mod personalization_engine;

// Re-export main components for easy access
pub use ai_cache::{AICache, AICacheConfig, CacheEntry, CacheStats};
pub use ai_coordinator::{AICoordinator, AICoordinatorConfig};
pub use embedding_engine::{
    EmbeddingEngine, EmbeddingEngineConfig, OpportunityEmbedding, SimilarityResult,
};
pub use model_router::{AIModelConfig, ModelRouter, ModelRouterConfig, RoutingDecision};
pub use personalization_engine::{
    PersonalizationEngine, PersonalizationEngineConfig, RankedOpportunity, UserPreferenceVector,
};

use crate::utils::ArbitrageResult;
use serde::{Deserialize, Serialize};

/// AI Services health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIServicesHealth {
    pub embedding_engine: bool,
    pub model_router: bool,
    pub personalization_engine: bool,
    pub ai_cache: bool,
    pub vectorize_available: bool,
    pub ai_gateway_available: bool,
    pub overall_health: bool,
    pub last_check: u64,
}

/// AI Services performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIServicesMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_response_time_ms: f64,
    pub cache_hit_rate: f64,
    pub vectorize_usage_rate: f64,
    pub ai_gateway_usage_rate: f64,
    pub fallback_usage_rate: f64,
    pub last_updated: u64,
}

/// Configuration for the entire AI Services module
#[derive(Debug, Clone)]
pub struct AIServicesConfig {
    pub enable_vectorize: bool,
    pub enable_ai_gateway: bool,
    pub enable_personalization: bool,
    pub enable_caching: bool,
    pub enable_fallback: bool,
    pub embedding_engine: EmbeddingEngineConfig,
    pub model_router: ModelRouterConfig,
    pub personalization_engine: PersonalizationEngineConfig,
    pub ai_cache: AICacheConfig,
    pub ai_coordinator: AICoordinatorConfig,
}

impl Default for AIServicesConfig {
    fn default() -> Self {
        Self {
            enable_vectorize: true,
            enable_ai_gateway: true,
            enable_personalization: true,
            enable_caching: true,
            enable_fallback: true,
            embedding_engine: EmbeddingEngineConfig::default(),
            model_router: ModelRouterConfig::default(),
            personalization_engine: PersonalizationEngineConfig::default(),
            ai_cache: AICacheConfig::default(),
            ai_coordinator: AICoordinatorConfig::default(),
        }
    }
}

impl AIServicesConfig {
    /// Create configuration optimized for high concurrency (1000-2500 users)
    pub fn high_concurrency() -> Self {
        Self {
            enable_vectorize: true,
            enable_ai_gateway: true,
            enable_personalization: true,
            enable_caching: true,
            enable_fallback: true,
            embedding_engine: EmbeddingEngineConfig::high_concurrency(),
            model_router: ModelRouterConfig::high_concurrency(),
            personalization_engine: PersonalizationEngineConfig::high_concurrency(),
            ai_cache: AICacheConfig::high_concurrency(),
            ai_coordinator: AICoordinatorConfig::high_concurrency(),
        }
    }

    /// Create configuration optimized for memory usage
    pub fn memory_optimized() -> Self {
        Self {
            enable_vectorize: true,
            enable_ai_gateway: true,
            enable_personalization: false, // Disable to save memory
            enable_caching: true,
            enable_fallback: true,
            embedding_engine: EmbeddingEngineConfig::memory_optimized(),
            model_router: ModelRouterConfig::memory_optimized(),
            personalization_engine: PersonalizationEngineConfig::memory_optimized(),
            ai_cache: AICacheConfig::memory_optimized(),
            ai_coordinator: AICoordinatorConfig::memory_optimized(),
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        self.embedding_engine.validate()?;
        self.model_router.validate()?;
        self.personalization_engine.validate()?;
        self.ai_cache.validate()?;
        self.ai_coordinator.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_services_config_default() {
        let config = AIServicesConfig::default();
        assert!(config.enable_vectorize);
        assert!(config.enable_ai_gateway);
        assert!(config.enable_personalization);
        assert!(config.enable_caching);
        assert!(config.enable_fallback);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_ai_services_config_high_concurrency() {
        let config = AIServicesConfig::high_concurrency();
        assert!(config.enable_vectorize);
        assert!(config.enable_ai_gateway);
        assert!(config.enable_personalization);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_ai_services_config_memory_optimized() {
        let config = AIServicesConfig::memory_optimized();
        assert!(config.enable_vectorize);
        assert!(config.enable_ai_gateway);
        assert!(!config.enable_personalization); // Disabled for memory optimization
        assert!(config.validate().is_ok());
    }
}
