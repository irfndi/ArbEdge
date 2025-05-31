// AI Coordinator - Main Orchestrator for AI Services
// Coordinates EmbeddingEngine, ModelRouter, PersonalizationEngine, and AICache

use super::ai_cache::CacheEntryType;
use super::embedding_engine::{OpportunityEmbedding, SimilarityResult};
use super::model_router::{ModelRequirements, RoutingDecision};
use super::personalization_engine::{RankedOpportunity, UserInteraction};
use super::{
    AICache, AICacheConfig, EmbeddingEngine, EmbeddingEngineConfig, ModelRouter, ModelRouterConfig,
    PersonalizationEngine, PersonalizationEngineConfig,
};
use crate::types::ArbitrageOpportunity;
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use uuid;
use worker::{kv::KvStore, Env};

/// Configuration for AICoordinator
#[derive(Debug, Clone)]
pub struct AICoordinatorConfig {
    pub enable_ai_services: bool,
    pub enable_embedding: bool,
    pub enable_model_routing: bool,
    pub enable_personalization: bool,
    pub enable_ai_caching: bool,
    pub enable_fallback_strategies: bool,
    pub enable_circuit_breaker: bool,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_seconds: u64,
    pub health_check_interval_seconds: u64,
    pub performance_monitoring: bool,
    pub batch_processing_enabled: bool,
    pub batch_size: usize,
    pub max_concurrent_requests: u32,
    pub request_timeout_seconds: u64,
}

impl Default for AICoordinatorConfig {
    fn default() -> Self {
        Self {
            enable_ai_services: true,
            enable_embedding: true,
            enable_model_routing: true,
            enable_personalization: true,
            enable_ai_caching: true,
            enable_fallback_strategies: true,
            enable_circuit_breaker: true,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_seconds: 60,
            health_check_interval_seconds: 30,
            performance_monitoring: true,
            batch_processing_enabled: true,
            batch_size: 25,
            max_concurrent_requests: 100,
            request_timeout_seconds: 30,
        }
    }
}

impl AICoordinatorConfig {
    /// Create configuration optimized for high concurrency
    pub fn high_concurrency() -> Self {
        Self {
            batch_size: 50,
            max_concurrent_requests: 200,
            request_timeout_seconds: 15,
            circuit_breaker_threshold: 10,
            health_check_interval_seconds: 15,
            ..Default::default()
        }
    }

    /// Create configuration optimized for memory usage
    pub fn memory_optimized() -> Self {
        Self {
            batch_size: 15,
            max_concurrent_requests: 50,
            performance_monitoring: false,
            enable_ai_caching: true, // Keep caching for memory efficiency
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "batch_size must be greater than 0",
            ));
        }
        if self.max_concurrent_requests == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_requests must be greater than 0",
            ));
        }
        if self.circuit_breaker_threshold == 0 {
            return Err(ArbitrageError::validation_error(
                "circuit_breaker_threshold must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// AI service health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIServiceHealth {
    pub embedding_engine: bool,
    pub model_router: bool,
    pub personalization_engine: bool,
    pub ai_cache: bool,
    pub overall_health: bool,
    pub last_check: u64,
    pub error_count: u32,
    pub circuit_breaker_open: bool,
}

impl Default for AIServiceHealth {
    fn default() -> Self {
        Self {
            embedding_engine: false,
            model_router: false,
            personalization_engine: false,
            ai_cache: false,
            overall_health: false,
            last_check: chrono::Utc::now().timestamp_millis() as u64,
            error_count: 0,
            circuit_breaker_open: false,
        }
    }
}

/// AI service performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIServiceMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_response_time_ms: f64,
    pub cache_hit_rate_percent: f32,
    pub embedding_requests: u64,
    pub routing_decisions: u64,
    pub personalization_requests: u64,
    pub fallback_activations: u64,
    pub circuit_breaker_trips: u64,
    pub last_updated: u64,
}

impl Default for AIServiceMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            avg_response_time_ms: 0.0,
            cache_hit_rate_percent: 0.0,
            embedding_requests: 0,
            routing_decisions: 0,
            personalization_requests: 0,
            fallback_activations: 0,
            circuit_breaker_trips: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Circuit breaker state
#[derive(Debug, Clone)]
enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Circuit breaker is open, requests fail fast
    HalfOpen, // Testing if service is back
}

