// Opportunity Categorization Service
// Task 9.5: User Experience & Opportunity Categorization

use crate::services::{
    core::analysis::market_analysis::{
        OpportunityType, RiskLevel, TimeHorizon, TradingOpportunity,
    },
    core::infrastructure::DatabaseManager,
    core::user::user_trading_preferences::{
        ExperienceLevel, RiskTolerance, TradingFocus, UserTradingPreferences,
    },
    UserTradingPreferencesService,
};
use crate::utils::{logger::Logger, ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ============= OPPORTUNITY CATEGORY TYPES =============

/// Enhanced opportunity categories for better user experience
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OpportunityCategory {
    #[serde(rename = "low_risk_arbitrage")]
    LowRiskArbitrage, // Conservative arbitrage opportunities
    #[serde(rename = "high_confidence_arbitrage")]
    HighConfidenceArbitrage, // High confidence arbitrage (90%+ confidence)
    #[serde(rename = "technical_signals")]
    TechnicalSignals, // Technical analysis signals
    #[serde(rename = "momentum_trading")]
    MomentumTrading, // Momentum-based opportunities
    #[serde(rename = "mean_reversion")]
    MeanReversion, // Mean reversion strategies
    #[serde(rename = "breakout_patterns")]
    BreakoutPatterns, // Price breakout patterns
    #[serde(rename = "hybrid_enhanced")]
    HybridEnhanced, // Arbitrage enhanced with technical analysis
    #[serde(rename = "ai_recommended")]
    AiRecommended, // AI-validated opportunities
    #[serde(rename = "beginner_friendly")]
    BeginnerFriendly, // Simple, low-risk opportunities for beginners
    #[serde(rename = "advanced_strategies")]
    AdvancedStrategies, // Complex strategies for experienced traders
}

impl OpportunityCategory {
    /// Get the display name for the category
    pub fn display_name(&self) -> &'static str {
        match self {
            OpportunityCategory::LowRiskArbitrage => "Low Risk Arbitrage",
            OpportunityCategory::HighConfidenceArbitrage => "High Confidence Arbitrage",
            OpportunityCategory::TechnicalSignals => "Technical Signals",
            OpportunityCategory::MomentumTrading => "Momentum Trading",
            OpportunityCategory::MeanReversion => "Mean Reversion",
            OpportunityCategory::BreakoutPatterns => "Breakout Patterns",
            OpportunityCategory::HybridEnhanced => "Enhanced Arbitrage",
            OpportunityCategory::AiRecommended => "AI Recommended",
            OpportunityCategory::BeginnerFriendly => "Beginner Friendly",
            OpportunityCategory::AdvancedStrategies => "Advanced Strategies",
        }
    }

    /// Get the description for the category
    pub fn description(&self) -> &'static str {
        match self {
            OpportunityCategory::LowRiskArbitrage => {
                "Conservative cross-exchange price differences with minimal risk"
            }
            OpportunityCategory::HighConfidenceArbitrage => {
                "High confidence arbitrage opportunities (90%+ accuracy)"
            }
            OpportunityCategory::TechnicalSignals => "Technical analysis based trading signals",
            OpportunityCategory::MomentumTrading => {
                "Opportunities based on price momentum and trends"
            }
            OpportunityCategory::MeanReversion => "Price reversion to mean value strategies",
            OpportunityCategory::BreakoutPatterns => {
                "Price breakout and pattern recognition opportunities"
            }
            OpportunityCategory::HybridEnhanced => {
                "Arbitrage opportunities enhanced with technical analysis"
            }
            OpportunityCategory::AiRecommended => {
                "AI-validated and recommended trading opportunities"
            }
            OpportunityCategory::BeginnerFriendly => {
                "Simple, low-risk opportunities perfect for beginners"
            }
            OpportunityCategory::AdvancedStrategies => {
                "Complex strategies requiring trading experience"
            }
        }
    }

    /// Get the risk assessment for the category
    pub fn risk_assessment(&self) -> RiskLevel {
        match self {
            OpportunityCategory::LowRiskArbitrage => RiskLevel::Low,
            OpportunityCategory::HighConfidenceArbitrage => RiskLevel::Low,
            OpportunityCategory::BeginnerFriendly => RiskLevel::Low,
            OpportunityCategory::TechnicalSignals => RiskLevel::Medium,
            OpportunityCategory::HybridEnhanced => RiskLevel::Medium,
            OpportunityCategory::AiRecommended => RiskLevel::Medium,
            OpportunityCategory::MomentumTrading => RiskLevel::High,
            OpportunityCategory::MeanReversion => RiskLevel::High,
            OpportunityCategory::BreakoutPatterns => RiskLevel::High,
            OpportunityCategory::AdvancedStrategies => RiskLevel::High,
        }
    }

    /// Check if category is suitable for user's experience level
    pub fn is_suitable_for_experience(&self, experience: &ExperienceLevel) -> bool {
        match experience {
            ExperienceLevel::Beginner => {
                matches!(
                    self,
                    OpportunityCategory::LowRiskArbitrage
                        | OpportunityCategory::HighConfidenceArbitrage
                        | OpportunityCategory::BeginnerFriendly
                )
            }
            ExperienceLevel::Intermediate => {
                !matches!(self, OpportunityCategory::AdvancedStrategies)
            }
            ExperienceLevel::Advanced => true, // Advanced users can access all categories
        }
    }
}

