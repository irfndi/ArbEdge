// User Trading Preferences Service
// Task 1.5: Trading focus selection and automation preferences management

use crate::services::core::infrastructure::DatabaseManager;
use crate::utils::{logger::Logger, ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
// use worker::*; // TODO: Re-enable when implementing worker functionality

/// Required onboarding steps that users must complete
const REQUIRED_ONBOARDING_STEPS: &[&str] = &[
    "trading_focus_selected",
    "automation_configured",
    "exchanges_connected",
];

// ============= TRADING PREFERENCE TYPES =============

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum TradingFocus {
    #[serde(rename = "arbitrage")]
    #[default]
    Arbitrage, // Default - focus on arbitrage opportunities (low risk)
    #[serde(rename = "technical")]
    Technical, // Focus on technical analysis trading (higher risk)
    #[serde(rename = "hybrid")]
    Hybrid, // Both arbitrage and technical (experienced users)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum AutomationLevel {
    #[serde(rename = "manual")]
    #[default]
    Manual, // Alerts only, user executes manually (default)
    #[serde(rename = "semi_auto")]
    SemiAuto, // Pre-approval required for each trade
    #[serde(rename = "full_auto")]
    FullAuto, // Automated execution based on rules
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum AutomationScope {
    #[serde(rename = "none")]
    #[default]
    None, // No automation (manual only)
    #[serde(rename = "arbitrage_only")]
    ArbitrageOnly, // Automate arbitrage trades only
    #[serde(rename = "technical_only")]
    TechnicalOnly, // Automate technical trades only
    #[serde(rename = "both")]
    Both, // Automate both types
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ExperienceLevel {
    #[serde(rename = "beginner")]
    #[default]
    Beginner, // New to trading, conservative approach
    #[serde(rename = "intermediate")]
    Intermediate, // Some trading experience
    #[serde(rename = "advanced")]
    Advanced, // Experienced trader
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum RiskTolerance {
    #[serde(rename = "conservative")]
    #[default]
    Conservative, // Low risk, capital preservation focused
    #[serde(rename = "balanced")]
    Balanced, // Moderate risk tolerance
    #[serde(rename = "aggressive")]
    Aggressive, // Higher risk tolerance
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTradingPreferences {
    pub preference_id: String,
    pub user_id: String,

    // Trading Focus Selection
    pub trading_focus: TradingFocus,
    pub experience_level: ExperienceLevel,
    pub risk_tolerance: RiskTolerance,

    // Automation Preferences
    pub automation_level: AutomationLevel,
    pub automation_scope: AutomationScope,

    // Feature Access Control
    pub arbitrage_enabled: bool,
    pub technical_enabled: bool,
    pub advanced_analytics_enabled: bool,

    // User Preferences
    pub preferred_notification_channels: Vec<String>, // ["telegram", "email", "push"]
    pub trading_hours_timezone: String,
    pub trading_hours_start: String, // "HH:MM" format
    pub trading_hours_end: String,   // "HH:MM" format

    // Onboarding Progress
    pub onboarding_completed: bool,
    pub tutorial_steps_completed: Vec<String>,

    // Timestamps
    pub created_at: u64,
    pub updated_at: u64,
}

impl UserTradingPreferences {
    pub fn new_default(user_id: String) -> Self {
        let preference_id = format!("pref_{}", user_id);

        // Use different timestamp generation for WASM vs native
        #[cfg(target_arch = "wasm32")]
        let now = js_sys::Date::now() as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            preference_id,
            user_id,
            trading_focus: TradingFocus::default(),
            experience_level: ExperienceLevel::default(),
            risk_tolerance: RiskTolerance::default(),
            automation_level: AutomationLevel::default(),
            automation_scope: AutomationScope::default(),
            arbitrage_enabled: true,           // Default access to arbitrage
            technical_enabled: false,          // Opt-in to technical trading
            advanced_analytics_enabled: false, // Opt-in to advanced features
            preferred_notification_channels: vec!["telegram".to_string()],
            trading_hours_timezone: "UTC".to_string(),
            trading_hours_start: "00:00".to_string(),
            trading_hours_end: "23:59".to_string(),
            onboarding_completed: false,
            tutorial_steps_completed: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn enable_technical_trading(&mut self) -> ArbitrageResult<()> {
        // Validate that user is not beginner for technical trading
        if self.experience_level == ExperienceLevel::Beginner {
            return Err(ArbitrageError::validation_error(
                "Technical trading requires intermediate or advanced experience level. Please update your experience level in your profile settings first, or choose 'Arbitrage' focus to continue with safer trading options."
            ));
        }

        self.technical_enabled = true;

        // Update timestamp
        #[cfg(target_arch = "wasm32")]
        {
            self.updated_at = js_sys::Date::now() as u64;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
        }

        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub fn set_automation_level(
        &mut self,
        level: AutomationLevel,
        scope: AutomationScope,
    ) -> ArbitrageResult<()> {
        // Validate automation access based on experience
        match level {
            AutomationLevel::Manual => {
                // Always allowed
            }
            AutomationLevel::SemiAuto => {
                if self.experience_level == ExperienceLevel::Beginner {
                    return Err(ArbitrageError::validation_error(
                        "Semi-automated trading requires intermediate or advanced experience",
                    ));
                }
            }
            AutomationLevel::FullAuto => {
                if self.experience_level != ExperienceLevel::Advanced {
                    return Err(ArbitrageError::validation_error(
                        "Full automation requires advanced experience level",
                    ));
                }
            }
        }

        self.automation_level = level;
        self.automation_scope = scope;

        // Update timestamp
        #[cfg(target_arch = "wasm32")]
        {
            self.updated_at = js_sys::Date::now() as u64;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
        }

        Ok(())
    }
}

// ============= FEATURE ACCESS CONTROL =============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureAccess {
    pub arbitrage_alerts: bool,
    pub technical_alerts: bool,
    pub arbitrage_automation: bool,
    pub technical_automation: bool,
    pub advanced_analytics: bool,
    pub priority_notifications: bool,
    pub ai_integration: bool,
    pub custom_indicators: bool,
}

impl FeatureAccess {
    pub fn from_preferences(preferences: &UserTradingPreferences) -> Self {
        Self {
            arbitrage_alerts: preferences.arbitrage_enabled,
            technical_alerts: preferences.technical_enabled,
            arbitrage_automation: preferences.automation_level != AutomationLevel::Manual
                && (preferences.automation_scope == AutomationScope::ArbitrageOnly
                    || preferences.automation_scope == AutomationScope::Both),
            technical_automation: preferences.automation_level != AutomationLevel::Manual
                && (preferences.automation_scope == AutomationScope::TechnicalOnly
                    || preferences.automation_scope == AutomationScope::Both)
                && preferences.technical_enabled,
            advanced_analytics: preferences.advanced_analytics_enabled,
            priority_notifications: preferences.experience_level != ExperienceLevel::Beginner,
            ai_integration: preferences.experience_level != ExperienceLevel::Beginner,
            custom_indicators: preferences.technical_enabled
                && preferences.experience_level != ExperienceLevel::Beginner,
        }
    }
}

// ============= PREFERENCE VALIDATION =============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceValidationResult {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub recommendations: Vec<String>,
}

impl PreferenceValidationResult {
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            warnings: vec![],
            errors: vec![],
            recommendations: vec![],
        }
    }

    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            is_valid: false,
            warnings: vec![],
            errors,
            recommendations: vec![],
        }
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn add_recommendation(&mut self, recommendation: String) {
        self.recommendations.push(recommendation);
    }
}

// ============= USER TRADING PREFERENCES SERVICE =============

pub struct UserTradingPreferencesService {
    d1_service: DatabaseManager,
    logger: Logger,
    // Simple in-memory cache with TTL of 5 minutes for frequently accessed preferences
    cache: Arc<Mutex<HashMap<String, (UserTradingPreferences, u64)>>>, // (prefs, timestamp)
}

impl UserTradingPreferencesService {
    pub fn new(d1_service: DatabaseManager, logger: Logger) -> Self {
        Self {
            d1_service,
            logger,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create default trading preferences for a new user
    pub async fn create_default_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserTradingPreferences> {
        let preferences = UserTradingPreferences::new_default(user_id.to_string());

        self.logger.info(&format!(
            "Creating default trading preferences for user: {}",
            user_id
        ));

        self.d1_service
            .store_trading_preferences(&preferences)
            .await?;

        Ok(preferences)
    }

    /// Get user trading preferences
    pub async fn get_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<UserTradingPreferences>> {
        self.d1_service.get_trading_preferences(user_id).await
    }

    /// Get preferences or create defaults if none exist (with caching, race condition safe)
    pub async fn get_or_create_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserTradingPreferences> {
        // Check cache first (TTL: 5 minutes = 300,000ms)
        if let Some(cached_prefs) = self.get_from_cache(user_id, 300_000) {
            return Ok(cached_prefs);
        }

        // Use atomic D1 operation to handle race conditions
        let preferences = self
            .d1_service
            .get_or_create_trading_preferences(user_id)
            .await?;

        // Cache the result
        self.cache_preferences(user_id, &preferences);

        Ok(preferences)
    }

    /// Get preferences from cache if valid (not expired)
    fn get_from_cache(&self, user_id: &str, ttl_ms: u64) -> Option<UserTradingPreferences> {
        if let Ok(cache) = self.cache.lock() {
            if let Some((prefs, timestamp)) = cache.get(user_id) {
                let now = self.get_current_timestamp();
                if now.saturating_sub(*timestamp) <= ttl_ms {
                    return Some(prefs.clone());
                }
            }
        }
        None
    }

    /// Cache preferences with current timestamp
    fn cache_preferences(&self, user_id: &str, preferences: &UserTradingPreferences) {
        if let Ok(mut cache) = self.cache.lock() {
            let now = self.get_current_timestamp();
            cache.insert(user_id.to_string(), (preferences.clone(), now));

            // Simple cache eviction: remove entries older than 10 minutes
            let eviction_threshold = now.saturating_sub(600_000); // 10 minutes
            cache.retain(|_, (_, timestamp)| *timestamp > eviction_threshold);
        }
    }

    /// Invalidate cache entry for user
    fn invalidate_cache(&self, user_id: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(user_id);
        }
    }

    /// Get current timestamp in milliseconds
    fn get_current_timestamp(&self) -> u64 {
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
    }

    /// Update user trading preferences
    pub async fn update_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<()> {
        // Validate preferences before updating
        let validation = self.validate_preferences(preferences);
        if !validation.is_valid {
            return Err(ArbitrageError::validation_error(format!(
                "Invalid preferences: {:?}",
                validation.errors
            )));
        }

        self.logger.info(&format!(
            "Updating trading preferences for user: {}",
            preferences.user_id
        ));

        let result = self
            .d1_service
            .update_trading_preferences(preferences)
            .await;

        // Invalidate cache after successful update
        if result.is_ok() {
            self.invalidate_cache(&preferences.user_id);
        }

        result
    }

    /// Delete user's trading preferences
    pub async fn delete_preferences(&self, user_id: &str) -> ArbitrageResult<bool> {
        self.logger.info(&format!(
            "Deleting trading preferences for user: {}",
            user_id
        ));

        // Delete from D1 (persistent storage)
        self.d1_service.delete_trading_preferences(user_id).await?;

        // Invalidate in-memory cache
        self.invalidate_cache(user_id);

        // Return true if deletion was successful (we assume it was since D1 didn't error)
        Ok(true)
    }

    /// Update trading focus
    pub async fn update_trading_focus(
        &self,
        user_id: &str,
        focus: TradingFocus,
    ) -> ArbitrageResult<UserTradingPreferences> {
        let mut preferences = self.get_or_create_preferences(user_id).await?;

        preferences.trading_focus = focus.clone();

        // Auto-enable technical trading if choosing technical or hybrid focus
        match focus {
            TradingFocus::Technical | TradingFocus::Hybrid => {
                if preferences.enable_technical_trading().is_err() {
                    // If they can't enable technical trading, suggest upgrading experience level
                    self.logger.warn(&format!(
                        "User {} attempted to enable technical trading but is beginner level",
                        user_id
                    ));
                    return Err(ArbitrageError::validation_error(
                        "Technical trading requires intermediate or advanced experience level. Please update your experience level first."
                    ));
                }
            }
            TradingFocus::Arbitrage => {
                // Keep current technical_enabled setting
            }
        }

        // Update timestamp
        #[cfg(target_arch = "wasm32")]
        {
            preferences.updated_at = js_sys::Date::now() as u64;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            preferences.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
        }

        self.update_preferences(&preferences).await?;

        Ok(preferences)
    }

    /// Update automation preferences
    pub async fn update_automation(
        &self,
        user_id: &str,
        level: AutomationLevel,
        scope: AutomationScope,
    ) -> ArbitrageResult<UserTradingPreferences> {
        let mut preferences = self.get_or_create_preferences(user_id).await?;

        preferences.set_automation_level(level, scope)?;

        // Update timestamp
        #[cfg(target_arch = "wasm32")]
        {
            preferences.updated_at = js_sys::Date::now() as u64;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            preferences.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
        }

        self.update_preferences(&preferences).await?;

        Ok(preferences)
    }

    /// Get feature access for user
    pub async fn get_feature_access(&self, user_id: &str) -> ArbitrageResult<FeatureAccess> {
        let preferences = self.get_or_create_preferences(user_id).await?;
        Ok(FeatureAccess::from_preferences(&preferences))
    }

    /// Validate preferences
    pub fn validate_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> PreferenceValidationResult {
        let mut result = PreferenceValidationResult::valid();

        // Validate automation level vs experience
        match preferences.automation_level {
            AutomationLevel::SemiAuto
                if preferences.experience_level == ExperienceLevel::Beginner =>
            {
                result.errors.push(
                    "Semi-automated trading requires intermediate or advanced experience"
                        .to_string(),
                );
                result.is_valid = false;
            }
            AutomationLevel::FullAuto
                if preferences.experience_level != ExperienceLevel::Advanced =>
            {
                result
                    .errors
                    .push("Full automation requires advanced experience level".to_string());
                result.is_valid = false;
            }
            _ => {}
        }

        // Validate technical trading vs experience
        if preferences.technical_enabled
            && preferences.experience_level == ExperienceLevel::Beginner
        {
            result.errors.push("Technical trading requires intermediate or advanced experience. Consider updating your experience level or choosing arbitrage focus for safer trading.".to_string());
            result.is_valid = false;
        }

        // Add recommendations
        if preferences.trading_focus == TradingFocus::Arbitrage
            && preferences.experience_level == ExperienceLevel::Advanced
        {
            result.add_recommendation("Consider exploring technical trading or hybrid approach for additional opportunities".to_string());
        }

        if preferences.automation_level == AutomationLevel::Manual
            && preferences.experience_level == ExperienceLevel::Advanced
        {
            result.add_recommendation(
                "Consider semi-automated or automated execution to optimize trading efficiency"
                    .to_string(),
            );
        }

        result
    }

    /// Complete onboarding step
    pub async fn complete_onboarding_step(
        &self,
        user_id: &str,
        step: &str,
    ) -> ArbitrageResult<UserTradingPreferences> {
        let mut preferences = self.get_or_create_preferences(user_id).await?;

        if !preferences
            .tutorial_steps_completed
            .contains(&step.to_string())
        {
            preferences.tutorial_steps_completed.push(step.to_string());
        }

        // Check if onboarding is complete
        let completed_required = REQUIRED_ONBOARDING_STEPS.iter().all(|step| {
            preferences
                .tutorial_steps_completed
                .contains(&step.to_string())
        });

        if completed_required && !preferences.onboarding_completed {
            preferences.onboarding_completed = true;
            self.logger
                .info(&format!("User {} completed onboarding", user_id));
        }

        // Update timestamp
        #[cfg(target_arch = "wasm32")]
        {
            preferences.updated_at = js_sys::Date::now() as u64;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            preferences.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
        }

        self.update_preferences(&preferences).await?;

        Ok(preferences)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_preferences() {
        let preferences = UserTradingPreferences::new_default("test_user".to_string());

        assert_eq!(preferences.trading_focus, TradingFocus::Arbitrage);
        assert_eq!(preferences.automation_level, AutomationLevel::Manual);
        assert_eq!(preferences.automation_scope, AutomationScope::None);
        assert_eq!(preferences.experience_level, ExperienceLevel::Beginner);
        assert_eq!(preferences.risk_tolerance, RiskTolerance::Conservative);
        assert!(preferences.arbitrage_enabled);
        assert!(!preferences.technical_enabled);
        assert!(!preferences.advanced_analytics_enabled);
        assert!(!preferences.onboarding_completed);
    }

    #[test]
    fn test_enable_technical_trading() {
        let mut preferences = UserTradingPreferences::new_default("test_user".to_string());

        // Should fail for beginners
        assert!(preferences.enable_technical_trading().is_err());

        // Should work for intermediate users
        preferences.experience_level = ExperienceLevel::Intermediate;
        assert!(preferences.enable_technical_trading().is_ok());
        assert!(preferences.technical_enabled);
    }

    #[test]
    fn test_automation_level_validation() {
        let mut preferences = UserTradingPreferences::new_default("test_user".to_string());

        // Manual should always work
        assert!(preferences
            .set_automation_level(AutomationLevel::Manual, AutomationScope::None)
            .is_ok());

        // Semi-auto should fail for beginners
        assert!(preferences
            .set_automation_level(AutomationLevel::SemiAuto, AutomationScope::ArbitrageOnly)
            .is_err());

        // Should work for intermediate
        preferences.experience_level = ExperienceLevel::Intermediate;
        assert!(preferences
            .set_automation_level(AutomationLevel::SemiAuto, AutomationScope::ArbitrageOnly)
            .is_ok());

        // Full auto should fail for intermediate
        assert!(preferences
            .set_automation_level(AutomationLevel::FullAuto, AutomationScope::Both)
            .is_err());

        // Should work for advanced
        preferences.experience_level = ExperienceLevel::Advanced;
        assert!(preferences
            .set_automation_level(AutomationLevel::FullAuto, AutomationScope::Both)
            .is_ok());
    }

    #[test]
    fn test_feature_access() {
        let mut preferences = UserTradingPreferences::new_default("test_user".to_string());
        preferences.experience_level = ExperienceLevel::Intermediate;
        preferences.technical_enabled = true;
        preferences.automation_level = AutomationLevel::SemiAuto;
        preferences.automation_scope = AutomationScope::ArbitrageOnly;

        let access = FeatureAccess::from_preferences(&preferences);

        assert!(access.arbitrage_alerts);
        assert!(access.technical_alerts);
        assert!(access.arbitrage_automation);
        assert!(!access.technical_automation); // Technical not in scope
        assert!(access.ai_integration); // Intermediate level
        assert!(access.custom_indicators); // Technical enabled + not beginner
    }

    // TODO: Implement integration test for preference validation with proper test environment
}
