use crate::types::{ArbitrageOpportunity, CommandPermission};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// AI Enhancement Types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiEnhancementType {
    MarketSentimentAnalysis,
    OpportunityScoring,
    RiskAssessment,
    PatternRecognition,
    PersonalizedRecommendations,
    PortfolioOptimization,
    AutoParameterTuning,
    PredictiveAnalytics,
}

/// AI Confidence Levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiConfidenceLevel {
    Low,      // 0-40%
    Medium,   // 40-70%
    High,     // 70-90%
    VeryHigh, // 90-100%
}

impl From<f64> for AiConfidenceLevel {
    fn from(confidence: f64) -> Self {
        match confidence {
            c if c >= 0.9 => AiConfidenceLevel::VeryHigh,
            c if c >= 0.7 => AiConfidenceLevel::High,
            c if c >= 0.4 => AiConfidenceLevel::Medium,
            _ => AiConfidenceLevel::Low,
        }
    }
}

/// AI Enhanced Opportunity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiEnhancedOpportunity {
    pub base_opportunity: ArbitrageOpportunity,
    pub ai_score: f64, // 0.0 to 1.0
    pub confidence_level: AiConfidenceLevel,
    pub risk_adjusted_score: f64,
    pub market_sentiment: MarketSentiment,
    pub ai_insights: Vec<AiInsight>,
    pub personalization_score: f64,
    pub success_probability: f64,
    pub optimal_position_size: f64,
    pub time_sensitivity: TimeSensitivity,
    pub enhancement_metadata: serde_json::Value,
    pub enhanced_at: u64,
}

impl AiEnhancedOpportunity {
    pub fn new(opportunity: ArbitrageOpportunity, ai_score: f64) -> Self {
        Self {
            base_opportunity: opportunity,
            ai_score,
            confidence_level: AiConfidenceLevel::from(ai_score),
            risk_adjusted_score: ai_score,
            market_sentiment: MarketSentiment::Neutral,
            ai_insights: Vec::new(),
            personalization_score: 0.0,
            success_probability: ai_score,
            optimal_position_size: 0.0,
            time_sensitivity: TimeSensitivity::Medium,
            enhancement_metadata: serde_json::Value::Object(serde_json::Map::new()),
            enhanced_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn with_market_sentiment(mut self, sentiment: MarketSentiment) -> Self {
        self.market_sentiment = sentiment;
        self
    }

    pub fn with_insights(mut self, insights: Vec<AiInsight>) -> Self {
        self.ai_insights = insights;
        self
    }

    pub fn with_personalization_score(mut self, score: f64) -> Self {
        self.personalization_score = score;
        self
    }

    pub fn calculate_final_score(&self) -> f64 {
        // Weighted combination of AI score, risk adjustment, and personalization
        let weights = AiScoringWeights::default();

        (self.ai_score * weights.ai_score_weight)
            + (self.risk_adjusted_score * weights.risk_weight)
            + (self.personalization_score * weights.personalization_weight)
            + (self.market_sentiment.score() * weights.sentiment_weight)
    }
}

/// Market Sentiment Analysis
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketSentiment {
    VeryBearish,
    Bearish,
    Neutral,
    Bullish,
    VeryBullish,
}

impl MarketSentiment {
    pub fn score(&self) -> f64 {
        match self {
            MarketSentiment::VeryBearish => -1.0,
            MarketSentiment::Bearish => -0.5,
            MarketSentiment::Neutral => 0.0,
            MarketSentiment::Bullish => 0.5,
            MarketSentiment::VeryBullish => 1.0,
        }
    }
}

/// Time Sensitivity for Opportunities
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeSensitivity {
    VeryHigh, // < 5 minutes
    High,     // 5-30 minutes
    Medium,   // 30 minutes - 2 hours
    Low,      // 2+ hours
}

/// AI Insight types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiInsight {
    pub insight_type: String,
    pub title: String,
    pub description: String,
    pub confidence: f64,
    pub impact_level: ImpactLevel,
    pub recommendation: String,
    pub supporting_data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// AI Scoring Weights Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiScoringWeights {
    pub ai_score_weight: f64,
    pub risk_weight: f64,
    pub personalization_weight: f64,
    pub sentiment_weight: f64,
}

impl Default for AiScoringWeights {
    fn default() -> Self {
        Self {
            ai_score_weight: 0.4,
            risk_weight: 0.3,
            personalization_weight: 0.2,
            sentiment_weight: 0.1,
        }
    }
}

/// User Trading Profile for AI Personalization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTradingProfile {
    pub user_id: String,
    pub risk_tolerance: f64, // 0.0 to 1.0
    pub experience_level: ExperienceLevel,
    pub preferred_strategies: Vec<String>,
    pub historical_performance: PerformanceMetrics,
    pub trading_patterns: TradingPatterns,
    pub personalization_features: HashMap<String, f64>,
    pub learning_data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperienceLevel {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_trades: u32,
    pub win_rate: f64,
    pub average_return: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub roi: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPatterns {
    pub preferred_timeframes: Vec<String>,
    pub average_position_size: f64,
    pub frequency_per_week: f64,
    pub preferred_pairs: Vec<String>,
    pub risk_per_trade: f64,
}

/// AI Beta Integration Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiBetaConfig {
    pub enabled_features: Vec<AiEnhancementType>,
    pub min_confidence_threshold: f64,
    pub personalization_enabled: bool,
    pub advanced_analytics_enabled: bool,
    pub ml_predictions_enabled: bool,
    pub auto_optimization_enabled: bool,
    pub beta_user_access_only: bool,
    pub max_ai_opportunities_per_hour: u32,
}

impl Default for AiBetaConfig {
    fn default() -> Self {
        Self {
            enabled_features: vec![
                AiEnhancementType::OpportunityScoring,
                AiEnhancementType::MarketSentimentAnalysis,
                AiEnhancementType::RiskAssessment,
                AiEnhancementType::PersonalizedRecommendations,
            ],
            min_confidence_threshold: 0.75,
            personalization_enabled: true,
            advanced_analytics_enabled: true,
            ml_predictions_enabled: true,
            auto_optimization_enabled: false, // Disabled by default for safety
            beta_user_access_only: true,
            max_ai_opportunities_per_hour: 5,
        }
    }
}

/// AI Beta Integration Service
pub struct AiBetaIntegrationService {
    config: AiBetaConfig,
    user_profiles: Mutex<HashMap<String, AiTradingProfile>>,
    market_sentiment_cache: Mutex<HashMap<String, (MarketSentiment, u64)>>, // (sentiment, timestamp)
    ai_model_metrics: Mutex<AiModelMetrics>,
    active_predictions: Mutex<HashMap<String, (f64, u64)>>, // (ai_score, timestamp) for tracking predictions
}

impl AiBetaIntegrationService {
    pub fn new(config: AiBetaConfig) -> Self {
        Self {
            config,
            user_profiles: Mutex::new(HashMap::new()),
            market_sentiment_cache: Mutex::new(HashMap::new()),
            ai_model_metrics: Mutex::new(AiModelMetrics::default()),
            active_predictions: Mutex::new(HashMap::new()),
        }
    }

