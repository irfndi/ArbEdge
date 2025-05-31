// Model Router - AI Model Selection and Routing Component
// Extracts and modularizes AI routing functionality from ai_gateway.rs

use crate::utils::{ArbitrageError, ArbitrageResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::Env;

/// AI Provider trait for different AI service providers
#[async_trait]
pub trait AIProvider: Send + Sync {
    async fn generate_text(&self, prompt: &str) -> ArbitrageResult<String>;
    async fn analyze_market_data(
        &self,
        data: &serde_json::Value,
    ) -> ArbitrageResult<serde_json::Value>;
    async fn get_embeddings(&self, text: &str) -> ArbitrageResult<Vec<f64>>;
    fn get_provider_name(&self) -> &str;
    fn is_available(&self) -> bool;
}

/// Configuration for ModelRouter
#[derive(Debug, Clone)]
pub struct ModelRouterConfig {
    pub enable_ai_gateway: bool,
    pub gateway_id: String,
    pub base_url: String,
    pub account_id: String,
    pub api_token: String,
    pub default_timeout_seconds: u64,
    pub max_retries: u32,
    pub enable_cost_tracking: bool,
    pub enable_model_analytics: bool,
    pub enable_intelligent_routing: bool,
    pub connection_pool_size: u32,
    pub batch_size: usize,
    pub enable_local_fallback: bool,
}

impl Default for ModelRouterConfig {
    fn default() -> Self {
        Self {
            enable_ai_gateway: true,
            gateway_id: "arbitrage-ai-gateway".to_string(),
            base_url: "https://gateway.ai.cloudflare.com".to_string(),
            account_id: "".to_string(),
            api_token: "".to_string(),
            default_timeout_seconds: 30,
            max_retries: 3,
            enable_cost_tracking: true,
            enable_model_analytics: true,
            enable_intelligent_routing: true,
            connection_pool_size: 10,
            batch_size: 25,
            enable_local_fallback: true,
        }
    }
}

impl ModelRouterConfig {
    /// Create configuration optimized for high concurrency
    pub fn high_concurrency() -> Self {
        Self {
            connection_pool_size: 20,
            batch_size: 50,
            default_timeout_seconds: 15,
            max_retries: 2,
            ..Default::default()
        }
    }

    /// Create configuration optimized for memory usage
    pub fn memory_optimized() -> Self {
        Self {
            connection_pool_size: 5,
            batch_size: 10,
            default_timeout_seconds: 45,
            enable_model_analytics: false, // Disable to save memory
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.connection_pool_size == 0 {
            return Err(ArbitrageError::validation_error(
                "connection_pool_size must be greater than 0",
            ));
        }
        if self.batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "batch_size must be greater than 0",
            ));
        }
        if self.default_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "default_timeout_seconds must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// AI model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIModelConfig {
    pub model_id: String,
    pub provider: String, // "openai", "anthropic", "workers-ai", "local"
    pub endpoint: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub cost_per_token: Option<f32>,
    pub estimated_latency_ms: u64,
    pub accuracy_score: f32,
    pub supported_tasks: Vec<String>,
    pub is_available: bool,
}

impl Default for AIModelConfig {
    fn default() -> Self {
        Self {
            model_id: "gpt-3.5-turbo".to_string(),
            provider: "openai".to_string(),
            endpoint: "/v1/chat/completions".to_string(),
            max_tokens: Some(1000),
            temperature: Some(0.7),
            top_p: Some(1.0),
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
            cost_per_token: Some(0.000002), // $0.002 per 1K tokens
            estimated_latency_ms: 2000,
            accuracy_score: 0.85,
            supported_tasks: vec![
                "opportunity_analysis".to_string(),
                "market_prediction".to_string(),
                "risk_assessment".to_string(),
            ],
            is_available: true,
        }
    }
}

/// Model requirements for intelligent routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequirements {
    pub task_type: String,
    pub max_latency_ms: Option<u64>,
    pub max_cost_per_request: Option<f32>,
    pub min_accuracy_score: Option<f32>,
    pub preferred_providers: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub fallback_enabled: bool,
    pub priority: RequestPriority,
}

