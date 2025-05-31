// User Repository - Specialized User Data Access Component
// Handles user profiles, API keys, trading preferences, and user-related operations

use super::{utils::*, Repository, RepositoryConfig, RepositoryHealth, RepositoryMetrics};
use crate::services::core::user::user_trading_preferences::UserTradingPreferences;
use crate::types::{
    RiskProfile, SubscriptionTier, UserAccessLevel, UserApiKey, UserPreferences, UserProfile,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use std::collections::HashMap;
use std::sync::Arc;
use worker::{wasm_bindgen::JsValue, D1Database};

/// Configuration for UserRepository
#[derive(Debug, Clone)]
pub struct UserRepositoryConfig {
    pub connection_pool_size: u32,
    pub batch_size: usize,
    pub cache_ttl_seconds: u64,
    pub enable_caching: bool,
    pub enable_metrics: bool,
    pub max_retries: u32,
    pub timeout_seconds: u64,
}

impl Default for UserRepositoryConfig {
    fn default() -> Self {
        Self {
            connection_pool_size: 20,
            batch_size: 50,
            cache_ttl_seconds: 300, // 5 minutes
            enable_caching: true,
            enable_metrics: true,
            max_retries: 3,
            timeout_seconds: 30,
        }
    }
}

impl RepositoryConfig for UserRepositoryConfig {
    fn validate(&self) -> ArbitrageResult<()> {
        if self.connection_pool_size == 0 {
            return Err(validation_error(
                "connection_pool_size",
                "must be greater than 0",
            ));
        }
        if self.batch_size == 0 {
            return Err(validation_error("batch_size", "must be greater than 0"));
        }
        if self.cache_ttl_seconds == 0 {
            return Err(validation_error(
                "cache_ttl_seconds",
                "must be greater than 0",
            ));
        }
        Ok(())
    }

    fn connection_pool_size(&self) -> u32 {
        self.connection_pool_size
    }

    fn batch_size(&self) -> usize {
        self.batch_size
    }

    fn cache_ttl_seconds(&self) -> u64 {
        self.cache_ttl_seconds
    }
}

/// User repository for specialized user data operations
pub struct UserRepository {
    db: Arc<D1Database>,
    config: UserRepositoryConfig,
    metrics: Arc<std::sync::Mutex<RepositoryMetrics>>,
    cache: Option<worker::kv::KvStore>,
}

impl UserRepository {
    /// Create new UserRepository
    pub fn new(db: Arc<D1Database>, config: UserRepositoryConfig) -> Self {
        let metrics = RepositoryMetrics {
            repository_name: "user_repository".to_string(),
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            avg_response_time_ms: 0.0,
            operations_per_second: 0.0,
            cache_hit_rate: 0.0,
            last_updated: current_timestamp_ms(),
        };

        Self {
            db,
            config,
            metrics: Arc::new(std::sync::Mutex::new(metrics)),
            cache: None,
        }
    }

    /// Set cache store for caching operations
    pub fn with_cache(mut self, cache: worker::kv::KvStore) -> Self {
        self.cache = Some(cache);
        self
    }

    // ============= USER PROFILE OPERATIONS =============

    /// Create a new user profile
    pub async fn create_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate profile
        self.validate_user_profile(profile)?;

        let result = self.store_user_profile_internal(profile).await;

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// Update an existing user profile
    pub async fn update_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate profile
        self.validate_user_profile(profile)?;

        let result = self.store_user_profile_internal(profile).await;

        // Invalidate cache if enabled
        if self.config.enable_caching && self.cache.is_some() {
            let _ = self.invalidate_user_cache(&profile.user_id).await;
        }

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// Get user profile by user ID
    pub async fn get_user_profile(&self, user_id: &str) -> ArbitrageResult<Option<UserProfile>> {
        let start_time = current_timestamp_ms();

        // Try cache first if enabled
        if self.config.enable_caching {
            if let Some(cached_profile) = self.get_cached_user_profile(user_id).await? {
                self.update_metrics(start_time, true).await;
                return Ok(Some(cached_profile));
            }
        }

        let result = self.get_user_profile_from_db(user_id).await;

        // Cache result if successful and caching is enabled
        if let Ok(Some(ref profile)) = result {
            if self.config.enable_caching {
                let _ = self.cache_user_profile(profile).await;
            }
        }

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// Get user profile by Telegram ID
    pub async fn get_user_by_telegram_id(
        &self,
        telegram_id: i64,
    ) -> ArbitrageResult<Option<UserProfile>> {
        let start_time = current_timestamp_ms();

        let stmt = self
            .db
            .prepare("SELECT * FROM user_profiles WHERE telegram_id = ?");

        let result = stmt
            .bind(&[JsValue::from_f64(telegram_id as f64)])
            .map_err(|e| database_error("bind parameters", e))?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        let profile_result = match result {
            Some(row) => {
                let profile = self.row_to_user_profile(row)?;
                Ok(Some(profile))
            }
            None => Ok(None),
        };

        // Update metrics
        self.update_metrics(start_time, profile_result.is_ok())
            .await;

        profile_result
    }

    /// List user profiles with pagination
    pub async fn list_user_profiles(
        &self,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ArbitrageResult<Vec<UserProfile>> {
        let start_time = current_timestamp_ms();
        let limit = limit.unwrap_or(50).min(1000); // Cap at 1000 for safety
        let offset = offset.unwrap_or(0).max(0);

        let stmt = self
            .db
            .prepare("SELECT * FROM user_profiles ORDER BY created_at DESC LIMIT ? OFFSET ?");

        let results = stmt
            .bind(&[
                JsValue::from_f64(limit as f64),
                JsValue::from_f64(offset as f64),
            ])
            .map_err(|e| database_error("bind parameters", e))?
            .all()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let mut profiles = Vec::new();
        let results_vec = results.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            if let Ok(profile) = self.row_to_user_profile(row) {
                profiles.push(profile);
            }
        }

        // Update metrics
        self.update_metrics(start_time, true).await;

        Ok(profiles)
    }

    /// Delete user profile
    pub async fn delete_user_profile(&self, user_id: &str) -> ArbitrageResult<bool> {
        let start_time = current_timestamp_ms();

        let stmt = self
            .db
            .prepare("DELETE FROM user_profiles WHERE user_id = ?");

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .run()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let success = result
            .meta()
            .map(|meta| meta.unwrap().changes.unwrap_or(0) > 0)
            .unwrap_or(false);

        // Invalidate cache if enabled
        if success && self.config.enable_caching {
            let _ = self.invalidate_user_cache(user_id).await;
        }

        self.update_metrics(start_time, success).await;
        Ok(success)
    }

    // ============= TRADING PREFERENCES OPERATIONS =============

    /// Store user trading preferences
    pub async fn store_trading_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate preferences
        self.validate_trading_preferences(preferences)?;

        // Serialize preferences to JSON
        let preferences_json = serde_json::to_string(preferences).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize preferences: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO user_trading_preferences (
                user_id, preferences_data, created_at, updated_at
            ) VALUES (?, ?, ?, ?)",
        );

        let now = current_timestamp_ms() as i64;
        let result = stmt
            .bind(&[
                preferences.user_id.clone().into(),
                preferences_json.into(),
                now.into(),
                now.into(),
            ])
            .map_err(|e| database_error("bind parameters", e))?
            .run()
            .await
            .map_err(|e| database_error("execute query", e));

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result.map(|_| ())
    }

    /// Get user trading preferences
    pub async fn get_trading_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<UserTradingPreferences>> {
        let start_time = current_timestamp_ms();

        let stmt = self
            .db
            .prepare("SELECT * FROM user_trading_preferences WHERE user_id = ?");

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        let preferences_result = match result {
            Some(row) => {
                let preferences = self.row_to_trading_preferences(row)?;
                Ok(Some(preferences))
            }
            None => Ok(None),
        };

        // Update metrics
        self.update_metrics(start_time, preferences_result.is_ok())
            .await;

        preferences_result
    }

    /// Update trading preferences
    pub async fn update_trading_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<()> {
        self.store_trading_preferences(preferences).await
    }

    /// Get or create default trading preferences
    pub async fn get_or_create_trading_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserTradingPreferences> {
        if let Some(preferences) = self.get_trading_preferences(user_id).await? {
            Ok(preferences)
        } else {
            // Create default preferences
            let default_preferences = UserTradingPreferences::new_default(user_id.to_string());
            self.store_trading_preferences(&default_preferences).await?;
            Ok(default_preferences)
        }
    }

    /// Delete trading preferences for user
    pub async fn delete_trading_preferences(&self, user_id: &str) -> ArbitrageResult<bool> {
        let start_time = current_timestamp_ms();

        let stmt = self
            .db
            .prepare("DELETE FROM user_trading_preferences WHERE user_id = ?");
        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .run()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let success = result
            .meta()
            .map(|meta| meta.unwrap().changes.unwrap_or(0) > 0)
            .unwrap_or(false);

        // Invalidate cache if enabled
        if success && self.config.enable_caching {
            let _ = self.invalidate_user_cache(user_id).await;
        }

        self.update_metrics(start_time, success).await;
        Ok(success)
    }

    // ============= API KEY OPERATIONS =============

    /// Store user API key
    pub async fn store_user_api_key(
        &self,
        user_id: &str,
        api_key: &UserApiKey,
    ) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate API key
        self.validate_api_key(api_key)?;

        // Serialize API key to JSON
        let api_key_json = serde_json::to_string(api_key).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize API key: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO user_api_keys (
                user_id, exchange_id, api_key_data, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?)",
        );

        let now = current_timestamp_ms() as i64;
        let result = stmt
            .bind(&[
                user_id.into(),
                api_key.provider.to_string().into(),
                api_key_json.into(),
                now.into(),
                now.into(),
            ])
            .map_err(|e| database_error("bind parameters", e))?
            .run()
            .await
            .map_err(|e| database_error("execute query", e));

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result.map(|_| ())
    }

    /// Get user API keys
    pub async fn get_user_api_keys(&self, user_id: &str) -> ArbitrageResult<Vec<UserApiKey>> {
        let start_time = current_timestamp_ms();

        let stmt = self
            .db
            .prepare("SELECT * FROM user_api_keys WHERE user_id = ?");

        let results = stmt
            .bind(&[user_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .all()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let mut api_keys = Vec::new();
        let results_vec = results.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            if let Ok(api_key) = self.row_to_user_api_key(row) {
                api_keys.push(api_key);
            }
        }

        // Update metrics
        self.update_metrics(start_time, true).await;

        Ok(api_keys)
    }

    // ============= INTERNAL HELPER METHODS =============

    async fn store_user_profile_internal(&self, profile: &UserProfile) -> ArbitrageResult<()> {
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

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO user_profiles (
                user_id, telegram_id, username, api_keys, 
                subscription_tier, trading_preferences, 
                created_at, updated_at, last_login_at, account_status, 
                beta_expires_at, account_balance_usdt
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            profile.user_id.clone().into(),
            JsValue::from_f64(profile.telegram_user_id.unwrap_or(0) as f64),
            profile.telegram_username.clone().unwrap_or_default().into(),
            api_keys_json.into(),
            configuration_json.into(), // write to configuration
            subscription_json.into(),  // write to trading_preferences
            (profile.created_at as i64).into(),
            (profile.updated_at as i64).into(),
            (profile.last_active.unwrap_or(0) as i64).into(),
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
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    async fn get_user_profile_from_db(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<UserProfile>> {
        let stmt = self
            .db
            .prepare("SELECT * FROM user_profiles WHERE user_id = ?");

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        match result {
            Some(row) => {
                let profile = self.row_to_user_profile(row)?;
                Ok(Some(profile))
            }
            None => Ok(None),
        }
    }

    fn row_to_user_profile(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<UserProfile> {
        let user_id = get_string_field(&row, "user_id")?;
        let telegram_user_id = get_i64_field(&row, "telegram_id", 0);
        let telegram_username = get_optional_string_field(&row, "username");

        let _api_keys: HashMap<String, serde_json::Value> =
            get_json_field(&row, "api_keys", HashMap::new());
        let subscription = get_json_field(&row, "subscription_tier", serde_json::Value::Null);
        let configuration = get_json_field(&row, "trading_preferences", serde_json::Value::Null);

        let created_at = get_i64_field(&row, "created_at", 0) as u64;
        let updated_at = get_i64_field(&row, "updated_at", 0) as u64;
        let last_active = get_i64_field(&row, "last_login_at", 0) as u64;

        let account_status =
            get_string_field(&row, "account_status").unwrap_or_else(|_| "active".to_string());
        let is_active = account_status == "active";

        let beta_expires_at = {
            let timestamp = get_i64_field(&row, "beta_expires_at", 0);
            if timestamp > 0 {
                Some(timestamp as u64)
            } else {
                None
            }
        };

        let account_balance_usdt = get_f64_field(&row, "account_balance_usdt", 0.0);

        Ok(UserProfile {
            user_id,
            telegram_user_id: Some(telegram_user_id),
            username: telegram_username.clone(),
            email: None, // Email not stored in this table
            subscription_tier: serde_json::from_value(subscription.clone())
                .unwrap_or(SubscriptionTier::Free),
            access_level: UserAccessLevel::Free, // Default access level
            is_active,
            created_at,
            last_login: Some(last_active),
            preferences: UserPreferences::default(),
            risk_profile: RiskProfile::default(),
            configuration: serde_json::from_value(configuration).unwrap_or_default(),
            api_keys: Vec::new(), // Will be populated separately
            invitation_code: get_optional_string_field(&row, "invitation_code"),
            beta_expires_at,
            updated_at,
            last_active: Some(last_active),
            total_trades: get_i64_field(&row, "total_trades", 0) as u32,
            total_pnl_usdt: get_f64_field(&row, "total_pnl_usdt", 0.0),
            account_balance_usdt,
            profile_metadata: get_optional_string_field(&row, "profile_metadata"),
            telegram_username,
            subscription: serde_json::from_value(subscription).unwrap_or_default(),
            group_admin_roles: Vec::new(), // Will be populated separately
        })
    }

    fn row_to_trading_preferences(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<UserTradingPreferences> {
        let preferences_data = get_string_field(&row, "preferences_data")?;
        serde_json::from_str(&preferences_data).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to deserialize trading preferences: {}", e))
        })
    }

    fn row_to_user_api_key(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<UserApiKey> {
        let api_key_data = get_string_field(&row, "api_key_data")?;
        serde_json::from_str(&api_key_data).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to deserialize API key: {}", e))
        })
    }

    // ============= VALIDATION METHODS =============

    fn validate_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
        validate_required_string(&profile.user_id, "user_id")?;

        if profile.account_balance_usdt < 0.0 {
            return Err(validation_error(
                "account_balance_usdt",
                "cannot be negative",
            ));
        }

        Ok(())
    }

    fn validate_trading_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<()> {
        validate_required_string(&preferences.user_id, "user_id")?;
        Ok(())
    }

    fn validate_api_key(&self, api_key: &UserApiKey) -> ArbitrageResult<()> {
        validate_required_string(&api_key.encrypted_key, "encrypted_key")?;
        if let Some(ref secret) = api_key.encrypted_secret {
            validate_required_string(secret, "encrypted_secret")?;
        }
        Ok(())
    }

    // ============= CACHING METHODS =============

    async fn cache_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("user_profile:{}", profile.user_id);
            let profile_json = serde_json::to_string(profile).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize profile for cache: {}", e))
            })?;

            cache
                .put(&cache_key, &profile_json)
                .map_err(|e| {
                    ArbitrageError::cache_error(format!("Failed to cache profile: {}", e))
                })?
                .execute()
                .await
                .map_err(|e| {
                    ArbitrageError::cache_error(format!("Failed to execute cache put: {}", e))
                })?;
        }
        Ok(())
    }

    async fn get_cached_user_profile(&self, user_id: &str) -> ArbitrageResult<Option<UserProfile>> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("user_profile:{}", user_id);

            match cache.get(&cache_key).text().await {
                Ok(Some(profile_json)) => {
                    let profile: UserProfile =
                        serde_json::from_str(&profile_json).map_err(|e| {
                            ArbitrageError::parse_error(format!(
                                "Failed to deserialize cached profile: {}",
                                e
                            ))
                        })?;
                    Ok(Some(profile))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(ArbitrageError::cache_error(format!(
                    "Failed to get cached profile: {}",
                    e
                ))),
            }
        } else {
            Ok(None)
        }
    }

    async fn invalidate_user_cache(&self, user_id: &str) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("user_profile:{}", user_id);
            cache.delete(&cache_key).await.map_err(|e| {
                ArbitrageError::cache_error(format!("Failed to invalidate cache: {}", e))
            })?;
        }
        Ok(())
    }

    // ============= METRICS METHODS =============

    async fn update_metrics(&self, start_time: u64, success: bool) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_operations += 1;

            if success {
                metrics.successful_operations += 1;
            } else {
                metrics.failed_operations += 1;
            }

            let response_time = current_timestamp_ms() - start_time;
            let total_time = metrics.avg_response_time_ms * (metrics.total_operations - 1) as f64
                + response_time as f64;
            metrics.avg_response_time_ms = total_time / metrics.total_operations as f64;

            metrics.last_updated = current_timestamp_ms();
        }
    }
}