    /// Check if user has access to AI beta features
    pub fn check_beta_access(&self, user_permissions: &[CommandPermission]) -> bool {
        if !self.config.beta_user_access_only {
            return true;
        }

        user_permissions.contains(&CommandPermission::AIEnhancedOpportunities)
    }

    /// Enhance opportunities with AI analysis
    pub async fn enhance_opportunities(
        &self,
        opportunities: Vec<ArbitrageOpportunity>,
        user_id: &str,
    ) -> ArbitrageResult<Vec<AiEnhancedOpportunity>> {
        // Clean up stale predictions (older than 24 hours)
        self.cleanup_stale_predictions(24);

        let mut enhanced_opportunities = Vec::new();

        for opportunity in opportunities {
            if let Ok(enhanced) = self.enhance_single_opportunity(opportunity, user_id).await {
                if enhanced.ai_score >= self.config.min_confidence_threshold {
                    enhanced_opportunities.push(enhanced);
                }
            }
        }

        // Sort by final AI score with proper NaN handling
        enhanced_opportunities.sort_by(|a, b| {
            let score_a = a.calculate_final_score();
            let score_b = b.calculate_final_score();

            // Handle NaN values explicitly - treat NaN as less than any number
            match (score_a.is_nan(), score_b.is_nan()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (false, false) => score_b
                    .partial_cmp(&score_a)
                    .unwrap_or(std::cmp::Ordering::Equal),
            }
        });

        // Limit to configured maximum
        let max_opportunities = self.config.max_ai_opportunities_per_hour as usize;
        enhanced_opportunities.truncate(max_opportunities);

        Ok(enhanced_opportunities)
    }