/// Request priority levels
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum RequestPriority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

/// AI Gateway routing decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub selected_model: AIModelConfig,
    pub routing_reason: String,
    pub estimated_cost: Option<f32>,
    pub estimated_latency_ms: u64,
    pub confidence_score: f32,
    pub fallback_models: Vec<AIModelConfig>,
    pub decision_time_ms: u64,
}

/// Model performance analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAnalytics {
    pub model_id: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_latency_ms: f32,
    pub average_cost_usd: f32,
    pub accuracy_score: f32,
    pub cache_hit_rate_percent: f32,
    pub error_rate_percent: f32,
    pub last_24h_requests: u64,
    pub last_updated: u64,
}

impl Default for ModelAnalytics {
    fn default() -> Self {
        Self {
            model_id: "".to_string(),
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_latency_ms: 0.0,
            average_cost_usd: 0.0,
            accuracy_score: 0.0,
            cache_hit_rate_percent: 0.0,
            error_rate_percent: 0.0,
            last_24h_requests: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Model Router for intelligent AI model selection and routing
pub struct ModelRouter {
    config: ModelRouterConfig,
    logger: crate::utils::logger::Logger,
    providers: HashMap<String, Box<dyn AIProvider + Send + Sync>>,
    #[allow(dead_code)] // TODO: Will be used for health monitoring
    last_health_check: Arc<std::sync::Mutex<Option<u64>>>,
    available_models: HashMap<String, AIModelConfig>,
    model_analytics: Arc<std::sync::Mutex<HashMap<String, ModelAnalytics>>>,
    ai_gateway_available: Arc<std::sync::Mutex<bool>>,
    cache: Option<worker::kv::KvStore>,
    metrics: Arc<std::sync::Mutex<RouterMetrics>>,
}

/// Performance metrics for model router
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterMetrics {
    pub total_routing_decisions: u64,
    pub successful_routes: u64,
    pub failed_routes: u64,
    pub fallback_routes: u64,
    pub avg_decision_time_ms: f64,
    pub cost_savings_usd: f64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub last_updated: u64,
}

impl Default for RouterMetrics {
    fn default() -> Self {
        Self {
            total_routing_decisions: 0,
            successful_routes: 0,
            failed_routes: 0,
            fallback_routes: 0,
            avg_decision_time_ms: 0.0,
            cost_savings_usd: 0.0,
            cache_hits: 0,
            cache_misses: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

impl ModelRouter {
    /// Create new ModelRouter instance
    pub fn new(env: &Env, mut config: ModelRouterConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Get Cloudflare credentials from environment
        if let Ok(account_id) = env.var("CLOUDFLARE_ACCOUNT_ID") {
            config.account_id = account_id.to_string();
        }
        if let Ok(api_token) = env.var("CLOUDFLARE_API_TOKEN") {
            config.api_token = api_token.to_string();
        }

        // Validate configuration
        config.validate()?;

        // Check if AI Gateway is available
        let ai_gateway_available = config.enable_ai_gateway
            && !config.account_id.is_empty()
            && !config.api_token.is_empty();

        if !ai_gateway_available && config.enable_ai_gateway {
            logger.warn(
                "AI Gateway service disabled: missing Cloudflare credentials, using local fallback",
            );
        }

        let mut router = Self {
            config,
            logger,
            providers: HashMap::new(),
            last_health_check: Arc::new(std::sync::Mutex::new(None)),
            available_models: HashMap::new(),
            model_analytics: Arc::new(std::sync::Mutex::new(HashMap::new())),
            ai_gateway_available: Arc::new(std::sync::Mutex::new(ai_gateway_available)),
            cache: None,
            metrics: Arc::new(std::sync::Mutex::new(RouterMetrics::default())),
        };

        // Initialize default models
        router.initialize_default_models()?;

        router.logger.info(&format!(
            "ModelRouter initialized: ai_gateway_enabled={}, models_available={}, intelligent_routing={}",
            ai_gateway_available, router.available_models.len(), router.config.enable_intelligent_routing
        ));

        Ok(router)
    }

    /// Set cache store for caching operations
    pub fn with_cache(mut self, cache: worker::kv::KvStore) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Route request to best available model
    pub async fn route_to_best_model(
        &self,
        requirements: &ModelRequirements,
    ) -> ArbitrageResult<RoutingDecision> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Try to get cached routing decision first
        if let Some(cached_decision) = self.get_cached_routing_decision(requirements).await? {
            self.update_cache_metrics(true).await;
            return Ok(cached_decision);
        }
        self.update_cache_metrics(false).await;

        let decision = if self.config.enable_intelligent_routing {
            self.intelligent_route(requirements).await?
        } else {
            self.simple_route(requirements).await?
        };

        // Cache the routing decision
        if self.cache.is_some() {
            let _ = self.cache_routing_decision(requirements, &decision).await;
        }

        // Update metrics
        let elapsed = chrono::Utc::now().timestamp_millis() as u64 - start_time;
        self.update_routing_metrics(elapsed, true, decision.fallback_models.is_empty())
            .await;

        Ok(decision)
    }

    /// Intelligent routing based on requirements and model performance
    async fn intelligent_route(
        &self,
        requirements: &ModelRequirements,
    ) -> ArbitrageResult<RoutingDecision> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Get models that support the required task
        let candidate_models = self.get_models_for_task(&requirements.task_type);

        if candidate_models.is_empty() {
            return Err(ArbitrageError::not_found(format!(
                "No models available for task: {}",
                requirements.task_type
            )));
        }

        // Score each model based on requirements
        let mut scored_models = Vec::new();
        for model in candidate_models {
            let score = self.calculate_model_score(&model, requirements).await?;
            scored_models.push((model, score));
        }

        // Sort by score (highest first) with explicit NaN handling
        scored_models.sort_by(|a, b| {
            // Handle NaN values explicitly by pushing them to the end
            match (a.1.is_nan(), b.1.is_nan()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater, // NaN goes to end (lower priority)
                (false, true) => std::cmp::Ordering::Less,    // Non-NaN comes first
                (false, false) => b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal),
            }
        });

        // Select the best model
        let (best_model, best_score) = scored_models
            .first()
            .ok_or_else(|| ArbitrageError::parse_error("No suitable model found"))?;

        // Prepare fallback models
        let fallback_models: Vec<AIModelConfig> = scored_models
            .iter()
            .skip(1)
            .take(3) // Top 3 fallback models
            .map(|(model, _)| model.clone())
            .collect();

        let estimated_cost = self.estimate_cost(best_model, requirements).await?;
        let decision_time = chrono::Utc::now().timestamp_millis() as u64 - start_time;

        Ok(RoutingDecision {
            selected_model: best_model.clone(),
            routing_reason: format!("Intelligent routing: score={:.3}", best_score),
            estimated_cost,
            estimated_latency_ms: best_model.estimated_latency_ms,
            confidence_score: *best_score,
            fallback_models,
            decision_time_ms: decision_time,
        })
    }

    /// Simple routing based on availability and basic requirements
    async fn simple_route(
        &self,
        requirements: &ModelRequirements,
    ) -> ArbitrageResult<RoutingDecision> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Get first available model for the task
        let candidate_models = self.get_models_for_task(&requirements.task_type);

        let selected_model = candidate_models.first().ok_or_else(|| {
            ArbitrageError::not_found(format!(
                "No models available for task: {}",
                requirements.task_type
            ))
        })?;

        let estimated_cost = self.estimate_cost(selected_model, requirements).await?;
        let decision_time = chrono::Utc::now().timestamp_millis() as u64 - start_time;

        Ok(RoutingDecision {
            selected_model: selected_model.clone(),
            routing_reason: "Simple routing: first available model".to_string(),
            estimated_cost,
            estimated_latency_ms: selected_model.estimated_latency_ms,
            confidence_score: 0.5, // Default confidence for simple routing
            fallback_models: Vec::new(),
            decision_time_ms: decision_time,
        })
    }

    /// Calculate model score based on requirements
    async fn calculate_model_score(
        &self,
        model: &AIModelConfig,
        requirements: &ModelRequirements,
    ) -> ArbitrageResult<f32> {
        let mut score = 0.0;
        let mut weight_sum = 0.0;

        // Accuracy score (weight: 0.4)
        if let Some(min_accuracy) = requirements.min_accuracy_score {
            if model.accuracy_score >= min_accuracy {
                score += model.accuracy_score * 0.4;
                weight_sum += 0.4;
            } else {
                return Ok(0.0); // Model doesn't meet minimum accuracy
            }
        } else {
            score += model.accuracy_score * 0.4;
            weight_sum += 0.4;
        }

        // Latency score (weight: 0.3)
        if let Some(max_latency) = requirements.max_latency_ms {
            if model.estimated_latency_ms <= max_latency {
                let latency_score = 1.0 - (model.estimated_latency_ms as f32 / max_latency as f32);
                score += latency_score * 0.3;
                weight_sum += 0.3;
            } else {
                return Ok(0.0); // Model doesn't meet latency requirement
            }
        } else {
            // Default latency scoring (prefer faster models)
            let latency_score = 1.0 - (model.estimated_latency_ms as f32 / 10000.0).min(1.0);
            score += latency_score * 0.3;
            weight_sum += 0.3;
        }

        // Cost score (weight: 0.2)
        if let Some(max_cost) = requirements.max_cost_per_request {
            if let Some(model_cost) = model.cost_per_token {
                let estimated_request_cost = model_cost * 1000.0; // Assume 1000 tokens per request
                if estimated_request_cost <= max_cost {
                    let cost_score = 1.0 - (estimated_request_cost / max_cost);
                    score += cost_score * 0.2;
                    weight_sum += 0.2;
                } else {
                    return Ok(0.0); // Model doesn't meet cost requirement
                }
            }
        } else if let Some(model_cost) = model.cost_per_token {
            // Default cost scoring (prefer cheaper models)
            let cost_score = 1.0 - (model_cost * 1000.0 / 0.1).min(1.0); // Normalize against $0.1 per request
            score += cost_score * 0.2;
            weight_sum += 0.2;
        }

        // Provider preference score (weight: 0.1)
        if requirements.preferred_providers.contains(&model.provider) {
            score += 1.0 * 0.1;
            weight_sum += 0.1;
        }

        // Normalize score
        if weight_sum > 0.0 {
            score /= weight_sum;
        } else {
            // If no weights were applied, default to 0.0
            score = 0.0;
        }

        // Apply priority boost
        match requirements.priority {
            RequestPriority::Critical => score *= 1.2,
            RequestPriority::High => score *= 1.1,
            RequestPriority::Normal => {}
            RequestPriority::Low => score *= 0.9,
        }

        // Apply model analytics boost
        if self.config.enable_model_analytics {
            if let Ok(analytics) = self.model_analytics.lock() {
                if let Some(model_analytics) = analytics.get(&model.model_id) {
                    let success_rate = if model_analytics.total_requests > 0 {
                        let rate = model_analytics.successful_requests as f32
                            / model_analytics.total_requests as f32;
                        // Ensure success rate is valid (not NaN or infinite)
                        if rate.is_nan() || rate.is_infinite() {
                            1.0 // Default to perfect success rate if calculation fails
                        } else {
                            rate.clamp(0.0, 1.0) // Ensure rate is between 0 and 1
                        }
                    } else {
                        1.0
                    };
                    score *= success_rate;
                }
            }
        }

        // Final validation: ensure score is finite and within bounds
        let final_score = if score.is_nan() || score.is_infinite() {
            0.0
        } else {
            score.clamp(0.0, 1.0)
        };

        Ok(final_score)
    }

    /// Get models that support a specific task
    fn get_models_for_task(&self, task_type: &str) -> Vec<AIModelConfig> {
        self.available_models
            .values()
            .filter(|model| {
                model.is_available
                    && (model.supported_tasks.contains(&task_type.to_string())
                        || model.supported_tasks.contains(&"all".to_string()))
            })
            .cloned()
            .collect()
    }

    /// Estimate cost for a request
    async fn estimate_cost(
        &self,
        model: &AIModelConfig,
        _requirements: &ModelRequirements,
    ) -> ArbitrageResult<Option<f32>> {
        if let Some(cost_per_token) = model.cost_per_token {
            // Estimate 1000 tokens per request (this could be more sophisticated)
            Ok(Some(cost_per_token * 1000.0))
        } else {
            Ok(None)
        }
    }

    /// Initialize default AI models
    fn initialize_default_models(&mut self) -> ArbitrageResult<()> {
        // OpenAI GPT-3.5 Turbo
        let gpt35_turbo = AIModelConfig {
            model_id: "gpt-3.5-turbo".to_string(),
            provider: "openai".to_string(),
            endpoint: "/v1/chat/completions".to_string(),
            max_tokens: Some(4000),
            temperature: Some(0.7),
            cost_per_token: Some(0.000002),
            estimated_latency_ms: 2000,
            accuracy_score: 0.85,
            supported_tasks: vec![
                "opportunity_analysis".to_string(),
                "market_prediction".to_string(),
            ],
            is_available: true,
            ..Default::default()
        };

        // OpenAI GPT-4
        let gpt4 = AIModelConfig {
            model_id: "gpt-4".to_string(),
            provider: "openai".to_string(),
            endpoint: "/v1/chat/completions".to_string(),
            max_tokens: Some(8000),
            temperature: Some(0.7),
            cost_per_token: Some(0.00003),
            estimated_latency_ms: 5000,
            accuracy_score: 0.95,
            supported_tasks: vec![
                "opportunity_analysis".to_string(),
                "risk_assessment".to_string(),
                "market_prediction".to_string(),
            ],
            is_available: true,
            ..Default::default()
        };

        // Anthropic Claude
        let claude = AIModelConfig {
            model_id: "claude-3-sonnet".to_string(),
            provider: "anthropic".to_string(),
            endpoint: "/v1/messages".to_string(),
            max_tokens: Some(4000),
            temperature: Some(0.7),
            cost_per_token: Some(0.000015),
            estimated_latency_ms: 3000,
            accuracy_score: 0.90,
            supported_tasks: vec![
                "opportunity_analysis".to_string(),
                "risk_assessment".to_string(),
            ],
            is_available: true,
            ..Default::default()
        };

        // Workers AI (Local fallback)
        let workers_ai = AIModelConfig {
            model_id: "workers-ai-llama".to_string(),
            provider: "workers-ai".to_string(),
            endpoint: "/ai/run/@cf/meta/llama-2-7b-chat-int8".to_string(),
            max_tokens: Some(2000),
            temperature: Some(0.7),
            cost_per_token: Some(0.000001),
            estimated_latency_ms: 1000,
            accuracy_score: 0.75,
            supported_tasks: vec!["opportunity_analysis".to_string()],
            is_available: true,
            ..Default::default()
        };

        // Local fallback model
        let local_fallback = AIModelConfig {
            model_id: "local-heuristic".to_string(),
            provider: "local".to_string(),
            endpoint: "/local/analyze".to_string(),
            max_tokens: Some(1000),
            temperature: None,
            cost_per_token: Some(0.0), // Free local processing
            estimated_latency_ms: 100,
            accuracy_score: 0.60,
            supported_tasks: vec!["all".to_string()],
            is_available: true,
            ..Default::default()
        };

        // Add models to available models
        self.available_models
            .insert(gpt35_turbo.model_id.clone(), gpt35_turbo);
        self.available_models.insert(gpt4.model_id.clone(), gpt4);
        self.available_models
            .insert(claude.model_id.clone(), claude);
        self.available_models
            .insert(workers_ai.model_id.clone(), workers_ai);
        self.available_models
            .insert(local_fallback.model_id.clone(), local_fallback);

        // Initialize analytics for each model
        if let Ok(mut analytics) = self.model_analytics.lock() {
            for model_id in self.available_models.keys() {
                analytics.insert(
                    model_id.clone(),
                    ModelAnalytics {
                        model_id: model_id.clone(),
                        ..Default::default()
                    },
                );
            }
        }

        Ok(())
    }

    /// Add or update a model configuration
    pub fn add_model(&mut self, model: AIModelConfig) {
        self.available_models
            .insert(model.model_id.clone(), model.clone());

        // Initialize analytics for new model
        if let Ok(mut analytics) = self.model_analytics.lock() {
            analytics.insert(
                model.model_id.clone(),
                ModelAnalytics {
                    model_id: model.model_id,
                    ..Default::default()
                },
            );
        }
    }

    /// Remove a model configuration
    pub fn remove_model(&mut self, model_id: &str) -> Option<AIModelConfig> {
        let removed = self.available_models.remove(model_id);

        // Remove analytics for removed model
        if let Ok(mut analytics) = self.model_analytics.lock() {
            analytics.remove(model_id);
        }

        removed
    }

    /// Get all available models
    pub fn get_available_models(&self) -> Vec<&AIModelConfig> {
        self.available_models.values().collect()
    }

    /// Get model analytics
    pub async fn get_model_analytics(&self, model_id: &str) -> ArbitrageResult<ModelAnalytics> {
        if let Ok(analytics) = self.model_analytics.lock() {
            analytics.get(model_id).cloned().ok_or_else(|| {
                ArbitrageError::parse_error(format!("Model analytics not found: {}", model_id))
            })
        } else {
            Err(ArbitrageError::parse_error(
                "Model analytics not found".to_string(),
            ))
        }
    }

    /// Update model analytics after a request
    pub async fn update_model_analytics(
        &self,
        model_id: &str,
        latency_ms: u64,
        cost_usd: Option<f32>,
        success: bool,
    ) -> ArbitrageResult<()> {
        if let Ok(mut analytics) = self.model_analytics.lock() {
            if let Some(model_analytics) = analytics.get_mut(model_id) {
                model_analytics.total_requests += 1;

                if success {
                    model_analytics.successful_requests += 1;
                } else {
                    model_analytics.failed_requests += 1;
                }

                // Update average latency
                let total_latency = model_analytics.average_latency_ms
                    * (model_analytics.total_requests - 1) as f32
                    + latency_ms as f32;
                model_analytics.average_latency_ms =
                    total_latency / model_analytics.total_requests as f32;

                // Update average cost
                if let Some(cost) = cost_usd {
                    let total_cost = model_analytics.average_cost_usd
                        * (model_analytics.total_requests - 1) as f32
                        + cost;
                    model_analytics.average_cost_usd =
                        total_cost / model_analytics.total_requests as f32;
                }

                // Update error rate
                model_analytics.error_rate_percent = (model_analytics.failed_requests as f32
                    / model_analytics.total_requests as f32)
                    * 100.0;

                model_analytics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
            }
        }

        Ok(())
    }

    /// Cache routing decision
    async fn cache_routing_decision(
        &self,
        requirements: &ModelRequirements,
        decision: &RoutingDecision,
    ) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            use sha2::{Digest, Sha256};
            let req_bytes = serde_json::to_vec(requirements)?;
            let hash = hex::encode(Sha256::digest(&req_bytes));
            let cache_key = format!("routing:{}:{}", requirements.task_type, &hash[0..16]); // first 16 hex chars

            let decision_json = serde_json::to_string(decision)?;

            cache
                .put(&cache_key, &decision_json)?
                .expiration_ttl(300) // 5 minutes TTL for routing decisions
                .execute()
                .await?;
        }
        Ok(())
    }