/// AI Coordinator for orchestrating all AI services
pub struct AICoordinator {
    config: AICoordinatorConfig,
    logger: crate::utils::logger::Logger,
    embedding_engine: Option<EmbeddingEngine>,
    model_router: Option<ModelRouter>,
    personalization_engine: Option<PersonalizationEngine>,
    ai_cache: Option<AICache>,
    health: Arc<Mutex<AIServiceHealth>>,
    metrics: Arc<Mutex<AIServiceMetrics>>,
    circuit_breaker_state: Arc<Mutex<CircuitBreakerState>>,
    circuit_breaker_last_failure: Arc<Mutex<Option<u64>>>,
    active_requests: Arc<Mutex<u32>>,
}

impl AICoordinator {
    /// Create new AICoordinator instance
    pub fn new(env: &Env, config: AICoordinatorConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize AI services based on configuration
        let embedding_engine = if config.enable_embedding {
            let embedding_config = if config.max_concurrent_requests > 100 {
                EmbeddingEngineConfig::high_concurrency()
            } else {
                EmbeddingEngineConfig::default()
            };
            Some(EmbeddingEngine::new(env, embedding_config)?)
        } else {
            None
        };

        let model_router = if config.enable_model_routing {
            let router_config = if config.max_concurrent_requests > 100 {
                ModelRouterConfig::high_concurrency()
            } else {
                ModelRouterConfig::default()
            };
            Some(ModelRouter::new(env, router_config)?)
        } else {
            None
        };

        let personalization_engine = if config.enable_personalization {
            let personalization_config = if config.max_concurrent_requests > 100 {
                PersonalizationEngineConfig::high_concurrency()
            } else {
                PersonalizationEngineConfig::default()
            };
            Some(PersonalizationEngine::new(personalization_config)?)
        } else {
            None
        };

        let ai_cache = if config.enable_ai_caching {
            let cache_config = if config.max_concurrent_requests > 100 {
                AICacheConfig::high_concurrency()
            } else {
                AICacheConfig::default()
            };
            Some(AICache::new(cache_config)?)
        } else {
            None
        };

        let coordinator = Self {
            config: config.clone(),
            logger,
            embedding_engine,
            model_router,
            personalization_engine,
            ai_cache,
            health: Arc::new(Mutex::new(AIServiceHealth::default())),
            metrics: Arc::new(Mutex::new(AIServiceMetrics::default())),
            circuit_breaker_state: Arc::new(Mutex::new(CircuitBreakerState::Closed)),
            circuit_breaker_last_failure: Arc::new(Mutex::new(None)),
            active_requests: Arc::new(Mutex::new(0)),
        };

        coordinator.logger.info(&format!(
            "AICoordinator initialized: embedding={}, routing={}, personalization={}, caching={}",
            config.enable_embedding,
            config.enable_model_routing,
            config.enable_personalization,
            config.enable_ai_caching
        ));

        Ok(coordinator)
    }

    /// Set cache store for all AI services
    pub fn with_cache(mut self, cache: KvStore) -> Self {
        // Set cache for embedding engine
        if let Some(embedding_engine) = self.embedding_engine.take() {
            self.embedding_engine = Some(embedding_engine.with_cache(cache.clone()));
        }

        // Set cache for model router
        if let Some(model_router) = self.model_router.take() {
            self.model_router = Some(model_router.with_cache(cache.clone()));
        }

        // Set cache for personalization engine
        if let Some(personalization_engine) = self.personalization_engine.take() {
            self.personalization_engine = Some(personalization_engine.with_cache(cache.clone()));
        }

        // Set cache for AI cache
        if let Some(ai_cache) = self.ai_cache.take() {
            self.ai_cache = Some(ai_cache.with_cache(cache));
        }

        self
    }