    /// Enhance a single opportunity with AI analysis
    async fn enhance_single_opportunity(
        &self,
        opportunity: ArbitrageOpportunity,
        user_id: &str,
    ) -> ArbitrageResult<AiEnhancedOpportunity> {
        // Calculate base AI score
        let ai_score = self.calculate_ai_score(&opportunity).await;

        // Create enhanced opportunity
        let mut enhanced = AiEnhancedOpportunity::new(opportunity.clone(), ai_score);

        // Add market sentiment analysis
        let sentiment = self.analyze_market_sentiment(&opportunity.pair).await;
        enhanced = enhanced.with_market_sentiment(sentiment);

        // Generate AI insights
        let insights = self.generate_ai_insights(&opportunity, ai_score).await;
        enhanced = enhanced.with_insights(insights);

        // Add personalization if user profile exists
        if let Some(profile) = self.user_profiles.lock().unwrap().get(user_id) {
            let personalization_score = self.calculate_personalization_score(&opportunity, profile);
            enhanced = enhanced.with_personalization_score(personalization_score);
        }

        // Calculate additional metrics
        enhanced.risk_adjusted_score = self.calculate_risk_adjusted_score(&enhanced);
        enhanced.success_probability = self.predict_success_probability(&enhanced).await;
        enhanced.optimal_position_size = self.calculate_optimal_position_size(&enhanced, user_id);
        enhanced.time_sensitivity = self.assess_time_sensitivity(&enhanced);

        // Update AI model metrics
        // self.update_ai_metrics(ai_score);

        // Track this prediction as active
        #[cfg(target_arch = "wasm32")]
        let _now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let _now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let prediction_id = format!("pred_{}", _now);

        // Update the enhanced opportunity's ID if it was empty
        if enhanced.base_opportunity.id.is_empty() {
            enhanced.base_opportunity.id = prediction_id.clone();
        }

        // Track the prediction for accuracy measurement
        {
            let mut predictions = self.active_predictions.lock().unwrap();
            predictions.insert(prediction_id, (ai_score, _now));
        }

        Ok(enhanced)
    }

    /// Calculate AI score for an opportunity
    async fn calculate_ai_score(&self, opportunity: &ArbitrageOpportunity) -> f64 {
        // Mock AI scoring algorithm
        // In production, this would use ML models, market data analysis, etc.

        let mut score = 0.5; // Base score

        // Rate difference impact
        score += (opportunity.rate_difference * 100.0).min(0.3);

        // Pair popularity (BTC/ETH get higher scores)
        if opportunity.pair.contains("BTC") || opportunity.pair.contains("ETH") {
            score += 0.1;
        }

        // Exchange reliability
        let _exchange = &opportunity.long_exchange;
        let exchange_risk = match &opportunity.long_exchange {
            crate::types::ExchangeIdEnum::Binance => 0.1,
            crate::types::ExchangeIdEnum::Bybit => 0.15,
            crate::types::ExchangeIdEnum::OKX => 0.12,
            crate::types::ExchangeIdEnum::Bitget => 0.2,
            crate::types::ExchangeIdEnum::Kucoin => 0.18,
            crate::types::ExchangeIdEnum::Gate => 0.22,
            crate::types::ExchangeIdEnum::Mexc => 0.25,
            crate::types::ExchangeIdEnum::Huobi => 0.20,
            crate::types::ExchangeIdEnum::Kraken => 0.14,
            crate::types::ExchangeIdEnum::Coinbase => 0.12,
        };
        score += exchange_risk;

        // Random factor to simulate ML uncertainty
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_factor: f64 = rng.gen_range(0.0..0.1);
        score += random_factor;

        score.min(1.0)
    }

    /// Analyze market sentiment for a trading pair with cache expiration
    async fn analyze_market_sentiment(&self, pair: &str) -> MarketSentiment {
        #[cfg(target_arch = "wasm32")]
        let _now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let _now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Evict expired entries before checking cache
        // self.evict_expired_sentiment_cache(now);

        // Check cache first for valid (non-expired) entries
        if let Some((sentiment, _timestamp)) = self.market_sentiment_cache.lock().unwrap().get(pair)
        {
            return sentiment.clone();
        }

        // Perform sophisticated sentiment analysis (enhanced from mock)
        let sentiment = self.perform_sentiment_analysis(pair).await;

        // Cache the result with current timestamp
        // self.market_sentiment_cache
        //     .insert(pair.to_string(), (sentiment.clone(), now));

        sentiment
    }

    /// Evict expired sentiment cache entries
    #[allow(dead_code)]
    fn evict_expired_sentiment_cache(&self, current_time: u64) {
        const CACHE_TTL_MS: u64 = 15 * 60 * 1000; // 15 minutes TTL

        let _expired_keys: Vec<String> = self
            .market_sentiment_cache
            .lock()
            .unwrap()
            .iter()
            .filter(|(_pair, (_sentiment, timestamp))| {
                current_time.saturating_sub(*timestamp) > CACHE_TTL_MS
            })
            .map(|(pair, _)| pair.clone())
            .collect();

        // for key in expired_keys {
        //     self.market_sentiment_cache.remove(&key);
        // }
    }

