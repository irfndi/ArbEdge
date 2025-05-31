// Database Manager - Central Repository Coordinator
// Manages all specialized repository components and provides unified access

use super::{
    utils::*, InvitationRepository, InvitationRepositoryConfig, Repository, RepositoryConfig,
    RepositoryHealth, RepositoryMetrics, UserRepository, UserRepositoryConfig,
};
use crate::services::core::user::user_trading_preferences::UserTradingPreferences;
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::{kv::KvStore, wasm_bindgen::JsValue, D1Database};

/// Configuration for DatabaseManager
#[derive(Debug, Clone)]
pub struct DatabaseManagerConfig {
    pub enable_health_monitoring: bool,
    pub health_check_interval_seconds: u64,
    pub enable_metrics_collection: bool,
    pub enable_auto_recovery: bool,
    pub max_retry_attempts: u32,
    pub connection_timeout_seconds: u64,
    pub enable_connection_pooling: bool,
    pub enable_transaction_support: bool,
    pub enable_migration_support: bool,
}

impl Default for DatabaseManagerConfig {
    fn default() -> Self {
        Self {
            enable_health_monitoring: true,
            health_check_interval_seconds: 30,
            enable_metrics_collection: true,
            enable_auto_recovery: true,
            max_retry_attempts: 3,
            connection_timeout_seconds: 30,
            enable_connection_pooling: true,
            enable_transaction_support: true,
            enable_migration_support: true,
        }
    }
}

impl RepositoryConfig for DatabaseManagerConfig {
    fn validate(&self) -> ArbitrageResult<()> {
        if self.health_check_interval_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "health_check_interval_seconds must be greater than 0",
            ));
        }
        if self.max_retry_attempts == 0 {
            return Err(ArbitrageError::validation_error(
                "max_retry_attempts must be greater than 0",
            ));
        }
        if self.connection_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "connection_timeout_seconds must be greater than 0",
            ));
        }
        Ok(())
    }

    fn connection_pool_size(&self) -> u32 {
        50 // Manager uses larger pool to coordinate all repositories
    }

    fn batch_size(&self) -> usize {
        100 // Manager can handle larger batches
    }

    fn cache_ttl_seconds(&self) -> u64 {
        300 // 5 minutes for manager-level caching
    }
}

/// Repository registration information
#[derive(Debug, Clone)]
pub struct RepositoryRegistration {
    pub name: String,
    pub repository_type: String,
    pub version: String,
    pub description: String,
    pub is_critical: bool,
    pub auto_initialize: bool,
    pub dependencies: Vec<String>,
    pub configuration: HashMap<String, serde_json::Value>,
}

/// Repository registry for managing multiple repositories
pub struct RepositoryRegistry {
    registrations: HashMap<String, RepositoryRegistration>,
    health_status: HashMap<String, RepositoryHealth>,
    metrics: HashMap<String, RepositoryMetrics>,
}

impl Default for RepositoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RepositoryRegistry {
    pub fn new() -> Self {
        Self {
            registrations: HashMap::new(),
            health_status: HashMap::new(),
            metrics: HashMap::new(),
        }
    }

    /// Register a repository metadata
    pub fn register_metadata(
        &mut self,
        name: String,
        registration: RepositoryRegistration,
    ) -> ArbitrageResult<()> {
        if self.registrations.contains_key(&name) {
            return Err(ArbitrageError::validation_error(format!(
                "Repository '{}' already registered",
                name
            )));
        }

        self.registrations.insert(name, registration);
        Ok(())
    }

    /// Get all repository names
    pub fn get_repository_names(&self) -> Vec<String> {
        self.registrations.keys().cloned().collect()
    }

    /// Get repository registration
    pub fn get_registration(&self, name: &str) -> Option<&RepositoryRegistration> {
        self.registrations.get(name)
    }

    /// Update health status
    pub fn update_health_status(&mut self, name: String, health: RepositoryHealth) {
        self.health_status.insert(name, health);
    }

    /// Get health status
    pub fn get_health_status(&self, name: &str) -> Option<&RepositoryHealth> {
        self.health_status.get(name)
    }

    /// Update metrics
    pub fn update_metrics(&mut self, name: String, metrics: RepositoryMetrics) {
        self.metrics.insert(name, metrics);
    }

    /// Get metrics
    pub fn get_metrics(&self, name: &str) -> Option<&RepositoryMetrics> {
        self.metrics.get(name)
    }

    /// Get all health statuses
    pub fn get_all_health_statuses(&self) -> &HashMap<String, RepositoryHealth> {
        &self.health_status
    }

    /// Get all metrics
    pub fn get_all_metrics(&self) -> &HashMap<String, RepositoryMetrics> {
        &self.metrics
    }
}

/// Database manager for coordinating all repositories
#[derive(Clone)]
pub struct DatabaseManager {
    db: Arc<D1Database>,
    config: DatabaseManagerConfig,
    registry: Arc<std::sync::Mutex<RepositoryRegistry>>,
    cache: Option<KvStore>,

