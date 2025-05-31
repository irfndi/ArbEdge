// Embedding Engine - Vector Generation and Similarity Search Component
// Extracts and modularizes vector functionality from vectorize_service.rs

use crate::types::{ArbitrageOpportunity, ArbitrageType, ExchangeIdEnum};
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use worker::{Env, Fetch, Method, RequestInit};

/// Configuration for EmbeddingEngine
#[derive(Debug, Clone)]
pub struct EmbeddingEngineConfig {
    pub enable_vectorize: bool,
    pub vectorize_index_name: String,
    pub embedding_dimensions: u32,
    pub similarity_threshold: f32,
    pub max_results: u32,
    pub cache_ttl_seconds: u64,
    pub vectorize_api_base_url: String,
    pub account_id: String,
    pub api_token: String,
    pub connection_pool_size: u32,
    pub batch_size: usize,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub enable_local_fallback: bool,
}

impl Default for EmbeddingEngineConfig {
    fn default() -> Self {
        Self {
            enable_vectorize: true,
            vectorize_index_name: "arbitrage-opportunities".to_string(),
            embedding_dimensions: 384,
            similarity_threshold: 0.7,
            max_results: 10,
            cache_ttl_seconds: 3600, // 1 hour
            vectorize_api_base_url: "https://api.cloudflare.com/client/v4".to_string(),
            account_id: "".to_string(),
            api_token: "".to_string(),
            connection_pool_size: 10,
            batch_size: 50,
            timeout_seconds: 30,
            max_retries: 3,
            enable_local_fallback: true,
        }
    }
}

impl EmbeddingEngineConfig {
    /// Create configuration optimized for high concurrency
    pub fn high_concurrency() -> Self {
        Self {
            connection_pool_size: 20,
            batch_size: 100,
            timeout_seconds: 15,
            max_retries: 2,
            cache_ttl_seconds: 1800, // 30 minutes for faster updates
            ..Default::default()
        }
    }

    /// Create configuration optimized for memory usage
    pub fn memory_optimized() -> Self {
        Self {
            connection_pool_size: 5,
            batch_size: 25,
            embedding_dimensions: 256, // Smaller dimensions
            max_results: 5,
            cache_ttl_seconds: 7200, // 2 hours for longer caching
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
        if self.embedding_dimensions == 0 {
            return Err(ArbitrageError::validation_error(
                "embedding_dimensions must be greater than 0",
            ));
        }
        if self.similarity_threshold < 0.0 || self.similarity_threshold > 1.0 {
            return Err(ArbitrageError::validation_error(
                "similarity_threshold must be between 0.0 and 1.0",
            ));
        }
        Ok(())
    }
}

/// Opportunity embedding for vector storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityEmbedding {
    pub opportunity_id: String,
    pub pair: String,
    pub exchange_combination: String,
    pub rate_difference: f32,
    pub risk_level: f32,
    pub market_conditions: Vec<f32>,
    pub embedding: Vec<f32>,
    pub metadata: OpportunityMetadata,
    pub created_at: u64,
}

/// Metadata for opportunity embeddings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityMetadata {
    pub opportunity_type: String,
    pub risk_category: String,
    pub profit_potential: f32,
    pub time_sensitivity: String,
    pub market_volatility: f32,
    pub liquidity_score: f32,
}

/// Similarity search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityResult {
    pub opportunity_id: String,
    pub similarity_score: f32,
    pub opportunity: ArbitrageOpportunity,
    pub metadata: OpportunityMetadata,
}