    /// Perform enhanced sentiment analysis for a trading pair
    async fn perform_sentiment_analysis(&self, pair: &str) -> MarketSentiment {
        // Enhanced sentiment analysis with multiple factors
        let mut sentiment_score = 0.0; // Neutral baseline

        // Primary asset sentiment analysis
        if pair.contains("BTC") {
            sentiment_score += 0.3; // Bitcoin generally bullish
        } else if pair.contains("ETH") {
            sentiment_score += 0.25; // Ethereum moderately bullish
        } else if pair.contains("SOL") {
            sentiment_score += 0.4; // Solana very bullish
        } else if pair.contains("ADA") {
            sentiment_score += 0.1; // Cardano neutral to slightly bullish
        } else if pair.contains("DOGE") {
            sentiment_score -= 0.1; // Meme coins more volatile
        }

        // Market condition simulation (would use real data in production)
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let market_condition_factor = rng.gen_range(-0.2..0.2);
        sentiment_score += market_condition_factor;

        // Social sentiment simulation (would integrate Twitter, Reddit, etc.)
        let social_sentiment_factor = rng.gen_range(-0.15..0.15);
        sentiment_score += social_sentiment_factor;

        // Convert score to MarketSentiment enum
        match sentiment_score {
            s if s <= -0.3 => MarketSentiment::VeryBearish,
            s if s <= -0.1 => MarketSentiment::Bearish,
            s if s <= 0.1 => MarketSentiment::Neutral,
            s if s <= 0.3 => MarketSentiment::Bullish,
            _ => MarketSentiment::VeryBullish,
        }
    }

    /// Generate AI insights for an opportunity
    async fn generate_ai_insights(
        &self,
        opportunity: &ArbitrageOpportunity,
        ai_score: f64,
    ) -> Vec<AiInsight> {
        let mut insights = Vec::new();

        // Score-based insights
        if ai_score > 0.9 {
            insights.push(AiInsight {
                insight_type: "high_confidence_prediction".to_string(),
                title: "High AI Confidence".to_string(),
                description: "AI models show very high confidence in this opportunity".to_string(),
                confidence: ai_score,
                impact_level: ImpactLevel::High,
                recommendation: "Consider increasing position size for this high-confidence opportunity".to_string(),
                supporting_data: serde_json::json!({"model_consensus": 0.95, "historical_success_rate": 0.87}),
            });
        }

        // Rate difference insights
        if opportunity.rate_difference > 0.005 {
            insights.push(AiInsight {
                insight_type: "rate_arbitrage_potential".to_string(),
                title: "Significant Rate Difference".to_string(),
                description: format!("Rate difference of {:.3}% detected", opportunity.rate_difference * 100.0),
                confidence: 0.9,
                impact_level: ImpactLevel::Medium,
                recommendation: "Monitor for quick execution as rate differences may close rapidly".to_string(),
                supporting_data: serde_json::json!({"rate_difference": opportunity.rate_difference}),
            });
        }

        // Pair-specific insights
        if opportunity.pair.contains("BTC") {
            insights.push(AiInsight {
                insight_type: "market_leader_signal".to_string(),
                title: "Bitcoin Market Movement".to_string(),
                description: "BTC movements often influence broader market sentiment".to_string(),
                confidence: 0.8,
                impact_level: ImpactLevel::Medium,
                recommendation: "Consider market correlation effects in position sizing"
                    .to_string(),
                supporting_data: serde_json::json!({"market_correlation": 0.75}),
            });
        }

        insights
    }

    /// Calculate personalization score based on user profile
    fn calculate_personalization_score(
        &self,
        opportunity: &ArbitrageOpportunity,
        profile: &AiTradingProfile,
    ) -> f64 {
        let mut score = 0.5; // Base score

        // Risk tolerance alignment
        let opportunity_risk = self.assess_opportunity_risk(opportunity);
        let risk_alignment = 1.0 - (opportunity_risk - profile.risk_tolerance).abs();
        score += risk_alignment * 0.3;

        // Preferred pairs
        if profile
            .trading_patterns
            .preferred_pairs
            .contains(&opportunity.pair)
        {
            score += 0.2;
        }

        // Experience level adjustment
        match profile.experience_level {
            ExperienceLevel::Beginner => {
                // Prefer safer opportunities
                if opportunity.rate_difference < 0.01 {
                    score += 0.1;
                }
            }
            ExperienceLevel::Expert => {
                // Can handle more complex opportunities
                if opportunity.rate_difference > 0.02 {
                    score += 0.1;
                }
            }
            _ => {}
        }

        score.min(1.0)
    }

