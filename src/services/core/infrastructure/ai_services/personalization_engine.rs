// Personalization Engine - User Preference Learning and Opportunity Ranking Component
// Extracts and modularizes personalization functionality from vectorize_service.rs

use crate::services::interfaces::telegram::telegram::{
    AlertSettings, DashboardLayout, DisplaySettings, NotificationSettings, UserPreferences,
};
use crate::types::ArbitrageOpportunity;
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

/// Configuration for PersonalizationEngine
#[derive(Debug, Clone)]
pub struct PersonalizationEngineConfig {
    pub enable_personalization: bool,
    pub enable_learning: bool,
    pub enable_ranking: bool,
    pub learning_rate: f32,
    pub preference_decay_rate: f32,
    pub min_interaction_threshold: u32,
    pub max_preference_history: u32,
    pub ranking_algorithm: RankingAlgorithm,
    pub cache_ttl_seconds: u64,
    pub batch_size: usize,
    pub connection_pool_size: u32,
    pub enable_real_time_updates: bool,
    pub enable_collaborative_filtering: bool,
}

impl Default for PersonalizationEngineConfig {
    fn default() -> Self {
        Self {
            enable_personalization: true,
            enable_learning: true,
            enable_ranking: true,
            learning_rate: 0.1,
            preference_decay_rate: 0.95,
            min_interaction_threshold: 5,
            max_preference_history: 1000,
            ranking_algorithm: RankingAlgorithm::Hybrid,
            cache_ttl_seconds: 900, // 15 minutes
            batch_size: 25,
            connection_pool_size: 12,
            enable_real_time_updates: true,
            enable_collaborative_filtering: false, // Disabled by default for privacy
        }
    }
}

impl PersonalizationEngineConfig {
    /// Create configuration optimized for high concurrency
    pub fn high_concurrency() -> Self {
        Self {
            connection_pool_size: 20,
            batch_size: 50,
            cache_ttl_seconds: 600, // 10 minutes for faster updates
            enable_real_time_updates: true,
            ..Default::default()
        }
    }

    /// Create configuration optimized for memory usage
    pub fn memory_optimized() -> Self {
        Self {
            connection_pool_size: 8,
            batch_size: 15,
            max_preference_history: 500,
            enable_collaborative_filtering: false,
            cache_ttl_seconds: 1800, // 30 minutes for less memory churn
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.learning_rate <= 0.0 || self.learning_rate > 1.0 {
            return Err(ArbitrageError::validation_error(
                "learning_rate must be between 0 and 1",
            ));
        }
        if self.preference_decay_rate <= 0.0 || self.preference_decay_rate > 1.0 {
            return Err(ArbitrageError::validation_error(
                "preference_decay_rate must be between 0 and 1",
            ));
        }
        if self.batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "batch_size must be greater than 0",
            ));
        }
        if self.connection_pool_size == 0 {
            return Err(ArbitrageError::validation_error(
                "connection_pool_size must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Ranking algorithms for opportunity personalization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum RankingAlgorithm {
    ContentBased,           // Based on opportunity features
    CollaborativeFiltering, // Based on similar users
    #[default]
    Hybrid,   // Combination of both
    MachineLearning,        // ML-based ranking
    Simple,                 // Basic scoring
}

/// User preference vector for personalization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferenceVector {
    pub user_id: String,
    pub exchange_preferences: HashMap<String, f32>, // Exchange preference scores
    pub asset_preferences: HashMap<String, f32>,    // Asset preference scores
    pub risk_tolerance: f32,                        // 0.0 (conservative) to 1.0 (aggressive)
    pub profit_threshold: f32,                      // Minimum profit percentage
    pub time_horizon: TimeHorizon,                  // Preferred opportunity duration
    pub interaction_count: u32,                     // Number of interactions
    pub last_updated: u64,                          // Timestamp of last update
    pub confidence_score: f32,                      // Confidence in preferences (0.0-1.0)
    pub feature_weights: HashMap<String, f32>,      // Feature importance weights
}

impl Default for UserPreferenceVector {
    fn default() -> Self {
        Self {
            user_id: "".to_string(),
            exchange_preferences: HashMap::new(),
            asset_preferences: HashMap::new(),
            risk_tolerance: 0.5,
            profit_threshold: 0.01, // 1% minimum profit
            time_horizon: TimeHorizon::Medium,
            interaction_count: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
            confidence_score: 0.0,
            feature_weights: HashMap::new(),
        }
    }
}

/// Time horizon preferences
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TimeHorizon {
    Short, // < 1 hour
    #[default]
    Medium, // 1-24 hours
    Long,  // > 24 hours
    Any,   // No preference
}

/// User interaction with opportunities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInteraction {
    pub user_id: String,
    pub opportunity_id: String,
    pub interaction_type: InteractionType,
    pub timestamp: u64,
    pub opportunity_features: HashMap<String, f32>,
    pub outcome: Option<InteractionOutcome>,
    pub feedback_score: Option<f32>, // User-provided feedback (0.0-1.0)
}