/// Risk indicator with detailed assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskIndicator {
    pub risk_level: RiskLevel,
    pub risk_score: f64,               // 0.0 to 100.0
    pub volatility_assessment: String, // "Low", "Medium", "High"
    pub liquidity_risk: String,        // "Low", "Medium", "High"
    pub market_risk: String,           // "Low", "Medium", "High"
    pub execution_risk: String,        // "Low", "Medium", "High"
    pub recommendation: String,        // Risk-based recommendation
    pub warnings: Vec<String>,         // Specific risk warnings
}

impl RiskIndicator {
    pub fn new(risk_level: RiskLevel, confidence_score: f64) -> Self {
        let risk_score = match risk_level {
            RiskLevel::Low => 10.0 + (confidence_score * 20.0),
            RiskLevel::Medium => 30.0 + (confidence_score * 30.0),
            RiskLevel::High => 60.0 + (confidence_score * 40.0),
        };

        let (volatility_assessment, liquidity_risk, market_risk, execution_risk) = match risk_level
        {
            RiskLevel::Low => ("Low", "Low", "Low", "Low"),
            RiskLevel::Medium => ("Medium", "Medium", "Medium", "Medium"),
            RiskLevel::High => ("High", "High", "High", "High"),
        };

        let recommendation = match risk_level {
            RiskLevel::Low => "Suitable for conservative investors. Low risk of capital loss.".to_string(),
            RiskLevel::Medium => "Moderate risk. Suitable for balanced portfolios with some risk tolerance.".to_string(),
            RiskLevel::High => "High risk, high reward. Only suitable for experienced traders with high risk tolerance.".to_string(),
        };

        let mut warnings = Vec::new();
        if risk_score > 70.0 {
            warnings.push("High volatility expected - use position sizing".to_string());
        }
        if risk_score > 80.0 {
            warnings.push("Consider stop-loss orders for risk management".to_string());
        }

        Self {
            risk_level,
            risk_score,
            volatility_assessment: volatility_assessment.to_string(),
            liquidity_risk: liquidity_risk.to_string(),
            market_risk: market_risk.to_string(),
            execution_risk: execution_risk.to_string(),
            recommendation,
            warnings,
        }
    }
}

/// Customizable alert configuration per category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryAlertConfig {
    pub category: OpportunityCategory,
    pub enabled: bool,
    pub min_confidence_threshold: f64,      // 0.0 to 1.0
    pub min_expected_return: f64,           // Minimum expected return percentage
    pub max_risk_level: RiskLevel,          // Maximum acceptable risk level
    pub notification_channels: Vec<String>, // ["telegram", "email", "push"]
    pub priority_level: AlertPriority,      // How urgent these alerts are
    pub cooldown_minutes: u32,              // Minimum time between alerts for this category
    pub max_alerts_per_hour: u32,           // Rate limiting per category
    pub custom_filters: HashMap<String, serde_json::Value>, // User-defined filters
}

impl Default for CategoryAlertConfig {
    fn default() -> Self {
        Self {
            category: OpportunityCategory::BeginnerFriendly,
            enabled: true,
            min_confidence_threshold: 0.7,
            min_expected_return: 1.0,
            max_risk_level: RiskLevel::Medium,
            notification_channels: vec!["telegram".to_string()],
            priority_level: AlertPriority::Medium,
            cooldown_minutes: 15,
            max_alerts_per_hour: 4,
            custom_filters: HashMap::new(),
        }
    }
}

/// Alert priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertPriority {
    #[serde(rename = "low")]
    Low, // No urgency, can wait
    #[serde(rename = "medium")]
    Medium, // Normal priority
    #[serde(rename = "high")]
    High, // Urgent, time-sensitive
    #[serde(rename = "critical")]
    Critical, // Immediate attention required
}