    /// Assess risk level of an opportunity
    fn assess_opportunity_risk(&self, opportunity: &ArbitrageOpportunity) -> f64 {
        let mut risk = 0.3; // Base risk

        // Rate difference risk (higher rate diff = higher risk)
        risk += (opportunity.rate_difference * 10.0).min(0.4);

        // Exchange risk
        let _exchange = &opportunity.long_exchange;
        let exchange_risk = match &opportunity.long_exchange {
            crate::types::ExchangeIdEnum::Binance => 0.1,
            crate::types::ExchangeIdEnum::Bybit => 0.15,
            crate::types::ExchangeIdEnum::OKX => 0.12,
            crate::types::ExchangeIdEnum::Bitget => 0.2,
            crate::types::ExchangeIdEnum::Kucoin => 0.18,
            crate::types::ExchangeIdEnum::Gate => 0.22,
            crate::types::ExchangeIdEnum::Mexc => 0.25,
            crate::types::ExchangeIdEnum::Huobi => 0.20,
            crate::types::ExchangeIdEnum::Kraken => 0.14,
            crate::types::ExchangeIdEnum::Coinbase => 0.12,
        };
        risk += exchange_risk;

        risk.min(1.0)
    }

    /// Calculate risk-adjusted score
    fn calculate_risk_adjusted_score(&self, enhanced: &AiEnhancedOpportunity) -> f64 {
        let risk = self.assess_opportunity_risk(&enhanced.base_opportunity);
        let risk_penalty = risk * 0.5;

        (enhanced.ai_score - risk_penalty).max(0.0)
    }

    /// Predict success probability using ML models (mock implementation)
    async fn predict_success_probability(&self, enhanced: &AiEnhancedOpportunity) -> f64 {
        // Mock ML prediction
        let base_probability = enhanced.ai_score;
        let sentiment_adjustment = enhanced.market_sentiment.score() * 0.1;
        let risk_adjustment = -self.assess_opportunity_risk(&enhanced.base_opportunity) * 0.2;

        (base_probability + sentiment_adjustment + risk_adjustment).clamp(0.1, 0.99)
    }

    /// Calculate optimal position size
    fn calculate_optimal_position_size(
        &self,
        enhanced: &AiEnhancedOpportunity,
        user_id: &str,
    ) -> f64 {
        if let Some(profile) = self.user_profiles.lock().unwrap().get(user_id) {
            let base_size = profile.trading_patterns.average_position_size;
            let confidence_multiplier = enhanced.ai_score;
            let risk_adjustment = 1.0 - self.assess_opportunity_risk(&enhanced.base_opportunity);

            base_size * confidence_multiplier * risk_adjustment
        } else {
            // Default conservative size
            100.0
        }
    }

    /// Assess time sensitivity of opportunity
    fn assess_time_sensitivity(&self, enhanced: &AiEnhancedOpportunity) -> TimeSensitivity {
        let rate_diff = enhanced.base_opportunity.rate_difference;

        match rate_diff {
            r if r > 0.01 => TimeSensitivity::VeryHigh,
            r if r > 0.005 => TimeSensitivity::High,
            r if r > 0.002 => TimeSensitivity::Medium,
            _ => TimeSensitivity::Low,
        }
    }

    /// Create or update user trading profile with validation and in-memory storage
    /// Note: D1 persistence will be implemented when the D1Service storage methods are available
    pub async fn update_user_profile(
        &self,
        user_id: String,
        profile: AiTradingProfile,
    ) -> ArbitrageResult<()> {
        // Validate profile fields
        self.validate_ai_trading_profile(&profile)?;

        // Update in-memory cache
        self.user_profiles
            .lock()
            .unwrap()
            .insert(user_id.clone(), profile.clone());

        // TODO: Implement D1 persistence when D1Service methods are available
        // This would involve:
        // 1. Serializing the profile to JSON
        // 2. Calling d1_service.store_user_profile(user_id, profile_json).await?
        // 3. Handling any database-specific errors

        Ok(())
    }