    // Specialized repositories
    user_repository: Option<Arc<UserRepository>>,
    invitation_repository: Option<Arc<InvitationRepository>>,

    // Manager metrics
    metrics: Arc<std::sync::Mutex<RepositoryMetrics>>,
    startup_time: u64,
}

impl DatabaseManager {
    /// Create new DatabaseManager
    pub fn new(db: Arc<D1Database>, config: DatabaseManagerConfig) -> Self {
        let metrics = RepositoryMetrics {
            repository_name: "database_manager".to_string(),
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
            registry: Arc::new(std::sync::Mutex::new(RepositoryRegistry::new())),
            cache: None,
            user_repository: None,
            invitation_repository: None,
            metrics: Arc::new(std::sync::Mutex::new(metrics)),
            startup_time: current_timestamp_ms(),
        }
    }

    /// Set cache store
    pub fn with_cache(mut self, cache: KvStore) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Initialize all repositories
    pub async fn initialize_repositories(&mut self) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Initialize UserRepository
        let user_config = UserRepositoryConfig::default();
        let mut user_repo = UserRepository::new(self.db.clone(), user_config);

        if let Some(ref cache) = self.cache {
            user_repo = user_repo.with_cache(cache.clone());
        }

        user_repo.initialize().await?;
        let user_repo_arc = Arc::new(user_repo);
        self.user_repository = Some(user_repo_arc.clone());

        // Register UserRepository metadata
        let user_registration = RepositoryRegistration {
            name: "user_repository".to_string(),
            repository_type: "UserRepository".to_string(),
            version: "1.0.0".to_string(),
            description: "Manages user profiles, preferences, and API keys".to_string(),
            is_critical: true,
            auto_initialize: true,
            dependencies: vec![],
            configuration: HashMap::new(),
        };

        if let Ok(mut registry) = self.registry.lock() {
            registry.register_metadata("user_repository".to_string(), user_registration)?;
        }

        // Initialize InvitationRepository
        let invitation_config = InvitationRepositoryConfig::default();
        let mut invitation_repo = InvitationRepository::new(self.db.clone(), invitation_config);

        if let Some(ref cache) = self.cache {
            invitation_repo = invitation_repo.with_cache(cache.clone());
        }

        invitation_repo.initialize().await?;
        let invitation_repo_arc = Arc::new(invitation_repo);
        self.invitation_repository = Some(invitation_repo_arc.clone());

        // Register InvitationRepository metadata
        let invitation_registration = RepositoryRegistration {
            name: "invitation_repository".to_string(),
            repository_type: "InvitationRepository".to_string(),
            version: "1.0.0".to_string(),
            description: "Manages invitation codes, usage tracking, and user invitations"
                .to_string(),
            is_critical: false,
            auto_initialize: true,
            dependencies: vec!["user_repository".to_string()],
            configuration: HashMap::new(),
        };

        if let Ok(mut registry) = self.registry.lock() {
            registry
                .register_metadata("invitation_repository".to_string(), invitation_registration)?;
        }

        // Update metrics
        self.update_metrics(start_time, true).await;