    /// Get cached routing decision
    async fn get_cached_routing_decision(
        &self,
        requirements: &ModelRequirements,
    ) -> ArbitrageResult<Option<RoutingDecision>> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!(
                "routing:{}:{}",
                requirements.task_type,
                serde_json::to_string(requirements)
                    .unwrap_or_default()
                    .len()
            );

            match cache.get(&cache_key).text().await {
                Ok(Some(decision_json)) => {
                    match serde_json::from_str::<RoutingDecision>(&decision_json) {
                        Ok(decision) => return Ok(Some(decision)),
                        Err(e) => {
                            self.logger.warn(&format!(
                                "Failed to deserialize cached routing decision: {}",
                                e
                            ));
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.logger
                        .warn(&format!("Failed to get cached routing decision: {}", e));
                }
            }
        }
        Ok(None)
    }

    /// Update routing metrics
    async fn update_routing_metrics(&self, elapsed_ms: u64, success: bool, is_fallback: bool) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_routing_decisions += 1;

            if success {
                metrics.successful_routes += 1;
            } else {
                metrics.failed_routes += 1;
            }

            if is_fallback {
                metrics.fallback_routes += 1;
            }

            // Update average decision time
            let total_time = metrics.avg_decision_time_ms
                * (metrics.total_routing_decisions - 1) as f64
                + elapsed_ms as f64;
            metrics.avg_decision_time_ms = total_time / metrics.total_routing_decisions as f64;

            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Update cache metrics
    async fn update_cache_metrics(&self, hit: bool) {
        if let Ok(mut metrics) = self.metrics.lock() {
            if hit {
                metrics.cache_hits += 1;
            } else {
                metrics.cache_misses += 1;
            }
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Get router performance metrics
    pub async fn get_metrics(&self) -> RouterMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Check if AI Gateway is available if enabled
        if self.config.enable_ai_gateway {
            return Ok(*self.ai_gateway_available.lock().unwrap());
        }

        // If AI Gateway is disabled, check if local fallback is available
        Ok(self.config.enable_local_fallback && !self.available_models.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_router_config_default() {
        let config = ModelRouterConfig::default();
        assert!(config.enable_ai_gateway);
        assert!(config.enable_intelligent_routing);
        assert!(config.enable_cost_tracking);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_model_router_config_high_concurrency() {
        let config = ModelRouterConfig::high_concurrency();
        assert_eq!(config.connection_pool_size, 20);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.default_timeout_seconds, 15);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_model_router_config_memory_optimized() {
        let config = ModelRouterConfig::memory_optimized();
        assert_eq!(config.connection_pool_size, 5);
        assert_eq!(config.batch_size, 10);
        assert!(!config.enable_model_analytics);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_ai_model_config_default() {
        let model = AIModelConfig::default();
        assert_eq!(model.model_id, "gpt-3.5-turbo");
        assert_eq!(model.provider, "openai");
        assert!(model.is_available);
        assert!(!model.supported_tasks.is_empty());
    }

    #[test]
    fn test_model_requirements_creation() {
        let requirements = ModelRequirements {
            task_type: "opportunity_analysis".to_string(),
            max_latency_ms: Some(3000),
            max_cost_per_request: Some(0.01),
            min_accuracy_score: Some(0.8),
            preferred_providers: vec!["openai".to_string()],
            required_capabilities: vec!["chat".to_string()],
            fallback_enabled: true,
            priority: RequestPriority::High,
        };

        assert_eq!(requirements.task_type, "opportunity_analysis");
        assert_eq!(requirements.max_latency_ms, Some(3000));
        assert!(requirements.fallback_enabled);
    }

    #[test]
    fn test_routing_decision_creation() {
        let model = AIModelConfig::default();
        let decision = RoutingDecision {
            selected_model: model.clone(),
            routing_reason: "Test routing".to_string(),
            estimated_cost: Some(0.002),
            estimated_latency_ms: 2000,
            confidence_score: 0.85,
            fallback_models: vec![model],
            decision_time_ms: 50,
        };

        assert_eq!(decision.routing_reason, "Test routing");
        assert_eq!(decision.estimated_cost, Some(0.002));
        assert_eq!(decision.confidence_score, 0.85);
    }
}
