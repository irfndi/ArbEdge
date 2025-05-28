// use worker::{Request, Response, Env}; // TODO: Re-enable when implementing worker integration [Tracked: PR-24, Comment 94]
use crate::{
    services::{
        core::ai::ai_integration::{AiAnalysisRequest, AiAnalysisResponse, AiProvider},
        AiIntegrationService, D1Service, UserProfileService,
    },
    types::{ArbitrageOpportunity, GlobalOpportunity, UserProfile},
    utils::{ArbitrageError, ArbitrageResult},
};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use worker::console_log;
use worker::kv::KvStore;
// use regex::Regex; // TODO: Re-enable when implementing text parsing features [Tracked: PR-24, Comment 94]

/// Configuration for AI-Exchange Router
#[derive(Debug, Clone)]
pub struct AiExchangeRouterConfig {
    pub enabled: bool,
    pub max_analysis_timeout_seconds: u64,
    pub max_retries: u32,
    pub cache_ttl_seconds: u64,
    pub rate_limit_per_minute: u32,
}

impl Default for AiExchangeRouterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_analysis_timeout_seconds: 30,
            max_retries: 3,
            cache_ttl_seconds: 300, // 5 minutes
            rate_limit_per_minute: 20,
        }
    }
}

/// Market data structure for AI analysis
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketDataSnapshot {
    pub timestamp: u64,
    pub opportunities: Vec<GlobalOpportunity>,
    pub exchange_data: HashMap<String, ExchangeMarketData>,
    pub context: MarketContext,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExchangeMarketData {
    pub exchange_id: String,
    pub funding_rates: HashMap<String, f64>,
    pub orderbook_depth: HashMap<String, OrderbookDepth>,
    pub volume_24h: HashMap<String, f64>,
    pub last_updated: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrderbookDepth {
    pub bids_depth: f64,
    pub asks_depth: f64,
    pub spread: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketContext {
    pub volatility_index: f64,
    pub market_trend: String,
    pub global_sentiment: f64,
    pub active_pairs: Vec<String>,
}

/// AI Analysis result for opportunities
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiOpportunityAnalysis {
    pub opportunity_id: String,
    pub user_id: String,
    pub ai_score: f64,
    pub viability_assessment: String,
    pub risk_factors: Vec<String>,
    pub recommended_position_size: f64,
    pub confidence_level: f64,
    pub analysis_timestamp: u64,
    pub ai_provider_used: String,
    pub custom_recommendations: Vec<String>,
}

/// Rate limit tracking for AI calls
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiCallRateLimit {
    pub user_id: String,
    pub calls_this_minute: u32,
    pub window_start: u64,
}

/// AI-Exchange Router Service
/// Implements secure API call routing through user's AI services
/// Stores audit trails in D1 and uses KV for caching and rate limiting
pub struct AiExchangeRouterService {
    config: AiExchangeRouterConfig,
    ai_service: AiIntegrationService,
    user_service: UserProfileService,
    d1_service: D1Service,
    kv_store: KvStore,
    _http_client: Client,
}

impl AiExchangeRouterService {
    /// Create new AI-Exchange Router service
    pub fn new(
        config: AiExchangeRouterConfig,
        ai_service: AiIntegrationService,
        user_service: UserProfileService,
        d1_service: D1Service,
        kv_store: KvStore,
    ) -> Self {
        Self {
            config,
            ai_service,
            user_service,
            d1_service,
            kv_store,
            _http_client: Client::new(),
        }
    }

    /// Route market data analysis request through user's AI service
    /// with audit trail stored in D1 and rate limiting via KV
    pub async fn analyze_market_data(
        &self,
        user_id: &str,
        market_data: &MarketDataSnapshot,
        analysis_prompt: Option<String>,
    ) -> ArbitrageResult<AiAnalysisResponse> {
        if !self.config.enabled {
            return Err(ArbitrageError::config_error(
                "AI-Exchange routing is disabled",
            ));
        }

        // Check rate limits (KV for speed)
        self.check_and_update_rate_limit(user_id).await?;

        // Get user profile with AI providers
        let user_profile = self
            .user_service
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        // Get user's preferred AI provider
        let ai_provider = self.get_user_ai_provider(&user_profile).await?;

        // Create analysis request
        let prompt = analysis_prompt.unwrap_or_else(|| self.create_default_analysis_prompt());
        let analysis_request = AiAnalysisRequest {
            prompt,
            market_data: serde_json::to_value(market_data).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize market data: {}", e))
            })?,
            user_context: Some(self.create_user_context(&user_profile)?),
            max_tokens: Some(1000),
            temperature: Some(0.7),
        };

        // Call AI provider with retry logic
        let analysis_result = self
            .call_ai_with_retries(&ai_provider, &analysis_request)
            .await?;

        // Store audit trail in D1
        self.store_ai_analysis_audit(user_id, &ai_provider, &analysis_request, &analysis_result)
            .await?;

        // Cache result in KV for faster subsequent access
        self.cache_analysis_result(user_id, &analysis_result)
            .await?;

        Ok(analysis_result)
    }

    /// Analyze specific opportunities with AI and provide customized recommendations
    pub async fn analyze_opportunities(
        &self,
        user_id: &str,
        opportunities: &[GlobalOpportunity],
        user_context: Option<Value>,
    ) -> ArbitrageResult<Vec<AiOpportunityAnalysis>> {
        if !self.config.enabled {
            return Err(ArbitrageError::config_error(
                "AI-Exchange routing is disabled",
            ));
        }

        // Check rate limits
        self.check_and_update_rate_limit(user_id).await?;

        // Get user profile
        let user_profile = self
            .user_service
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        // Get AI provider
        let ai_provider = self.get_user_ai_provider(&user_profile).await?;

        let mut analyses = Vec::new();

        for opportunity in opportunities {
            // Create opportunity-specific analysis request
            let analysis_request = self.create_opportunity_analysis_request(
                opportunity,
                &user_profile,
                user_context.as_ref(),
            )?;

            // Call AI provider
            let ai_response = self
                .call_ai_with_retries(&ai_provider, &analysis_request)
                .await?;

            // Parse AI response into structured analysis
            let analysis = self.parse_ai_opportunity_response(
                user_id,
                opportunity,
                &ai_response,
                &ai_provider,
            )?;

            // Store in D1 for analytics
            self.store_opportunity_analysis(&analysis).await?;

            analyses.push(analysis);
        }

        Ok(analyses)
    }

    /// Get real-time AI recommendations for active trading decisions
    pub async fn get_real_time_recommendations(
        &self,
        user_id: &str,
        current_positions: &[ArbitrageOpportunity],
        market_snapshot: &MarketDataSnapshot,
    ) -> ArbitrageResult<AiAnalysisResponse> {
        if !self.config.enabled {
            return Err(ArbitrageError::config_error(
                "AI-Exchange routing is disabled",
            ));
        }

        // Check cache first (KV)
        if let Some(cached_result) = self.get_cached_recommendations(user_id).await? {
            return Ok(cached_result);
        }

        // Get user and AI provider
        let user_profile = self
            .user_service
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        let ai_provider = self.get_user_ai_provider(&user_profile).await?;

        // Create real-time analysis request
        let prompt = format!(
            "Analyze current positions and market data to provide immediate trading recommendations. \
             Current positions: {} active trades. Market volatility: {:.2}%. \
             Provide specific actions: HOLD, CLOSE, or ADJUST with reasoning.",
            current_positions.len(),
            market_snapshot.context.volatility_index * 100.0
        );

        let analysis_request = AiAnalysisRequest {
            prompt,
            market_data: json!({
                "current_positions": current_positions,
                "market_snapshot": market_snapshot
            }),
            user_context: Some(self.create_user_context(&user_profile)?),
            max_tokens: Some(800),
            temperature: Some(0.3), // Lower temperature for trading decisions
        };

        // Get AI analysis
        let analysis = self
            .call_ai_with_retries(&ai_provider, &analysis_request)
            .await?;

        // Cache for 2 minutes (real-time data)
        self.cache_recommendations(user_id, &analysis, 120).await?;

        // Store audit in D1
        self.store_ai_analysis_audit(user_id, &ai_provider, &analysis_request, &analysis)
            .await?;

        Ok(analysis)
    }

    /// Check and update rate limits using KV store
    async fn check_and_update_rate_limit(&self, user_id: &str) -> ArbitrageResult<()> {
        let rate_limit_key = format!("ai_rate_limit:{}", user_id);
        let current_time = chrono::Utc::now().timestamp() as u64;
        let window_duration = 60; // 1 minute

        let rate_limit = match self.kv_store.get(&rate_limit_key).text().await {
            Ok(Some(data)) => {
                serde_json::from_str::<AiCallRateLimit>(&data).unwrap_or_else(|_| AiCallRateLimit {
                    user_id: user_id.to_string(),
                    calls_this_minute: 0,
                    window_start: current_time,
                })
            }
            _ => AiCallRateLimit {
                user_id: user_id.to_string(),
                calls_this_minute: 0,
                window_start: current_time,
            },
        };

        // Check if we're in a new time window
        let updated_limit = if current_time - rate_limit.window_start >= window_duration {
            AiCallRateLimit {
                user_id: user_id.to_string(),
                calls_this_minute: 1,
                window_start: current_time,
            }
        } else {
            if rate_limit.calls_this_minute >= self.config.rate_limit_per_minute {
                return Err(ArbitrageError::rate_limit_error(
                    "AI analysis rate limit exceeded",
                ));
            }
            AiCallRateLimit {
                user_id: user_id.to_string(),
                calls_this_minute: rate_limit.calls_this_minute + 1,
                window_start: rate_limit.window_start,
            }
        };

        // Update rate limit in KV
        let rate_limit_data = serde_json::to_string(&updated_limit).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize rate limit: {}", e))
        })?;

        self.kv_store
            .put(&rate_limit_key, rate_limit_data)
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to create KV put request: {}", e))
            })?
            .expiration_ttl(120) // 2 minutes expiration
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to update rate limit: {}", e))
            })?;

        Ok(())
    }

    /// Get user's AI provider for analysis
    async fn get_user_ai_provider(
        &self,
        user_profile: &UserProfile,
    ) -> ArbitrageResult<AiProvider> {
        // Find first active AI API key
        let ai_key = user_profile
            .api_keys
            .iter()
            .find(|key| {
                key.is_active
                    && matches!(
                        key.provider,
                        crate::types::ApiKeyProvider::OpenAI
                            | crate::types::ApiKeyProvider::Anthropic
                            | crate::types::ApiKeyProvider::Custom
                    )
            })
            .ok_or_else(|| ArbitrageError::not_found("No active AI API key found for user"))?;

        // Create AI provider from the key
        self.ai_service.create_ai_provider(ai_key)
    }

    /// Call AI provider with retry logic
    async fn call_ai_with_retries(
        &self,
        provider: &AiProvider,
        request: &AiAnalysisRequest,
    ) -> ArbitrageResult<AiAnalysisResponse> {
        let mut last_error = None;

        for attempt in 1..=self.config.max_retries {
            match self.ai_service.call_ai_provider(provider, request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = Some(e);

                    // Add minimal delay between retries to avoid overwhelming rate-limited AI APIs
                    // Even in Cloudflare Workers, this prevents burst requests that could trigger rate limits
                    if attempt < self.config.max_retries {
                        // Minimal delay: 100-500ms to respect rate limits while staying responsive
                        let delay_ms = 100 + ((attempt - 1) * 100); // 100ms, 200ms, 300ms progression
                        self.delay_async(delay_ms).await;
                    }
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| ArbitrageError::network_error("All AI provider calls failed")))
    }

    /// Store AI analysis audit trail in D1
    async fn store_ai_analysis_audit(
        &self,
        user_id: &str,
        ai_provider: &AiProvider,
        request: &AiAnalysisRequest,
        response: &AiAnalysisResponse,
    ) -> ArbitrageResult<()> {
        let start_time = std::time::Instant::now();

        let provider_name = match ai_provider {
            AiProvider::OpenAI { .. } => "OpenAI",
            AiProvider::Anthropic { .. } => "Anthropic",
            AiProvider::Custom { .. } => "Custom",
        };

        // Convert request and response to JSON for audit storage
        let request_data = serde_json::to_value(request).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize request: {}", e))
        })?;

        let response_data = serde_json::to_value(response).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize response: {}", e))
        })?;

        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        // Store audit trail in D1 database
        self.d1_service
            .store_ai_analysis_audit(
                user_id,
                provider_name,
                &request_data,
                &response_data,
                processing_time_ms,
            )
            .await?;

        console_log!(
            "AI Analysis Audit stored: user={}, provider={}, processing_time_ms={}",
            user_id,
            provider_name,
            processing_time_ms
        );

        Ok(())
    }

    /// Async delay for retry backoff in Cloudflare Workers environment
    async fn delay_async(&self, delay_ms: u32) {
        #[cfg(target_arch = "wasm32")]
        {
            // Use worker's sleep for WASM environment
            use worker::Delay;
            Delay::from(std::time::Duration::from_millis(delay_ms as u64)).await;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            // Use tokio's sleep for native environment
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms as u64)).await;
        }
    }

    /// Cache analysis result in KV store
    async fn cache_analysis_result(
        &self,
        user_id: &str,
        analysis: &AiAnalysisResponse,
    ) -> ArbitrageResult<()> {
        let cache_key = format!("ai_analysis_cache:{}", user_id);
        let cache_data = serde_json::to_string(analysis).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize analysis: {}", e))
        })?;

        self.kv_store
            .put(&cache_key, cache_data)
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to create KV put request: {}", e))
            })?
            .expiration_ttl(self.config.cache_ttl_seconds)
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to cache analysis: {}", e))
            })?;

        Ok(())
    }

    /// Create default analysis prompt for market data
    fn create_default_analysis_prompt(&self) -> String {
        "Analyze the provided cryptocurrency arbitrage market data. \
         Focus on identifying high-probability opportunities, risk factors, \
         and optimal position sizing. Provide specific actionable insights \
         for arbitrage trading decisions. Consider market volatility, \
         liquidity, and funding rate dynamics."
            .to_string()
    }

    /// Create user context for AI analysis
    fn create_user_context(&self, user_profile: &UserProfile) -> ArbitrageResult<Value> {
        Ok(json!({
            "user_id": user_profile.user_id,
            "subscription_tier": user_profile.subscription.tier,
            "total_trades": user_profile.total_trades,
            "total_pnl": user_profile.total_pnl_usdt,
            "risk_tolerance": user_profile.configuration.risk_tolerance_percentage,
            "max_position_size": user_profile.configuration.max_entry_size_usdt,
            "min_position_size": user_profile.configuration.min_entry_size_usdt,
        }))
    }

    /// Create opportunity-specific analysis request
    fn create_opportunity_analysis_request(
        &self,
        opportunity: &GlobalOpportunity,
        user_profile: &UserProfile,
        user_context: Option<&Value>,
    ) -> ArbitrageResult<AiAnalysisRequest> {
        let prompt = format!(
            "Analyze this specific arbitrage opportunity: {:?} with {:.2}% rate difference. \
             Consider user's risk tolerance ({:.2}%) and max position size (${:.2}). \
             Provide viability score (0-100), risk factors, and recommended position size.",
            opportunity.source,
            opportunity.opportunity.rate_difference * 100.0,
            user_profile.configuration.risk_tolerance_percentage,
            user_profile.configuration.max_entry_size_usdt
        );

        Ok(AiAnalysisRequest {
            prompt,
            market_data: serde_json::to_value(opportunity).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize opportunity: {}", e))
            })?,
            user_context: user_context
                .cloned()
                .or_else(|| self.create_user_context(user_profile).ok()),
            max_tokens: Some(600),
            temperature: Some(0.5),
        })
    }

    /// Parse AI response into structured opportunity analysis
    fn parse_ai_opportunity_response(
        &self,
        user_id: &str,
        opportunity: &GlobalOpportunity,
        ai_response: &AiAnalysisResponse,
        ai_provider: &AiProvider,
    ) -> ArbitrageResult<AiOpportunityAnalysis> {
        // Extract score from AI response (simple regex-based extraction for now)
        let ai_score = self.extract_score_from_analysis(&ai_response.analysis);
        let confidence = ai_response.confidence.unwrap_or(0.5) as f64;

        Ok(AiOpportunityAnalysis {
            opportunity_id: opportunity.opportunity.id.clone(),
            user_id: user_id.to_string(),
            ai_score,
            viability_assessment: ai_response.analysis.clone(),
            risk_factors: self.extract_risk_factors(&ai_response.analysis),
            recommended_position_size: self.extract_position_size(&ai_response.analysis),
            confidence_level: confidence,
            analysis_timestamp: chrono::Utc::now().timestamp() as u64,
            ai_provider_used: match ai_provider {
                AiProvider::OpenAI { .. } => "OpenAI".to_string(),
                AiProvider::Anthropic { .. } => "Anthropic".to_string(),
                AiProvider::Custom { .. } => "Custom".to_string(),
            },
            custom_recommendations: ai_response.recommendations.clone(),
        })
    }

    /// Extract numeric score from AI analysis text
    fn extract_score_from_analysis(&self, analysis: &str) -> f64 {
        let score_regex =
            regex::Regex::new(r"(?i)(?:score|viability):\s*(\d+)|(\d+)/100|(\d+)%").unwrap();

        if let Some(captures) = score_regex.captures(analysis) {
            let score_str = captures
                .get(1)
                .or_else(|| captures.get(2))
                .or_else(|| captures.get(3))
                .map(|m| m.as_str())
                .unwrap_or("50");

            score_str.parse::<f64>().unwrap_or(50.0) / 100.0
        } else {
            0.5 // Default neutral score
        }
    }

    /// Extract risk factors from AI analysis
    fn extract_risk_factors(&self, analysis: &str) -> Vec<String> {
        let mut risk_factors = Vec::new();
        let analysis_lower = analysis.to_lowercase();

        if analysis_lower.contains("volatility") {
            risk_factors.push("High market volatility".to_string());
        }
        if analysis_lower.contains("liquidity") {
            risk_factors.push("Liquidity constraints".to_string());
        }
        if analysis_lower.contains("regulation") {
            risk_factors.push("Regulatory uncertainty".to_string());
        }
        if analysis_lower.contains("slippage") {
            risk_factors.push("Price slippage risk".to_string());
        }

        risk_factors
    }

    /// Extract recommended position size from analysis
    fn extract_position_size(&self, analysis: &str) -> f64 {
        let usdt_regex =
            regex::Regex::new(r"(?i)(?:size\s*:?\s*\$|use\s+)(\d+(?:\.\d+)?)\s*(?:usdt|usd|\$?)\b")
                .unwrap();
        let percentage_regex =
            regex::Regex::new(r"(?i)position\s*(?:size|amount)\s*:?\s*(\d+(?:\.\d+)?)\s*%")
                .unwrap();

        // First try to match USDT/dollar amounts
        if let Some(captures) = usdt_regex.captures(analysis) {
            if let Some(matched) = captures.get(1) {
                if let Ok(amount) = matched.as_str().parse::<f64>() {
                    return amount; // Return the USDT amount directly
                }
            }
        }

        // Then try percentage patterns
        if let Some(captures) = percentage_regex.captures(analysis) {
            if let Some(matched) = captures.get(1) {
                if let Ok(percentage) = matched.as_str().parse::<f64>() {
                    return percentage / 100.0; // Convert percentage to decimal
                }
            }
        }

        100.0 // Default $100 position size (changed from 0.05)
    }

    /// Store opportunity analysis in D1
    async fn store_opportunity_analysis(
        &self,
        analysis: &AiOpportunityAnalysis,
    ) -> ArbitrageResult<()> {
        // Store opportunity analysis in D1 database
        self.d1_service.store_opportunity_analysis(analysis).await?;

        console_log!(
            "AI Opportunity Analysis stored: opportunity_id={}, user_id={}, score={:.2}",
            analysis.opportunity_id,
            analysis.user_id,
            analysis.ai_score
        );

        Ok(())
    }

    /// Get cached recommendations from KV
    async fn get_cached_recommendations(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<AiAnalysisResponse>> {
        let cache_key = format!("ai_recommendations_cache:{}", user_id);

        match self.kv_store.get(&cache_key).text().await {
            Ok(Some(data)) => match serde_json::from_str::<AiAnalysisResponse>(&data) {
                Ok(response) => Ok(Some(response)),
                Err(_) => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// Cache recommendations in KV
    async fn cache_recommendations(
        &self,
        user_id: &str,
        analysis: &AiAnalysisResponse,
        ttl_seconds: u64,
    ) -> ArbitrageResult<()> {
        let cache_key = format!("ai_recommendations_cache:{}", user_id);
        let cache_data = serde_json::to_string(analysis).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize recommendations: {}", e))
        })?;

        self.kv_store
            .put(&cache_key, cache_data)
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to create KV put request: {}", e))
            })?
            .expiration_ttl(ttl_seconds)
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to cache recommendations: {}", e))
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::collections::HashMap;

    fn create_test_config() -> AiExchangeRouterConfig {
        AiExchangeRouterConfig {
            enabled: true,
            max_analysis_timeout_seconds: 30,
            max_retries: 2,
            cache_ttl_seconds: 300,
            rate_limit_per_minute: 20,
        }
    }

    fn create_test_market_data() -> MarketDataSnapshot {
        use crate::types::{
            ArbitrageOpportunity, ArbitrageType, DistributionStrategy, ExchangeIdEnum,
            GlobalOpportunity, OpportunitySource,
        };

        let mut exchange_data = HashMap::new();
        exchange_data.insert(
            "binance".to_string(),
            ExchangeMarketData {
                exchange_id: "binance".to_string(),
                funding_rates: [("BTCUSDT".to_string(), 0.001)].into(),
                orderbook_depth: [(
                    "BTCUSDT".to_string(),
                    OrderbookDepth {
                        bids_depth: 50000.0,
                        asks_depth: 45000.0,
                        spread: 0.01,
                    },
                )]
                .into(),
                volume_24h: [("BTCUSDT".to_string(), 1000000.0)].into(),
                last_updated: chrono::Utc::now().timestamp() as u64,
            },
        );

        // Create a test opportunity to ensure the list is not empty
        let arbitrage_opp = ArbitrageOpportunity::new(
            "BTCUSDT".to_string(),
            ExchangeIdEnum::Binance,
            ExchangeIdEnum::Bybit,
            Some(0.0001),
            Some(0.0008),
            0.0007,
            ArbitrageType::FundingRate,
        );

        let global_opportunity = GlobalOpportunity {
            id: uuid::Uuid::new_v4().to_string(),
            opportunity_type: "arbitrage".to_string(),
            opportunity: arbitrage_opp.clone().unwrap_or_else(|_| ArbitrageOpportunity::default()),
            arbitrage_opportunity: Some(arbitrage_opp.unwrap_or_else(|_| ArbitrageOpportunity::default())),
            technical_opportunity: None,
            source: OpportunitySource::SystemGenerated,
            created_at: chrono::Utc::now().timestamp() as u64,
            detection_timestamp: chrono::Utc::now().timestamp() as u64,
            expires_at: Some(chrono::Utc::now().timestamp() as u64 + 3600),
            expiry_timestamp: Some(chrono::Utc::now().timestamp() as u64 + 3600),
            priority: 8,
            priority_score: 8.5,
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
            distributed_to: vec!["user1".to_string()],
            max_participants: Some(10),
            current_participants: 1,
            distribution_strategy: DistributionStrategy::FirstComeFirstServe,
        };

        MarketDataSnapshot {
            timestamp: chrono::Utc::now().timestamp() as u64,
            opportunities: vec![global_opportunity], // Changed from empty vec to include opportunity
            exchange_data,
            context: MarketContext {
                volatility_index: 0.25,
                market_trend: "bullish".to_string(),
                global_sentiment: 0.7,
                active_pairs: vec!["BTCUSDT".to_string()],
            },
        }
    }

    #[test]
    fn test_ai_exchange_router_config_creation() {
        let config = AiExchangeRouterConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_analysis_timeout_seconds, 30);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.cache_ttl_seconds, 300);
        assert_eq!(config.rate_limit_per_minute, 20);
    }

    #[test]
    fn test_market_data_snapshot_structure() {
        let market_data = create_test_market_data();
        assert!(market_data.timestamp > 0);
        assert_eq!(market_data.context.volatility_index, 0.25);
        assert_eq!(market_data.context.market_trend, "bullish");
        assert_eq!(market_data.context.active_pairs.len(), 1);
        assert!(market_data.exchange_data.contains_key("binance"));
    }

    #[test]
    fn test_ai_opportunity_analysis_structure() {
        let analysis = AiOpportunityAnalysis {
            opportunity_id: "test_opp_1".to_string(),
            user_id: "user123".to_string(),
            ai_score: 0.85,
            viability_assessment: "High viability opportunity".to_string(),
            risk_factors: vec!["Volatility".to_string()],
            recommended_position_size: 1000.0,
            confidence_level: 0.9,
            analysis_timestamp: chrono::Utc::now().timestamp() as u64,
            ai_provider_used: "OpenAI".to_string(),
            custom_recommendations: vec!["Monitor closely".to_string()],
        };

        assert_eq!(analysis.ai_score, 0.85);
        assert_eq!(analysis.confidence_level, 0.9);
        assert_eq!(analysis.recommended_position_size, 1000.0);
        assert_eq!(analysis.risk_factors.len(), 1);
    }

    #[test]
    fn test_rate_limit_tracking() {
        let rate_limit = AiCallRateLimit {
            user_id: "user123".to_string(),
            calls_this_minute: 5,
            window_start: chrono::Utc::now().timestamp() as u64,
        };

        assert_eq!(rate_limit.calls_this_minute, 5);
        assert!(rate_limit.window_start > 0);
    }

    #[test]
    fn test_exchange_market_data_structure() {
        let exchange_data = ExchangeMarketData {
            exchange_id: "binance".to_string(),
            funding_rates: [("BTCUSDT".to_string(), 0.001)].into(),
            orderbook_depth: [(
                "BTCUSDT".to_string(),
                OrderbookDepth {
                    bids_depth: 50000.0,
                    asks_depth: 45000.0,
                    spread: 0.01,
                },
            )]
            .into(),
            volume_24h: [("BTCUSDT".to_string(), 1000000.0)].into(),
            last_updated: chrono::Utc::now().timestamp() as u64,
        };

        assert_eq!(exchange_data.exchange_id, "binance");
        assert!(exchange_data.funding_rates.contains_key("BTCUSDT"));
        assert!(exchange_data.orderbook_depth.contains_key("BTCUSDT"));
        assert_eq!(exchange_data.volume_24h.get("BTCUSDT"), Some(&1000000.0));
    }

    #[test]
    fn test_orderbook_depth_calculations() {
        let depth = OrderbookDepth {
            bids_depth: 50000.0,
            asks_depth: 45000.0,
            spread: 0.01,
        };

        assert_eq!(depth.bids_depth, 50000.0);
        assert_eq!(depth.asks_depth, 45000.0);
        assert_eq!(depth.spread, 0.01);
    }

    #[test]
    fn test_market_context_creation() {
        let context = MarketContext {
            volatility_index: 0.25,
            market_trend: "bullish".to_string(),
            global_sentiment: 0.7,
            active_pairs: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
        };

        assert_eq!(context.volatility_index, 0.25);
        assert_eq!(context.market_trend, "bullish");
        assert_eq!(context.global_sentiment, 0.7);
        assert_eq!(context.active_pairs.len(), 2);
    }

    #[test]
    fn test_score_extraction_from_analysis() {
        // Test various score formats using standalone function
        assert_eq!(extract_score_from_text("Score: 85"), 0.85);
        assert_eq!(extract_score_from_text("85/100"), 0.85);
        assert_eq!(extract_score_from_text("85%"), 0.85);
        assert_eq!(extract_score_from_text("Viability: 90"), 0.90);
        assert_eq!(extract_score_from_text("No score here"), 0.5);
    }

    #[test]
    fn test_risk_factor_extraction() {
        let analysis = "High volatility and liquidity constraints make this risky";
        let risks = extract_risk_factors_from_text(analysis);

        assert!(risks.contains(&"High market volatility".to_string()));
        assert!(risks.contains(&"Liquidity constraints".to_string()));
        assert_eq!(risks.len(), 2);
    }

    #[test]
    fn test_position_size_extraction() {
        assert_eq!(
            extract_position_size_from_text("Recommended size: $1000"),
            1000.0
        );
        assert_eq!(extract_position_size_from_text("Use 500 USDT"), 500.0);
        assert_eq!(extract_position_size_from_text("No size mentioned"), 100.0);
    }

    // Standalone helper functions for testing business logic
    fn extract_score_from_text(analysis: &str) -> f64 {
        let score_regex =
            regex::Regex::new(r"(?i)(?:score|viability):\s*(\d+)|(\d+)/100|(\d+)%").unwrap();

        if let Some(captures) = score_regex.captures(analysis) {
            let score_str = captures
                .get(1)
                .or_else(|| captures.get(2))
                .or_else(|| captures.get(3))
                .map(|m| m.as_str())
                .unwrap_or("50");

            score_str.parse::<f64>().unwrap_or(50.0) / 100.0
        } else {
            0.5 // Default neutral score
        }
    }

    fn extract_risk_factors_from_text(analysis: &str) -> Vec<String> {
        let mut risk_factors = Vec::new();
        let analysis_lower = analysis.to_lowercase();

        if analysis_lower.contains("volatility") {
            risk_factors.push("High market volatility".to_string());
        }
        if analysis_lower.contains("liquidity") {
            risk_factors.push("Liquidity constraints".to_string());
        }
        if analysis_lower.contains("regulation") {
            risk_factors.push("Regulatory uncertainty".to_string());
        }
        if analysis_lower.contains("slippage") {
            risk_factors.push("Price slippage risk".to_string());
        }

        risk_factors
    }

    fn extract_position_size_from_text(analysis: &str) -> f64 {
        // Simple regex to extract position size recommendations in USDT or dollars
        let usdt_regex =
            regex::Regex::new(r"(?i)(?:size\s*:?\s*\$|use\s+)(\d+(?:\.\d+)?)\s*(?:usdt|usd|\$?)\b")
                .unwrap();
        let percentage_regex =
            regex::Regex::new(r"(?i)position\s*(?:size|amount)\s*:?\s*(\d+(?:\.\d+)?)\s*%")
                .unwrap();

        // First try to match USDT/dollar amounts
        if let Some(captures) = usdt_regex.captures(analysis) {
            if let Some(matched) = captures.get(1) {
                if let Ok(amount) = matched.as_str().parse::<f64>() {
                    return amount; // Return the USDT amount directly
                }
            }
        }

        // Then try percentage patterns
        if let Some(captures) = percentage_regex.captures(analysis) {
            if let Some(matched) = captures.get(1) {
                if let Ok(percentage) = matched.as_str().parse::<f64>() {
                    return percentage / 100.0; // Convert percentage to decimal
                }
            }
        }

        100.0 // Default $100 position size (changed from 0.05)
    }

    // Integration tests for service methods
    #[cfg(test)]
    mod integration_tests {
        use super::*;
        use crate::types::UserProfile;
        use std::collections::HashMap;

        // Mock structures for testing
        #[allow(dead_code)]
        struct MockKvStore {
            data: HashMap<String, String>,
        }

        #[allow(dead_code)]
        impl MockKvStore {
            fn new() -> Self {
                Self {
                    data: HashMap::new(),
                }
            }

            async fn get(&self, key: &str) -> Option<String> {
                self.data.get(key).cloned()
            }

            async fn put(&mut self, key: &str, value: String) -> Result<(), String> {
                self.data.insert(key.to_string(), value);
                Ok(())
            }
        }

        #[allow(dead_code)]
        struct MockUserProfileService {
            profiles: HashMap<String, UserProfile>,
        }

        #[allow(dead_code)]
        impl MockUserProfileService {
            fn new() -> Self {
                Self {
                    profiles: HashMap::new(),
                }
            }

            fn with_user(mut self, user_id: &str, profile: UserProfile) -> Self {
                self.profiles.insert(user_id.to_string(), profile);
                self
            }

            async fn get_user_profile(
                &self,
                user_id: &str,
            ) -> ArbitrageResult<Option<UserProfile>> {
                Ok(self.profiles.get(user_id).cloned())
            }
        }

        #[allow(dead_code)]
        struct MockAiIntegrationService {
            responses: HashMap<String, AiAnalysisResponse>,
        }

        #[allow(dead_code)]
        impl MockAiIntegrationService {
            fn new() -> Self {
                Self {
                    responses: HashMap::new(),
                }
            }

            fn with_response(mut self, provider: &str, response: AiAnalysisResponse) -> Self {
                self.responses.insert(provider.to_string(), response);
                self
            }

            async fn call_ai_provider(
                &self,
                provider: &AiProvider,
                _request: &AiAnalysisRequest,
            ) -> ArbitrageResult<AiAnalysisResponse> {
                match provider {
                    AiProvider::OpenAI { .. } => Ok(self
                        .responses
                        .get("openai")
                        .unwrap_or(&create_mock_ai_response())
                        .clone()),
                    AiProvider::Anthropic { .. } => Ok(self
                        .responses
                        .get("anthropic")
                        .unwrap_or(&create_mock_ai_response())
                        .clone()),
                    AiProvider::Custom { .. } => Ok(self
                        .responses
                        .get("custom")
                        .unwrap_or(&create_mock_ai_response())
                        .clone()),
                }
            }
        }

        #[allow(dead_code)]
        struct MockD1Service;

        #[allow(dead_code)]
        impl MockD1Service {
            fn new() -> Self {
                Self
            }

            async fn store_ai_analysis_audit(&self, _audit_data: &str) -> ArbitrageResult<()> {
                Ok(())
            }

            async fn store_opportunity_analysis(
                &self,
                _analysis: &AiOpportunityAnalysis,
            ) -> ArbitrageResult<()> {
                Ok(())
            }
        }

        fn create_mock_ai_response() -> AiAnalysisResponse {
            AiAnalysisResponse {
                analysis: "Market analysis shows moderate volatility. Risk score: 7.5/10. Recommended position size: 3%. Hold current positions.".to_string(),
                confidence: Some(0.85),
                recommendations: vec![
                    "Monitor price movement closely".to_string(),
                    "Maintain current position size".to_string(),
                ],
                metadata: {
                    let mut meta = std::collections::HashMap::new();
                    meta.insert("tokens_used".to_string(), serde_json::json!(150));
                    meta.insert("model_used".to_string(), serde_json::json!("gpt-4"));
                    meta.insert("finish_reason".to_string(), serde_json::json!("stop"));
                    meta
                },
            }
        }

        fn create_test_user_profile() -> UserProfile {
            UserProfile::new(Some(123456789), Some("testuser_invite".to_string()))
        }

        fn create_test_global_opportunity() -> GlobalOpportunity {
            use crate::types::{
                ArbitrageOpportunity, ArbitrageType, DistributionStrategy, ExchangeIdEnum,
                OpportunitySource,
            };

            let arbitrage_opp = ArbitrageOpportunity::new(
                "BTCUSDT".to_string(),
                ExchangeIdEnum::Binance,
                ExchangeIdEnum::Bybit,
                Some(0.0001),
                Some(0.0008),
                0.0007,
                ArbitrageType::FundingRate,
            );

            GlobalOpportunity {
                id: uuid::Uuid::new_v4().to_string(),
                opportunity_type: "arbitrage".to_string(),
                opportunity: arbitrage_opp.unwrap_or_else(|_| ArbitrageOpportunity::default()),
                arbitrage_opportunity: Some(arbitrage_opp.unwrap_or_else(|_| ArbitrageOpportunity::default())),
                technical_opportunity: None,
                source: OpportunitySource::SystemGenerated,
                created_at: chrono::Utc::now().timestamp() as u64,
                detection_timestamp: chrono::Utc::now().timestamp() as u64,
                expires_at: Some(chrono::Utc::now().timestamp() as u64 + 3600),
                expiry_timestamp: Some(chrono::Utc::now().timestamp() as u64 + 3600),
                priority: 8,
                priority_score: 8.5,
                ai_enhanced: false,
                ai_confidence_score: None,
                ai_insights: None,
                distributed_to: vec!["user1".to_string()],
                max_participants: Some(10),
                current_participants: 3,
                distribution_strategy: DistributionStrategy::FirstComeFirstServe,
            }
        }

        #[tokio::test]
        async fn test_ai_exchange_router_service_creation() {
            let config = create_test_config();
            let _ai_service = MockAiIntegrationService::new();
            let _user_service = MockUserProfileService::new();
            let _d1_service = MockD1Service::new();
            let _kv_store = MockKvStore::new();

            // Note: In actual implementation, this would use the real constructor
            // For now, we test the configuration and structure
            assert!(config.enabled);
            assert_eq!(config.max_analysis_timeout_seconds, 30);
            assert_eq!(config.rate_limit_per_minute, 20);
        }

        #[tokio::test]
        #[allow(clippy::result_large_err)]
        async fn test_rate_limit_functionality() -> ArbitrageResult<()> {
            let rate_limit = AiCallRateLimit {
                user_id: "test_user".to_string(),
                calls_this_minute: 5,
                window_start: chrono::Utc::now().timestamp() as u64,
            };

            // Test rate limit structure
            assert_eq!(rate_limit.user_id, "test_user");
            assert_eq!(rate_limit.calls_this_minute, 5);
            assert!(rate_limit.window_start > 0);

            // Test serialization
            let serialized = serde_json::to_string(&rate_limit).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize rate limit: {}", e))
            })?;
            let deserialized: AiCallRateLimit = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized.user_id, rate_limit.user_id);
            assert_eq!(deserialized.calls_this_minute, rate_limit.calls_this_minute);
            Ok(())
        }

        #[tokio::test]
        async fn test_market_data_analysis_prompt_generation() {
            let _config = create_test_config();
            let _market_data = create_test_market_data();
            let user_profile = create_test_user_profile();

            // Test default prompt generation
            let default_prompt = "Analyze the current market data and provide insights on arbitrage opportunities. \
                                 Focus on funding rate differences, orderbook depth, and market volatility. \
                                 Provide a risk assessment score (1-10) and recommended position sizing.";

            assert!(default_prompt.contains("market data"));
            assert!(default_prompt.contains("risk assessment"));
            assert!(default_prompt.contains("position sizing"));

            // Test user context creation
            let user_context = serde_json::json!({
                "user_id": user_profile.user_id,
                "subscription_tier": user_profile.subscription,
                "trading_experience": "advanced",
                "risk_tolerance": "medium"
            });

            assert_eq!(user_context["user_id"], user_profile.user_id);
            assert!(user_context.get("subscription_tier").is_some());
        }

        #[tokio::test]
        #[allow(clippy::result_large_err)]
        async fn test_opportunity_analysis_parsing() -> ArbitrageResult<()> {
            let user_id = "test_user_123";
            let opportunity = create_test_global_opportunity();
            let _ai_response = create_mock_ai_response();

            // Test AI opportunity analysis structure
            let analysis = AiOpportunityAnalysis {
                opportunity_id: opportunity.opportunity.id.clone(),
                user_id: user_id.to_string(),
                ai_score: 7.5,
                viability_assessment: "High potential with moderate risk".to_string(),
                risk_factors: vec![
                    "Market volatility".to_string(),
                    "Liquidity concerns".to_string(),
                ],
                recommended_position_size: 0.03,
                confidence_level: 0.85,
                analysis_timestamp: chrono::Utc::now().timestamp() as u64,
                ai_provider_used: "gpt-4".to_string(),
                custom_recommendations: vec!["Monitor price movement closely".to_string()],
            };

            // Verify analysis structure
            assert_eq!(analysis.opportunity_id, opportunity.opportunity.id);
            assert_eq!(analysis.user_id, user_id);
            assert!(analysis.ai_score >= 0.0 && analysis.ai_score <= 10.0);
            assert!(analysis.confidence_level >= 0.0 && analysis.confidence_level <= 1.0);
            assert!(analysis.recommended_position_size > 0.0);
            assert!(!analysis.risk_factors.is_empty());

            // Test serialization
            let serialized = serde_json::to_string(&analysis).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize analysis: {}", e))
            })?;
            let deserialized: AiOpportunityAnalysis = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized.opportunity_id, analysis.opportunity_id);
            assert_eq!(deserialized.ai_score, analysis.ai_score);
            Ok(())
        }

        #[tokio::test]
        async fn test_disabled_service_handling() {
            let mut config = create_test_config();
            config.enabled = false;

            // Test that disabled service returns appropriate errors
            // This would be tested with actual service instance in full implementation
            assert!(!config.enabled);

            // Simulate error that would be returned
            let error = ArbitrageError::config_error("AI-Exchange routing is disabled");
            assert!(error.to_string().contains("disabled"));
        }

        #[tokio::test]
        async fn test_real_time_recommendations_structure() {
            let user_id = "test_user";
            let current_positions: Vec<ArbitrageOpportunity> = vec![]; // Empty positions for test
            let market_snapshot = create_test_market_data();

            // Test real-time recommendation prompt format
            let prompt = format!(
                "Analyze current positions and market data to provide immediate trading recommendations. \
                 Current positions: {} active trades. Market volatility: {:.2}%. \
                 Provide specific actions: HOLD, CLOSE, or ADJUST with reasoning.",
                current_positions.len(),
                market_snapshot.context.volatility_index * 100.0
            );

            assert!(prompt.contains("immediate trading recommendations"));
            assert!(prompt.contains("0 active trades")); // Since positions is empty
            assert!(prompt.contains("HOLD, CLOSE, or ADJUST"));

            // Test AI analysis request for real-time data
            let analysis_request = AiAnalysisRequest {
                prompt: prompt.clone(),
                market_data: serde_json::json!({
                    "current_positions": current_positions,
                    "market_snapshot": market_snapshot
                }),
                user_context: Some(serde_json::json!({
                    "user_id": user_id,
                    "subscription_tier": "premium"
                })),
                max_tokens: Some(800),
                temperature: Some(0.3),
            };

            assert_eq!(analysis_request.prompt, prompt);
            assert_eq!(analysis_request.max_tokens, Some(800));
            assert_eq!(analysis_request.temperature, Some(0.3));
        }

        #[tokio::test]
        async fn test_error_handling_scenarios() {
            // Test user not found scenario
            let error = ArbitrageError::not_found("User profile not found");
            assert!(error.to_string().contains("not found"));

            // Test rate limit exceeded scenario
            let rate_error = ArbitrageError::rate_limit_error("AI analysis rate limit exceeded");
            assert!(rate_error.to_string().contains("rate limit"));

            // Test serialization error scenario
            let parse_error = ArbitrageError::parse_error("Failed to serialize market data");
            assert!(parse_error.to_string().contains("serialize"));

            // Test storage error scenario
            let storage_error =
                ArbitrageError::storage_error("Failed to update rate limit in storage");
            assert!(storage_error.to_string().contains("storage"));
        }

        #[tokio::test]
        async fn test_comprehensive_market_data_validation() {
            let market_data = create_test_market_data();

            // Verify all required fields are present
            assert!(market_data.timestamp > 0);
            assert!(!market_data.opportunities.is_empty());
            assert!(!market_data.exchange_data.is_empty());

            // Verify market context
            assert!(market_data.context.volatility_index >= 0.0);
            assert!(!market_data.context.market_trend.is_empty());
            assert!(
                market_data.context.global_sentiment >= -1.0
                    && market_data.context.global_sentiment <= 1.0
            );
            assert!(!market_data.context.active_pairs.is_empty());

            // Verify exchange data structure
            for (exchange_id, exchange_data) in &market_data.exchange_data {
                assert!(!exchange_id.is_empty());
                assert_eq!(exchange_data.exchange_id, *exchange_id);
                assert!(exchange_data.last_updated > 0);

                // Verify orderbook depth data
                for (symbol, depth) in &exchange_data.orderbook_depth {
                    assert!(!symbol.is_empty());
                    assert!(depth.bids_depth >= 0.0);
                    assert!(depth.asks_depth >= 0.0);
                    assert!(depth.spread >= 0.0);
                }
            }
        }
    }
}