        Ok(())
    }

    /// Get UserRepository
    pub fn get_user_repository(&self) -> Option<Arc<UserRepository>> {
        self.user_repository.clone()
    }

    /// Get InvitationRepository
    pub fn get_invitation_repository(&self) -> Option<Arc<InvitationRepository>> {
        self.invitation_repository.clone()
    }

    /// Perform health check on all repositories
    pub async fn health_check_all_repositories(
        &self,
    ) -> ArbitrageResult<HashMap<String, RepositoryHealth>> {
        let start_time = current_timestamp_ms();
        let mut health_results = HashMap::new();

        // Check UserRepository
        if let Some(ref user_repo) = self.user_repository {
            match user_repo.health_check().await {
                Ok(health) => {
                    health_results.insert("user_repository".to_string(), health);
                }
                Err(_e) => {
                    let health = RepositoryHealth {
                        repository_name: "user_repository".to_string(),
                        is_healthy: false,
                        database_healthy: false,
                        cache_healthy: true,
                        last_health_check: current_timestamp_ms(),
                        response_time_ms: (current_timestamp_ms() - start_time) as f64,
                        error_rate: 100.0,
                    };
                    health_results.insert("user_repository".to_string(), health);
                }
            }
        }

        // Check InvitationRepository
        if let Some(ref invitation_repo) = self.invitation_repository {
            match invitation_repo.health_check().await {
                Ok(health) => {
                    health_results.insert("invitation_repository".to_string(), health);
                }
                Err(_e) => {
                    let health = RepositoryHealth {
                        repository_name: "invitation_repository".to_string(),
                        is_healthy: false,
                        database_healthy: false,
                        cache_healthy: true,
                        last_health_check: current_timestamp_ms(),
                        response_time_ms: (current_timestamp_ms() - start_time) as f64,
                        error_rate: 100.0,
                    };
                    health_results.insert("invitation_repository".to_string(), health);
                }
            }
        }

        // Update manager metrics
        self.update_metrics(start_time, true).await;

        Ok(health_results)
    }

    /// Get metrics from all repositories
    pub async fn get_all_repository_metrics(&self) -> HashMap<String, RepositoryMetrics> {
        let mut all_metrics = HashMap::new();

        // Get UserRepository metrics
        if let Some(ref user_repo) = self.user_repository {
            let metrics = user_repo.get_metrics().await;
            all_metrics.insert("user_repository".to_string(), metrics);
        }

        // Get InvitationRepository metrics
        if let Some(ref invitation_repo) = self.invitation_repository {
            let metrics = invitation_repo.get_metrics().await;
            all_metrics.insert("invitation_repository".to_string(), metrics);
        }

        all_metrics
    }

    /// Execute a query with parameters (legacy compatibility method)
    pub async fn query(
        &self,
        query: &str,
        params: &[worker::wasm_bindgen::JsValue],
    ) -> ArbitrageResult<worker::D1Result> {
        let start_time = current_timestamp_ms();

        let stmt = self.db.prepare(query);
        let result = stmt
            .bind(params)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)));

        self.update_metrics(start_time, result.is_ok()).await;
        result
    }

    /// Execute a statement (legacy compatibility method)
    pub async fn execute(
        &self,
        query: &str,
        params: &[worker::wasm_bindgen::JsValue],
    ) -> ArbitrageResult<worker::D1Result> {
        let start_time = current_timestamp_ms();

        let stmt = self.db.prepare(query);
        let result = stmt
            .bind(params)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .run()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute statement: {}", e))
            });

        self.update_metrics(start_time, result.is_ok()).await;
        result
    }

    /// Execute a transaction (legacy compatibility method)
    pub async fn execute_transaction<F, T>(&self, transaction_fn: F) -> ArbitrageResult<T>
    where
        F: FnOnce(&worker::D1Database) -> ArbitrageResult<T>,
    {
        let start_time = current_timestamp_ms();

        // Note: Cloudflare D1 doesn't support explicit transactions yet
        // This is a compatibility wrapper that executes the function directly
        let result = transaction_fn(&self.db);

        self.update_metrics(start_time, result.is_ok()).await;
        result
    }

    /// Get database health summary
    pub async fn get_database_health_summary(&self) -> ArbitrageResult<DatabaseHealthSummary> {
        let health_results = self.health_check_all_repositories().await?;

        let total_repositories = health_results.len() as u32;
        let healthy_repositories = health_results.values().filter(|h| h.is_healthy).count() as u32;
        let unhealthy_repositories = total_repositories - healthy_repositories;

        let overall_healthy = unhealthy_repositories == 0;
        let health_percentage = if total_repositories > 0 {
            (healthy_repositories as f64 / total_repositories as f64) * 100.0
        } else {
            100.0
        };

        Ok(DatabaseHealthSummary {
            overall_healthy,
            total_repositories,
            healthy_repositories,
            unhealthy_repositories,
            health_percentage,
            last_updated: current_timestamp_ms(),
            repository_health: health_results,
        })
    }

    /// Shutdown all repositories
    pub async fn shutdown_all_repositories(&self) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();
        let mut errors = Vec::new();

        // Shutdown InvitationRepository
        if let Some(ref invitation_repo) = self.invitation_repository {
            if let Err(e) = invitation_repo.shutdown().await {
                errors.push(format!("invitation_repository: {}", e));
            }
        }

        // Shutdown UserRepository
        if let Some(ref user_repo) = self.user_repository {
            if let Err(e) = user_repo.shutdown().await {
                errors.push(format!("user_repository: {}", e));
            }
        }

        // Update metrics
        self.update_metrics(start_time, errors.is_empty()).await;

        if !errors.is_empty() {
            return Err(ArbitrageError::database_error(format!(
                "Shutdown errors: {}",
                errors.join(", ")
            )));
        }

        Ok(())
    }

    /// Get database manager configuration
    pub fn get_config(&self) -> &DatabaseManagerConfig {
        &self.config
    }

    /// Get uptime in seconds
    pub fn get_uptime_seconds(&self) -> u64 {
        (current_timestamp_ms() - self.startup_time) / 1000
    }

    // ============= INTERNAL HELPER METHODS =============

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

    // ============= AI DATA REPOSITORY METHODS =============

    /// Get trading analytics for user (AI Intelligence compatibility)
    pub async fn get_trading_analytics(
        &self,
        _user_id: &str,
        limit: Option<i32>,
    ) -> ArbitrageResult<Vec<serde_json::Value>> {
        // This is a compatibility method for AI Intelligence
        // In a real implementation, this would query trading analytics data
        let _limit = limit.unwrap_or(100);

        // For now, return empty analytics as this data would come from a trading analytics repository
        // that hasn't been implemented yet
        Ok(vec![])
    }

    /// Store AI opportunity enhancement (AI Intelligence compatibility)
    pub async fn store_ai_opportunity_enhancement(
        &self,
        enhancement: &crate::services::core::ai::ai_intelligence::AiOpportunityEnhancement,
    ) -> ArbitrageResult<()> {
        // This would typically be handled by an AI data repository
        // For now, we'll store it in a generic AI enhancements table
        let enhancement_data = serde_json::to_string(enhancement).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize enhancement: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO ai_enhancements (
                enhancement_id, user_id, enhancement_type, enhancement_data,
                confidence_score, created_at
            ) VALUES (?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            enhancement.opportunity_id.clone().into(),
            enhancement.user_id.clone().into(),
            "opportunity_enhancement".into(),
            enhancement_data.into(),
            enhancement.ai_confidence_score.into(),
            (enhancement.analysis_timestamp as i64).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Store AI portfolio analysis (AI Intelligence compatibility)
    pub async fn store_ai_portfolio_analysis(
        &self,
        analysis: &crate::services::core::ai::ai_intelligence::AiPortfolioAnalysis,
    ) -> ArbitrageResult<()> {
        let analysis_data = serde_json::to_string(analysis).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize analysis: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO ai_portfolio_analysis (
                analysis_id, user_id, analysis_data, performance_score, created_at
            ) VALUES (?, ?, ?, ?, ?)",
        );

        let analysis_id = format!(
            "portfolio_{}_{}",
            analysis.user_id, analysis.analysis_timestamp
        );

        // Calculate a performance score based on diversification and risk scores
        let performance_score = (analysis.diversification_score
            + (1.0 - analysis.correlation_risk_score)
            + (1.0 - analysis.concentration_risk_score))
            / 3.0;

        stmt.bind(&[
            analysis_id.into(),
            analysis.user_id.clone().into(),
            analysis_data.into(),
            performance_score.into(),
            (analysis.analysis_timestamp as i64).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Store AI performance insights (AI Intelligence compatibility)
    pub async fn store_ai_performance_insights(
        &self,
        insights: &crate::services::core::ai::ai_intelligence::AiPerformanceInsights,
    ) -> ArbitrageResult<()> {
        let insights_data = serde_json::to_string(insights).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize insights: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO ai_performance_insights (
                insight_id, user_id, insights_data, performance_score, created_at
            ) VALUES (?, ?, ?, ?, ?)",
        );

        let insight_id = format!("insights_{}_{}", insights.user_id, insights.generated_at);

        stmt.bind(&[
            insight_id.into(),
            insights.user_id.clone().into(),
            insights_data.into(),
            insights.performance_score.into(),
            (insights.generated_at as i64).into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Store AI parameter suggestion (AI Intelligence compatibility)
    pub async fn store_ai_parameter_suggestion(
        &self,
        user_id: &str,
        suggestion: &crate::services::core::ai::ai_intelligence::ParameterSuggestion,
    ) -> ArbitrageResult<()> {
        let suggestion_data = serde_json::to_string(suggestion).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize suggestion: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO ai_parameter_suggestions (
                suggestion_id, user_id, parameter_name, suggestion_data, 
                confidence_score, created_at
            ) VALUES (?, ?, ?, ?, ?, ?)",
        );

        let suggestion_id = format!(
            "param_{}_{}",
            suggestion.parameter_name,
            chrono::Utc::now().timestamp_millis()
        );

        stmt.bind(&[
            suggestion_id.into(),
            user_id.into(),
            suggestion.parameter_name.clone().into(),
            suggestion_data.into(),
            suggestion.confidence.into(),
            chrono::Utc::now().timestamp_millis().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get invitation usage by user (Invitation Service compatibility)
    pub async fn get_invitation_usage_by_user(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<
        Option<crate::services::core::invitation::invitation_service::InvitationUsage>,
    > {
        if let Some(ref invitation_repo) = self.invitation_repository {
            invitation_repo.get_invitation_usage_by_user(user_id).await
        } else {
            // Fallback: query directly from database
            let stmt = self.db.prepare(
                "SELECT * FROM invitation_usage WHERE user_id = ? ORDER BY used_at DESC LIMIT 1",
            );

            let result = stmt
                .bind(&[user_id.into()])
                .map_err(|e| {
                    ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
                })?
                .first::<HashMap<String, serde_json::Value>>(None)
                .await
                .map_err(|e| {
                    ArbitrageError::database_error(format!("Failed to execute query: {}", e))
                })?;

            if let Some(row) = result {
                // Convert row to InvitationUsage
                let invitation_id = row
                    .get("invitation_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let telegram_id = row.get("telegram_id").and_then(|v| v.as_i64()).unwrap_or(0);

                let used_at_ms = row.get("used_at").and_then(|v| v.as_i64()).unwrap_or(0);

                let beta_expires_at_ms = row
                    .get("beta_expires_at")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                let used_at = chrono::DateTime::from_timestamp_millis(used_at_ms)
                    .unwrap_or(chrono::Utc::now());

                let beta_expires_at = chrono::DateTime::from_timestamp_millis(beta_expires_at_ms)
                    .unwrap_or(chrono::Utc::now());

                Ok(Some(
                    crate::services::core::invitation::invitation_service::InvitationUsage {
                        invitation_id,
                        user_id: user_id.to_string(),
                        telegram_id,
                        used_at,
                        beta_expires_at,
                    },
                ))
            } else {
                Ok(None)
            }
        }
    }

    /// Update user profile (User Profile Service compatibility)
    pub async fn update_user_profile(
        &self,
        user_id: &str,
        profile_data: &serde_json::Value,
    ) -> ArbitrageResult<()> {
        if let Some(ref user_repo) = self.user_repository {
            // Use the user repository if available
            // Convert JSON to UserProfile first
            let profile: crate::types::UserProfile = serde_json::from_value(profile_data.clone())
                .map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to parse profile: {}", e))
            })?;

            user_repo.update_user_profile(&profile).await
        } else {
            // Fallback: direct database update
            let profile_str = serde_json::to_string(profile_data).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize profile: {}", e))
            })?;

            let stmt = self.db.prepare(
                "UPDATE user_profiles SET profile_data = ?, updated_at = ? WHERE user_id = ?",
            );

            stmt.bind(&[
                profile_str.into(),
                chrono::Utc::now().to_rfc3339().into(),
                user_id.into(),
            ])
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
    }

    /// Store user API key (User Profile Service compatibility)
    pub async fn store_user_api_key(
        &self,
        user_id: &str,
        api_key: &crate::types::UserApiKey,
    ) -> ArbitrageResult<()> {
        let api_key_data = serde_json::to_string(api_key).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize API key: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO user_api_keys (
                user_id, api_key_data, created_at, updated_at
            ) VALUES (?, ?, ?, ?)",
        );

        stmt.bind(&[
            user_id.into(),
            api_key_data.into(),
            chrono::Utc::now().to_rfc3339().into(),
            chrono::Utc::now().to_rfc3339().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Create invitation code (User Profile Service compatibility)
    pub async fn create_invitation_code(
        &self,
        invitation: &crate::types::InvitationCode,
    ) -> ArbitrageResult<()> {
        if let Some(ref invitation_repo) = self.invitation_repository {
            invitation_repo.create_invitation_code(invitation).await
        } else {
            // Fallback: direct database insert
            let invitation_data = serde_json::to_string(invitation).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize invitation: {}", e))
            })?;

            let stmt = self.db.prepare(
                "INSERT INTO invitation_codes (
                    code, invitation_data, created_at, updated_at
                ) VALUES (?, ?, ?, ?)",
            );

            stmt.bind(&[
                invitation.code.clone().into(),
                invitation_data.into(),
                chrono::Utc::now().to_rfc3339().into(),
                chrono::Utc::now().to_rfc3339().into(),
            ])
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
    }

    /// Get invitation code (User Profile Service compatibility)
    pub async fn get_invitation_code(
        &self,
        code: &str,
    ) -> ArbitrageResult<Option<crate::types::InvitationCode>> {
        if let Some(ref invitation_repo) = self.invitation_repository {
            invitation_repo.get_invitation_code(code).await
        } else {
            // Fallback: direct database query
            let stmt = self
                .db
                .prepare("SELECT invitation_data FROM invitation_codes WHERE code = ?");

            let result = stmt
                .bind(&[code.into()])
                .map_err(|e| {
                    ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
                })?
                .first::<HashMap<String, serde_json::Value>>(None)
                .await
                .map_err(|e| {
                    ArbitrageError::database_error(format!("Failed to execute query: {}", e))
                })?;

            if let Some(row) = result {
                if let Some(invitation_data) = row.get("invitation_data") {
                    let invitation: crate::types::InvitationCode =
                        serde_json::from_value(invitation_data.clone()).map_err(|e| {
                            ArbitrageError::parse_error(format!(
                                "Failed to parse invitation: {}",
                                e
                            ))
                        })?;
                    Ok(Some(invitation))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        }
    }

    /// Update invitation code (User Profile Service compatibility)
    pub async fn update_invitation_code(
        &self,
        invitation: &crate::types::InvitationCode,
    ) -> ArbitrageResult<()> {
        if let Some(ref invitation_repo) = self.invitation_repository {
            invitation_repo.update_invitation_code(invitation).await
        } else {
            // Fallback: direct database update
            let invitation_data = serde_json::to_string(invitation).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize invitation: {}", e))
            })?;

            let stmt = self.db.prepare(
                "UPDATE invitation_codes SET invitation_data = ?, updated_at = ? WHERE code = ?",
            );

            stmt.bind(&[
                invitation_data.into(),
                chrono::Utc::now().to_rfc3339().into(),
                invitation.code.clone().into(),
            ])
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
    }

    /// List user profiles (User Profile Service compatibility)
    pub async fn list_user_profiles(
        &self,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ArbitrageResult<Vec<crate::types::UserProfile>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let stmt = self.db.prepare(
            "SELECT profile_data FROM user_profiles ORDER BY created_at DESC LIMIT ? OFFSET ?",
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
        for row in result.results::<HashMap<String, serde_json::Value>>()? {
            if let Some(profile_data) = row.get("profile_data") {
                let profile: crate::types::UserProfile =
                    serde_json::from_value(profile_data.clone()).map_err(|e| {
                        ArbitrageError::parse_error(format!("Failed to parse profile: {}", e))
                    })?;
                profiles.push(profile);
            }
        }

        Ok(profiles)
    }

    /// Store trading preferences (Trading Preferences Service compatibility)
    pub async fn store_trading_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<()> {
        let preferences_data = serde_json::to_string(preferences).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize preferences: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO trading_preferences (
                user_id, preferences_data, created_at, updated_at
            ) VALUES (?, ?, ?, ?)",
        );

        stmt.bind(&[
            preferences.user_id.clone().into(),
            preferences_data.into(),
            chrono::Utc::now().to_rfc3339().into(),
            chrono::Utc::now().to_rfc3339().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get trading preferences (Trading Preferences Service compatibility)
    pub async fn get_trading_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<UserTradingPreferences>> {
        let stmt = self
            .db
            .prepare("SELECT preferences_data FROM trading_preferences WHERE user_id = ?");

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        if let Some(row) = result {
            if let Some(preferences_data) = row.get("preferences_data") {
                let preferences: UserTradingPreferences =
                    serde_json::from_value(preferences_data.clone()).map_err(|e| {
                        ArbitrageError::parse_error(format!("Failed to parse preferences: {}", e))
                    })?;
                Ok(Some(preferences))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Get or create trading preferences (Trading Preferences Service compatibility)
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

    /// Update trading preferences (Trading Preferences Service compatibility)
    pub async fn update_trading_preferences(
        &self,
        preferences: &UserTradingPreferences,
    ) -> ArbitrageResult<()> {
        self.store_trading_preferences(preferences).await
    }

    /// Delete trading preferences (Trading Preferences Service compatibility)
    pub async fn delete_trading_preferences(&self, user_id: &str) -> ArbitrageResult<()> {
        let stmt = self
            .db
            .prepare("DELETE FROM trading_preferences WHERE user_id = ?");

        stmt.bind(&[user_id.into()])
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

    /// Store AI analysis audit (AI Exchange Router compatibility)
    pub async fn store_ai_analysis_audit(
        &self,
        user_id: &str,
        analysis_type: &str,
        analysis_data: &serde_json::Value,
        confidence_score: f64,
    ) -> ArbitrageResult<()> {
        let analysis_data_str = serde_json::to_string(analysis_data).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize analysis data: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT INTO ai_analysis_audit (
                user_id, analysis_type, analysis_data, confidence_score, created_at
            ) VALUES (?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            user_id.into(),
            analysis_type.into(),
            analysis_data_str.into(),
            confidence_score.into(),
            chrono::Utc::now().to_rfc3339().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Store opportunity analysis (AI Exchange Router compatibility)
    pub async fn store_opportunity_analysis(
        &self,
        analysis: &serde_json::Value,
    ) -> ArbitrageResult<()> {
        let analysis_data = serde_json::to_string(analysis).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize analysis: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT INTO opportunity_analysis (
                analysis_data, created_at
            ) VALUES (?, ?)",
        );

        stmt.bind(&[analysis_data.into(), chrono::Utc::now().to_rfc3339().into()])
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

    /// Get user opportunity preferences (Opportunity Categorization compatibility)
    pub async fn get_user_opportunity_preferences(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<serde_json::Value>> {
        let stmt = self
            .db
            .prepare("SELECT preferences_data FROM user_opportunity_preferences WHERE user_id = ?");

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        if let Some(row) = result {
            Ok(row.get("preferences_data").cloned())
        } else {
            Ok(None)
        }
    }

    /// Store user opportunity preferences (Opportunity Categorization compatibility)
    pub async fn store_user_opportunity_preferences(
        &self,
        user_id: &str,
        preferences: &serde_json::Value,
    ) -> ArbitrageResult<()> {
        let preferences_data = serde_json::to_string(preferences).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize preferences: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO user_opportunity_preferences (
                user_id, preferences_data, created_at, updated_at
            ) VALUES (?, ?, ?, ?)",
        );

        stmt.bind(&[
            user_id.into(),
            preferences_data.into(),
            chrono::Utc::now().to_rfc3339().into(),
            chrono::Utc::now().to_rfc3339().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get user profile by user ID
    pub async fn get_user_profile(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<crate::types::UserProfile>> {
        if let Some(ref user_repo) = self.user_repository {
            user_repo.get_user_profile(user_id).await
        } else {
            // Fallback: direct database query
            let stmt = self
                .db
                .prepare("SELECT profile_data FROM user_profiles WHERE user_id = ?");

            let result = stmt
                .bind(&[user_id.into()])
                .map_err(|e| {
                    ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
                })?
                .first::<HashMap<String, serde_json::Value>>(None)
                .await
                .map_err(|e| {
                    ArbitrageError::database_error(format!("Failed to execute query: {}", e))
                })?;

            if let Some(row) = result {
                if let Some(profile_data) = row.get("profile_data") {
                    let profile: crate::types::UserProfile =
                        serde_json::from_value(profile_data.clone()).map_err(|e| {
                            ArbitrageError::parse_error(format!("Failed to parse profile: {}", e))
                        })?;
                    Ok(Some(profile))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        }
    }

    /// Get user profile by telegram ID
    pub async fn get_user_by_telegram_id(
        &self,
        telegram_user_id: i64,
    ) -> ArbitrageResult<Option<crate::types::UserProfile>> {
        if let Some(ref user_repo) = self.user_repository {
            user_repo.get_user_by_telegram_id(telegram_user_id).await
        } else {
            // Fallback: direct database query
            let stmt = self
                .db
                .prepare("SELECT profile_data FROM user_profiles WHERE telegram_user_id = ?");

            let result = stmt
                .bind(&[telegram_user_id.into()])
                .map_err(|e| {
                    ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
                })?
                .first::<HashMap<String, serde_json::Value>>(None)
                .await
                .map_err(|e| {
                    ArbitrageError::database_error(format!("Failed to execute query: {}", e))
                })?;

            if let Some(row) = result {
                if let Some(profile_data) = row.get("profile_data") {
                    let profile: crate::types::UserProfile =
                        serde_json::from_value(profile_data.clone()).map_err(|e| {
                            ArbitrageError::parse_error(format!("Failed to parse profile: {}", e))
                        })?;
                    Ok(Some(profile))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        }
    }

    /// Create user profile
    pub async fn create_user_profile(
        &self,
        profile: &crate::types::UserProfile,
    ) -> ArbitrageResult<()> {
        if let Some(ref user_repo) = self.user_repository {
            user_repo.create_user_profile(profile).await
        } else {
            // Fallback: direct database insert
            let profile_data = serde_json::to_string(profile).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize profile: {}", e))
            })?;

            let stmt = self.db.prepare(
                "INSERT INTO user_profiles (
                    user_id, telegram_user_id, profile_data, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?)",
            );

            stmt.bind(&[
                profile.user_id.clone().into(),
                profile.telegram_user_id.into(),
                profile_data.into(),
                chrono::Utc::now().to_rfc3339().into(),
                chrono::Utc::now().to_rfc3339().into(),
            ])
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
    }

    /// Prepare a SQL statement for execution
    pub fn prepare(&self, sql: &str) -> worker::D1PreparedStatement {
        self.db.prepare(sql)
    }

    /// Store configuration template (Dynamic Config Service compatibility)
    pub async fn store_config_template(&self, template: &serde_json::Value) -> ArbitrageResult<()> {
        // For now, store as a generic configuration template
        let template_data = serde_json::to_string(template).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize template: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO config_templates (
                template_id, template_data, created_at, updated_at
            ) VALUES (?, ?, ?, ?)",
        );

        let template_id = format!("template_{}", chrono::Utc::now().timestamp_millis());

        stmt.bind(&[
            template_id.into(),
            template_data.into(),
            chrono::Utc::now().to_rfc3339().into(),
            chrono::Utc::now().to_rfc3339().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get configuration template (Dynamic Config Service compatibility)
    pub async fn get_config_template(
        &self,
        template_id: &str,
    ) -> ArbitrageResult<Option<serde_json::Value>> {
        let stmt = self
            .db
            .prepare("SELECT template_data FROM config_templates WHERE template_id = ?");

        let result = stmt
            .bind(&[template_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        if let Some(row) = result {
            if let Some(template_data) = row.get("template_data") {
                let template: serde_json::Value = serde_json::from_value(template_data.clone())
                    .map_err(|e| {
                        ArbitrageError::parse_error(format!("Failed to parse template: {}", e))
                    })?;
                Ok(Some(template))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Health check method for database connectivity
    pub async fn health_check(&self) -> bool {
        // Try a simple query to test database connectivity
        (self
            .db
            .prepare("SELECT 1")
            .first::<serde_json::Value>(None)
            .await)
            .is_ok()
    }

    /// Store configuration preset (Dynamic Config Service compatibility)
    pub async fn store_config_preset(
        &self,
        preset: &crate::services::core::user::dynamic_config::ConfigPreset,
    ) -> ArbitrageResult<()> {
        let _preset_data = serde_json::to_string(preset).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize preset: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO config_presets (
                preset_id, name, description, template_id, parameter_values, 
                risk_level, target_audience, created_at, is_system_preset
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            JsValue::from_str(&preset.preset_id),
            JsValue::from_str(&preset.name),
            JsValue::from_str(&preset.description),
            JsValue::from_str(&preset.template_id),
            JsValue::from_str(&serde_json::to_string(&preset.parameter_values)?),
            JsValue::from_str(&format!("{:?}", preset.risk_level)),
            JsValue::from_str(&preset.target_audience),
            JsValue::from_f64(preset.created_at as f64),
            JsValue::from_bool(preset.is_system_preset),
        ])?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to store preset: {}", e)))?;

        Ok(())
    }

    /// Deactivate user config (Dynamic Config Service compatibility)
    pub async fn deactivate_user_config(
        &self,
        user_id: &str,
        template_id: &str,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "UPDATE user_config_instances SET is_active = false, updated_at = ? 
             WHERE user_id = ? AND template_id = ? AND is_active = true",
        );

        stmt.bind(&[
            chrono::Utc::now().to_rfc3339().into(),
            user_id.into(),
            template_id.into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Store user config instance (Dynamic Config Service compatibility)
    pub async fn store_user_config_instance(
        &self,
        instance: &crate::services::core::user::dynamic_config::UserConfigInstance,
    ) -> ArbitrageResult<()> {
        let _instance_data = serde_json::to_string(instance).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize instance: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO user_config_instances (
                instance_id, user_id, template_id, preset_id, parameter_values,
                version, is_active, created_at, updated_at, rollback_data
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            instance.instance_id.clone().into(),
            instance.user_id.clone().into(),
            instance.template_id.clone().into(),
            instance.preset_id.clone().unwrap_or_default().into(),
            serde_json::to_string(&instance.parameter_values)?.into(),
            (instance.version as i64).into(),
            instance.is_active.into(),
            (instance.created_at as i64).into(),
            (instance.updated_at as i64).into(),
            instance.rollback_data.clone().unwrap_or_default().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Get user config instance (Dynamic Config Service compatibility)
    pub async fn get_user_config_instance(
        &self,
        user_id: &str,
        template_id: &str,
    ) -> ArbitrageResult<Option<crate::services::core::user::dynamic_config::UserConfigInstance>>
    {
        let stmt = self.db.prepare(
            "SELECT * FROM user_config_instances 
             WHERE user_id = ? AND template_id = ? AND is_active = true 
             ORDER BY version DESC LIMIT 1",
        );

        let result = stmt
            .bind(&[user_id.into(), template_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        if let Some(row) = result {
            // Convert row to UserConfigInstance
            let instance_id = row
                .get("instance_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let user_id = row
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let template_id = row
                .get("template_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let preset_id = row
                .get("preset_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let parameter_values: std::collections::HashMap<String, serde_json::Value> = row
                .get("parameter_values")
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            let version = row.get("version").and_then(|v| v.as_i64()).unwrap_or(1) as u32;
            let is_active = row
                .get("is_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let created_at = row.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0) as u64;
            let updated_at = row.get("updated_at").and_then(|v| v.as_i64()).unwrap_or(0) as u64;
            let rollback_data = row
                .get("rollback_data")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            Ok(Some(
                crate::services::core::user::dynamic_config::UserConfigInstance {
                    instance_id,
                    user_id,
                    template_id,
                    preset_id,
                    parameter_values,
                    version,
                    is_active,
                    created_at,
                    updated_at,
                    rollback_data,
                },
            ))
        } else {
            Ok(None)
        }
    }
}

impl Repository for DatabaseManager {
    fn name(&self) -> &str {
        "database_manager"
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
        // DatabaseManager initialization is handled in initialize_repositories
        Ok(())
    }

    async fn shutdown(&self) -> ArbitrageResult<()> {
        self.shutdown_all_repositories().await
    }
}

/// Database health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHealthSummary {
    pub overall_healthy: bool,
    pub total_repositories: u32,
    pub healthy_repositories: u32,
    pub unhealthy_repositories: u32,
    pub health_percentage: f64,
    pub last_updated: u64,
    pub repository_health: HashMap<String, RepositoryHealth>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_manager_config_validation() {
        let mut config = DatabaseManagerConfig::default();
        assert!(config.validate().is_ok());

        config.health_check_interval_seconds = 0;
        assert!(config.validate().is_err());

        config.health_check_interval_seconds = 30;
        config.max_retry_attempts = 0;
        assert!(config.validate().is_err());

        config.max_retry_attempts = 3;
        config.connection_timeout_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_repository_registry() {
        let mut registry = RepositoryRegistry::new();
        assert_eq!(registry.get_repository_names().len(), 0);

        // Note: We can't easily test repository registration without implementing
        // a mock repository that implements the Repository trait
    }

    #[test]
    fn test_repository_registration() {
        let registration = RepositoryRegistration {
            name: "test_repo".to_string(),
            repository_type: "TestRepository".to_string(),
            version: "1.0.0".to_string(),
            description: "Test repository".to_string(),
            is_critical: true,
            auto_initialize: true,
            dependencies: vec!["dependency1".to_string()],
            configuration: HashMap::new(),
        };

        assert_eq!(registration.name, "test_repo");
        assert_eq!(registration.repository_type, "TestRepository");
        assert!(registration.is_critical);
        assert!(registration.auto_initialize);
        assert_eq!(registration.dependencies.len(), 1);
    }

    #[test]
    fn test_database_health_summary() {
        let health_summary = DatabaseHealthSummary {
            overall_healthy: true,
            total_repositories: 2,
            healthy_repositories: 2,
            unhealthy_repositories: 0,
            health_percentage: 100.0,
            last_updated: current_timestamp_ms(),
            repository_health: HashMap::new(),
        };

        assert!(health_summary.overall_healthy);
        assert_eq!(health_summary.total_repositories, 2);
        assert_eq!(health_summary.healthy_repositories, 2);
        assert_eq!(health_summary.unhealthy_repositories, 0);
        assert_eq!(health_summary.health_percentage, 100.0);
    }
}