/// Enhanced trading opportunity with categorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorizedOpportunity {
    pub base_opportunity: TradingOpportunity,
    pub categories: Vec<OpportunityCategory>, // Multiple categories can apply
    pub primary_category: OpportunityCategory, // Main category for this opportunity
    pub risk_indicator: RiskIndicator,
    pub user_suitability_score: f64, // 0.0 to 1.0 - how suitable for this user
    pub personalization_factors: Vec<String>, // Why this is suitable/unsuitable
    pub alert_eligible: bool,        // Whether this should trigger alerts
    pub alert_priority: AlertPriority, // Priority if alerting
    pub enhanced_metadata: HashMap<String, serde_json::Value>, // Additional categorization data
    pub categorized_at: u64,         // When categorization was performed
}

/// User's opportunity preferences and alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOpportunityPreferences {
    pub user_id: String,
    pub enabled_categories: Vec<OpportunityCategory>,
    pub alert_configs: Vec<CategoryAlertConfig>,
    pub global_alert_settings: GlobalAlertSettings,
    pub personalization_settings: PersonalizationSettings,
    pub created_at: u64,
    pub updated_at: u64,
}

impl UserOpportunityPreferences {
    /// Create default preferences for a user
    pub fn default_for_user(user_id: String) -> Self {
        Self {
            user_id,
            enabled_categories: vec![
                OpportunityCategory::LowRiskArbitrage,
                OpportunityCategory::HighConfidenceArbitrage,
            ],
            alert_configs: vec![],
            global_alert_settings: GlobalAlertSettings::default(),
            personalization_settings: PersonalizationSettings::default(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }
}

/// Global alert settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalAlertSettings {
    pub alerts_enabled: bool,
    pub quiet_hours_start: String, // "HH:MM" format
    pub quiet_hours_end: String,   // "HH:MM" format
    pub max_total_alerts_per_hour: u32,
    pub max_total_alerts_per_day: u32,
    pub emergency_alerts_only: bool, // Only critical alerts during quiet hours
    pub preferred_timezone: String,
}

impl Default for GlobalAlertSettings {
    fn default() -> Self {
        Self {
            alerts_enabled: true,
            quiet_hours_start: "22:00".to_string(),
            quiet_hours_end: "08:00".to_string(),
            max_total_alerts_per_hour: 12,
            max_total_alerts_per_day: 100,
            emergency_alerts_only: false,
            preferred_timezone: "UTC".to_string(),
        }
    }
}

/// Personalization settings for opportunity filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalizationSettings {
    pub learn_from_interactions: bool, // Learn from user's opportunity interactions
    pub auto_adjust_thresholds: bool,  // Automatically adjust confidence thresholds
    pub preferred_trading_pairs: Vec<String>, // User's preferred trading pairs
    pub excluded_trading_pairs: Vec<String>, // Pairs to exclude
    pub preferred_exchanges: Vec<String>, // Preferred exchanges
    pub excluded_exchanges: Vec<String>, // Exchanges to exclude
    pub max_simultaneous_opportunities: u32, // Max opportunities to show at once
    pub diversity_preference: f64,     // 0.0 to 1.0 - preference for diverse opportunities
}

impl Default for PersonalizationSettings {
    fn default() -> Self {
        Self {
            learn_from_interactions: true,
            auto_adjust_thresholds: false,
            preferred_trading_pairs: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            excluded_trading_pairs: Vec::new(),
            preferred_exchanges: Vec::new(),
            excluded_exchanges: Vec::new(),
            max_simultaneous_opportunities: 10,
            diversity_preference: 0.5,
        }
    }
}

/// Main opportunity categorization service
pub struct OpportunityCategorizationService {
    db_manager: DatabaseManager,
    preferences_service: UserTradingPreferencesService,
    logger: Logger,
    // Cache for user preferences to avoid repeated DB calls (using Arc<Mutex<>> for thread safety)
    user_prefs_cache: Arc<Mutex<HashMap<String, (UserOpportunityPreferences, u64)>>>, // (preferences, cache_time)
}