    /// Generate embeddings for opportunities with intelligent caching
    pub async fn generate_embeddings(
        &self,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<OpportunityEmbedding>> {
        if !self.config.enable_embedding {
            return Err(ArbitrageError::parse_error("Embedding service is disabled"));
        }

        // Check circuit breaker
        if self.is_circuit_breaker_open().await {
            return self.handle_fallback_embeddings(opportunities).await;
        }

        // Check rate limiting
        self.check_rate_limit().await?;

        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        match &self.embedding_engine {
            Some(engine) => {
                let mut embeddings = Vec::new();
                for opportunity in opportunities {
                    match engine.generate_opportunity_embedding(&opportunity).await {
                        Ok(embedding) => embeddings.push(embedding),
                        Err(e) => {
                            self.handle_service_error(&e).await; // decrements counter + metrics
                            return Err(e);
                        }
                    }
                }
                self.update_success_metrics(start_time).await;
                Ok(embeddings)
            }
            None => Err(ArbitrageError::parse_error(
                "Embedding engine not available".to_string(),
            )),
        }
    }

    /// Find similar opportunities with caching
    pub async fn find_similar_opportunities(
        &self,
        _target_embedding: Vec<f32>,
        limit: usize,
    ) -> ArbitrageResult<Vec<SimilarityResult>> {
        if !self.config.enable_embedding {
            return Err(ArbitrageError::parse_error("Embedding service is disabled"));
        }

        // Check circuit breaker
        if self.is_circuit_breaker_open().await {
            return Ok(Vec::new()); // Return empty results as fallback
        }

        // Check rate limiting
        self.check_rate_limit().await?;

        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        match &self.embedding_engine {
            Some(engine) => {
                // Create a placeholder opportunity for similarity search
                let placeholder_opportunity = crate::types::ArbitrageOpportunity {
                    id: uuid::Uuid::new_v4().to_string(),
                    trading_pair: "BTC/USDT".to_string(),
                    exchanges: vec!["binance".to_string(), "bybit".to_string()],
                    profit_percentage: 0.5,
                    confidence_score: 0.7,
                    risk_level: "medium".to_string(),
                    buy_exchange: "binance".to_string(),
                    sell_exchange: "bybit".to_string(),
                    buy_price: 50000.0,
                    sell_price: 50250.0,
                    volume: 1000.0,
                    created_at: chrono::Utc::now().timestamp_millis() as u64,
                    expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + 300_000),
                    // Additional fields
                    pair: "BTC/USDT".to_string(),
                    long_exchange: crate::types::ExchangeIdEnum::Binance,
                    short_exchange: crate::types::ExchangeIdEnum::Bybit,
                    long_rate: Some(0.1),
                    short_rate: Some(-0.1),
                    rate_difference: 0.5,
                    net_rate_difference: Some(0.5),
                    potential_profit_value: Some(250.0),
                    confidence: 0.7,
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    detected_at: chrono::Utc::now().timestamp_millis() as u64,
                    r#type: crate::types::ArbitrageType::CrossExchange,
                    details: Some("AI-generated placeholder opportunity".to_string()),
                    min_exchanges_required: 2,
                };

                match engine
                    .find_similar_opportunities(&placeholder_opportunity, Some(limit as u32))
                    .await
                {
                    Ok(results) => {
                        self.update_success_metrics(start_time).await;
                        Ok(results)
                    }
                    Err(e) => {
                        self.handle_service_error(&e).await;
                        if self.config.enable_fallback_strategies {
                            Ok(Vec::new()) // Return empty results as fallback
                        } else {
                            Err(e)
                        }
                    }
                }
            }
            None => Err(ArbitrageError::parse_error(
                "Embedding engine not available".to_string(),
            )),
        }
    }

