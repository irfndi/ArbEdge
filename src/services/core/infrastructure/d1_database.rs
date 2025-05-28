use crate::services::core::user::user_trading_preferences::UserTradingPreferences;
use crate::types::{
    Balance, InvitationCode, TradingAnalytics, UserApiKey, UserInvitation, UserProfile,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use worker::{D1Database, Env, Result};

/// Invitation usage record for beta tracking
#[derive(Debug, Clone)]
pub struct InvitationUsage {
    pub invitation_id: String,
    pub user_id: String,
    pub telegram_id: i64,
    pub used_at: DateTime<Utc>,
    pub beta_expires_at: DateTime<Utc>,
}

use worker::wasm_bindgen::JsValue;

// Type aliases for better readability
type UserOpportunityPreferences =
    crate::services::core::opportunities::opportunity_categorization::UserOpportunityPreferences;
type BalanceHistoryEntry =
    crate::services::core::infrastructure::fund_monitoring::BalanceHistoryEntry;
type DynamicConfigTemplate = crate::services::core::user::dynamic_config::DynamicConfigTemplate;
type ConfigPreset = crate::services::core::user::dynamic_config::ConfigPreset;
type UserConfigInstance = crate::services::core::user::dynamic_config::UserConfigInstance;
type NotificationTemplate =
    crate::services::core::infrastructure::notifications::NotificationTemplate;
type AlertTrigger = crate::services::core::infrastructure::notifications::AlertTrigger;
type Notification = crate::services::core::infrastructure::notifications::Notification;
type NotificationHistory =
    crate::services::core::infrastructure::notifications::NotificationHistory;
type AiOpportunityEnhancement =
    crate::services::core::ai::ai_intelligence::AiOpportunityEnhancement;
type AiPortfolioAnalysis = crate::services::core::ai::ai_intelligence::AiPortfolioAnalysis;
type AiPerformanceInsights = crate::services::core::ai::ai_intelligence::AiPerformanceInsights;
type ParameterSuggestion = crate::services::core::ai::ai_intelligence::ParameterSuggestion;
type AiOpportunityAnalysis =
    crate::services::core::trading::ai_exchange_router::AiOpportunityAnalysis;

/// D1Service provides database operations using Cloudflare D1 SQL database
/// This service handles persistent storage for user profiles, invitations, analytics, orders, opporunities, etc.
#[derive(Clone)]
pub struct D1Service {
    db: Arc<D1Database>,
}

impl D1Service {
    /// Create a new D1Service instance
    pub fn new(env: &Env) -> Result<Self> {
        let db = env.d1("ArbEdgeD1")?;
        Ok(D1Service { db: Arc::new(db) })
    }

    /// Get a reference to the underlying D1 database
    /// WARNING: This method exposes raw database access and should only be used internally
    /// Consider using high-level methods instead of direct database access
    pub(crate) fn database(&self) -> &D1Database {
        &self.db
    }

    // ============= USER PROFILE OPERATIONS =============

    /// Create a new user profile in D1 database
    pub async fn create_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
        self.store_user_profile(profile).await
    }

    /// Update a user profile in D1 database
    pub async fn update_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
        self.store_user_profile(profile).await
    }

    /// Store a user profile in D1 database
    pub async fn store_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
        // Serialize complex fields to JSON
        let api_keys_json = serde_json::to_string(&profile.api_keys).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize API keys: {}", e))
        })?;

        let subscription_json = serde_json::to_string(&profile.subscription).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize subscription: {}", e))
        })?;

        let configuration_json = serde_json::to_string(&profile.configuration).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize configuration: {}", e))
        })?;

        // Prepare INSERT OR REPLACE statement
        let stmt = self.db.prepare(
            "
            INSERT OR REPLACE INTO user_profiles (
                user_id, telegram_id, username, api_keys, 
                subscription_tier, trading_preferences, 
                created_at, updated_at, last_login_at, account_status, 
                beta_expires_at, account_balance_usdt
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        // Bind parameters and execute
        stmt.bind(&[
            profile.user_id.clone().into(),
            JsValue::from_f64(profile.telegram_user_id.unwrap_or(0) as f64),
            profile.telegram_username.clone().unwrap_or_default().into(),
            api_keys_json.into(),
            subscription_json.into(),
            configuration_json.into(),
            (profile.created_at as i64).into(),
            (profile.updated_at as i64).into(),
            (profile.last_active as i64).into(),
            if profile.is_active {
                "active"
            } else {
                "deactivated"
            }
            .into(),
            profile
                .beta_expires_at
                .map(|t| t as i64)
                .unwrap_or(0)
                .into(),
            profile.account_balance_usdt.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Retrieve a user profile by user ID
    pub async fn get_user_profile(&self, user_id: &str) -> ArbitrageResult<Option<UserProfile>> {
        let stmt = self
            .db
            .prepare("SELECT * FROM user_profiles WHERE user_id = ?");

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let profile = self.row_to_user_profile(row)?;
                Ok(Some(profile))
            }
            None => Ok(None),
        }
    }

    /// Retrieve a user profile by Telegram ID
    pub async fn get_user_by_telegram_id(
        &self,
        telegram_user_id: i64,
    ) -> ArbitrageResult<Option<UserProfile>> {
        let stmt = self
            .db
            .prepare("SELECT * FROM user_profiles WHERE telegram_id = ?");

        let result = stmt
            .bind(&[JsValue::from_f64(telegram_user_id as f64)])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let profile = self.row_to_user_profile(row)?;
                Ok(Some(profile))
            }
            None => Ok(None),
        }
    }

    /// List user profiles with pagination
    pub async fn list_user_profiles(
        &self,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ArbitrageResult<Vec<UserProfile>> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        let stmt = self.db.prepare(
            "
            SELECT * FROM user_profiles 
            ORDER BY created_at DESC 
            LIMIT ? OFFSET ?
        ",
        );

        let result = stmt
            .bind(&[limit.into(), offset.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let mut profiles = Vec::new();
        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        for row in results {
            profiles.push(self.row_to_user_profile(row)?);
        }

        Ok(profiles)
    }

    /// Delete a user profile
    pub async fn delete_user_profile(&self, user_id: &str) -> ArbitrageResult<bool> {
        let stmt = self
            .db
            .prepare("DELETE FROM user_profiles WHERE user_id = ?");

        let _result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .run()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        // For now, just return true if no error occurred
        // In a production system, we'd need to check if rows were actually affected
        Ok(true)
    }

    /// Delete a trading opportunity by opportunity ID
    pub async fn delete_trading_opportunity(&self, opportunity_id: &str) -> ArbitrageResult<bool> {
        // Check if trading_opportunities table exists first
        let table_check = self
            .db
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='trading_opportunities'")
            .first::<HashMap<String, Value>>(None)
            .await;

        match table_check {
            Ok(Some(_)) => {
                // Table exists, proceed with deletion
                let stmt = self
                    .db
                    .prepare("DELETE FROM trading_opportunities WHERE opportunity_id = ?");

                let _result = stmt
                    .bind(&[opportunity_id.into()])
                    .map_err(|e| {
                        ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
                    })?
                    .run()
                    .await
                    .map_err(|e| {
                        ArbitrageError::database_error(format!("Failed to execute deletion: {}", e))
                    })?;

                // Check if any rows were affected
                // Note: Worker D1Result doesn't expose changes() method directly
                // For now, we'll assume success if no error occurred
                // In a production system, we'd need to check meta() for affected rows
                Ok(true)
            }
            Ok(None) => {
                // Table doesn't exist - this is acceptable in test environment
                // Return false to indicate no deletion occurred
                Ok(false)
            }
            Err(e) => {
                // Database error checking table existence
                Err(ArbitrageError::database_error(format!(
                    "Failed to check table existence: {}",
                    e
                )))
            }
        }
    }

    // ============= USER TRADING PREFERENCES OPERATIONS =============

    /// Store user trading preferences in D1 database
    pub async fn store_trading_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<()> {
        let trading_focus_str = serde_json::to_string(&preferences.trading_focus).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize trading focus: {}", e))
        })?;

        let experience_level_str =
            serde_json::to_string(&preferences.experience_level).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize experience level: {}", e))
            })?;

        let risk_tolerance_str =
            serde_json::to_string(&preferences.risk_tolerance).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize risk tolerance: {}", e))
            })?;

        let automation_level_str =
            serde_json::to_string(&preferences.automation_level).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize automation level: {}", e))
            })?;

        let automation_scope_str =
            serde_json::to_string(&preferences.automation_scope).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize automation scope: {}", e))
            })?;

        let notification_channels_json =
            serde_json::to_string(&preferences.preferred_notification_channels).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to serialize notification channels: {}",
                    e
                ))
            })?;

        let tutorial_steps_json = serde_json::to_string(&preferences.tutorial_steps_completed)
            .map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize tutorial steps: {}", e))
            })?;

        let stmt = self.db.prepare(
            "
            INSERT OR REPLACE INTO user_trading_preferences (
                preference_id, user_id, trading_focus, experience_level, risk_tolerance,
                automation_level, automation_scope, arbitrage_enabled, technical_enabled,
                advanced_analytics_enabled, preferred_notification_channels,
                trading_hours_timezone, trading_hours_start, trading_hours_end,
                onboarding_completed, tutorial_steps_completed, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            preferences.preference_id.clone().into(),
            preferences.user_id.clone().into(),
            trading_focus_str.into(),
            experience_level_str.into(),
            risk_tolerance_str.into(),
            automation_level_str.into(),
            automation_scope_str.into(),
            preferences.arbitrage_enabled.into(),
            preferences.technical_enabled.into(),
            preferences.advanced_analytics_enabled.into(),
            notification_channels_json.into(),
            preferences.trading_hours_timezone.clone().into(),
            preferences.trading_hours_start.clone().into(),
            preferences.trading_hours_end.clone().into(),
            preferences.onboarding_completed.into(),
            tutorial_steps_json.into(),
            (preferences.created_at as i64).into(),
            (preferences.updated_at as i64).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get user trading preferences by user ID
    pub async fn get_trading_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<UserTradingPreferences>> {
        let stmt = self
            .db
            .prepare("SELECT * FROM user_trading_preferences WHERE user_id = ?");

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let preferences = self.row_to_trading_preferences(row)?;
                Ok(Some(preferences))
            }
            None => Ok(None),
        }
    }

    /// Update user trading preferences
    pub async fn update_trading_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<()> {
        self.store_trading_preferences(preferences).await
    }

    /// Atomically get or create trading preferences (race condition safe)
    pub async fn get_or_create_trading_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserTradingPreferences> {
        // First attempt to get existing preferences
        if let Some(existing_prefs) = self.get_trading_preferences(user_id).await? {
            return Ok(existing_prefs);
        }

        // Create default preferences
        let default_prefs = UserTradingPreferences::new_default(user_id.to_string());

        // Use INSERT OR IGNORE for race condition safety
        let trading_focus_str =
            serde_json::to_string(&default_prefs.trading_focus).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize trading focus: {}", e))
            })?;

        let experience_level_str =
            serde_json::to_string(&default_prefs.experience_level).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize experience level: {}", e))
            })?;

        let risk_tolerance_str =
            serde_json::to_string(&default_prefs.risk_tolerance).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize risk tolerance: {}", e))
            })?;

        let automation_level_str =
            serde_json::to_string(&default_prefs.automation_level).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize automation level: {}", e))
            })?;

        let automation_scope_str =
            serde_json::to_string(&default_prefs.automation_scope).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize automation scope: {}", e))
            })?;

        let notification_channels_json =
            serde_json::to_string(&default_prefs.preferred_notification_channels).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to serialize notification channels: {}",
                    e
                ))
            })?;

        let tutorial_steps_json = serde_json::to_string(&default_prefs.tutorial_steps_completed)
            .map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize tutorial steps: {}", e))
            })?;

        // Use INSERT OR IGNORE to handle race conditions
        let stmt = self.db.prepare(
            "
            INSERT OR IGNORE INTO user_trading_preferences (
                preference_id, user_id, trading_focus, experience_level, risk_tolerance,
                automation_level, automation_scope, arbitrage_enabled, technical_enabled,
                advanced_analytics_enabled, preferred_notification_channels,
                trading_hours_timezone, trading_hours_start, trading_hours_end,
                onboarding_completed, tutorial_steps_completed, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            default_prefs.preference_id.clone().into(),
            default_prefs.user_id.clone().into(),
            trading_focus_str.into(),
            experience_level_str.into(),
            risk_tolerance_str.into(),
            automation_level_str.into(),
            automation_scope_str.into(),
            default_prefs.arbitrage_enabled.into(),
            default_prefs.technical_enabled.into(),
            default_prefs.advanced_analytics_enabled.into(),
            notification_channels_json.into(),
            default_prefs.trading_hours_timezone.clone().into(),
            default_prefs.trading_hours_start.clone().into(),
            default_prefs.trading_hours_end.clone().into(),
            default_prefs.onboarding_completed.into(),
            tutorial_steps_json.into(),
            (default_prefs.created_at as i64).into(),
            (default_prefs.updated_at as i64).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        // Get the final preferences (handles case where another thread created them)
        self.get_trading_preferences(user_id).await?.ok_or_else(|| {
            ArbitrageError::database_error("Failed to retrieve preferences after creation")
        })
    }

    /// Delete user trading preferences
    pub async fn delete_trading_preferences(&self, user_id: &str) -> ArbitrageResult<bool> {
        let stmt = self
            .db
            .prepare("DELETE FROM user_trading_preferences WHERE user_id = ?");

        let _result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .run()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        Ok(true)
    }

    // ============= USER OPPORTUNITY PREFERENCES OPERATIONS =============

    /// Store user opportunity preferences in D1 database
    pub async fn store_user_opportunity_preferences(
        &self,
        user_id: &str,
        preferences_json: &str,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            INSERT OR REPLACE INTO user_opportunity_preferences 
            (user_id, preferences_data, created_at, updated_at) 
            VALUES (?, ?, ?, ?)
        ",
        );

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

        stmt.bind(&[
            user_id.into(),
            preferences_json.into(),
            (now as i64).into(),
            (now as i64).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| {
            ArbitrageError::database_error(format!(
                "Failed to store user opportunity preferences for {}: {}",
                user_id, e
            ))
        })?;

        Ok(())
    }

    /// Get user opportunity preferences from D1 database
    pub async fn get_user_opportunity_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<UserOpportunityPreferences>> {
        let stmt = self.db.prepare(
            "SELECT preferences_data FROM user_opportunity_preferences WHERE user_id = ? LIMIT 1",
        );

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let preferences_json = self.get_string_field(&row, "preferences_data")?;
                let preferences: UserOpportunityPreferences =
                    serde_json::from_str(&preferences_json).map_err(|e| {
                        ArbitrageError::parse_error(format!(
                            "Failed to parse user opportunity preferences: {}",
                            e
                        ))
                    })?;
                Ok(Some(preferences))
            }
            None => Ok(None),
        }
    }

    // ============= API KEY OPERATIONS =============

    /// Store a user API key
    pub async fn store_user_api_key(
        &self,
        user_id: &str,
        api_key: &UserApiKey,
    ) -> ArbitrageResult<()> {
        let provider_json = serde_json::to_string(&api_key.provider).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize provider: {}", e))
        })?;

        let permissions_json = serde_json::to_string(&api_key.permissions).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize permissions: {}", e))
        })?;

        let stmt = self.db.prepare(
            "
            INSERT OR REPLACE INTO user_api_keys (
                id, user_id, provider, encrypted_key, encrypted_secret,
                metadata, is_active, created_at, last_used, permissions
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            api_key.id.clone().into(),
            user_id.into(),
            provider_json.into(),
            api_key.encrypted_key.clone().into(),
            api_key.encrypted_secret.clone().unwrap_or_default().into(),
            api_key.metadata.to_string().into(),
            api_key.is_active.into(),
            (api_key.created_at as i64).into(),
            api_key.last_used.map(|t| t as i64).unwrap_or(0).into(),
            permissions_json.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    // ============= INVITATION CODE OPERATIONS =============

    /// Create invitation code
    pub async fn create_invitation_code(&self, invitation: &InvitationCode) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            INSERT INTO invitation_codes (
                code, created_by, created_at, expires_at, max_uses,
                current_uses, is_active, purpose
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            invitation.code.clone().into(),
            invitation.created_by.clone().unwrap_or_default().into(),
            (invitation.created_at as i64).into(),
            invitation.expires_at.map(|t| t as i64).unwrap_or(0).into(),
            invitation.max_uses.map(|u| u as i64).unwrap_or(0).into(),
            (invitation.current_uses as i64).into(),
            invitation.is_active.into(),
            invitation.purpose.clone().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get invitation code
    pub async fn get_invitation_code(&self, code: &str) -> ArbitrageResult<Option<InvitationCode>> {
        let stmt = self
            .db
            .prepare("SELECT * FROM invitation_codes WHERE code = ?");

        let result = stmt
            .bind(&[code.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let invitation = self.row_to_invitation_code(row)?;
                Ok(Some(invitation))
            }
            None => Ok(None),
        }
    }

    /// Update invitation code
    pub async fn update_invitation_code(&self, invitation: &InvitationCode) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            UPDATE invitation_codes SET 
                current_uses = ?, is_active = ?
            WHERE code = ?
        ",
        );

        stmt.bind(&[
            (invitation.current_uses as i64).into(),
            invitation.is_active.into(),
            invitation.code.clone().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    // ============= INVITATION USAGE OPERATIONS =============

    /// Store invitation usage record for beta tracking
    pub async fn store_invitation_usage(&self, usage: &InvitationUsage) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            INSERT INTO invitation_usage (
                invitation_id, user_id, telegram_id, used_at, beta_expires_at
            ) VALUES (?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            usage.invitation_id.clone().into(),
            usage.user_id.clone().into(),
            JsValue::from_f64(usage.telegram_id as f64),
            usage.used_at.timestamp_millis().into(),
            usage.beta_expires_at.timestamp_millis().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get invitation usage by user ID
    pub async fn get_invitation_usage_by_user(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<InvitationUsage>> {
        let stmt = self
            .db
            .prepare("SELECT * FROM invitation_usage WHERE user_id = ? LIMIT 1");

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let usage = self.row_to_invitation_usage(row)?;
                Ok(Some(usage))
            }
            None => Ok(None),
        }
    }

    /// Check if user has active beta access
    ///
    /// **Performance Note**: This query uses complex conditions with typeof() checks.
    /// For production deployment, consider adding a composite index on (user_id, beta_expires_at)
    /// columns for better query performance.
    pub async fn has_active_beta_access(&self, user_id: &str) -> ArbitrageResult<bool> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let stmt = self.db.prepare(
            "SELECT beta_expires_at FROM invitation_usage WHERE user_id = ? AND (
                (typeof(beta_expires_at) = 'integer' AND beta_expires_at > ?) OR
                (typeof(beta_expires_at) = 'text' AND beta_expires_at > datetime('now'))
            ) LIMIT 1",
        );

        let result = stmt
            .bind(&[user_id.into(), now_ms.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        Ok(result.is_some())
    }

    // ============= USER INVITATION OPERATIONS =============

    /// Store a user invitation
    pub async fn store_user_invitation(&self, invitation: &UserInvitation) -> ArbitrageResult<()> {
        let invitation_data_json =
            serde_json::to_string(&invitation.invitation_data).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize invitation data: {}", e))
            })?;

        let stmt = self.db.prepare(
            "
            INSERT OR REPLACE INTO user_invitations (
                invitation_id, inviter_user_id, invitee_identifier,
                invitation_type, status, message, invitation_data,
                created_at, expires_at, accepted_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        let expires_at = invitation
            .expires_at
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(0);
        let accepted_at = invitation
            .accepted_at
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(0);
        let created_at = invitation.created_at.timestamp_millis();

        stmt.bind(&[
            invitation.invitation_id.clone().into(),
            invitation.inviter_user_id.clone().into(),
            invitation.invitee_identifier.clone().into(),
            invitation.invitation_type.to_string().into(),
            invitation.status.to_string().into(),
            invitation.message.clone().unwrap_or_default().into(),
            invitation_data_json.into(),
            created_at.into(),
            expires_at.into(),
            accepted_at.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Retrieve user invitations by user ID
    pub async fn get_user_invitations(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Vec<UserInvitation>> {
        let stmt = self.db.prepare(
            "
            SELECT * FROM user_invitations 
            WHERE inviter_user_id = ? 
            ORDER BY created_at DESC
        ",
        );

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let mut invitations = Vec::new();
        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        for row in results {
            invitations.push(self.row_to_user_invitation(row)?);
        }

        Ok(invitations)
    }

    // ============= TRADING ANALYTICS OPERATIONS =============

    /// Store trading analytics data
    pub async fn store_trading_analytics(
        &self,
        analytics: &TradingAnalytics,
    ) -> ArbitrageResult<()> {
        let metric_data_json = serde_json::to_string(&analytics.metric_data).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize metric data: {}", e))
        })?;

        let analytics_metadata_json = serde_json::to_string(&analytics.analytics_metadata)
            .map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to serialize analytics metadata: {}",
                    e
                ))
            })?;

        let stmt = self.db.prepare(
            "
            INSERT INTO trading_analytics (
                analytics_id, user_id, metric_type, metric_value,
                metric_data, exchange_id, trading_pair, opportunity_type,
                timestamp, session_id, analytics_metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        let timestamp = analytics.timestamp.timestamp_millis();

        stmt.bind(&[
            analytics.analytics_id.clone().into(),
            analytics.user_id.clone().into(),
            analytics.metric_type.clone().into(),
            analytics.metric_value.into(),
            metric_data_json.into(),
            analytics.exchange_id.clone().unwrap_or_default().into(),
            analytics.trading_pair.clone().unwrap_or_default().into(),
            analytics
                .opportunity_type
                .clone()
                .unwrap_or_default()
                .into(),
            timestamp.into(),
            analytics.session_id.clone().unwrap_or_default().into(),
            analytics_metadata_json.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Retrieve trading analytics for a user
    pub async fn get_trading_analytics(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> ArbitrageResult<Vec<TradingAnalytics>> {
        let limit = limit.unwrap_or(100);

        let stmt = self.db.prepare(
            "
            SELECT * FROM trading_analytics 
            WHERE user_id = ? 
            ORDER BY timestamp DESC 
            LIMIT ?
        ",
        );

        let result = stmt
            .bind(&[user_id.into(), limit.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let mut analytics = Vec::new();
        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        for row in results {
            analytics.push(self.row_to_trading_analytics(row)?);
        }

        Ok(analytics)
    }

    // ============= HELPER METHODS =============

    /// Convert database row to UserProfile
    #[allow(clippy::result_large_err)]
    fn row_to_user_profile(&self, row: HashMap<String, Value>) -> ArbitrageResult<UserProfile> {
        // Parse required fields
        let user_id = row
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::parse_error("Missing user_id".to_string()))?
            .to_string();

        let telegram_user_id = row
            .get("telegram_id")
            .and_then(|v| v.as_i64())
            .filter(|&id| id > 0); // Convert to Option, filtering out invalid IDs

        let telegram_username = row
            .get("username")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // Parse JSON fields
        let api_keys: Vec<UserApiKey> = row
            .get("api_keys")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let subscription = row
            .get("subscription")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let configuration = row
            .get("configuration")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let invitation_code = row
            .get("invitation_code")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // Parse timestamp fields
        let created_at = row.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

        let updated_at = row.get("updated_at").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

        let last_active = row.get("last_active").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

        let is_active = row
            .get("is_active")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let total_trades = 0u32; // Default value since column doesn't exist yet

        let total_pnl_usdt = 0.0f64; // Default value since column doesn't exist yet

        let account_balance_usdt = row
            .get("account_balance_usdt")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Parse profile_metadata field
        let profile_metadata = row
            .get("profile_metadata")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok());

        // Parse beta_expires_at field
        let beta_expires_at = row
            .get("beta_expires_at")
            .and_then(|v| v.as_i64())
            .filter(|&t| t > 0)
            .map(|t| t as u64);

        Ok(UserProfile {
            user_id,
            telegram_user_id,
            telegram_username,
            subscription,
            configuration,
            api_keys,
            invitation_code,
            created_at,
            updated_at,
            last_active,
            is_active,
            total_trades,
            total_pnl_usdt,
            account_balance_usdt,
            profile_metadata,
            beta_expires_at,
        })
    }

    /// Convert database row to InvitationCode
    #[allow(clippy::result_large_err)]
    fn row_to_invitation_code(
        &self,
        row: HashMap<String, Value>,
    ) -> ArbitrageResult<InvitationCode> {
        let code = row
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::parse_error("Missing code".to_string()))?
            .to_string();

        let created_by = row
            .get("created_by")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let created_at = row.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

        let expires_at = row
            .get("expires_at")
            .and_then(|v| v.as_i64())
            .filter(|&t| t > 0)
            .map(|t| t as u64);

        let max_uses = row
            .get("max_uses")
            .and_then(|v| v.as_i64())
            .filter(|&u| u > 0)
            .map(|u| u as u32);

        let current_uses = row
            .get("current_uses")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u32;

        let is_active = row
            .get("is_active")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let purpose = row
            .get("purpose")
            .and_then(|v| v.as_str())
            .unwrap_or("general")
            .to_string();

        Ok(InvitationCode {
            code,
            created_by,
            created_at,
            expires_at,
            max_uses,
            current_uses,
            is_active,
            purpose,
        })
    }

    /// Convert database row to InvitationUsage
    #[allow(clippy::result_large_err)]
    fn row_to_invitation_usage(
        &self,
        row: HashMap<String, Value>,
    ) -> ArbitrageResult<InvitationUsage> {
        let invitation_id = self.get_string_field(&row, "invitation_id")?;
        let user_id = self.get_string_field(&row, "user_id")?;
        let telegram_id = self.get_i64_field(&row, "telegram_id", 0);

        // Handle both integer timestamps (new format) and RFC3339 strings (legacy format) for backward compatibility
        let used_at = if let Some(timestamp_ms) = row.get("used_at").and_then(|v| v.as_i64()) {
            // New integer timestamp format
            chrono::DateTime::from_timestamp_millis(timestamp_ms)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .ok_or_else(|| {
                    ArbitrageError::parse_error("Invalid used_at timestamp".to_string())
                })?
        } else {
            // Legacy RFC3339 string format for backward compatibility
            let used_at_str = self.get_string_field(&row, "used_at")?;
            chrono::DateTime::parse_from_rfc3339(&used_at_str)
                .map_err(|e| ArbitrageError::parse_error(format!("Invalid used_at format: {}", e)))?
                .with_timezone(&chrono::Utc)
        };

        let beta_expires_at = if let Some(timestamp_ms) =
            row.get("beta_expires_at").and_then(|v| v.as_i64())
        {
            // New integer timestamp format
            chrono::DateTime::from_timestamp_millis(timestamp_ms)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .ok_or_else(|| {
                    ArbitrageError::parse_error("Invalid beta_expires_at timestamp".to_string())
                })?
        } else {
            // Legacy RFC3339 string format for backward compatibility
            let beta_expires_at_str = self.get_string_field(&row, "beta_expires_at")?;
            chrono::DateTime::parse_from_rfc3339(&beta_expires_at_str)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Invalid beta_expires_at format: {}", e))
                })?
                .with_timezone(&chrono::Utc)
        };

        Ok(InvitationUsage {
            invitation_id,
            user_id,
            telegram_id,
            used_at,
            beta_expires_at,
        })
    }

    /// Convert database row to UserInvitation
    #[allow(clippy::result_large_err)]
    fn row_to_user_invitation(
        &self,
        row: HashMap<String, Value>,
    ) -> ArbitrageResult<UserInvitation> {
        let invitation_code = self.get_string_field(&row, "invitation_code")?;
        let invited_user_id = self.get_string_field(&row, "invited_user_id")?;
        let invited_by = self.get_optional_string_field(&row, "invited_by");
        let used_at = self.get_i64_field(&row, "used_at", 0) as u64; // Assuming used_at is stored as i64/timestamp
        let invitation_metadata = self
            .get_optional_string_field(&row, "invitation_metadata")
            .and_then(|s| serde_json::from_str(&s).ok());

        // These fields seem to be from an older version or a misinterpretation.
        // The current UserInvitation struct (as per types.rs around line 2387)
        // does not directly contain these. If these are still needed,
        // the UserInvitation struct in types.rs must be updated first.
        // For now, we will initialize with the fields defined in UserInvitation.

        let invitation_id = self
            .get_string_field(&row, "invitation_id")
            .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()); // Fallback if not in DB

        let inviter_user_id = self
            .get_string_field(&row, "inviter_user_id")
            .unwrap_or_else(|_| "".to_string()); // Fallback

        let invitee_identifier = self
            .get_string_field(&row, "invitee_identifier")
            .unwrap_or_else(|_| "".to_string()); // Fallback

        let invitation_type = self
            .get_string_field(&row, "invitation_type")
            .unwrap_or_else(|_| "standard".to_string()); // Fallback

        let status = self
            .get_string_field(&row, "status")
            .unwrap_or_else(|_| "pending".to_string()); // Fallback

        let message = self.get_optional_string_field(&row, "message");

        let invitation_data = self
            .get_optional_string_field(&row, "invitation_data")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null);

        let created_at = row
            .get("created_at")
            .and_then(|v| v.as_i64())
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        let expires_at = row
            .get("expires_at")
            .and_then(|v| v.as_i64())
            .filter(|&ts| ts > 0)
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let accepted_at = row
            .get("accepted_at")
            .and_then(|v| v.as_i64())
            .filter(|&ts| ts > 0)
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|dt| dt.with_timezone(&chrono::Utc));

        Ok(UserInvitation {
            invitation_code,
            invited_user_id,
            invited_by,
            used_at,
            invitation_metadata,
            // Fields that were previously parsed but might be from an older struct version
            // or need to be added to the current UserInvitation struct if still relevant.
            invitation_id,
            inviter_user_id,
            invitee_identifier,
            invitation_type,
            status,
            message,
            invitation_data,
            created_at,
            expires_at,
            accepted_at,
        })
    }

    /// Convert database row to TradingAnalytics
    #[allow(clippy::result_large_err)]
    fn row_to_trading_analytics(
        &self,
        row: HashMap<String, Value>,
    ) -> ArbitrageResult<TradingAnalytics> {
        // Parse required fields
        let analytics_id = row
            .get("analytics_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::parse_error("Missing analytics_id".to_string()))?
            .to_string();

        let user_id = row
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::parse_error("Missing user_id".to_string()))?
            .to_string();

        let metric_type = row
            .get("metric_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::parse_error("Missing metric_type".to_string()))?
            .to_string();

        let metric_value = row
            .get("metric_value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ArbitrageError::parse_error("Missing metric_value".to_string()))?;

        let metric_data: serde_json::Value = row
            .get("metric_data")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(json!({}));

        let analytics_metadata: serde_json::Value = row
            .get("analytics_metadata")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(json!({}));

        let exchange_id = row
            .get("exchange_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let trading_pair = row
            .get("trading_pair")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let opportunity_type = row
            .get("opportunity_type")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let session_id = row
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let timestamp = row
            .get("timestamp")
            .and_then(|v| v.as_i64())
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        Ok(TradingAnalytics {
            user_id: user_id.clone(),
            total_trades: 0,
            successful_trades: 0,
            total_pnl_usdt: 0.0,
            best_trade_pnl: 0.0,
            worst_trade_pnl: 0.0,
            average_trade_size: 0.0,
            total_volume_traded: 0.0,
            win_rate_percentage: 0.0,
            average_holding_time_hours: 0.0,
            risk_score: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
            analytics_id,
            metric_type,
            metric_value,
            metric_data,
            exchange_id,
            trading_pair,
            opportunity_type,
            timestamp,
            session_id,
            analytics_metadata,
        })
    }

    /// Store AI analysis audit trail in D1 database
    pub async fn store_ai_analysis_audit(
        &self,
        user_id: &str,
        ai_provider: &str,
        request_data: &serde_json::Value,
        response_data: &serde_json::Value,
        processing_time_ms: u64,
    ) -> ArbitrageResult<()> {
        let audit_id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().timestamp_millis();

        let stmt = self.db.prepare(
            "INSERT INTO ai_analysis_audit (
                audit_id, user_id, ai_provider, 
                request_data, response_data, processing_time_ms,
                timestamp
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            audit_id.into(),
            user_id.into(),
            ai_provider.into(),
            serde_json::to_string(request_data)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize request_data: {}", e))
                })?
                .into(),
            serde_json::to_string(response_data)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize response_data: {}", e))
                })?
                .into(),
            (processing_time_ms as i64).into(),
            timestamp.into(),
        ])?
        .run()
        .await
        .map_err(|e| {
            ArbitrageError::database_error(format!("Failed to store AI analysis audit: {}", e))
        })?;

        Ok(())
    }

    /// Store opportunity analysis in D1 database
    pub async fn store_opportunity_analysis(
        &self,
        analysis: &AiOpportunityAnalysis,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "INSERT INTO ai_opportunity_analysis (
                opportunity_id, user_id, ai_score, viability_assessment,
                risk_factors, recommended_position_size, confidence_level,
                analysis_timestamp, ai_provider_used, custom_recommendations
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            analysis.opportunity_id.as_str().into(),
            analysis.user_id.as_str().into(),
            analysis.ai_score.into(),
            analysis.viability_assessment.as_str().into(),
            serde_json::to_string(&analysis.risk_factors)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize risk_factors: {}", e))
                })?
                .into(),
            analysis.recommended_position_size.into(),
            analysis.confidence_level.into(),
            (analysis.analysis_timestamp as i64).into(),
            analysis.ai_provider_used.as_str().into(),
            serde_json::to_string(&analysis.custom_recommendations)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize custom_recommendations: {}",
                        e
                    ))
                })?
                .into(),
        ])?
        .run()
        .await
        .map_err(|e| {
            ArbitrageError::database_error(format!("Failed to store opportunity analysis: {}", e))
        })?;

        Ok(())
    }

    /// Health check for D1 database connection
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let stmt = self.db.prepare("SELECT 1 as test");
        let result = stmt.first::<serde_json::Value>(None).await;
        match result {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    // ============= GENERIC QUERY HELPERS =============

    #[cfg(target_arch = "wasm32")]
    /// ⚠️ **SECURITY WARNING**: Execute a prepared statement with parameters (for INSERT, UPDATE, DELETE)
    ///
    /// **SQL INJECTION RISK**: This method accepts raw SQL strings. Only use with:
    /// - Hardcoded SQL statements
    /// - Properly parameterized queries using the `params` array
    /// - Never pass user input directly in the `sql` parameter
    ///
    /// Consider using specific typed methods instead of this generic query executor.
    pub async fn execute_query(&self, sql: &str, params: &[JsValue]) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(sql);
        stmt.bind(params)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .run()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    /// ⚠️ **SECURITY WARNING**: Execute a query that returns results (for SELECT)
    ///
    /// **SQL INJECTION RISK**: This method accepts raw SQL strings. Only use with:
    /// - Hardcoded SQL statements  
    /// - Properly parameterized queries using the `params` array  
    /// - Never pass user input directly in the `sql` parameter
    ///
    /// Consider using specific typed methods instead of this generic query executor.
    pub async fn query_internal(
        &self,
        sql: &str,
        params: &[JsValue],
    ) -> ArbitrageResult<QueryResult> {
        let stmt = self.db.prepare(sql);
        let result = stmt
            .bind(params)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        Ok(QueryResult { results })
    }

    #[cfg(target_arch = "wasm32")]
    /// ⚠️ **SECURITY WARNING**: Execute a query that returns a single result (for SELECT with LIMIT 1)
    ///
    /// **SQL INJECTION RISK**: This method accepts raw SQL strings. Only use with:
    /// - Hardcoded SQL statements
    /// - Properly parameterized queries using the `params` array
    /// - Never pass user input directly in the `sql` parameter
    ///
    /// Consider using specific typed methods instead of this generic query executor.
    pub async fn query_first(
        &self,
        sql: &str,
        params: &[JsValue],
    ) -> ArbitrageResult<Option<HashMap<String, Value>>> {
        let stmt = self.db.prepare(sql);
        let result = stmt
            .bind(params)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;
        Ok(result)
    }

    // ============= SIMPLIFIED QUERY HELPERS FOR INVITATION SERVICE =============

    /// ⚠️ **SECURITY WARNING**: Execute a prepared statement with parameters (for INSERT, UPDATE, DELETE)
    ///
    /// **SQL INJECTION RISK**: This method accepts raw SQL strings. Only use with:
    /// - Hardcoded SQL statements
    /// - Properly parameterized queries using the `params` array
    /// - Never pass user input directly in the `sql` parameter
    ///
    /// Consider using specific typed methods instead of this generic query executor.
    /// Accepts parameters that implement Into<JsValue> for convenience
    pub async fn execute(&self, sql: &str, params: &[serde_json::Value]) -> ArbitrageResult<()> {
        #[cfg(target_arch = "wasm32")]
        {
            let js_params: Vec<JsValue> = params
                .iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => JsValue::from_str(s),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            JsValue::from_f64(i as f64)
                        } else if let Some(f) = n.as_f64() {
                            JsValue::from_f64(f)
                        } else {
                            JsValue::from_str(&n.to_string())
                        }
                    }
                    serde_json::Value::Bool(b) => JsValue::from_bool(*b),
                    serde_json::Value::Null => JsValue::NULL,
                    _ => JsValue::from_str(&v.to_string()),
                })
                .collect();

            self.execute_query(sql, &js_params).await
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (sql, params); // Suppress unused variable warnings
                                   // Non-WASM implementation - return error for now
            Err(ArbitrageError::not_implemented(
                "Database operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
            ))
        }
    }

    /// ⚠️ **SECURITY WARNING**: Execute a query that returns results (for SELECT)
    ///
    /// **SQL INJECTION RISK**: This method accepts raw SQL strings. Only use with:
    /// - Hardcoded SQL statements
    /// - Properly parameterized queries using the `params` array
    /// - Never pass user input directly in the `sql` parameter
    ///
    /// Consider using specific typed methods instead of this generic query executor.
    /// Accepts parameters that implement Into<JsValue> for convenience
    pub async fn query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> ArbitrageResult<Vec<HashMap<String, String>>> {
        #[cfg(target_arch = "wasm32")]
        {
            let js_params: Vec<JsValue> = params
                .iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => JsValue::from_str(s),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            JsValue::from_f64(i as f64)
                        } else if let Some(f) = n.as_f64() {
                            JsValue::from_f64(f)
                        } else {
                            JsValue::from_str(&n.to_string())
                        }
                    }
                    serde_json::Value::Bool(b) => JsValue::from_bool(*b),
                    serde_json::Value::Null => JsValue::NULL,
                    _ => JsValue::from_str(&v.to_string()),
                })
                .collect();

            let result = self.query_internal(&sql, &js_params).await?;

            // Convert QueryResult to Vec<HashMap<String, String>>
            let mut string_results = Vec::new();
            for row in result.results {
                let mut string_row = HashMap::new();
                for (key, value) in row {
                    let string_value = match value {
                        Value::String(s) => s,
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Null => "".to_string(),
                        _ => value.to_string(),
                    };
                    string_row.insert(key, string_value);
                }
                string_results.push(string_row);
            }

            Ok(string_results)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (sql, params); // Suppress unused variable warnings
                                   // Non-WASM implementation - return error for now
            Err(ArbitrageError::not_implemented(
                "Database operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
            ))
        }
    }

    // ============= DATABASE TRANSACTION SUPPORT =============

    /// Begin a database transaction
    pub async fn begin_transaction(&self) -> ArbitrageResult<()> {
        self.execute("BEGIN TRANSACTION", &[]).await
    }

    /// Commit a database transaction
    pub async fn commit_transaction(&self) -> ArbitrageResult<()> {
        self.execute("COMMIT", &[]).await
    }

    /// Rollback a database transaction
    pub async fn rollback_transaction(&self) -> ArbitrageResult<()> {
        self.execute("ROLLBACK", &[]).await
    }

    /// Execute multiple operations within a transaction
    /// If any operation fails, the entire transaction is rolled back
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute_transaction<F, T>(&self, operations: F) -> ArbitrageResult<T>
    where
        F: FnOnce(
            &Self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = ArbitrageResult<T>> + Send + '_>,
        >,
    {
        // Begin transaction
        self.begin_transaction().await?;

        // Execute operations
        match operations(self).await {
            Ok(result) => {
                // Commit on success
                self.commit_transaction().await?;
                Ok(result)
            }
            Err(e) => {
                // Rollback on failure
                if let Err(rollback_err) = self.rollback_transaction().await {
                    crate::utils::logger::logger().error(&format!(
                        "Failed to rollback transaction: {:?}",
                        rollback_err
                    ));
                }
                Err(e)
            }
        }
    }

    /// Execute multiple operations within a transaction (WASM version without Send bounds)
    /// If any operation fails, the entire transaction is rolled back
    #[cfg(target_arch = "wasm32")]
    pub async fn execute_transaction<F, T>(&self, operations: F) -> ArbitrageResult<T>
    where
        F: FnOnce(
            &Self,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = ArbitrageResult<T>> + '_>>,
    {
        // Begin transaction
        self.begin_transaction().await?;

        // Execute operations
        match operations(self).await {
            Ok(result) => {
                // Commit on success
                self.commit_transaction().await?;
                Ok(result)
            }
            Err(e) => {
                // Rollback on failure
                if let Err(rollback_err) = self.rollback_transaction().await {
                    crate::utils::logger::logger().error(&format!(
                        "Failed to rollback transaction: {:?}",
                        rollback_err
                    ));
                }
                Err(e)
            }
        }
    }

    // ============= FUND MONITORING OPERATIONS =============

    /// Store balance history entry
    pub async fn store_balance_history(&self, entry: &BalanceHistoryEntry) -> ArbitrageResult<()> {
        let balance_json = serde_json::to_string(&entry.balance).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize balance: {}", e))
        })?;

        let stmt = self.db.prepare(
            "
            INSERT INTO balance_history (
                id, user_id, exchange_id, asset, balance_data, 
                usd_value, timestamp, snapshot_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            entry.id.clone().into(),
            entry.user_id.clone().into(),
            entry.exchange_id.clone().into(),
            entry.asset.clone().into(),
            balance_json.into(),
            entry.usd_value.into(),
            (entry.timestamp as i64).into(),
            entry.snapshot_id.clone().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get balance history for a user with optional filters
    pub async fn get_balance_history(
        &self,
        user_id: &str,
        exchange_id: Option<&str>,
        asset: Option<&str>,
        from_timestamp: Option<u64>,
        to_timestamp: Option<u64>,
        limit: Option<u32>,
    ) -> ArbitrageResult<Vec<BalanceHistoryEntry>> {
        let mut query = "SELECT * FROM balance_history WHERE user_id = ?".to_string();
        let mut params: Vec<serde_json::Value> = vec![user_id.into()];

        if let Some(exchange) = exchange_id {
            query.push_str(" AND exchange_id = ?");
            params.push(exchange.into());
        }

        if let Some(asset_filter) = asset {
            query.push_str(" AND asset = ?");
            params.push(asset_filter.into());
        }

        if let Some(from_ts) = from_timestamp {
            query.push_str(" AND timestamp >= ?");
            params.push((from_ts as i64).into());
        }

        if let Some(to_ts) = to_timestamp {
            query.push_str(" AND timestamp <= ?");
            params.push((to_ts as i64).into());
        }

        query.push_str(" ORDER BY timestamp DESC");

        if let Some(limit_val) = limit {
            query.push_str(" LIMIT ?");
            params.push((limit_val as i64).into());
        }

        let stmt = self.db.prepare(&query);

        let result = stmt
            .bind(
                &params
                    .iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.as_str().into(),
                        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0).into(),
                        _ => "".into(),
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let mut entries = Vec::new();
        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        for row in results {
            entries.push(self.row_to_balance_history(row)?);
        }

        Ok(entries)
    }

    /// Convert database row to BalanceHistoryEntry
    #[allow(clippy::result_large_err)]
    fn row_to_balance_history(
        &self,
        row: HashMap<String, Value>,
    ) -> ArbitrageResult<BalanceHistoryEntry> {
        let balance_data = row
            .get("balance_data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::parse_error("Missing balance_data field".to_string()))?;

        let balance: Balance = serde_json::from_str(balance_data).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse balance data: {}", e))
        })?;

        Ok(BalanceHistoryEntry {
            id: row
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            user_id: row
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            exchange_id: row
                .get("exchange_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            asset: row
                .get("asset")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            balance,
            usd_value: row.get("usd_value").and_then(|v| v.as_f64()).unwrap_or(0.0),
            timestamp: row.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            snapshot_id: row
                .get("snapshot_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        })
    }

    // ============= DYNAMIC CONFIGURATION OPERATIONS =============

    /// Store a dynamic configuration template
    pub async fn store_config_template(
        &self,
        template: &DynamicConfigTemplate,
    ) -> ArbitrageResult<()> {
        let template_json = serde_json::to_string(template).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize template: {}", e))
        })?;

        let stmt = self.db.prepare(
            "
            INSERT INTO dynamic_config_templates (
                template_id, name, description, version, category, 
                parameters, created_at, created_by, is_system_template, 
                subscription_tier_required
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            template.template_id.clone().into(),
            template.name.clone().into(),
            template.description.clone().into(),
            template.version.clone().into(),
            serde_json::to_string(&template.category)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize category: {}", e))
                })?
                .into(),
            template_json.into(),
            (template.created_at as i64).into(),
            template.created_by.clone().into(),
            template.is_system_template.into(),
            serde_json::to_string(&template.subscription_tier_required)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize subscription_tier_required: {}",
                        e
                    ))
                })?
                .into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get a dynamic configuration template by ID
    pub async fn get_config_template(
        &self,
        template_id: &str,
    ) -> ArbitrageResult<Option<HashMap<String, Value>>> {
        let stmt = self
            .db
            .prepare("SELECT parameters FROM dynamic_config_templates WHERE template_id = ?");

        let result = stmt
            .bind(&[template_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        Ok(result)
    }

    /// Store a dynamic configuration preset
    pub async fn store_config_preset(&self, preset: &ConfigPreset) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            INSERT INTO dynamic_config_presets (
                preset_id, name, description, template_id, parameter_values, 
                risk_level, target_audience, created_at, is_system_preset
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            preset.preset_id.clone().into(),
            preset.name.clone().into(),
            preset.description.clone().into(),
            preset.template_id.clone().into(),
            serde_json::to_string(&preset.parameter_values)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize parameter_values: {}",
                        e
                    ))
                })?
                .into(),
            serde_json::to_string(&preset.risk_level)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize risk_level: {}", e))
                })?
                .into(),
            preset.target_audience.clone().into(),
            (preset.created_at as i64).into(),
            preset.is_system_preset.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Deactivate user configuration instances
    pub async fn deactivate_user_config(
        &self,
        user_id: &str,
        template_id: &str,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            UPDATE user_config_instances 
            SET is_active = false 
            WHERE user_id = ? AND template_id = ? AND is_active = true
        ",
        );

        stmt.bind(&[user_id.into(), template_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .run()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        Ok(())
    }

    /// Store a user configuration instance
    pub async fn store_user_config_instance(
        &self,
        instance: &UserConfigInstance,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            INSERT INTO user_config_instances (
                instance_id, user_id, template_id, preset_id, parameter_values, 
                version, is_active, created_at, updated_at, rollback_data
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            instance.instance_id.clone().into(),
            instance.user_id.clone().into(),
            instance.template_id.clone().into(),
            instance.preset_id.as_deref().unwrap_or("").into(),
            serde_json::to_string(&instance.parameter_values)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize parameter_values: {}",
                        e
                    ))
                })?
                .into(),
            (instance.version as i64).into(),
            instance.is_active.into(),
            (instance.created_at as i64).into(),
            (instance.updated_at as i64).into(),
            instance.rollback_data.as_deref().unwrap_or("").into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get user's active configuration instance
    pub async fn get_user_config_instance(
        &self,
        user_id: &str,
        template_id: &str,
    ) -> ArbitrageResult<Option<HashMap<String, Value>>> {
        let stmt = self.db.prepare(
            "
            SELECT * FROM user_config_instances 
            WHERE user_id = ? AND template_id = ? AND is_active = true 
            ORDER BY version DESC LIMIT 1
        ",
        );

        let result = stmt
            .bind(&[user_id.into(), template_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        Ok(result)
    }

    // ============= NOTIFICATION SYSTEM OPERATIONS =============

    /// Store a notification template
    pub async fn store_notification_template(
        &self,
        template: &NotificationTemplate,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            INSERT OR REPLACE INTO notification_templates (
                template_id, name, description, category, title_template, 
                message_template, priority, channels, variables, 
                is_system_template, is_active, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            template.template_id.clone().into(),
            template.name.clone().into(),
            template.description.clone().unwrap_or_default().into(),
            template.category.clone().into(),
            template.title_template.clone().into(),
            template.message_template.clone().into(),
            template.priority.clone().into(),
            serde_json::to_string(&template.channels)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize channels: {}", e))
                })?
                .into(),
            serde_json::to_string(&template.variables)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize variables: {}", e))
                })?
                .into(),
            template.is_system_template.into(),
            template.is_active.into(),
            (template.created_at as i64).into(),
            (template.updated_at as i64).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get notification template by ID
    pub async fn get_notification_template(
        &self,
        template_id: &str,
    ) -> ArbitrageResult<Option<NotificationTemplate>> {
        let stmt = self.db.prepare(
            "SELECT * FROM notification_templates WHERE template_id = ? AND is_active = TRUE",
        );

        let result = stmt
            .bind(&[template_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        if let Some(row) = result {
            Ok(Some(self.row_to_notification_template(row)?))
        } else {
            Ok(None)
        }
    }

    /// Store an alert trigger
    pub async fn store_alert_trigger(&self, trigger: &AlertTrigger) -> ArbitrageResult<()> {
        // Use INSERT with ON CONFLICT instead of INSERT OR REPLACE to be compatible with triggers
        let stmt = self.db.prepare(
            "
            INSERT INTO alert_triggers (
                trigger_id, user_id, name, description, trigger_type, 
                conditions, template_id, is_active, priority, channels, 
                cooldown_minutes, max_alerts_per_hour, created_at, 
                updated_at, last_triggered_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(trigger_id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                trigger_type = excluded.trigger_type,
                conditions = excluded.conditions,
                template_id = excluded.template_id,
                is_active = excluded.is_active,
                priority = excluded.priority,
                channels = excluded.channels,
                cooldown_minutes = excluded.cooldown_minutes,
                max_alerts_per_hour = excluded.max_alerts_per_hour,
                updated_at = excluded.updated_at
        ",
        );

        stmt.bind(&[
            trigger.trigger_id.clone().into(),
            trigger.user_id.clone().into(),
            trigger.name.clone().into(),
            trigger.description.clone().unwrap_or_default().into(),
            trigger.trigger_type.clone().into(),
            serde_json::to_string(&trigger.conditions)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize conditions: {}", e))
                })?
                .into(),
            trigger.template_id.clone().unwrap_or_default().into(),
            trigger.is_active.into(),
            trigger.priority.clone().into(),
            serde_json::to_string(&trigger.channels)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize channels: {}", e))
                })?
                .into(),
            (trigger.cooldown_minutes as i64).into(),
            (trigger.max_alerts_per_hour as i64).into(),
            (trigger.created_at as i64).into(),
            (trigger.updated_at as i64).into(),
            trigger
                .last_triggered_at
                .map(|t| t as i64)
                .unwrap_or(0)
                .into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get user's alert triggers
    pub async fn get_user_alert_triggers(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Vec<AlertTrigger>> {
        let stmt = self.db.prepare(
            "
            SELECT * FROM alert_triggers 
            WHERE user_id = ? AND is_active = TRUE 
            ORDER BY priority DESC, created_at ASC
        ",
        );

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let mut triggers = Vec::new();
        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        for row in results {
            triggers.push(self.row_to_alert_trigger(row)?);
        }

        Ok(triggers)
    }

    /// Store a notification
    pub async fn store_notification(&self, notification: &Notification) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            INSERT INTO notifications (
                notification_id, user_id, trigger_id, template_id, title, 
                message, category, priority, notification_data, channels, 
                status, created_at, scheduled_at, sent_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            notification.notification_id.clone().into(),
            notification.user_id.clone().into(),
            notification.trigger_id.clone().unwrap_or_default().into(),
            notification.template_id.clone().unwrap_or_default().into(),
            notification.title.clone().into(),
            notification.message.clone().into(),
            notification.category.clone().into(),
            notification.priority.clone().into(),
            serde_json::to_string(&notification.notification_data)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize notification_data: {}",
                        e
                    ))
                })?
                .into(),
            serde_json::to_string(&notification.channels)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize channels: {}", e))
                })?
                .into(),
            notification.status.clone().into(),
            (notification.created_at as i64).into(),
            notification
                .scheduled_at
                .map(|t| t as i64)
                .unwrap_or(0)
                .into(),
            notification.sent_at.map(|t| t as i64).unwrap_or(0).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Update notification status
    pub async fn update_notification_status(
        &self,
        notification_id: &str,
        status: &str,
        sent_at: Option<u64>,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            UPDATE notifications 
            SET status = ?, sent_at = ? 
            WHERE notification_id = ?
        ",
        );

        stmt.bind(&[
            status.into(),
            sent_at.map(|t| t as i64).unwrap_or(0).into(),
            notification_id.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Store notification delivery history
    pub async fn store_notification_history(
        &self,
        history: &NotificationHistory,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            INSERT INTO notification_history (
                history_id, notification_id, user_id, channel, delivery_status, 
                response_data, error_message, delivery_time_ms, retry_count, 
                attempted_at, delivered_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            history.history_id.clone().into(),
            history.notification_id.clone().into(),
            history.user_id.clone().into(),
            history.channel.clone().into(),
            history.delivery_status.clone().into(),
            serde_json::to_string(&history.response_data)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize response_data: {}", e))
                })?
                .into(),
            history.error_message.clone().unwrap_or_default().into(),
            history
                .delivery_time_ms
                .map(|t| t as i64)
                .unwrap_or(0)
                .into(),
            (history.retry_count as i64).into(),
            (history.attempted_at as i64).into(),
            history.delivered_at.map(|t| t as i64).unwrap_or(0).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get user's notification history
    pub async fn get_user_notification_history(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> ArbitrageResult<Vec<NotificationHistory>> {
        let limit_clause = limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default();
        let query = format!(
            "
            SELECT nh.*, n.title, n.category 
            FROM notification_history nh
            LEFT JOIN notifications n ON nh.notification_id = n.notification_id
            WHERE nh.user_id = ? 
            ORDER BY nh.attempted_at DESC{}
        ",
            limit_clause
        );

        let stmt = self.db.prepare(&query);

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let mut history = Vec::new();
        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        for row in results {
            history.push(self.row_to_notification_history(row)?);
        }

        Ok(history)
    }

    /// Get notification history by notification_id and channel for delivery status check
    pub async fn get_notification_history(
        &self,
        notification_id: &str,
        channel: &str,
    ) -> ArbitrageResult<Option<NotificationHistory>> {
        let stmt = self.db.prepare(
            "SELECT * FROM notification_history 
             WHERE notification_id = ? AND channel = ? 
             ORDER BY attempted_at DESC 
             LIMIT 1",
        );

        let result = stmt
            .bind(&[notification_id.into(), channel.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let history = self.row_to_notification_history(row)?;
                Ok(Some(history))
            }
            None => Ok(None),
        }
    }

    /// Update alert trigger last triggered time
    pub async fn update_trigger_last_triggered(
        &self,
        trigger_id: &str,
        timestamp: u64,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "
            UPDATE alert_triggers 
            SET last_triggered_at = ? 
            WHERE trigger_id = ?
        ",
        );

        stmt.bind(&[(timestamp as i64).into(), trigger_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .run()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        Ok(())
    }

    // ============= HELPER METHODS FOR SAFE ROW CONVERSION =============

    /// Helper method to safely extract string field from database row
    #[allow(clippy::result_large_err)]
    fn get_string_field(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
    ) -> ArbitrageResult<String> {
        row.get(field_name)
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ArbitrageError::parse_error(format!("Missing or invalid {} field", field_name))
            })
            .map(|s| s.to_string())
    }

    /// Helper method to safely extract optional string field from database row
    fn get_optional_string_field(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
    ) -> Option<String> {
        row.get(field_name)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    /// Helper method to safely extract boolean field from database row
    fn get_bool_field(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
        default: bool,
    ) -> bool {
        row.get(field_name)
            .and_then(|v| v.as_bool())
            .or_else(|| {
                // Try parsing from string if it's not a boolean
                row.get(field_name)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<bool>().ok())
            })
            .unwrap_or(default)
    }

    /// Helper method to safely extract integer field from database row
    fn get_i64_field(&self, row: &HashMap<String, Value>, field_name: &str, default: i64) -> i64 {
        row.get(field_name)
            .and_then(|v| v.as_i64())
            .or_else(|| {
                // Try parsing from string if it's not an integer
                row.get(field_name)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<i64>().ok())
            })
            .unwrap_or(default)
    }

    /// Helper method to safely extract float field from database row
    #[allow(dead_code)]
    fn get_f64_field(&self, row: &HashMap<String, Value>, field_name: &str, default: f64) -> f64 {
        row.get(field_name)
            .and_then(|v| v.as_f64())
            .or_else(|| {
                // Try parsing from string if it's not a float
                row.get(field_name)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
            })
            .unwrap_or(default)
    }

    /// Helper method to safely extract and parse JSON field from database row
    #[allow(dead_code)]
    fn get_json_field<T: serde::de::DeserializeOwned>(
        &self,
        row: &HashMap<String, Value>,
        field_name: &str,
        default: T,
    ) -> T {
        row.get(field_name)
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(default)
    }

    // Helper methods for converting database rows to structs
    #[allow(clippy::result_large_err)]
    fn row_to_trading_preferences(
        &self,
        row: HashMap<String, Value>,
    ) -> ArbitrageResult<UserTradingPreferences> {
        // Extract required string fields safely
        let preference_id = self.get_string_field(&row, "preference_id")?;
        let user_id = self.get_string_field(&row, "user_id")?;

        // Extract enum fields with proper error handling
        let trading_focus_str = self.get_string_field(&row, "trading_focus")?;
        let trading_focus: crate::services::core::user::user_trading_preferences::TradingFocus =
            serde_json::from_str(trading_focus_str.trim_matches('"')).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to parse trading focus '{}': {}",
                    trading_focus_str, e
                ))
            })?;

        let experience_level_str = self.get_string_field(&row, "experience_level")?;
        let experience_level: crate::services::core::user::user_trading_preferences::ExperienceLevel =
            serde_json::from_str(experience_level_str.trim_matches('"')).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to parse experience level '{}': {}",
                    experience_level_str, e
                ))
            })?;

        let risk_tolerance_str = self.get_string_field(&row, "risk_tolerance")?;
        let risk_tolerance: crate::services::core::user::user_trading_preferences::RiskTolerance =
            serde_json::from_str(risk_tolerance_str.trim_matches('"')).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to parse risk tolerance '{}': {}",
                    risk_tolerance_str, e
                ))
            })?;

        let automation_level_str = self.get_string_field(&row, "automation_level")?;
        let automation_level: crate::services::core::user::user_trading_preferences::AutomationLevel =
            serde_json::from_str(automation_level_str.trim_matches('"')).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to parse automation level '{}': {}",
                    automation_level_str, e
                ))
            })?;

        let automation_scope_str = self.get_string_field(&row, "automation_scope")?;
        let automation_scope: crate::services::core::user::user_trading_preferences::AutomationScope =
            serde_json::from_str(automation_scope_str.trim_matches('"')).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to parse automation scope '{}': {}",
                    automation_scope_str, e
                ))
            })?;

        // Extract JSON array fields safely
        let notification_channels_str =
            self.get_string_field(&row, "preferred_notification_channels")?;
        let preferred_notification_channels: Vec<String> =
            serde_json::from_str(&notification_channels_str).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to parse notification channels '{}': {}",
                    notification_channels_str, e
                ))
            })?;

        let tutorial_steps_str = self.get_string_field(&row, "tutorial_steps_completed")?;
        let tutorial_steps_completed: Vec<String> = serde_json::from_str(&tutorial_steps_str)
            .map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to parse tutorial steps '{}': {}",
                    tutorial_steps_str, e
                ))
            })?;

        // Extract other fields safely with defaults
        let trading_hours_timezone = self
            .get_string_field(&row, "trading_hours_timezone")
            .unwrap_or_else(|_| "UTC".to_string());
        let trading_hours_start = self
            .get_string_field(&row, "trading_hours_start")
            .unwrap_or_else(|_| "00:00".to_string());
        let trading_hours_end = self
            .get_string_field(&row, "trading_hours_end")
            .unwrap_or_else(|_| "23:59".to_string());

        Ok(UserTradingPreferences {
            preference_id,
            user_id,
            trading_focus,
            experience_level,
            risk_tolerance,
            automation_level,
            automation_scope,
            arbitrage_enabled: self.get_bool_field(&row, "arbitrage_enabled", true),
            technical_enabled: self.get_bool_field(&row, "technical_enabled", false),
            advanced_analytics_enabled: self.get_bool_field(
                &row,
                "advanced_analytics_enabled",
                false,
            ),
            preferred_notification_channels,
            trading_hours_timezone,
            trading_hours_start,
            trading_hours_end,
            onboarding_completed: self.get_bool_field(&row, "onboarding_completed", false),
            tutorial_steps_completed,
            created_at: self.get_i64_field(&row, "created_at", 0) as u64,
            updated_at: self.get_i64_field(&row, "updated_at", 0) as u64,
        })
    }

    #[allow(clippy::result_large_err)]
    fn row_to_notification_template(
        &self,
        row: HashMap<String, Value>,
    ) -> ArbitrageResult<NotificationTemplate> {
        let template_id = self.get_string_field(&row, "template_id")?;
        let name = self.get_string_field(&row, "name")?;
        let category = self.get_string_field(&row, "category")?;
        let title_template = self.get_string_field(&row, "title_template")?;
        let message_template = self.get_string_field(&row, "message_template")?;
        let priority = self.get_string_field(&row, "priority")?;

        let description = self.get_optional_string_field(&row, "description");

        let channels_str = self.get_string_field(&row, "channels")?;
        let channels = serde_json::from_str(&channels_str).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse channels '{}': {}",
                channels_str, e
            ))
        })?;

        let variables_str = self.get_string_field(&row, "variables")?;
        let variables = serde_json::from_str(&variables_str).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse variables '{}': {}",
                variables_str, e
            ))
        })?;

        Ok(NotificationTemplate {
            template_id,
            name,
            description,
            category,
            title_template,
            message_template,
            priority,
            channels,
            variables,
            is_system_template: self.get_bool_field(&row, "is_system_template", false),
            is_active: self.get_bool_field(&row, "is_active", true),
            created_at: self.get_i64_field(&row, "created_at", 0) as u64,
            updated_at: self.get_i64_field(&row, "updated_at", 0) as u64,
        })
    }

    #[allow(clippy::result_large_err)]
    fn row_to_alert_trigger(&self, row: HashMap<String, Value>) -> ArbitrageResult<AlertTrigger> {
        let trigger_id = self.get_string_field(&row, "trigger_id")?;
        let user_id = self.get_string_field(&row, "user_id")?;
        let name = self.get_string_field(&row, "name")?;
        let trigger_type = self.get_string_field(&row, "trigger_type")?;
        let priority = self.get_string_field(&row, "priority")?;

        let description = self.get_optional_string_field(&row, "description");
        let template_id = self.get_optional_string_field(&row, "template_id");

        let conditions_str = self.get_string_field(&row, "conditions")?;
        let conditions = serde_json::from_str(&conditions_str).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse conditions '{}': {}",
                conditions_str, e
            ))
        })?;

        let channels_str = self.get_string_field(&row, "channels")?;
        let channels = serde_json::from_str(&channels_str).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse channels '{}': {}",
                channels_str, e
            ))
        })?;

        let last_triggered_val = self.get_i64_field(&row, "last_triggered_at", 0);
        let last_triggered_at = if last_triggered_val > 0 {
            Some(last_triggered_val as u64)
        } else {
            None
        };

        Ok(AlertTrigger {
            trigger_id,
            user_id,
            name,
            description,
            trigger_type,
            conditions,
            template_id,
            is_active: self.get_bool_field(&row, "is_active", true),
            priority,
            channels,
            cooldown_minutes: self.get_i64_field(&row, "cooldown_minutes", 5) as u32,
            max_alerts_per_hour: self.get_i64_field(&row, "max_alerts_per_hour", 10) as u32,
            created_at: self.get_i64_field(&row, "created_at", 0) as u64,
            updated_at: self.get_i64_field(&row, "updated_at", 0) as u64,
            last_triggered_at,
        })
    }

    #[allow(clippy::result_large_err)]
    fn row_to_notification_history(
        &self,
        row: HashMap<String, Value>,
    ) -> ArbitrageResult<NotificationHistory> {
        let history_id = self.get_string_field(&row, "history_id")?;
        let notification_id = self.get_string_field(&row, "notification_id")?;
        let user_id = self.get_string_field(&row, "user_id")?;
        let channel = self.get_string_field(&row, "channel")?;
        let delivery_status = self.get_string_field(&row, "delivery_status")?;

        let error_message = self.get_optional_string_field(&row, "error_message");

        let response_data_str = self.get_string_field(&row, "response_data")?;
        let response_data = serde_json::from_str(&response_data_str).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse response_data '{}': {}",
                response_data_str, e
            ))
        })?;

        let delivery_time_val = self.get_i64_field(&row, "delivery_time_ms", 0);
        let delivery_time_ms = if delivery_time_val > 0 {
            Some(delivery_time_val as u64)
        } else {
            None
        };

        let delivered_at_val = self.get_i64_field(&row, "delivered_at", 0);
        let delivered_at = if delivered_at_val > 0 {
            Some(delivered_at_val as u64)
        } else {
            None
        };

        Ok(NotificationHistory {
            history_id,
            notification_id,
            user_id,
            channel,
            delivery_status,
            response_data,
            error_message,
            delivery_time_ms,
            retry_count: self.get_i64_field(&row, "retry_count", 0) as u32,
            attempted_at: self.get_i64_field(&row, "attempted_at", 0) as u64,
            delivered_at,
        })
    }

    // ============= AI INTELLIGENCE OPERATIONS =============

    /// Store AI opportunity enhancement in D1 database
    pub async fn store_ai_opportunity_enhancement(
        &self,
        enhancement: &AiOpportunityEnhancement,
    ) -> ArbitrageResult<()> {
        let enhancement_json = serde_json::to_string(enhancement).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to serialize AI opportunity enhancement: {}",
                e
            ))
        })?;

        let stmt = self.db.prepare(
            "
            INSERT INTO ai_opportunity_enhancements (
                opportunity_id, user_id, ai_confidence_score, ai_risk_assessment, 
                ai_recommendations, position_sizing_suggestion, timing_score, 
                technical_confirmation, portfolio_impact_score, ai_provider_used, 
                analysis_timestamp, enhancement_data
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            enhancement.opportunity_id.clone().into(),
            enhancement.user_id.clone().into(),
            enhancement.ai_confidence_score.into(),
            serde_json::to_string(&enhancement.ai_risk_assessment)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize risk assessment: {}",
                        e
                    ))
                })?
                .into(),
            serde_json::to_string(&enhancement.ai_recommendations)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize recommendations: {}",
                        e
                    ))
                })?
                .into(),
            enhancement.position_sizing_suggestion.into(),
            enhancement.timing_score.into(),
            enhancement.technical_confirmation.into(),
            enhancement.portfolio_impact_score.into(),
            enhancement.ai_provider_used.clone().into(),
            (enhancement.analysis_timestamp as i64).into(),
            enhancement_json.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| {
            ArbitrageError::database_error(format!(
                "Failed to store AI opportunity enhancement: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Store AI portfolio analysis in D1 database
    pub async fn store_ai_portfolio_analysis(
        &self,
        analysis: &AiPortfolioAnalysis,
    ) -> ArbitrageResult<()> {
        let analysis_json = serde_json::to_string(analysis).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize AI portfolio analysis: {}", e))
        })?;

        let stmt = self.db.prepare(
            "
            INSERT INTO ai_portfolio_analyses (
                user_id, correlation_risk_score, concentration_risk_score, 
                diversification_score, recommended_adjustments, overexposure_warnings, 
                optimal_allocation_suggestions, analysis_timestamp, analysis_data
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            analysis.user_id.clone().into(),
            analysis.correlation_risk_score.into(),
            analysis.concentration_risk_score.into(),
            analysis.diversification_score.into(),
            serde_json::to_string(&analysis.recommended_adjustments)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize recommended adjustments: {}",
                        e
                    ))
                })?
                .into(),
            serde_json::to_string(&analysis.overexposure_warnings)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize overexposure warnings: {}",
                        e
                    ))
                })?
                .into(),
            serde_json::to_string(&analysis.optimal_allocation_suggestions)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize allocation suggestions: {}",
                        e
                    ))
                })?
                .into(),
            (analysis.analysis_timestamp as i64).into(),
            analysis_json.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| {
            ArbitrageError::database_error(format!("Failed to store AI portfolio analysis: {}", e))
        })?;

        Ok(())
    }

    /// Store AI performance insights in D1 database
    pub async fn store_ai_performance_insights(
        &self,
        insights: &AiPerformanceInsights,
    ) -> ArbitrageResult<()> {
        let insights_json = serde_json::to_string(insights).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to serialize AI performance insights: {}",
                e
            ))
        })?;

        let stmt = self.db.prepare(
            "
            INSERT INTO ai_performance_insights (
                user_id, performance_score, strengths, weaknesses, 
                suggested_focus_adjustment, parameter_optimization_suggestions, 
                learning_recommendations, automation_readiness_score, 
                generated_at, insights_data
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

        stmt.bind(&[
            insights.user_id.clone().into(),
            insights.performance_score.into(),
            serde_json::to_string(&insights.strengths)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize strengths: {}", e))
                })?
                .into(),
            serde_json::to_string(&insights.weaknesses)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize weaknesses: {}", e))
                })?
                .into(),
            serde_json::to_string(&insights.suggested_focus_adjustment)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize focus adjustment: {}",
                        e
                    ))
                })?
                .into(),
            serde_json::to_string(&insights.parameter_optimization_suggestions)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize parameter suggestions: {}",
                        e
                    ))
                })?
                .into(),
            serde_json::to_string(&insights.learning_recommendations)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to serialize learning recommendations: {}",
                        e
                    ))
                })?
                .into(),
            insights.automation_readiness_score.into(),
            (insights.generated_at as i64).into(),
            insights_json.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| {
            ArbitrageError::database_error(format!(
                "Failed to store AI performance insights: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Store AI parameter suggestion in D1 database
    pub async fn store_ai_parameter_suggestion(
        &self,
        user_id: &str,
        suggestion: &ParameterSuggestion,
    ) -> ArbitrageResult<()> {
        let suggestion_json = serde_json::to_string(suggestion).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize parameter suggestion: {}", e))
        })?;

        let stmt = self.db.prepare(
            "
            INSERT INTO ai_parameter_suggestions (
                user_id, parameter_name, current_value, suggested_value, 
                rationale, impact_assessment, confidence, suggestion_data, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ",
        );

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

        stmt.bind(&[
            user_id.into(),
            suggestion.parameter_name.clone().into(),
            suggestion.current_value.clone().into(),
            suggestion.suggested_value.clone().into(),
            suggestion.rationale.clone().into(),
            suggestion.impact_assessment.into(),
            suggestion.confidence.into(),
            suggestion_json.into(),
            (now as i64).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| {
            ArbitrageError::database_error(format!(
                "Failed to store AI parameter suggestion: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Get AI opportunity enhancements for a user
    pub async fn get_ai_opportunity_enhancements(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> ArbitrageResult<Vec<AiOpportunityEnhancement>> {
        let limit_val = limit.unwrap_or(50);

        let stmt = self.db.prepare(
            "
            SELECT enhancement_data FROM ai_opportunity_enhancements 
            WHERE user_id = ? 
            ORDER BY analysis_timestamp DESC 
            LIMIT ?
        ",
        );

        let result = stmt
            .bind(&[user_id.into(), limit_val.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let mut enhancements = Vec::new();
        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        for row in results {
            let enhancement_data = self.get_string_field(&row, "enhancement_data")?;
            let enhancement: AiOpportunityEnhancement = serde_json::from_str(&enhancement_data)
                .map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to parse AI opportunity enhancement: {}",
                        e
                    ))
                })?;
            enhancements.push(enhancement);
        }

        Ok(enhancements)
    }

    /// Get AI performance insights for a user
    pub async fn get_ai_performance_insights(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> ArbitrageResult<Vec<AiPerformanceInsights>> {
        let limit_val = limit.unwrap_or(10);

        let stmt = self.db.prepare(
            "
            SELECT insights_data FROM ai_performance_insights 
            WHERE user_id = ? 
            ORDER BY generated_at DESC 
            LIMIT ?
        ",
        );

        let result = stmt
            .bind(&[user_id.into(), limit_val.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let mut insights_list = Vec::new();
        let results = result.results::<HashMap<String, Value>>().map_err(|e| {
            ArbitrageError::database_error(format!("Failed to parse results: {}", e))
        })?;

        for row in results {
            let insights_data = self.get_string_field(&row, "insights_data")?;
            let insights: AiPerformanceInsights =
                serde_json::from_str(&insights_data).map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to parse AI performance insights: {}",
                        e
                    ))
                })?;
            insights_list.push(insights);
        }

        Ok(insights_list)
    }

    /// Get user analytics data
    #[cfg(target_arch = "wasm32")]
    pub async fn get_user_analytics(&self, user_id: &str) -> ArbitrageResult<serde_json::Value> {
        let query = r#"
            SELECT 
                COUNT(DISTINCT ta.id) as total_trades,
                COALESCE(SUM(ta.pnl_usdt), 0.0) as total_pnl,
                COALESCE(AVG(ta.pnl_usdt), 0.0) as avg_pnl,
                COUNT(CASE WHEN ta.pnl_usdt > 0 THEN 1 END) * 100.0 / COUNT(*) as win_rate,
                COALESCE(AVG(ta.entry_size_usdt), 0.0) as avg_trade_size
            FROM trading_analytics ta 
            WHERE ta.user_id = ?1
        "#;

        let result = self
            .query_internal(query, &[JsValue::from_str(user_id)])
            .await?;

        if let Some(row) = result.results.first() {
            Ok(serde_json::json!({
                "total_trades": self.get_i64_field(row, "total_trades", 0),
                "total_pnl": self.get_f64_field(row, "total_pnl", 0.0),
                "avg_pnl": self.get_f64_field(row, "avg_pnl", 0.0),
                "win_rate": self.get_f64_field(row, "win_rate", 0.0),
                "avg_trade_size": self.get_f64_field(row, "avg_trade_size", 0.0),
                "user_id": user_id
            }))
        } else {
            Ok(serde_json::json!({
                "total_trades": 0,
                "total_pnl": 0.0,
                "avg_pnl": 0.0,
                "win_rate": 0.0,
                "avg_trade_size": 0.0,
                "user_id": user_id
            }))
        }
    }

    /// Get user analytics data (non-WASM fallback)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_user_analytics(&self, user_id: &str) -> ArbitrageResult<serde_json::Value> {
        let _ = user_id; // Suppress unused variable warning
        Err(ArbitrageError::not_implemented(
            "Analytics operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
        ))
    }

    /// Get aggregated analytics for all users
    #[cfg(target_arch = "wasm32")]
    pub async fn get_all_user_analytics(&self) -> ArbitrageResult<serde_json::Value> {
        let query = r#"
            SELECT 
                COUNT(DISTINCT up.user_id) as total_users,
                COUNT(DISTINCT CASE WHEN up.is_active = 1 THEN up.user_id END) as active_users,
                COUNT(DISTINCT CASE WHEN up.subscription_tier IN ('Premium', 'Enterprise', 'SuperAdmin') THEN up.user_id END) as premium_users,
                COALESCE(SUM(ta.pnl_usdt), 0.0) as total_system_pnl,
                COUNT(DISTINCT ta.id) as total_system_trades
            FROM user_profiles up 
            LEFT JOIN trading_analytics ta ON up.user_id = ta.user_id
        "#;

        let result = self.query_internal(query, &[]).await?;

        if let Some(row) = result.results.first() {
            Ok(serde_json::json!({
                "total_users": self.get_i64_field(row, "total_users", 0),
                "active_users": self.get_i64_field(row, "active_users", 0),
                "premium_users": self.get_i64_field(row, "premium_users", 0),
                "total_system_pnl": self.get_f64_field(row, "total_system_pnl", 0.0),
                "total_system_trades": self.get_i64_field(row, "total_system_trades", 0),
                "timestamp": chrono::Utc::now().timestamp()
            }))
        } else {
            Ok(serde_json::json!({
                "total_users": 0,
                "active_users": 0,
                "premium_users": 0,
                "total_system_pnl": 0.0,
                "total_system_trades": 0,
                "timestamp": chrono::Utc::now().timestamp()
            }))
        }
    }

    /// Get aggregated analytics for all users (non-WASM fallback)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_all_user_analytics(&self) -> ArbitrageResult<serde_json::Value> {
        Err(ArbitrageError::not_implemented(
            "Analytics operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
        ))
    }

    /// Get all users (admin function)
    #[cfg(target_arch = "wasm32")]
    pub async fn get_all_users(&self) -> ArbitrageResult<Vec<serde_json::Value>> {
        let query = r#"
            SELECT 
                user_id,
                telegram_user_id,
                telegram_username,
                subscription_tier,
                is_active,
                created_at,
                last_active,
                total_trades,
                account_balance_usdt
            FROM user_profiles 
            ORDER BY created_at DESC 
            LIMIT 1000
        "#;

        let result = self.query_internal(query, &[]).await?;

        let users: Vec<serde_json::Value> = result.results.into_iter().map(|row| {
            serde_json::json!({
                "user_id": self.get_optional_string_field(&row, "user_id").unwrap_or_default(),
                "telegram_user_id": self.get_i64_field(&row, "telegram_user_id", 0),
                "subscription_tier": self.get_optional_string_field(&row, "subscription_tier").unwrap_or_else(|| "Free".to_string()),
                "is_active": self.get_bool_field(&row, "is_active", false),
                "created_at": self.get_optional_string_field(&row, "created_at").unwrap_or_default(),
                "last_active": self.get_optional_string_field(&row, "last_active").unwrap_or_default(),
                "total_trades": self.get_i64_field(&row, "total_trades", 0),
                "account_balance_usdt": self.get_f64_field(&row, "account_balance_usdt", 0.0)
            })
        }).collect();

        Ok(users)
    }

    /// Get all users (admin function) (non-WASM fallback)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_all_users(&self) -> ArbitrageResult<Vec<serde_json::Value>> {
        Err(ArbitrageError::not_implemented(
            "User management operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
        ))
    }

    /// Get all invitations (admin function)
    #[cfg(target_arch = "wasm32")]
    pub async fn get_all_invitations(&self) -> ArbitrageResult<Vec<serde_json::Value>> {
        let query = r#"
            SELECT 
                code,
                created_by,
                subscription_tier,
                max_uses,
                current_uses,
                expires_at,
                is_active,
                created_at
            FROM invitation_codes 
            ORDER BY created_at DESC 
            LIMIT 1000
        "#;

        let result = self.query_internal(query, &[]).await?;

        let invitations: Vec<serde_json::Value> = result.results.into_iter().map(|row| {
            serde_json::json!({
                "code": self.get_optional_string_field(&row, "code").unwrap_or_default(),
                "created_by": self.get_optional_string_field(&row, "created_by").unwrap_or_default(),
                "subscription_tier": self.get_optional_string_field(&row, "subscription_tier").unwrap_or_else(|| "Free".to_string()),
                "max_uses": self.get_i64_field(&row, "max_uses", 1) as i32,
                "current_uses": self.get_i64_field(&row, "current_uses", 0) as i32,
                "created_at": self.get_optional_string_field(&row, "created_at").unwrap_or_default(),
                "is_active": self.get_bool_field(&row, "is_active", false),
                "expires_at": self.get_optional_string_field(&row, "expires_at").unwrap_or_default()
            })
        }).collect();

        Ok(invitations)
    }

    /// Get all invitations (admin function) (non-WASM fallback)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_all_invitations(&self) -> ArbitrageResult<Vec<serde_json::Value>> {
        Err(ArbitrageError::not_implemented(
            "Invitation management operations are not supported on non-WASM platforms. This service requires Cloudflare Workers environment.".to_string()
        ))
    }
}

/// Query result wrapper for compatibility with dynamic config service
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub results: Vec<HashMap<String, Value>>,
}