impl OpportunityCategorizationService {
    pub fn new(
        db_manager: DatabaseManager,
        preferences_service: UserTradingPreferencesService,
        logger: Logger,
    ) -> Self {
        Self {
            db_manager,
            preferences_service,
            logger,
            user_prefs_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Categorize a trading opportunity for a specific user
    pub async fn categorize_opportunity(
        &self,
        opportunity: TradingOpportunity,
        user_id: &str,
    ) -> ArbitrageResult<CategorizedOpportunity> {
        // Get user trading preferences
        let user_prefs = self
            .preferences_service
            .get_or_create_preferences(user_id)
            .await?;
        let user_opp_prefs = self.get_user_opportunity_preferences(user_id).await?;

        // Determine categories for this opportunity
        let categories = self.determine_opportunity_categories(&opportunity);
        let primary_category = self.select_primary_category(&categories, &user_prefs);

        // Create risk indicator
        let risk_indicator =
            RiskIndicator::new(opportunity.risk_level.clone(), opportunity.confidence_score);

        // Calculate user suitability score
        let (suitability_score, personalization_factors) = self.calculate_user_suitability(
            &opportunity,
            &categories,
            &user_prefs,
            &user_opp_prefs,
        );

        // Determine if this should trigger alerts
        let (alert_eligible, alert_priority) =
            self.determine_alert_eligibility(&primary_category, &opportunity, &user_opp_prefs);

        // Generate enhanced metadata
        let enhanced_metadata = self.generate_enhanced_metadata(&opportunity, &categories);

        // Get current timestamp
        #[cfg(target_arch = "wasm32")]
        let now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(CategorizedOpportunity {
            base_opportunity: opportunity,
            categories,
            primary_category,
            risk_indicator,
            user_suitability_score: suitability_score,
            personalization_factors,
            alert_eligible,
            alert_priority,
            enhanced_metadata,
            categorized_at: now,
        })
    }

    /// Filter opportunities based on user preferences and categorization
    pub async fn filter_opportunities_for_user(
        &self,
        opportunities: Vec<TradingOpportunity>,
        user_id: &str,
    ) -> ArbitrageResult<Vec<CategorizedOpportunity>> {
        let mut categorized_opportunities = Vec::new();

        // Categorize each opportunity
        for opportunity in opportunities {
            match self.categorize_opportunity(opportunity, user_id).await {
                Ok(categorized) => {
                    // Only include if it passes user filters
                    if self.passes_user_filters(&categorized, user_id).await? {
                        categorized_opportunities.push(categorized);
                    }
                }
                Err(e) => {
                    self.logger
                        .warn(&format!("Failed to categorize opportunity: {}", e));
                    continue;
                }
            }
        }

        // Sort by suitability score (highest first)
        categorized_opportunities.sort_by(|a, b| {
            b.user_suitability_score
                .partial_cmp(&a.user_suitability_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply user's maximum simultaneous opportunities limit
        let user_opp_prefs = self.get_user_opportunity_preferences(user_id).await?;
        let max_count = user_opp_prefs
            .personalization_settings
            .max_simultaneous_opportunities as usize;
        categorized_opportunities.truncate(max_count);

        Ok(categorized_opportunities)
    }

    /// Get or create user opportunity preferences
    pub async fn get_user_opportunity_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserOpportunityPreferences> {
        // Check cache first and clean up stale entries
        self.evict_stale_cache_entries();

        // Get current timestamp
        #[cfg(target_arch = "wasm32")]
        let now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Check if we have a valid cached entry (cache TTL: 1 hour = 3600000 ms)
        {
            if let Ok(cache) = self.user_prefs_cache.lock() {
                if let Some((cached_prefs, cache_time)) = cache.get(user_id) {
                    if now - cache_time < 3600000 {
                        // 1 hour TTL
                        return Ok(cached_prefs.clone());
                    }
                }
            }
        }

        // Try to load from D1 database first
        let opportunity_prefs = match self
            .db_manager
            .get_user_opportunity_preferences(user_id)
            .await?
        {
            Some(prefs) => {
                // Convert serde_json::Value to UserOpportunityPreferences
                serde_json::from_value::<UserOpportunityPreferences>(prefs).map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to parse user preferences: {}", e))
                })?
            }
            None => {
                // Create default preferences
                let default_prefs =
                    UserOpportunityPreferences::default_for_user(user_id.to_string());

                // Store default preferences in database
                let preferences_value = serde_json::to_value(&default_prefs).map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize preferences: {}", e))
                })?;

                self.db_manager
                    .store_user_opportunity_preferences(&default_prefs.user_id, &preferences_value)
                    .await?;

                default_prefs
            }
        };

        // Store in cache
        {
            if let Ok(mut cache) = self.user_prefs_cache.lock() {
                cache.insert(user_id.to_string(), (opportunity_prefs.clone(), now));
            }
        }

        Ok(opportunity_prefs)
    }

    /// Evict stale entries from the user preferences cache
    /// Removes entries older than 1 hour to prevent unbounded memory growth
    fn evict_stale_cache_entries(&self) {
        #[cfg(target_arch = "wasm32")]
        let now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Remove entries older than 1 hour
        const CACHE_TTL_MS: u64 = 3600000; // 1 hour
        if let Ok(mut cache) = self.user_prefs_cache.lock() {
            let initial_count = cache.len();

            cache.retain(|_user_id, (_prefs, cache_time)| now - *cache_time < CACHE_TTL_MS);

            // Log cache cleanup if significant number of entries were removed
            let remaining_entries = cache.len();
            let evicted_count = initial_count - remaining_entries;

            if evicted_count > 0 {
                self.logger.debug(&format!(
                    "Cache eviction completed. Evicted {} stale entries, {} entries remaining in user preferences cache", 
                    evicted_count, remaining_entries
                ));
            }
        }
    }

    /// Manually clear the user preferences cache (useful for testing or memory management)
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.user_prefs_cache.lock() {
            let cleared_count = cache.len();
            cache.clear();

            if cleared_count > 0 {
                self.logger.info(&format!(
                    "Manually cleared {} entries from user preferences cache",
                    cleared_count
                ));
            }
        }
    }