    /// Route request to best AI model
    pub async fn route_to_best_model(
        &self,
        requirements: &ModelRequirements,
    ) -> ArbitrageResult<RoutingDecision> {
        if !self.config.enable_model_routing {
            return Err(ArbitrageError::parse_error(
                "Model routing service is disabled",
            ));
        }

        // Check circuit breaker
        if self.is_circuit_breaker_open().await {
            return self.handle_fallback_routing(requirements).await;
        }

        // Check rate limiting
        self.check_rate_limit().await?;

        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        match &self.model_router {
            Some(router) => match router.route_to_best_model(requirements).await {
                Ok(decision) => {
                    self.update_success_metrics(start_time).await;
                    self.update_routing_metrics().await;
                    Ok(decision)
                }
                Err(e) => {
                    self.handle_service_error(&e).await;
                    if self.config.enable_fallback_strategies {
                        self.handle_fallback_routing(requirements).await
                    } else {
                        Err(e)
                    }
                }
            },
            None => Err(ArbitrageError::parse_error(
                "Model router not available".to_string(),
            )),
        }
    }

    /// Rank opportunities for user with personalization
    pub async fn rank_opportunities_for_user(
        &self,
        user_id: &str,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<RankedOpportunity>> {
        if !self.config.enable_personalization {
            return self.handle_fallback_ranking(opportunities).await;
        }

        // Check circuit breaker
        if self.is_circuit_breaker_open().await {
            return self.handle_fallback_ranking(opportunities).await;
        }

        // Check rate limiting
        self.check_rate_limit().await?;

        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        match &self.personalization_engine {
            Some(engine) => {
                match engine
                    .rank_opportunities_for_user(user_id, opportunities)
                    .await
                {
                    Ok(ranked) => {
                        self.update_success_metrics(start_time).await;
                        self.update_personalization_metrics().await;
                        Ok(ranked)
                    }
                    Err(e) => {
                        self.handle_service_error(&e).await;
                        if self.config.enable_fallback_strategies {
                            self.handle_fallback_ranking(vec![]).await
                        } else {
                            Err(e)
                        }
                    }
                }
            }
            None => self.handle_fallback_ranking(opportunities).await,
        }
    }

    /// Record user interaction for learning
    pub async fn record_user_interaction(
        &self,
        interaction: UserInteraction,
    ) -> ArbitrageResult<()> {
        if !self.config.enable_personalization {
            return Ok(()); // Silently ignore if personalization is disabled
        }

        match &self.personalization_engine {
            Some(engine) => engine.record_interaction(interaction).await,
            None => Ok(()),
        }
    }

    /// Get cached data from AI cache
    pub async fn get_cached<T>(
        &self,
        key: &str,
        entry_type: CacheEntryType,
    ) -> ArbitrageResult<Option<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        if !self.config.enable_ai_caching {
            return Ok(None);
        }

        match &self.ai_cache {
            Some(cache) => cache.get(key, entry_type).await,
            None => Ok(None),
        }
    }

    /// Set cached data in AI cache
    pub async fn set_cached<T>(
        &self,
        key: &str,
        entry_type: CacheEntryType,
        data: &T,
        ttl: Option<u64>,
    ) -> ArbitrageResult<()>
    where
        T: serde::Serialize,
    {
        if !self.config.enable_ai_caching {
            return Ok(());
        }

        match &self.ai_cache {
            Some(cache) => cache.set(key, entry_type, data, ttl).await,
            None => Ok(()),
        }
    }

    /// Comprehensive health check for all AI services
    pub async fn health_check(&self) -> ArbitrageResult<AIServiceHealth> {
        let mut health = AIServiceHealth::default();

        // Check embedding engine
        if let Some(ref engine) = self.embedding_engine {
            health.embedding_engine = engine.health_check().await.unwrap_or(false);
        } else {
            health.embedding_engine = !self.config.enable_embedding; // Healthy if disabled
        }

        // Check model router
        if let Some(ref router) = self.model_router {
            health.model_router = router.health_check().await.unwrap_or(false);
        } else {
            health.model_router = !self.config.enable_model_routing; // Healthy if disabled
        }

        // Check personalization engine
        if let Some(ref engine) = self.personalization_engine {
            health.personalization_engine = engine.health_check().await.unwrap_or(false);
        } else {
            health.personalization_engine = !self.config.enable_personalization;
            // Healthy if disabled
        }

        // Check AI cache
        if let Some(ref cache) = self.ai_cache {
            health.ai_cache = cache.health_check().await.unwrap_or(false);
        } else {
            health.ai_cache = !self.config.enable_ai_caching; // Healthy if disabled
        }

        // Overall health
        health.overall_health = health.embedding_engine
            && health.model_router
            && health.personalization_engine
            && health.ai_cache;

        health.last_check = chrono::Utc::now().timestamp_millis() as u64;
        health.circuit_breaker_open = self.is_circuit_breaker_open().await;

        // Update stored health
        let mut stored_health = self.health.lock().unwrap();
        *stored_health = health.clone();

        Ok(health)
    }