impl Repository for UserRepository {
    fn name(&self) -> &str {
        "user_repository"
    }

    async fn health_check(&self) -> ArbitrageResult<RepositoryHealth> {
        let start_time = current_timestamp_ms();

        // Test basic database connectivity
        let test_result = self
            .db
            .prepare("SELECT 1 as test")
            .first::<HashMap<String, serde_json::Value>>(None)
            .await;

        let is_healthy = test_result.is_ok();
        let response_time = current_timestamp_ms() - start_time;

        let metrics = self.metrics.lock().unwrap();
        let _success_rate = if metrics.total_operations > 0 {
            metrics.successful_operations as f64 / metrics.total_operations as f64
        } else {
            1.0
        };

        Ok(RepositoryHealth {
            repository_name: self.name().to_string(),
            is_healthy,
            database_healthy: is_healthy,
            cache_healthy: true,
            last_health_check: current_timestamp_ms(),
            response_time_ms: response_time as f64,
            error_rate: if is_healthy { 0.0 } else { 100.0 },
        })
    }

    async fn get_metrics(&self) -> RepositoryMetrics {
        self.metrics.lock().unwrap().clone()
    }

    async fn initialize(&self) -> ArbitrageResult<()> {
        // Create tables if they don't exist
        let create_user_profiles_table = "
            CREATE TABLE IF NOT EXISTS user_profiles (
                user_id TEXT PRIMARY KEY,
                telegram_id INTEGER,
                username TEXT,
                api_keys TEXT,
                subscription_tier TEXT,
                trading_preferences TEXT,
                created_at INTEGER,
                updated_at INTEGER,
                last_login_at INTEGER,
                account_status TEXT DEFAULT 'active',
                beta_expires_at INTEGER,
                account_balance_usdt REAL DEFAULT 0.0
            )
        ";

        let create_trading_preferences_table = "
            CREATE TABLE IF NOT EXISTS user_trading_preferences (
                user_id TEXT PRIMARY KEY,
                preferences_data TEXT,
                created_at INTEGER,
                updated_at INTEGER
            )
        ";

        let create_api_keys_table = "
            CREATE TABLE IF NOT EXISTS user_api_keys (
                user_id TEXT,
                exchange_id TEXT,
                api_key_data TEXT,
                created_at INTEGER,
                updated_at INTEGER,
                PRIMARY KEY (user_id, exchange_id)
            )
        ";

        // Execute table creation
        self.db
            .exec(create_user_profiles_table)
            .await
            .map_err(|e| database_error("create user_profiles table", e))?;

        self.db
            .exec(create_trading_preferences_table)
            .await
            .map_err(|e| database_error("create user_trading_preferences table", e))?;

        self.db
            .exec(create_api_keys_table)
            .await
            .map_err(|e| database_error("create user_api_keys table", e))?;

        // Create indexes for better performance
        let create_telegram_index = "CREATE INDEX IF NOT EXISTS idx_user_profiles_telegram_id ON user_profiles(telegram_id)";
        let create_status_index =
            "CREATE INDEX IF NOT EXISTS idx_user_profiles_status ON user_profiles(account_status)";

        self.db
            .exec(create_telegram_index)
            .await
            .map_err(|e| database_error("create telegram_id index", e))?;

        self.db
            .exec(create_status_index)
            .await
            .map_err(|e| database_error("create status index", e))?;

        Ok(())
    }