    /// Update user's opportunity preferences
    pub async fn update_user_opportunity_preferences(
        &self,
        preferences: &UserOpportunityPreferences,
    ) -> ArbitrageResult<()> {
        // Serialize preferences to JSON
        let preferences_value = serde_json::to_value(preferences).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to serialize opportunity preferences: {}",
                e
            ))
        })?;

        // Store in database using D1Service
        self.db_manager
            .store_user_opportunity_preferences(&preferences.user_id, &preferences_value)
            .await?;

        // Update cache with new preferences
        let now = {
            #[cfg(target_arch = "wasm32")]
            {
                js_sys::Date::now() as u64
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            }
        };

        {
            if let Ok(mut cache) = self.user_prefs_cache.lock() {
                cache.insert(preferences.user_id.clone(), (preferences.clone(), now));
            }
        }

        self.logger.info(&format!(
            "Successfully updated and persisted opportunity preferences for user: {}",
            preferences.user_id
        ));

        Ok(())
    }

    /// Add or update alert configuration for a specific category
    pub async fn update_category_alert_config(
        &self,
        user_id: &str,
        config: CategoryAlertConfig,
    ) -> ArbitrageResult<()> {
        let mut user_prefs = self.get_user_opportunity_preferences(user_id).await?;

        // Find and update existing config or add new one
        if let Some(existing_config) = user_prefs
            .alert_configs
            .iter_mut()
            .find(|c| c.category == config.category)
        {
            *existing_config = config;
        } else {
            user_prefs.alert_configs.push(config);
        }

        // Update timestamp
        #[cfg(target_arch = "wasm32")]
        {
            user_prefs.updated_at = js_sys::Date::now() as u64;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            user_prefs.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
        }

        self.update_user_opportunity_preferences(&user_prefs).await
    }

    /// Get category statistics for a user
    pub async fn get_user_category_statistics(
        &self,
        _user_id: &str,
        _days: u32,
    ) -> ArbitrageResult<HashMap<OpportunityCategory, CategoryStatistics>> {
        // TODO: In real implementation, query D1 for historical data
        // For now, return empty statistics
        Ok(HashMap::new())
    }

    // ============= PRIVATE HELPER METHODS =============

    /// Determine which categories apply to an opportunity
    fn determine_opportunity_categories(
        &self,
        opportunity: &TradingOpportunity,
    ) -> Vec<OpportunityCategory> {
        let mut categories = Vec::new();

        // Categorize based on opportunity type
        match opportunity.opportunity_type {
            OpportunityType::Arbitrage => {
                if opportunity.confidence_score >= 0.9 {
                    categories.push(OpportunityCategory::HighConfidenceArbitrage);
                }
                if opportunity.risk_level == RiskLevel::Low {
                    categories.push(OpportunityCategory::LowRiskArbitrage);
                }
                if opportunity.confidence_score >= 0.8 && opportunity.risk_level == RiskLevel::Low {
                    categories.push(OpportunityCategory::BeginnerFriendly);
                }
            }
            OpportunityType::Technical => {
                categories.push(OpportunityCategory::TechnicalSignals);

                // Analyze indicators to determine specific technical category - now O(1) instead of O(n)
                if opportunity
                    .indicators_used
                    .contains(&"momentum".to_string())
                {
                    categories.push(OpportunityCategory::MomentumTrading);
                }
                if opportunity.indicators_used.contains(&"rsi".to_string()) {
                    categories.push(OpportunityCategory::MeanReversion);
                }
                if opportunity
                    .indicators_used
                    .contains(&"breakout".to_string())
                {
                    categories.push(OpportunityCategory::BreakoutPatterns);
                }

                if opportunity.risk_level == RiskLevel::High {
                    categories.push(OpportunityCategory::AdvancedStrategies);
                }
            }
            OpportunityType::ArbitrageTechnical => {
                categories.push(OpportunityCategory::HybridEnhanced);
                if opportunity.confidence_score >= 0.85 {
                    categories.push(OpportunityCategory::AiRecommended);
                }
            }
        }

        // AI recommended category for high-quality opportunities
        if opportunity.confidence_score >= 0.85 && opportunity.risk_level != RiskLevel::High {
            categories.push(OpportunityCategory::AiRecommended);
        }

        categories
    }

    /// Select the primary category from multiple categories
    fn select_primary_category(
        &self,
        categories: &[OpportunityCategory],
        user_prefs: &UserTradingPreferences,
    ) -> OpportunityCategory {
        // Priority based on user's trading focus
        let priority_order = match user_prefs.trading_focus {
            TradingFocus::Arbitrage => vec![
                OpportunityCategory::HighConfidenceArbitrage,
                OpportunityCategory::LowRiskArbitrage,
                OpportunityCategory::HybridEnhanced,
                OpportunityCategory::BeginnerFriendly,
                OpportunityCategory::AiRecommended,
            ],
            TradingFocus::Technical => vec![
                OpportunityCategory::TechnicalSignals,
                OpportunityCategory::MomentumTrading,
                OpportunityCategory::MeanReversion,
                OpportunityCategory::BreakoutPatterns,
                OpportunityCategory::AdvancedStrategies,
                OpportunityCategory::AiRecommended,
            ],
            TradingFocus::Hybrid => vec![
                OpportunityCategory::HybridEnhanced,
                OpportunityCategory::AiRecommended,
                OpportunityCategory::HighConfidenceArbitrage,
                OpportunityCategory::TechnicalSignals,
                OpportunityCategory::LowRiskArbitrage,
                OpportunityCategory::MomentumTrading,
            ],
        };

        // Find the first category in priority order that exists in the opportunity
        for priority_cat in priority_order {
            if categories.contains(&priority_cat) {
                return priority_cat;
            }
        }

        // Default to first category if no priority match
        categories
            .first()
            .cloned()
            .unwrap_or(OpportunityCategory::BeginnerFriendly)
    }

    /// Calculate how suitable this opportunity is for the user
    fn calculate_user_suitability(
        &self,
        opportunity: &TradingOpportunity,
        categories: &[OpportunityCategory],
        user_prefs: &UserTradingPreferences,
        user_opp_prefs: &UserOpportunityPreferences,
    ) -> (f64, Vec<String>) {
        let mut score: f64 = 0.5; // Base score
        let mut factors = Vec::new();

        // Check if opportunity matches user's trading focus
        let focus_match = match user_prefs.trading_focus {
            TradingFocus::Arbitrage => {
                matches!(
                    opportunity.opportunity_type,
                    OpportunityType::Arbitrage | OpportunityType::ArbitrageTechnical
                )
            }
            TradingFocus::Technical => {
                matches!(opportunity.opportunity_type, OpportunityType::Technical)
            }
            TradingFocus::Hybrid => true, // Hybrid users like everything
        };

        if focus_match {
            score += 0.2;
            factors.push("Matches your trading focus".to_string());
        }

        // Check experience level compatibility
        let experience_suitable = categories
            .iter()
            .any(|cat| cat.is_suitable_for_experience(&user_prefs.experience_level));

        if experience_suitable {
            score += 0.15;
            factors.push("Suitable for your experience level".to_string());
        } else {
            score -= 0.15; // Less harsh penalty for experience mismatch
            factors.push("May be too advanced for current experience level".to_string());
        }

        // Risk tolerance alignment
        let risk_aligned = match user_prefs.risk_tolerance {
            RiskTolerance::Conservative => opportunity.risk_level == RiskLevel::Low,
            RiskTolerance::Balanced => opportunity.risk_level != RiskLevel::High,
            RiskTolerance::Aggressive => true, // Aggressive users accept all risk levels
        };

        if risk_aligned {
            score += 0.1;
            factors.push("Aligns with your risk tolerance".to_string());
        } else {
            score -= 0.15;
            factors.push("Risk level may not match your tolerance".to_string());
        }

        // Confidence score factor
        if opportunity.confidence_score >= 0.8 {
            score += 0.1;
            factors.push("High confidence opportunity".to_string());
        }

        // Expected return factor
        if opportunity.expected_return >= 2.0 {
            score += 0.05;
            factors.push("Attractive expected return".to_string());
        }

        // Trading pair preference
        if user_opp_prefs
            .personalization_settings
            .preferred_trading_pairs
            .contains(&opportunity.trading_pair)
        {
            score += 0.1;
            factors.push("Preferred trading pair".to_string());
        }

        // Excluded pair check
        if user_opp_prefs
            .personalization_settings
            .excluded_trading_pairs
            .contains(&opportunity.trading_pair)
        {
            score -= 0.5;
            factors.push("Excluded trading pair".to_string());
        }

        // Clamp score between 0.0 and 1.0
        score = score.clamp(0.0, 1.0);

        (score, factors)
    }

    /// Determine if opportunity should trigger alerts
    fn determine_alert_eligibility(
        &self,
        primary_category: &OpportunityCategory,
        opportunity: &TradingOpportunity,
        user_prefs: &UserOpportunityPreferences,
    ) -> (bool, AlertPriority) {
        // Find alert config for this category
        let alert_config = user_prefs
            .alert_configs
            .iter()
            .find(|config| config.category == *primary_category)
            .cloned()
            .unwrap_or_default();

        // Check if alerts are enabled for this category
        if !alert_config.enabled || !user_prefs.global_alert_settings.alerts_enabled {
            return (false, AlertPriority::Low);
        }

        // Check thresholds
        let meets_confidence =
            opportunity.confidence_score >= alert_config.min_confidence_threshold;
        let meets_return = opportunity.expected_return >= alert_config.min_expected_return;
        let meets_risk = opportunity.risk_level <= alert_config.max_risk_level;

        let eligible = meets_confidence && meets_return && meets_risk;

        // Determine priority
        let priority = if opportunity.confidence_score >= 0.95 && opportunity.expected_return >= 5.0
        {
            AlertPriority::Critical
        } else if opportunity.confidence_score >= 0.85 && opportunity.expected_return >= 3.0 {
            AlertPriority::High
        } else {
            alert_config.priority_level
        };

        (eligible, priority)
    }

    /// Check if opportunity passes all user filters
    async fn passes_user_filters(
        &self,
        categorized_opp: &CategorizedOpportunity,
        user_id: &str,
    ) -> ArbitrageResult<bool> {
        let user_opp_prefs = self.get_user_opportunity_preferences(user_id).await?;

        // Check if any of the opportunity's categories are enabled by the user
        let category_enabled = categorized_opp
            .categories
            .iter()
            .any(|cat| user_opp_prefs.enabled_categories.contains(cat));

        if !category_enabled {
            return Ok(false);
        }

        // Check excluded exchanges
        let excluded_exchange = categorized_opp
            .base_opportunity
            .exchanges
            .iter()
            .any(|exchange| {
                user_opp_prefs
                    .personalization_settings
                    .excluded_exchanges
                    .contains(exchange)
            });

        if excluded_exchange {
            return Ok(false);
        }

        // Check excluded trading pairs
        if user_opp_prefs
            .personalization_settings
            .excluded_trading_pairs
            .contains(&categorized_opp.base_opportunity.trading_pair)
        {
            return Ok(false);
        }

        // Check minimum suitability score
        if categorized_opp.user_suitability_score < 0.3 {
            return Ok(false);
        }

        Ok(true)
    }

    /// Generate enhanced metadata for the opportunity
    fn generate_enhanced_metadata(
        &self,
        opportunity: &TradingOpportunity,
        categories: &[OpportunityCategory],
    ) -> HashMap<String, serde_json::Value> {
        let mut metadata = HashMap::new();

        // Add category information
        let category_names: Vec<String> = categories
            .iter()
            .map(|cat| cat.display_name().to_string())
            .collect();
        metadata.insert(
            "category_names".to_string(),
            serde_json::json!(category_names),
        );

        // Add risk assessment details
        metadata.insert(
            "risk_factors".to_string(),
            serde_json::json!({
                "volatility": match opportunity.risk_level {
                    RiskLevel::Low => "Low volatility expected",
                    RiskLevel::Medium => "Moderate volatility possible",
                    RiskLevel::High => "High volatility likely"
                },
                "time_sensitivity": match opportunity.time_horizon {
                    TimeHorizon::Immediate => "Immediate action required",
                    TimeHorizon::Short => "Short-term opportunity",
                    TimeHorizon::Medium => "Medium-term trade",
                    TimeHorizon::Long => "Long-term investment"
                }
            }),
        );

        // Add trading recommendations
        metadata.insert(
            "recommendations".to_string(),
            serde_json::json!({
                "position_sizing": match opportunity.risk_level {
                    RiskLevel::Low => "Standard position size acceptable",
                    RiskLevel::Medium => "Consider reduced position size",
                    RiskLevel::High => "Use small position size and tight stops"
                },
                "entry_strategy": "Consider market conditions before entry",
                "exit_strategy": if opportunity.stop_loss.is_some() {
                    "Stop loss and take profit levels provided"
                } else {
                    "Set appropriate stop loss and take profit levels"
                }
            }),
        );

        metadata
    }
}