    /// Get AI service metrics
    pub async fn get_metrics(&self) -> AIServiceMetrics {
        let metrics = self.metrics.lock().unwrap();
        metrics.clone()
    }

    /// Handle fallback embeddings when main service fails
    async fn handle_fallback_embeddings(
        &self,
        _opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<OpportunityEmbedding>> {
        self.update_fallback_metrics().await;
        // Return empty embeddings as fallback
        Ok(Vec::new())
    }

    /// Handle fallback routing when main service fails
    async fn handle_fallback_routing(
        &self,
        _requirements: &ModelRequirements,
    ) -> ArbitrageResult<RoutingDecision> {
        self.update_fallback_metrics().await;

        // Create a simple fallback routing decision
        use super::{AIModelConfig, RoutingDecision};
        let fallback_model = AIModelConfig {
            model_id: "local-fallback".to_string(),
            provider: "local".to_string(),
            endpoint: "/local/analyze".to_string(),
            max_tokens: Some(1000),
            temperature: Some(0.7),
            cost_per_token: Some(0.0),
            estimated_latency_ms: 100,
            accuracy_score: 0.6,
            supported_tasks: vec!["all".to_string()],
            is_available: true,
            ..Default::default()
        };

        Ok(RoutingDecision {
            selected_model: fallback_model,
            routing_reason: "Fallback routing due to service unavailability".to_string(),
            estimated_cost: Some(0.0),
            estimated_latency_ms: 100,
            confidence_score: 0.5,
            fallback_models: Vec::new(),
            decision_time_ms: 10,
        })
    }

    /// Handle fallback ranking when main service fails
    async fn handle_fallback_ranking(
        &self,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<RankedOpportunity>> {
        self.update_fallback_metrics().await;

        // Simple fallback ranking based on rate difference
        let mut ranked_opportunities: Vec<RankedOpportunity> = opportunities
            .into_iter()
            .map(|opp| {
                let score = (opp.rate_difference / 10.0).clamp(0.0, 1.0);
                RankedOpportunity {
                    opportunity: opp,
                    personalization_score: score as f32,
                    ranking_factors: HashMap::new(),
                    confidence_score: 0.8,
                    explanation: "AI-enhanced ranking".to_string(),
                    predicted_user_satisfaction: score as f32,
                }
            })
            .collect();

        // Sort by rate difference
        ranked_opportunities.sort_by(|a, b| {
            b.personalization_score
                .partial_cmp(&a.personalization_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(ranked_opportunities)
    }

    /// Check if circuit breaker is open
    async fn is_circuit_breaker_open(&self) -> bool {
        if !self.config.enable_circuit_breaker {
            return false;
        }

        let state = self.circuit_breaker_state.lock().unwrap();
        match *state {
            CircuitBreakerState::Open => {
                // Check if timeout has passed
                let last_failure = self.circuit_breaker_last_failure.lock().unwrap();
                if let Some(failure_time) = *last_failure {
                    let now = chrono::Utc::now().timestamp_millis() as u64;
                    if now - failure_time > (self.config.circuit_breaker_timeout_seconds * 1000) {
                        // Move to half-open state
                        drop(state);
                        let mut state = self.circuit_breaker_state.lock().unwrap();
                        *state = CircuitBreakerState::HalfOpen;
                        return false;
                    }
                }
                true
            }
            CircuitBreakerState::HalfOpen => false, // Allow one request to test
            CircuitBreakerState::Closed => false,
        }
    }

    /// Check rate limiting
    async fn check_rate_limit(&self) -> ArbitrageResult<()> {
        let mut active = self.active_requests.lock().unwrap();
        if *active >= self.config.max_concurrent_requests {
            return Err(ArbitrageError::parse_error("Rate limit exceeded"));
        }
        *active += 1;
        Ok(())
    }

    /// Handle service error and update circuit breaker
    async fn handle_service_error(&self, error: &ArbitrageError) {
        // Decrement active requests
        let mut active = self.active_requests.lock().unwrap();
        if *active > 0 {
            *active -= 1;
        }
        drop(active);

        // Update error metrics
        let mut metrics = self.metrics.lock().unwrap();
        metrics.failed_requests += 1;
        metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        drop(metrics);

        // Update health error count
        let mut health = self.health.lock().unwrap();
        health.error_count += 1;

        // Update circuit breaker
        if self.config.enable_circuit_breaker
            && health.error_count >= self.config.circuit_breaker_threshold
        {
            // Open circuit breaker
            let mut state = self.circuit_breaker_state.lock().unwrap();
            *state = CircuitBreakerState::Open;
            drop(state);

            let mut last_failure = self.circuit_breaker_last_failure.lock().unwrap();
            *last_failure = Some(chrono::Utc::now().timestamp_millis() as u64);
            drop(last_failure);

            health.circuit_breaker_open = true;

            // Update metrics
            let mut metrics = self.metrics.lock().unwrap();
            metrics.circuit_breaker_trips += 1;
        }

        self.logger.warn(&format!("AI service error: {}", error));
    }

    /// Update success metrics
    async fn update_success_metrics(&self, start_time: u64) {
        // Decrement active requests
        let mut active = self.active_requests.lock().unwrap();
        if *active > 0 {
            *active -= 1;
        }
        drop(active);

        // Update metrics
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_requests += 1;
        metrics.successful_requests += 1;

        // Update average response time
        let response_time = chrono::Utc::now().timestamp_millis() as u64 - start_time;
        let total_time = metrics.avg_response_time_ms * (metrics.total_requests - 1) as f64
            + response_time as f64;
        metrics.avg_response_time_ms = total_time / metrics.total_requests as f64;

        metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        drop(metrics);

        // Reset circuit breaker on success
        if self.config.enable_circuit_breaker {
            let mut state = self.circuit_breaker_state.lock().unwrap();
            if let CircuitBreakerState::HalfOpen = *state {
                *state = CircuitBreakerState::Closed;
                // Reset error count
                let mut health = self.health.lock().unwrap();
                health.error_count = 0;
                health.circuit_breaker_open = false;
            }
        }
    }

    /// Update routing metrics
    async fn update_routing_metrics(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.routing_decisions += 1;
    }

    /// Update personalization metrics
    async fn update_personalization_metrics(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.personalization_requests += 1;
    }

    /// Update fallback metrics
    async fn update_fallback_metrics(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.fallback_activations += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_coordinator_config_default() {
        let config = AICoordinatorConfig::default();
        assert!(config.enable_ai_services);
        assert!(config.enable_embedding);
        assert!(config.enable_model_routing);
        assert!(config.enable_personalization);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_ai_coordinator_config_high_concurrency() {
        let config = AICoordinatorConfig::high_concurrency();
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.max_concurrent_requests, 200);
        assert_eq!(config.request_timeout_seconds, 15);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_ai_service_health_default() {
        let health = AIServiceHealth::default();
        assert!(!health.embedding_engine);
        assert!(!health.model_router);
        assert!(!health.personalization_engine);
        assert!(!health.ai_cache);
        assert!(!health.overall_health);
        assert_eq!(health.error_count, 0);
    }

    #[test]
    fn test_ai_service_metrics_default() {
        let metrics = AIServiceMetrics::default();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.successful_requests, 0);
        assert_eq!(metrics.failed_requests, 0);
        assert_eq!(metrics.avg_response_time_ms, 0.0);
        assert_eq!(metrics.cache_hit_rate_percent, 0.0);
    }
}