/// Vectorize API request structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizeRequest {
    pub id: String,
    pub values: Vec<f32>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizeQueryRequest {
    pub vector: Vec<f32>,
    #[serde(rename = "topK")]
    pub top_k: u32,
    pub filter: Option<serde_json::Value>,
    #[serde(rename = "returnValues")]
    pub return_values: bool,
    #[serde(rename = "returnMetadata")]
    pub return_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizeQueryResponse {
    pub success: bool,
    pub result: VectorizeQueryResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizeQueryResult {
    pub matches: Vec<VectorizeMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizeMatch {
    pub id: String,
    pub score: f32,
    pub values: Option<Vec<f32>>,
    pub metadata: Option<serde_json::Value>,
}

/// Embedding Engine for vector generation and similarity search
pub struct EmbeddingEngine {
    config: EmbeddingEngineConfig,
    logger: crate::utils::logger::Logger,
    vectorize_available: Arc<std::sync::Mutex<bool>>,
    #[allow(dead_code)] // TODO: Will be used for health monitoring
    last_health_check: Arc<std::sync::Mutex<Option<u64>>>,
    cache: Option<worker::kv::KvStore>,
    metrics: Arc<std::sync::Mutex<EmbeddingMetrics>>,
}

/// Performance metrics for embedding engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMetrics {
    pub total_embeddings_generated: u64,
    pub total_similarity_searches: u64,
    pub vectorize_requests: u64,
    pub local_fallback_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub avg_embedding_time_ms: f64,
    pub avg_search_time_ms: f64,
    pub success_rate: f64,
    pub last_updated: u64,
}

impl Default for EmbeddingMetrics {
    fn default() -> Self {
        Self {
            total_embeddings_generated: 0,
            total_similarity_searches: 0,
            vectorize_requests: 0,
            local_fallback_requests: 0,
            cache_hits: 0,
            cache_misses: 0,
            avg_embedding_time_ms: 0.0,
            avg_search_time_ms: 0.0,
            success_rate: 1.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

impl EmbeddingEngine {
    /// Create new EmbeddingEngine instance
    pub fn new(env: &Env, mut config: EmbeddingEngineConfig) -> ArbitrageResult<Self> {
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

        // Check if Vectorize is available
        let vectorize_available = config.enable_vectorize
            && !config.account_id.is_empty()
            && !config.api_token.is_empty();

        if !vectorize_available && config.enable_vectorize {
            logger.warn(
                "Vectorize service disabled: missing Cloudflare credentials, using local fallback",
            );
        }

        logger.info(&format!(
            "EmbeddingEngine initialized: vectorize_enabled={}, fallback_enabled={}, dimensions={}",
            vectorize_available, config.enable_local_fallback, config.embedding_dimensions
        ));

        Ok(Self {
            config,
            logger,
            vectorize_available: Arc::new(std::sync::Mutex::new(vectorize_available)),
            last_health_check: Arc::new(std::sync::Mutex::new(None)),
            cache: None,
            metrics: Arc::new(std::sync::Mutex::new(EmbeddingMetrics::default())),
        })
    }

    /// Set cache store for caching operations
    pub fn with_cache(mut self, cache: worker::kv::KvStore) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Generate embedding for an opportunity
    pub async fn generate_opportunity_embedding(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> ArbitrageResult<OpportunityEmbedding> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Try to get from cache first
        if let Some(cached_embedding) = self.get_cached_embedding(&opportunity.id).await? {
            self.update_cache_metrics(true).await;
            return Ok(cached_embedding);
        }
        self.update_cache_metrics(false).await;

        // Generate embedding vector
        let embedding = self.create_embedding_vector(opportunity).await?;

        // Create metadata
        let metadata = self.create_opportunity_metadata(opportunity).await?;

        let opportunity_embedding = OpportunityEmbedding {
            opportunity_id: opportunity.id.clone(),
            pair: opportunity.pair.clone(),
            exchange_combination: format!(
                "{:?}-{:?}",
                opportunity.long_exchange, opportunity.short_exchange
            ),
            rate_difference: opportunity.rate_difference as f32,
            risk_level: self.calculate_opportunity_risk(opportunity),
            market_conditions: self.extract_market_conditions(opportunity),
            embedding,
            metadata,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
        };

        // Cache the embedding
        if self.cache.is_some() {
            let _ = self.cache_embedding(&opportunity_embedding).await;
        }

        // Store in Vectorize if available
        if *self.vectorize_available.lock().unwrap() {
            let _ = self.store_in_vectorize(&opportunity_embedding).await;
        }

        // Update metrics
        let elapsed = chrono::Utc::now().timestamp_millis() as u64 - start_time;
        self.update_embedding_metrics(elapsed, true).await;

        Ok(opportunity_embedding)
    }

    /// Find similar opportunities using vector similarity
    pub async fn find_similar_opportunities(
        &self,
        reference_opportunity: &ArbitrageOpportunity,
        limit: Option<u32>,
    ) -> ArbitrageResult<Vec<SimilarityResult>> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let limit = limit.unwrap_or(self.config.max_results).min(100);

        // Generate embedding for reference opportunity
        let reference_embedding = self
            .generate_opportunity_embedding(reference_opportunity)
            .await?;

        let results = if *self.vectorize_available.lock().unwrap() {
            // Use Vectorize for similarity search
            match self
                .search_vectorize(&reference_embedding.embedding, limit)
                .await
            {
                Ok(results) => {
                    self.update_vectorize_metrics().await;
                    results
                }
                Err(e) => {
                    self.logger.warn(&format!(
                        "Vectorize search failed, using local fallback: {}",
                        e
                    ));
                    self.search_local_fallback(&reference_embedding.embedding, limit)
                        .await?
                }
            }
        } else {
            // Use local fallback
            self.search_local_fallback(&reference_embedding.embedding, limit)
                .await?
        };

        // Update metrics
        let elapsed = chrono::Utc::now().timestamp_millis() as u64 - start_time;
        self.update_search_metrics(elapsed, true).await;

        Ok(results)
    }

    /// Create embedding vector from opportunity data
    async fn create_embedding_vector(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> ArbitrageResult<Vec<f32>> {
        let mut features = Vec::new();

        // Basic opportunity features
        features.push(opportunity.rate_difference as f32);
        features.push(self.calculate_opportunity_risk(opportunity));

        // Exchange features (one-hot encoding)
        let exchanges = [
            ExchangeIdEnum::Binance,
            ExchangeIdEnum::Bybit,
            ExchangeIdEnum::OKX,
            ExchangeIdEnum::Coinbase,
            ExchangeIdEnum::Kraken,
        ];

        for exchange in &exchanges {
            features.push(if opportunity.long_exchange == *exchange {
                1.0
            } else {
                0.0
            });
            features.push(if opportunity.short_exchange == *exchange {
                1.0
            } else {
                0.0
            });
        }

        // Pair features (hash-based encoding)
        let pair_hash = self.hash_pair(&opportunity.pair);
        features.extend(pair_hash);

        // Market condition features
        features.extend(self.extract_market_conditions(opportunity));

        // Pad or truncate to target dimensions
        self.normalize_vector_dimensions(features)
    }

    /// Extract market conditions as feature vector
    fn extract_market_conditions(&self, opportunity: &ArbitrageOpportunity) -> Vec<f32> {
        let mut conditions = Vec::new();

        // Time-based features
        let now = chrono::Utc::now();
        conditions.push((now.hour() as f32) / 24.0); // Hour of day normalized
        conditions.push((now.weekday().num_days_from_monday() as f32) / 7.0); // Day of week

        // Opportunity-specific features
        conditions.push(opportunity.rate_difference.abs() as f32);
        conditions.push(0.0); // Placeholder for volume

        // Risk indicators
        conditions.push(self.calculate_opportunity_risk(opportunity));
        conditions.push(self.calculate_time_sensitivity_factor(opportunity));

        conditions
    }

    /// Calculate opportunity risk score
    fn calculate_opportunity_risk(&self, opportunity: &ArbitrageOpportunity) -> f32 {
        let mut risk_score = 0.0;

        // Rate difference risk (higher difference = higher risk)
        risk_score += (opportunity.rate_difference.abs() * 10.0).min(1.0);

        // Exchange risk (some exchanges are riskier)
        let exchange_risk = match (&opportunity.long_exchange, &opportunity.short_exchange) {
            (ExchangeIdEnum::Binance, ExchangeIdEnum::Coinbase) => 0.1,
            (ExchangeIdEnum::Coinbase, ExchangeIdEnum::Binance) => 0.1,
            _ => 0.2,
        };
        risk_score += exchange_risk;

        (risk_score.min(1.0)) as f32
    }

    /// Calculate time sensitivity factor
    fn calculate_time_sensitivity_factor(&self, _opportunity: &ArbitrageOpportunity) -> f32 {
        // For now, return a default value
        // In a real implementation, this would consider market volatility,
        // opportunity age, and other time-sensitive factors
        0.5
    }

    /// Hash pair name into feature vector
    fn hash_pair(&self, pair: &str) -> Vec<f32> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        pair.hash(&mut hasher);
        let hash = hasher.finish();

        // Convert hash to normalized feature vector
        let mut features = Vec::new();
        for i in 0..8 {
            features.push(((hash >> (i * 8)) & 0xFF) as f32 / 255.0);
        }
        features
    }

    /// Normalize vector to target dimensions
    fn normalize_vector_dimensions(&self, mut vector: Vec<f32>) -> ArbitrageResult<Vec<f32>> {
        let target_dims = self.config.embedding_dimensions as usize;

        if vector.len() > target_dims {
            vector.truncate(target_dims);
        } else if vector.len() < target_dims {
            // Pad with zeros
            vector.resize(target_dims, 0.0);
        }

        // Normalize vector to unit length
        let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for value in &mut vector {
                *value /= magnitude;
            }
        }

        Ok(vector)
    }

    /// Create opportunity metadata
    async fn create_opportunity_metadata(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> ArbitrageResult<OpportunityMetadata> {
        Ok(OpportunityMetadata {
            opportunity_type: "arbitrage".to_string(),
            risk_category: self.categorize_risk(self.calculate_opportunity_risk(opportunity)),
            profit_potential: opportunity.rate_difference.abs() as f32,
            time_sensitivity: "medium".to_string(),
            market_volatility: 0.5, // Default value
            liquidity_score: 0.0,   // Placeholder for liquidity_score
        })
    }

    /// Categorize risk level
    fn categorize_risk(&self, risk_score: f32) -> String {
        match risk_score {
            r if r < 0.3 => "low".to_string(),
            r if r < 0.7 => "medium".to_string(),
            _ => "high".to_string(),
        }
    }

    /// Store embedding in Vectorize
    async fn store_in_vectorize(&self, embedding: &OpportunityEmbedding) -> ArbitrageResult<()> {
        let request = VectorizeRequest {
            id: embedding.opportunity_id.clone(),
            values: embedding.embedding.clone(),
            metadata: serde_json::to_value(&embedding.metadata)?,
        };

        let url = format!(
            "{}/accounts/{}/vectorize/indexes/{}/upsert",
            self.config.vectorize_api_base_url,
            self.config.account_id,
            self.config.vectorize_index_name
        );

        let payload = serde_json::json!({
            "vectors": [request]
        });

        let mut request_init = RequestInit::new();
        request_init.method = Method::Post;
        request_init.headers = worker::Headers::new();
        request_init.headers.set(
            "Authorization",
            &format!("Bearer {}", self.config.api_token),
        )?;
        request_init
            .headers
            .set("Content-Type", "application/json")?;
        request_init.body = Some(payload.to_string().into());

        let response = Fetch::Url(url.parse()?).send().await?;

        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(format!(
                "Vectorize upsert failed: {}",
                response.status_code()
            )));
        }

        Ok(())
    }

    /// Search using Vectorize
    async fn search_vectorize(
        &self,
        query_vector: &[f32],
        limit: u32,
    ) -> ArbitrageResult<Vec<SimilarityResult>> {
        let query = VectorizeQueryRequest {
            vector: query_vector.to_vec(),
            top_k: limit,
            filter: None,
            return_values: false,
            return_metadata: true,
        };

        let url = format!(
            "{}/accounts/{}/vectorize/indexes/{}/query",
            self.config.vectorize_api_base_url,
            self.config.account_id,
            self.config.vectorize_index_name
        );

        let mut request_init = RequestInit::new();
        request_init.method = Method::Post;
        request_init.headers = worker::Headers::new();
        request_init.headers.set(
            "Authorization",
            &format!("Bearer {}", self.config.api_token),
        )?;
        request_init
            .headers
            .set("Content-Type", "application/json")?;
        request_init.body = Some(serde_json::to_string(&query)?.into());

        let mut response = Fetch::Url(url.parse()?).send().await?;

        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(format!(
                "Vectorize query failed: {}",
                response.status_code()
            )));
        }

        let response_data: VectorizeQueryResponse = response.json().await?;

        let mut results = Vec::new();
        for match_result in response_data.result.matches {
            if match_result.score >= self.config.similarity_threshold {
                if let Some(metadata) = match_result.metadata {
                    if let Ok(opportunity_metadata) =
                        serde_json::from_value::<OpportunityMetadata>(metadata)
                    {
                        // Create a placeholder opportunity (in real implementation, fetch from database)
                        let opportunity = self.create_placeholder_opportunity(&match_result.id);

                        results.push(SimilarityResult {
                            opportunity_id: match_result.id,
                            similarity_score: match_result.score,
                            opportunity,
                            metadata: opportunity_metadata,
                        });
                    }
                }
            }
        }

        Ok(results)
    }

    /// Local fallback similarity search
    async fn search_local_fallback(
        &self,
        _query_vector: &[f32],
        limit: u32,
    ) -> ArbitrageResult<Vec<SimilarityResult>> {
        self.update_fallback_metrics().await;

        // For now, return empty results
        // In a real implementation, this would use a local vector database
        // or implement cosine similarity search against cached embeddings
        self.logger.info(&format!(
            "Local fallback search requested for {} results",
            limit
        ));
        Ok(Vec::new())
    }

    /// Create placeholder opportunity for demo purposes
    fn create_placeholder_opportunity(&self, _opportunity_id: &str) -> ArbitrageOpportunity {
        // Create test opportunity with correct field names
        ArbitrageOpportunity {
            id: "test_123".to_string(),
            pair: "BTCUSDT".to_string(),
            long_exchange: ExchangeIdEnum::Binance,
            short_exchange: ExchangeIdEnum::Bybit,
            long_rate: Some(0.01),
            short_rate: Some(-0.01),
            rate_difference: 0.02,
            net_rate_difference: Some(0.02),
            potential_profit_value: Some(20.0),
            confidence: 0.8,
            volume: 1000.0,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            detected_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: chrono::Utc::now().timestamp_millis() as u64 + 300_000,
            r#type: ArbitrageType::FundingRate,
            details: Some("Test opportunity".to_string()),
            min_exchanges_required: 2,
        }
    }

    /// Cache embedding
    async fn cache_embedding(&self, embedding: &OpportunityEmbedding) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("embedding:{}", embedding.opportunity_id);
            let embedding_json = serde_json::to_string(embedding)?;

            cache
                .put(&cache_key, &embedding_json)?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await?;
        }
        Ok(())
    }

    /// Get cached embedding
    async fn get_cached_embedding(
        &self,
        opportunity_id: &str,
    ) -> ArbitrageResult<Option<OpportunityEmbedding>> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("embedding:{}", opportunity_id);

            match cache.get(&cache_key).text().await {
                Ok(Some(embedding_json)) => {
                    match serde_json::from_str::<OpportunityEmbedding>(&embedding_json) {
                        Ok(embedding) => return Ok(Some(embedding)),
                        Err(e) => {
                            self.logger
                                .warn(&format!("Failed to deserialize cached embedding: {}", e));
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.logger
                        .warn(&format!("Failed to get cached embedding: {}", e));
                }
            }
        }
        Ok(None)
    }

    /// Update embedding metrics
    async fn update_embedding_metrics(&self, elapsed_ms: u64, success: bool) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_embeddings_generated += 1;

            // Update average embedding time
            let total_time = metrics.avg_embedding_time_ms
                * (metrics.total_embeddings_generated - 1) as f64
                + elapsed_ms as f64;
            metrics.avg_embedding_time_ms = total_time / metrics.total_embeddings_generated as f64;

            // Update success rate
            if success {
                metrics.success_rate =
                    (metrics.success_rate * (metrics.total_embeddings_generated - 1) as f64 + 1.0)
                        / metrics.total_embeddings_generated as f64;
            } else {
                metrics.success_rate = (metrics.success_rate
                    * (metrics.total_embeddings_generated - 1) as f64)
                    / metrics.total_embeddings_generated as f64;
            }

            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Update search metrics
    async fn update_search_metrics(&self, elapsed_ms: u64, _success: bool) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_similarity_searches += 1;

            // Update average search time
            let total_time = metrics.avg_search_time_ms
                * (metrics.total_similarity_searches - 1) as f64
                + elapsed_ms as f64;
            metrics.avg_search_time_ms = total_time / metrics.total_similarity_searches as f64;

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

    /// Update Vectorize metrics
    async fn update_vectorize_metrics(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.vectorize_requests += 1;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Update fallback metrics
    async fn update_fallback_metrics(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.local_fallback_requests += 1;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Get performance metrics
    pub async fn get_metrics(&self) -> EmbeddingMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Check Vectorize availability if enabled
        if self.config.enable_vectorize {
            // Perform a simple health check query
            // For now, just return the current status
            return Ok(*self.vectorize_available.lock().unwrap());
        }

        // If Vectorize is disabled, check if local fallback is available
        Ok(self.config.enable_local_fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_engine_config_default() {
        let config = EmbeddingEngineConfig::default();
        assert!(config.enable_vectorize);
        assert!(config.enable_local_fallback);
        assert_eq!(config.embedding_dimensions, 384);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_embedding_engine_config_high_concurrency() {
        let config = EmbeddingEngineConfig::high_concurrency();
        assert_eq!(config.connection_pool_size, 20);
        assert_eq!(config.batch_size, 100);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_embedding_engine_config_memory_optimized() {
        let config = EmbeddingEngineConfig::memory_optimized();
        assert_eq!(config.connection_pool_size, 5);
        assert_eq!(config.embedding_dimensions, 256);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_opportunity_metadata_creation() {
        let metadata = OpportunityMetadata {
            opportunity_type: "arbitrage".to_string(),
            risk_category: "low".to_string(),
            profit_potential: 0.002,
            time_sensitivity: "medium".to_string(),
            market_volatility: 0.5,
            liquidity_score: 1.0,
        };

        assert_eq!(metadata.opportunity_type, "arbitrage");
        assert_eq!(metadata.risk_category, "low");
        assert_eq!(metadata.profit_potential, 0.002);
    }
}