/// Statistics for opportunity categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStatistics {
    pub category: OpportunityCategory,
    pub total_opportunities: u32,
    pub accepted_opportunities: u32,
    pub declined_opportunities: u32,
    pub average_confidence: f64,
    pub average_return: f64,
    pub success_rate: f64, // Percentage of profitable trades
    pub total_pnl: f64,    // Total profit/loss for this category
    pub last_updated: u64,
}

// ============= TESTS =============

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::logger::{LogLevel, Logger};

    #[allow(dead_code)]
    fn create_test_service() -> OpportunityCategorizationService {
        let _logger = Logger::new(LogLevel::Info);
        // Note: In real tests, we'd use mock D1Service and UserTradingPreferencesService
        // For unit testing, we focus on testing individual methods that don't require DB
        // OpportunityCategorizationService::new(mock_d1_service, mock_preferences_service, logger)

        // For now, we test the methods that don't require service instantiation
        // This avoids the need for complex mock setup in unit tests
        panic!(
            "This helper is for integration tests only - use direct method testing for unit tests"
        )
    }

    #[test]
    fn test_opportunity_category_display_names() {
        assert_eq!(
            OpportunityCategory::LowRiskArbitrage.display_name(),
            "Low Risk Arbitrage"
        );
        assert_eq!(
            OpportunityCategory::TechnicalSignals.display_name(),
            "Technical Signals"
        );
        assert_eq!(
            OpportunityCategory::BeginnerFriendly.display_name(),
            "Beginner Friendly"
        );
    }

    #[test]
    fn test_category_risk_assessment() {
        assert_eq!(
            OpportunityCategory::LowRiskArbitrage.risk_assessment(),
            RiskLevel::Low
        );
        assert_eq!(
            OpportunityCategory::MomentumTrading.risk_assessment(),
            RiskLevel::High
        );
        assert_eq!(
            OpportunityCategory::TechnicalSignals.risk_assessment(),
            RiskLevel::Medium
        );
    }

    #[test]
    fn test_experience_level_suitability() {
        let beginner_suitable = OpportunityCategory::BeginnerFriendly;
        let advanced_only = OpportunityCategory::AdvancedStrategies;

        assert!(beginner_suitable.is_suitable_for_experience(&ExperienceLevel::Beginner));
        assert!(!advanced_only.is_suitable_for_experience(&ExperienceLevel::Beginner));
        assert!(advanced_only.is_suitable_for_experience(&ExperienceLevel::Advanced));
    }

    #[test]
    fn test_risk_indicator_creation() {
        let risk_indicator = RiskIndicator::new(RiskLevel::Low, 0.9);
        assert_eq!(risk_indicator.risk_level, RiskLevel::Low);
        assert!(risk_indicator.risk_score < 50.0); // Low risk should have low score
        assert_eq!(risk_indicator.volatility_assessment, "Low");
    }

    #[test]
    fn test_alert_priority_levels() {
        let low_priority = AlertPriority::Low;
        let critical_priority = AlertPriority::Critical;
        assert_ne!(low_priority, critical_priority);
    }

    #[test]
    fn test_default_alert_config() {
        let default_config = CategoryAlertConfig::default();
        assert!(default_config.enabled);
        assert!(default_config.min_confidence_threshold > 0.0);
        assert!(default_config.cooldown_minutes > 0);
    }

    #[test]
    fn test_global_alert_settings_default() {
        let settings = GlobalAlertSettings::default();
        assert!(settings.alerts_enabled);
        assert_eq!(settings.quiet_hours_start, "22:00");
        assert_eq!(settings.quiet_hours_end, "08:00");
        assert!(settings.max_total_alerts_per_hour > 0);
    }

    #[test]
    fn test_personalization_settings_default() {
        let settings = PersonalizationSettings::default();
        assert!(settings.learn_from_interactions);
        assert!(!settings.preferred_trading_pairs.is_empty());
        assert!(settings.max_simultaneous_opportunities > 0);
        assert!(settings.diversity_preference >= 0.0 && settings.diversity_preference <= 1.0);
    }
}