/// Types of user interactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionType {
    View,
    Click,
    Execute,
    Dismiss,
    Favorite,
    Share,
}

/// Outcome of user interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionOutcome {
    Positive, // User benefited from the opportunity
    Negative, // User lost money or was dissatisfied
    Neutral,  // No clear outcome
}

/// Ranked opportunity with personalization score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedOpportunity {
    pub opportunity: ArbitrageOpportunity,
    pub personalization_score: f32,
    pub ranking_factors: HashMap<String, f32>,
    pub confidence_score: f32,
    pub explanation: String,
    pub predicted_user_satisfaction: f32,
}

/// Personalization analytics and metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalizationMetrics {
    pub total_users: u64,
    pub active_users_24h: u64,
    pub total_interactions: u64,
    pub avg_personalization_score: f32,
    pub avg_confidence_score: f32,
    pub ranking_accuracy: f32,
    pub cache_hit_rate_percent: f32,
    pub learning_convergence_rate: f32,
    pub last_updated: u64,
}

impl Default for PersonalizationMetrics {
    fn default() -> Self {
        Self {
            total_users: 0,
            active_users_24h: 0,
            total_interactions: 0,
            avg_personalization_score: 0.0,
            avg_confidence_score: 0.0,
            ranking_accuracy: 0.0,
            cache_hit_rate_percent: 0.0,
            learning_convergence_rate: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Personalization Engine for user preference learning and opportunity ranking
pub struct PersonalizationEngine {
    config: PersonalizationEngineConfig,
    logger: crate::utils::logger::Logger,
    cache: Option<KvStore>,
    #[allow(dead_code)] // TODO: Will be used for feature extraction in future implementation
    feature_extractors: HashMap<String, Box<dyn FeatureExtractor + Send + Sync>>,
    user_preferences: Arc<std::sync::Mutex<HashMap<String, UserPreferenceVector>>>,
    interaction_history: Arc<std::sync::Mutex<Vec<UserInteraction>>>,
    metrics: Arc<std::sync::Mutex<PersonalizationMetrics>>,
}

/// Trait for extracting features from opportunities
pub trait FeatureExtractor {
    fn extract_features(&self, opportunity: &ArbitrageOpportunity) -> HashMap<String, f64>;
    fn get_feature_names(&self) -> Vec<String>;
}

/// Basic feature extractor implementation
pub struct BasicFeatureExtractor;

impl FeatureExtractor for BasicFeatureExtractor {
    fn extract_features(&self, opportunity: &ArbitrageOpportunity) -> HashMap<String, f64> {
        let mut features = HashMap::new();

        // Rate difference (main feature)
        features.insert("rate_difference".to_string(), opportunity.rate_difference);

        // Net rate difference if available
        if let Some(net_diff) = opportunity.net_rate_difference {
            features.insert("net_rate_difference".to_string(), net_diff);
        }

        // Potential profit if available
        if let Some(profit) = opportunity.potential_profit_value {
            features.insert("potential_profit_value".to_string(), profit);
        }

        // Exchange features
        features.insert(format!("exchange_{}", opportunity.long_exchange), 1.0);
        features.insert(format!("exchange_{}", opportunity.short_exchange), 1.0);

        // Pair feature (use pair field directly)
        features.insert(format!("pair_{}", opportunity.pair), 1.0);

        // Time-based features
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let age_minutes = (now - opportunity.timestamp) / (1000 * 60);
        features.insert("age_minutes".to_string(), age_minutes as f64);

        features
    }

    fn get_feature_names(&self) -> Vec<String> {
        vec![
            "rate_difference".to_string(),
            "net_rate_difference".to_string(),
            "potential_profit_value".to_string(),
            "exchange_long_exchange".to_string(),
            "exchange_short_exchange".to_string(),
            "pair_pair".to_string(),
            "age_minutes".to_string(),
        ]
    }
}

impl PersonalizationEngine {
    /// Create new PersonalizationEngine instance
    pub fn new(config: PersonalizationEngineConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        let mut feature_extractors: HashMap<String, Box<dyn FeatureExtractor + Send + Sync>> =
            HashMap::new();
        feature_extractors.insert("basic".to_string(), Box::new(BasicFeatureExtractor));

        let engine = Self {
            config,
            logger,
            cache: None,
            feature_extractors,
            user_preferences: Arc::new(std::sync::Mutex::new(HashMap::new())),
            interaction_history: Arc::new(std::sync::Mutex::new(Vec::new())),
            metrics: Arc::new(std::sync::Mutex::new(PersonalizationMetrics::default())),
        };

        engine.logger.info(&format!(
            "PersonalizationEngine initialized: personalization_enabled={}, learning_enabled={}, ranking_algorithm={:?}",
            engine.config.enable_personalization, engine.config.enable_learning, engine.config.ranking_algorithm
        ));

        Ok(engine)
    }

    /// Set cache store for caching operations
    pub fn with_cache(mut self, cache: KvStore) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Rank opportunities for a specific user
    pub async fn rank_opportunities_for_user(
        &self,
        user_id: &str,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<RankedOpportunity>> {
        if !self.config.enable_ranking {
            // Return opportunities with default ranking
            return Ok(opportunities
                .into_iter()
                .map(|opp| RankedOpportunity {
                    opportunity: opp,
                    personalization_score: 0.5,
                    ranking_factors: HashMap::new(),
                    confidence_score: 0.5,
                    explanation: "Ranking disabled".to_string(),
                    predicted_user_satisfaction: 0.5,
                })
                .collect());
        }

        // Get user preferences
        let user_preferences = self.get_user_preferences(user_id).await?;

        // Rank opportunities based on algorithm
        let ranked_opportunities = match self.config.ranking_algorithm {
            RankingAlgorithm::ContentBased => {
                self.content_based_ranking(&user_preferences, opportunities)
                    .await?
            }
            RankingAlgorithm::CollaborativeFiltering => {
                self.collaborative_filtering_ranking(user_id, opportunities)
                    .await?
            }
            RankingAlgorithm::Hybrid => {
                self.hybrid_ranking(&user_preferences, user_id, opportunities)
                    .await?
            }
            RankingAlgorithm::MachineLearning => {
                self.ml_based_ranking(&user_preferences, opportunities)
                    .await?
            }
            RankingAlgorithm::Simple => {
                self.simple_ranking(&user_preferences, opportunities)
                    .await?
            }
        };

        // Update metrics
        self.update_ranking_metrics(ranked_opportunities.len())
            .await;

        Ok(ranked_opportunities)
    }

    /// Content-based ranking using user preferences
    async fn content_based_ranking(
        &self,
        user_preferences: &UserPreferenceVector,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<RankedOpportunity>> {
        let mut ranked_opportunities = Vec::new();

        for opportunity in opportunities {
            let features = self.extract_opportunity_features(&opportunity);
            let score = self.calculate_content_based_score(user_preferences, &features);

            let mut ranking_factors = HashMap::new();
            ranking_factors.insert("content_similarity".to_string(), score);
            ranking_factors.insert(
                "user_confidence".to_string(),
                user_preferences.confidence_score,
            );

            ranked_opportunities.push(RankedOpportunity {
                opportunity,
                personalization_score: score,
                ranking_factors,
                confidence_score: user_preferences.confidence_score,
                explanation: "Content-based ranking using user preferences".to_string(),
                predicted_user_satisfaction: score * user_preferences.confidence_score,
            });
        }

        // Sort by personalization score (highest first)
        ranked_opportunities.sort_by(|a, b| {
            b.personalization_score
                .partial_cmp(&a.personalization_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(ranked_opportunities)
    }

    /// Collaborative filtering ranking (simplified implementation)
    async fn collaborative_filtering_ranking(
        &self,
        _user_id: &str,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<RankedOpportunity>> {
        // For now, fall back to content-based ranking
        // In a full implementation, this would find similar users and use their preferences
        let user_preferences = self.get_user_preferences(_user_id).await?;
        self.content_based_ranking(&user_preferences, opportunities)
            .await
    }

    /// Hybrid ranking combining multiple approaches
    async fn hybrid_ranking(
        &self,
        _user_preferences: &UserPreferenceVector,
        _user_id: &str,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<RankedOpportunity>> {
        let mut ranked_opportunities = Vec::new();

        for opportunity in opportunities {
            let features = self.extract_opportunity_features(&opportunity);

            // Content-based score (weight: 0.7)
            let content_score = self.calculate_content_based_score(_user_preferences, &features);

            // Simple collaborative score (weight: 0.2)
            let collaborative_score = self
                .calculate_simple_collaborative_score(&opportunity)
                .await;

            // Popularity score (weight: 0.1)
            let popularity_score = self.calculate_popularity_score(&opportunity).await;

            // Combine scores
            let final_score =
                content_score * 0.7 + collaborative_score * 0.2 + popularity_score * 0.1;

            let mut ranking_factors = HashMap::new();
            ranking_factors.insert("content_score".to_string(), content_score);
            ranking_factors.insert("collaborative_score".to_string(), collaborative_score);
            ranking_factors.insert("popularity_score".to_string(), popularity_score);
            ranking_factors.insert("final_score".to_string(), final_score);

            ranked_opportunities.push(RankedOpportunity {
                opportunity,
                personalization_score: final_score,
                ranking_factors,
                confidence_score: _user_preferences.confidence_score,
                explanation:
                    "Hybrid ranking combining content, collaborative, and popularity signals"
                        .to_string(),
                predicted_user_satisfaction: final_score * _user_preferences.confidence_score,
            });
        }

        // Sort by personalization score (highest first)
        ranked_opportunities.sort_by(|a, b| {
            b.personalization_score
                .partial_cmp(&a.personalization_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(ranked_opportunities)
    }

    /// Machine learning-based ranking (simplified implementation)
    async fn ml_based_ranking(
        &self,
        _user_preferences: &UserPreferenceVector,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<RankedOpportunity>> {
        // For now, use an enhanced content-based approach
        // In a full implementation, this would use trained ML models
        self.content_based_ranking(_user_preferences, opportunities)
            .await
    }

    /// Simple ranking based on basic criteria
    async fn simple_ranking(
        &self,
        _user_preferences: &UserPreferenceVector,
        opportunities: Vec<ArbitrageOpportunity>,
    ) -> ArbitrageResult<Vec<RankedOpportunity>> {
        let mut ranked_opportunities = Vec::new();

        for opportunity in opportunities {
            // Simple scoring based on profit and risk
            let profit_score = (opportunity.rate_difference / 10.0).min(1.0); // Normalize to 0-1
            let risk_score = 1.0 - 0.5; // Default risk score since field doesn't exist
            let simple_score = (profit_score + risk_score) / 2.0;

            let mut ranking_factors = HashMap::new();
            ranking_factors.insert("profit_score".to_string(), profit_score);
            ranking_factors.insert("risk_score".to_string(), risk_score);

            ranked_opportunities.push(RankedOpportunity {
                opportunity,
                personalization_score: simple_score as f32,
                ranking_factors: ranking_factors
                    .into_iter()
                    .map(|(k, v)| (k, v as f32))
                    .collect(),
                confidence_score: 0.5,
                explanation: "Simple ranking based on profit and risk".to_string(),
                predicted_user_satisfaction: simple_score as f32,
            });
        }

        // Sort by personalization score (highest first)
        ranked_opportunities.sort_by(|a, b| {
            b.personalization_score
                .partial_cmp(&a.personalization_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(ranked_opportunities)
    }

    /// Calculate content-based score using user preferences
    fn calculate_content_based_score(
        &self,
        user_preferences: &UserPreferenceVector,
        features: &HashMap<String, f64>,
    ) -> f32 {
        let mut score = 0.0;
        let mut weight_sum = 0.0;

        // Exchange preference
        for (exchange, preference) in &user_preferences.exchange_preferences {
            if features.contains_key(&format!("exchange_{}", exchange)) {
                score += preference * 0.3;
                weight_sum += 0.3;
                break;
            }
        }

        // Asset preference
        for (asset, preference) in &user_preferences.asset_preferences {
            if features.contains_key(&format!("asset_{}", asset)) {
                score += preference * 0.2;
                weight_sum += 0.2;
                break;
            }
        }

        // Profit alignment
        if let Some(profit_pct) = features.get("rate_difference") {
            if *profit_pct >= user_preferences.profit_threshold as f64 {
                let profit_score = (*profit_pct / 10.0).min(1.0); // Normalize
                score += (profit_score * 0.3) as f32;
                weight_sum += 0.3;
            }
        }

        // Risk alignment
        if let Some(risk_score) = features.get("risk_score") {
            let risk_alignment = 1.0 - (user_preferences.risk_tolerance as f64 - risk_score).abs();
            score += (risk_alignment * 0.2) as f32;
            weight_sum += 0.2;
        }

        // Normalize score
        if weight_sum > 0.0 {
            score / weight_sum
        } else {
            0.5 // Default score
        }
    }

    /// Calculate simple collaborative score
    async fn calculate_simple_collaborative_score(
        &self,
        _opportunity: &ArbitrageOpportunity,
    ) -> f32 {
        // Simplified implementation - in practice, this would analyze similar users
        0.5
    }

    /// Calculate popularity score
    async fn calculate_popularity_score(&self, _opportunity: &ArbitrageOpportunity) -> f32 {
        // Simplified implementation - in practice, this would track opportunity popularity
        0.5
    }

    /// Extract features from an arbitrage opportunity
    fn extract_opportunity_features(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> HashMap<String, f64> {
        let mut features = HashMap::new();

        // Rate difference (main feature)
        features.insert("rate_difference".to_string(), opportunity.rate_difference);

        // Net rate difference if available
        if let Some(net_diff) = opportunity.net_rate_difference {
            features.insert("net_rate_difference".to_string(), net_diff);
        }

        // Potential profit if available
        if let Some(profit) = opportunity.potential_profit_value {
            features.insert("potential_profit_value".to_string(), profit);
        }

        // Exchange features
        features.insert(format!("exchange_{}", opportunity.long_exchange), 1.0);
        features.insert(format!("exchange_{}", opportunity.short_exchange), 1.0);

        // Pair feature (use pair field directly)
        features.insert(format!("pair_{}", opportunity.pair), 1.0);

        // Time-based features
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let age_minutes = (now - opportunity.timestamp) / (1000 * 60);
        features.insert("age_minutes".to_string(), age_minutes as f64);

        features
    }

    /// Record user interaction for learning
    pub async fn record_interaction(&self, interaction: UserInteraction) -> ArbitrageResult<()> {
        if !self.config.enable_learning {
            return Ok(());
        }

        // Store interaction
        if let Ok(mut history) = self.interaction_history.lock() {
            history.push(interaction.clone());

            // Limit history size
            if history.len() > self.config.max_preference_history as usize {
                history.remove(0);
            }
        }

        // Update user preferences based on interaction
        self.update_user_preferences_from_interaction(&interaction)
            .await?;

        // Update metrics
        self.update_interaction_metrics().await;

        Ok(())
    }

    /// Update user preferences based on interaction
    async fn update_user_preferences_from_interaction(
        &self,
        interaction: &UserInteraction,
    ) -> ArbitrageResult<()> {
        let mut user_preferences = self.get_user_preferences(&interaction.user_id).await?;

        // Update interaction count
        user_preferences.interaction_count += 1;

        // Calculate learning weight based on interaction type and outcome
        let learning_weight = self.calculate_learning_weight(interaction);

        // Update preferences based on opportunity features
        for feature in interaction.opportunity_features.keys() {
            if feature.starts_with("exchange_") {
                let exchange = feature.strip_prefix("exchange_").unwrap_or("");
                let current_pref = user_preferences
                    .exchange_preferences
                    .get(exchange)
                    .unwrap_or(&0.5);
                let new_pref =
                    current_pref + (learning_weight - current_pref) * self.config.learning_rate;
                user_preferences
                    .exchange_preferences
                    .insert(exchange.to_string(), new_pref.clamp(0.0, 1.0));
            } else if feature.starts_with("asset_") {
                let asset = feature.strip_prefix("asset_").unwrap_or("");
                let current_pref = user_preferences
                    .asset_preferences
                    .get(asset)
                    .unwrap_or(&0.5);
                let new_pref =
                    current_pref + (learning_weight - current_pref) * self.config.learning_rate;
                user_preferences
                    .asset_preferences
                    .insert(asset.to_string(), new_pref.clamp(0.0, 1.0));
            }
        }

        // Update confidence score based on interaction count
        user_preferences.confidence_score =
            (user_preferences.interaction_count as f32 / 100.0).min(1.0);
        user_preferences.last_updated = chrono::Utc::now().timestamp_millis() as u64;

        // Store updated preferences
        self.store_user_preferences(&user_preferences).await?;

        Ok(())
    }

    /// Calculate learning weight from interaction
    fn calculate_learning_weight(&self, interaction: &UserInteraction) -> f32 {
        let base_weight = match interaction.interaction_type {
            InteractionType::Execute => 1.0,
            InteractionType::Favorite => 0.8,
            InteractionType::Click => 0.6,
            InteractionType::View => 0.3,
            InteractionType::Share => 0.7,
            InteractionType::Dismiss => 0.1,
        };

        let outcome_modifier = match interaction.outcome {
            Some(InteractionOutcome::Positive) => 1.2,
            Some(InteractionOutcome::Negative) => 0.3,
            Some(InteractionOutcome::Neutral) => 1.0,
            None => 1.0,
        };

        let feedback_modifier = interaction.feedback_score.unwrap_or(1.0);

        (base_weight * outcome_modifier * feedback_modifier).clamp(0.0, 1.0)
    }

    /// Get user preferences (from cache or create new)
    pub async fn get_user_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserPreferenceVector> {
        let prefs_to_cache = {
            if let Ok(preferences) = self.user_preferences.lock() {
                preferences.get(user_id).cloned()
            } else {
                None
            }
        };

        if let Some(prefs) = prefs_to_cache {
            let _ = self.cache_user_preferences(&prefs).await;
            return Ok(prefs);
        }

        // Create new preferences
        let new_prefs = UserPreferenceVector {
            user_id: user_id.to_string(),
            ..Default::default()
        };

        // Store new preferences
        self.store_user_preferences(&new_prefs).await?;

        Ok(new_prefs)
    }

    /// Store user preferences
    async fn store_user_preferences(
        &self,
        preferences: &UserPreferenceVector,
    ) -> ArbitrageResult<()> {
        // Store in memory
        if let Ok(mut prefs) = self.user_preferences.lock() {
            prefs.insert(preferences.user_id.clone(), preferences.clone());
        }

        // Cache preferences
        self.cache_user_preferences(preferences).await?;

        Ok(())
    }

    /// Cache user preferences
    async fn cache_user_preferences(
        &self,
        preferences: &UserPreferenceVector,
    ) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("user_prefs:{}", preferences.user_id);
            let prefs_json = serde_json::to_string(preferences)?;

            cache
                .put(&cache_key, &prefs_json)?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await?;
        }
        Ok(())
    }

    /// Get cached user preferences
    // TODO: Will be used for caching user preferences in future implementation
    #[allow(dead_code)]
    async fn get_cached_user_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<UserPreferenceVector>> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("user_prefs:{}", user_id);

            match cache.get(&cache_key).text().await {
                Ok(Some(prefs_json)) => {
                    match serde_json::from_str::<UserPreferenceVector>(&prefs_json) {
                        Ok(preferences) => return Ok(Some(preferences)),
                        Err(e) => {
                            self.logger.warn(&format!(
                                "Failed to deserialize cached user preferences: {}",
                                e
                            ));
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.logger
                        .warn(&format!("Failed to get cached user preferences: {}", e));
                }
            }
        }
        Ok(None)
    }

    /// Update ranking metrics
    async fn update_ranking_metrics(&self, _opportunities_ranked: usize) {
        if let Ok(mut metrics) = self.metrics.lock() {
            // Update basic metrics (simplified)
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Update interaction metrics
    async fn update_interaction_metrics(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_interactions += 1;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Get personalization metrics
    pub async fn get_metrics(&self) -> PersonalizationMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Check if personalization is enabled and functioning
        Ok(self.config.enable_personalization)
    }

    // TODO: Will be used for analyzing user preferences in future implementation
    #[allow(dead_code)]
    async fn analyze_user_preferences(
        &self,
        user_id: &str,
        opportunities: &[ArbitrageOpportunity],
    ) -> ArbitrageResult<UserPreferences> {
        let _user_prefs = self.get_user_preferences(user_id).await?;

        // Analyze user preferences based on historical interactions
        let preferences = UserPreferences {
            user_id: user_id.to_string(),
            notification_settings: NotificationSettings::default(),
            display_settings: DisplaySettings::default(),
            alert_settings: AlertSettings::default(),
            command_aliases: std::collections::HashMap::new(),
            dashboard_layout: DashboardLayout::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        // Note: In a full implementation, we would analyze opportunities to refine preferences
        // For now, we return default preferences
        let _ = opportunities; // Suppress unused warning

        Ok(preferences)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_personalization_engine_config_default() {
        let config = PersonalizationEngineConfig::default();
        assert!(config.enable_personalization);
        assert!(config.enable_learning);
        assert!(config.enable_ranking);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_personalization_engine_config_high_concurrency() {
        let config = PersonalizationEngineConfig::high_concurrency();
        assert_eq!(config.connection_pool_size, 20);
        assert_eq!(config.batch_size, 50);
        assert!(config.enable_real_time_updates);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_user_preference_vector_default() {
        let prefs = UserPreferenceVector::default();
        assert_eq!(prefs.risk_tolerance, 0.5);
        assert_eq!(prefs.profit_threshold, 0.01);
        assert_eq!(prefs.interaction_count, 0);
        assert_eq!(prefs.confidence_score, 0.0);
    }

    #[test]
    fn test_user_interaction_creation() {
        let interaction = UserInteraction {
            user_id: "test_user".to_string(),
            opportunity_id: "test_opp".to_string(),
            interaction_type: InteractionType::Execute,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            opportunity_features: HashMap::new(),
            outcome: Some(InteractionOutcome::Positive),
            feedback_score: Some(0.9),
        };

        assert_eq!(interaction.user_id, "test_user");
        assert_eq!(interaction.feedback_score, Some(0.9));
    }

    #[test]
    fn test_basic_feature_extractor() {
        let extractor = BasicFeatureExtractor;
        let feature_names = extractor.get_feature_names();

        assert!(feature_names.contains(&"rate_difference".to_string()));
        assert!(feature_names.contains(&"net_rate_difference".to_string()));
        assert!(feature_names.contains(&"potential_profit_value".to_string()));
    }
}