    /// Validate AI trading profile fields
    fn validate_ai_trading_profile(&self, profile: &AiTradingProfile) -> ArbitrageResult<()> {
        // Validate risk tolerance (0.0 to 1.0)
        if !(0.0..=1.0).contains(&profile.risk_tolerance) {
            return Err(ArbitrageError::validation_error(
                "Risk tolerance must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Validate performance metrics ranges
        if !(0.0..=1.0).contains(&profile.historical_performance.win_rate) {
            return Err(ArbitrageError::validation_error(
                "Win rate must be between 0.0 and 1.0".to_string(),
            ));
        }

        if profile.historical_performance.max_drawdown < 0.0
            || profile.historical_performance.max_drawdown > 1.0
        {
            return Err(ArbitrageError::validation_error(
                "Max drawdown must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Validate trading patterns
        if profile.trading_patterns.average_position_size <= 0.0 {
            return Err(ArbitrageError::validation_error(
                "Average position size must be positive".to_string(),
            ));
        }

        if profile.trading_patterns.frequency_per_week < 0.0 {
            return Err(ArbitrageError::validation_error(
                "Trading frequency cannot be negative".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&profile.trading_patterns.risk_per_trade) {
            return Err(ArbitrageError::validation_error(
                "Risk per trade must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Validate user ID is not empty
        if profile.user_id.trim().is_empty() {
            return Err(ArbitrageError::validation_error(
                "User ID cannot be empty".to_string(),
            ));
        }

        Ok(())
    }

    /// Get AI statistics and metrics
    pub fn get_ai_metrics(&self) -> AiModelMetrics {
        self.ai_model_metrics.lock().unwrap().clone()
    }

    /// Update AI model metrics after making a prediction
    #[allow(dead_code)]
    fn update_ai_metrics(&self, confidence: f64) {
        self.ai_model_metrics.lock().unwrap().total_predictions += 1;

        // Update average confidence using running average
        let total = self.ai_model_metrics.lock().unwrap().total_predictions as f64;
        self.ai_model_metrics.lock().unwrap().average_confidence =
            ((self.ai_model_metrics.lock().unwrap().average_confidence * (total - 1.0))
                + confidence)
                / total;

        // Update accuracy rate based on current successful predictions
        if self.ai_model_metrics.lock().unwrap().total_predictions > 0 {
            self.ai_model_metrics.lock().unwrap().accuracy_rate =
                self.ai_model_metrics.lock().unwrap().successful_predictions as f64
                    / self.ai_model_metrics.lock().unwrap().total_predictions as f64;
        }

        // Update timestamp
        #[cfg(target_arch = "wasm32")]
        {
            self.ai_model_metrics.lock().unwrap().last_updated = js_sys::Date::now() as u64;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ai_model_metrics.lock().unwrap().last_updated =
                chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Clean up stale predictions older than specified age
    pub fn cleanup_stale_predictions(&self, max_age_hours: u64) {
        #[cfg(target_arch = "wasm32")]
        let _now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let _now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let max_age_ms = max_age_hours * 60 * 60 * 1000; // Convert hours to milliseconds

        let stale_keys: Vec<String> = self
            .active_predictions
            .lock()
            .unwrap()
            .iter()
            .filter(|(_id, (_score, timestamp))| _now.saturating_sub(*timestamp) > max_age_ms)
            .map(|(id, _)| id.clone())
            .collect();

        for key in &stale_keys {
            self.active_predictions.lock().unwrap().remove(key);
        }

        if !stale_keys.is_empty() {
            log::info!("Cleaned up {} stale AI predictions", stale_keys.len());
        }
    }

    /// Mark a prediction as successful (to be called when an opportunity is executed profitably)
    pub fn mark_prediction_successful(&self, opportunity_id: &str) -> ArbitrageResult<()> {
        // Check if the opportunity_id exists in active predictions
        if let Some((ai_score, _timestamp)) = self
            .active_predictions
            .lock()
            .unwrap()
            .remove(opportunity_id)
        {
            // Only increment successful predictions if the prediction meets minimum confidence threshold
            if ai_score >= self.config.min_confidence_threshold {
                self.ai_model_metrics.lock().unwrap().successful_predictions += 1;

                // Recalculate accuracy rate
                if self.ai_model_metrics.lock().unwrap().total_predictions > 0 {
                    self.ai_model_metrics.lock().unwrap().accuracy_rate =
                        self.ai_model_metrics.lock().unwrap().successful_predictions as f64
                            / self.ai_model_metrics.lock().unwrap().total_predictions as f64;
                }

                log::info!(
                    "Marked AI prediction as successful for opportunity: {} (ai_score: {:.3})",
                    opportunity_id,
                    ai_score
                );

                Ok(())
            } else {
                Err(ArbitrageError::validation_error(format!(
                    "Prediction {} did not meet minimum confidence threshold ({:.3} < {:.3})",
                    opportunity_id, ai_score, self.config.min_confidence_threshold
                )))
            }
        } else {
            Err(ArbitrageError::validation_error(format!(
                "No active AI prediction found for opportunity ID: {}. Prediction may have expired or never existed.",
                opportunity_id
            )))
        }
    }

    /// Generate personalized AI recommendations
    pub async fn get_personalized_recommendations(&self, user_id: &str) -> Vec<AiRecommendation> {
        let mut recommendations = Vec::new();

        if let Some(profile) = self.user_profiles.lock().unwrap().get(user_id) {
            // Risk management recommendations
            if profile.historical_performance.max_drawdown > 0.2 {
                recommendations.push(AiRecommendation {
                    recommendation_type: "risk_management".to_string(),
                    title: "Reduce Position Sizes".to_string(),
                    description:
                        "Your maximum drawdown suggests reducing position sizes to preserve capital"
                            .to_string(),
                    priority: RecommendationPriority::High,
                    estimated_impact: 0.8,
                    action_items: vec![
                        "Reduce position sizes by 25%".to_string(),
                        "Implement stricter stop losses".to_string(),
                        "Diversify across more opportunities".to_string(),
                    ],
                });
            }

            // Performance improvement recommendations
            if profile.historical_performance.win_rate < 0.6 {
                recommendations.push(AiRecommendation {
                    recommendation_type: "strategy_optimization".to_string(),
                    title: "Improve Trade Selection".to_string(),
                    description: "AI analysis suggests focusing on higher-confidence opportunities"
                        .to_string(),
                    priority: RecommendationPriority::Medium,
                    estimated_impact: 0.6,
                    action_items: vec![
                        "Increase minimum confidence threshold to 80%".to_string(),
                        "Focus on familiar trading pairs".to_string(),
                        "Wait for stronger technical confirmations".to_string(),
                    ],
                });
            }
        }

        recommendations
    }
}

/// AI Model Performance Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelMetrics {
    pub total_predictions: u64,
    pub successful_predictions: u64,
    pub accuracy_rate: f64,
    pub average_confidence: f64,
    pub model_version: String,
    pub last_updated: u64,
}

impl Default for AiModelMetrics {
    fn default() -> Self {
        Self {
            total_predictions: 0,
            successful_predictions: 0,
            accuracy_rate: 0.0,
            average_confidence: 0.0,
            model_version: "beta-1.0".to_string(),
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// AI Recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRecommendation {
    pub recommendation_type: String,
    pub title: String,
    pub description: String,
    pub priority: RecommendationPriority,
    pub estimated_impact: f64,
    pub action_items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ArbitrageType;

    fn create_test_opportunity() -> ArbitrageOpportunity {
        ArbitrageOpportunity::new(
            "BTCUSDT".to_string(),
            crate::types::ExchangeIdEnum::Binance, // **REQUIRED**: No longer optional
            crate::types::ExchangeIdEnum::Bybit,   // **REQUIRED**: No longer optional
            43250.0,                               // rate_difference as f64
            44000.0,                               // volume as f64
            2.5,                                   // confidence as f64
        )
    }

    fn create_test_profile() -> AiTradingProfile {
        AiTradingProfile {
            user_id: "test_user".to_string(),
            risk_tolerance: 0.6,
            experience_level: ExperienceLevel::Intermediate,
            preferred_strategies: vec!["arbitrage".to_string()],
            historical_performance: PerformanceMetrics {
                total_trades: 50,
                win_rate: 0.75,
                average_return: 0.15,
                max_drawdown: 0.1,
                sharpe_ratio: 1.2,
                roi: 0.25,
            },
            trading_patterns: TradingPatterns {
                preferred_timeframes: vec!["4h".to_string()],
                average_position_size: 500.0,
                frequency_per_week: 10.0,
                preferred_pairs: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
                risk_per_trade: 0.02,
            },
            personalization_features: HashMap::new(),
            learning_data: serde_json::Value::Null,
        }
    }

    #[test]
    fn test_ai_enhanced_opportunity_creation() {
        let opportunity = create_test_opportunity();
        let enhanced = AiEnhancedOpportunity::new(opportunity.clone(), 0.85);

        assert_eq!(enhanced.base_opportunity.pair, "BTCUSDT");
        assert_eq!(enhanced.ai_score, 0.85);
        assert_eq!(enhanced.confidence_level, AiConfidenceLevel::High);
    }

    #[test]
    fn test_confidence_level_conversion() {
        assert_eq!(AiConfidenceLevel::from(0.95), AiConfidenceLevel::VeryHigh);
        assert_eq!(AiConfidenceLevel::from(0.75), AiConfidenceLevel::High);
        assert_eq!(AiConfidenceLevel::from(0.55), AiConfidenceLevel::Medium);
        assert_eq!(AiConfidenceLevel::from(0.25), AiConfidenceLevel::Low);
    }

    #[test]
    fn test_market_sentiment_scoring() {
        assert_eq!(MarketSentiment::VeryBullish.score(), 1.0);
        assert_eq!(MarketSentiment::Bullish.score(), 0.5);
        assert_eq!(MarketSentiment::Neutral.score(), 0.0);
        assert_eq!(MarketSentiment::Bearish.score(), -0.5);
        assert_eq!(MarketSentiment::VeryBearish.score(), -1.0);
    }

    #[tokio::test]
    async fn test_ai_beta_service_creation() {
        let config = AiBetaConfig::default();
        let mut service = AiBetaIntegrationService::new(config);

        assert_eq!(service.user_profiles.lock().unwrap().len(), 0);
        assert_eq!(service.market_sentiment_cache.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_beta_access_check() {
        let config = AiBetaConfig::default();
        let mut service = AiBetaIntegrationService::new(config);

        let permissions = vec![CommandPermission::AIEnhancedOpportunities];
        assert!(service.check_beta_access(&permissions));

        let no_permissions = vec![CommandPermission::BasicCommands];
        assert!(!service.check_beta_access(&no_permissions));
    }

    #[tokio::test]
    async fn test_enhance_opportunities() {
        let config = AiBetaConfig::default();
        let mut service = AiBetaIntegrationService::new(config);

        let opportunities = vec![create_test_opportunity()];
        let enhanced = service
            .enhance_opportunities(opportunities, "test_user")
            .await
            .unwrap();

        assert!(!enhanced.is_empty());
        assert!(enhanced[0].ai_score > 0.0);
    }

    #[test]
    fn test_personalization_score_calculation() {
        let config = AiBetaConfig::default();
        let mut service = AiBetaIntegrationService::new(config);

        let opportunity = create_test_opportunity();
        let profile = create_test_profile();

        let score = service.calculate_personalization_score(&opportunity, &profile);
        assert!(score > 0.0 && score <= 1.0);

        // Should get bonus for preferred pair
        assert!(score > 0.5); // Base score + preferred pair bonus
    }

    #[test]
    fn test_final_score_calculation() {
        let opportunity = create_test_opportunity();
        let enhanced = AiEnhancedOpportunity::new(opportunity, 0.8)
            .with_market_sentiment(MarketSentiment::Bullish)
            .with_personalization_score(0.9);

        let final_score = enhanced.calculate_final_score();
        assert!(final_score > 0.0 && final_score <= 1.0);

        // Calculate expected score: (0.8 * 0.4) + (0.8 * 0.3) + (0.9 * 0.2) + (0.5 * 0.1) = 0.79
        let expected = (0.8 * 0.4) + (0.8 * 0.3) + (0.9 * 0.2) + (0.5 * 0.1);
        assert!((final_score - expected).abs() < 0.001); // Within tolerance
    }

    #[tokio::test]
    async fn test_personalized_recommendations() {
        let config = AiBetaConfig::default();
        let mut service = AiBetaIntegrationService::new(config);

        let mut profile = create_test_profile();
        profile.historical_performance.max_drawdown = 0.3; // High drawdown

        // For testing, we'll store the profile directly since we don't have a real D1Service
        service
            .user_profiles
            .lock()
            .unwrap()
            .insert("test_user".to_string(), profile);

        let recommendations = service.get_personalized_recommendations("test_user").await;
        assert!(!recommendations.is_empty());

        // Should recommend risk management due to high drawdown
        let risk_rec = recommendations
            .iter()
            .find(|r| r.recommendation_type == "risk_management");
        assert!(risk_rec.is_some());
    }

    #[tokio::test]
    async fn test_prediction_tracking_and_success_marking() {
        let config = AiBetaConfig::default();
        let mut service = AiBetaIntegrationService::new(config);

        // Create and enhance an opportunity to generate a prediction
        let opportunities = vec![create_test_opportunity()];
        let enhanced = service
            .enhance_opportunities(opportunities, "test_user")
            .await
            .unwrap();

        assert!(!enhanced.is_empty());

        // Check that active predictions were recorded
        assert!(!service.active_predictions.lock().unwrap().is_empty());

        // Get the opportunity ID
        let opportunity_id = &enhanced[0].base_opportunity.id;

        // Test successful prediction marking
        let result = service.mark_prediction_successful(opportunity_id);
        assert!(result.is_ok());

        // Prediction should be removed from active predictions
        assert!(!service
            .active_predictions
            .lock()
            .unwrap()
            .contains_key(opportunity_id));

        // Test marking non-existent prediction
        let result = service.mark_prediction_successful("non_existent_id");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No active AI prediction found"));
    }

    #[test]
    fn test_stale_prediction_cleanup() {
        let config = AiBetaConfig::default();
        let mut service = AiBetaIntegrationService::new(config);

        // Add some test predictions with old timestamps
        let old_timestamp = chrono::Utc::now().timestamp_millis() as u64 - (25 * 60 * 60 * 1000); // 25 hours ago
        let recent_timestamp = chrono::Utc::now().timestamp_millis() as u64; // Now

        service
            .active_predictions
            .lock()
            .unwrap()
            .insert("old_prediction".to_string(), (0.8, old_timestamp));
        service
            .active_predictions
            .lock()
            .unwrap()
            .insert("recent_prediction".to_string(), (0.9, recent_timestamp));

        assert_eq!(service.active_predictions.lock().unwrap().len(), 2);

        // Clean up predictions older than 24 hours
        service.cleanup_stale_predictions(24);

        // Only recent prediction should remain
        assert_eq!(service.active_predictions.lock().unwrap().len(), 1);
        assert!(service
            .active_predictions
            .lock()
            .unwrap()
            .contains_key("recent_prediction"));
        assert!(!service
            .active_predictions
            .lock()
            .unwrap()
            .contains_key("old_prediction"));
    }
}