    async fn shutdown(&self) -> ArbitrageResult<()> {
        // Clear cache if enabled
        if self.config.enable_caching && self.cache.is_some() {
            // Cache cleanup would go here if needed
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_repository_config_validation() {
        let mut config = UserRepositoryConfig::default();
        assert!(config.validate().is_ok());

        config.connection_pool_size = 0;
        assert!(config.validate().is_err());

        config.connection_pool_size = 10;
        config.batch_size = 0;
        assert!(config.validate().is_err());

        config.batch_size = 50;
        config.cache_ttl_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_user_profile_validation() {
        let config = UserRepositoryConfig::default();
        let db = Arc::new(unsafe { std::mem::zeroed() }); // Mock for testing
        let repo = UserRepository::new(db, config);

        let mut profile = UserProfile {
            user_id: "test_user_123".to_string(),
            telegram_user_id: Some(123456789),
            username: Some("testuser".to_string()),
            email: Some("test@example.com".to_string()),
            subscription_tier: SubscriptionTier::Free,
            access_level: UserAccessLevel::Registered,
            is_active: true,
            created_at: current_timestamp_ms(),
            last_login: None,
            preferences: UserPreferences::default(),
            risk_profile: RiskProfile::default(),
            subscription: UserSubscription::default(),
            configuration: UserConfiguration::default(),
            api_keys: Vec::new(),
            invitation_code: None,
            beta_expires_at: None,
            updated_at: current_timestamp_ms(),
            last_active: Some(current_timestamp_ms()),
            total_trades: 0,
            total_pnl_usdt: 0.0,
            account_balance_usdt: 0.0,
            profile_metadata: None,
            telegram_username: Some("testuser".to_string()),
            group_admin_roles: Vec::new(),
        };

        assert!(repo.validate_user_profile(&profile).is_ok());

        profile.user_id = "".to_string();
        assert!(repo.validate_user_profile(&profile).is_err());

        profile.user_id = "test_user_123".to_string();
        profile.account_balance_usdt = -10.0;
        assert!(repo.validate_user_profile(&profile).is_err());
    }
}
