// Config Repository - Specialized Configuration Data Access Component
// Handles dynamic config templates, presets, and user config instances

use super::{utils::*, Repository, RepositoryConfig, RepositoryHealth, RepositoryMetrics};
use crate::services::core::user::dynamic_config::{
    ConfigPreset, DynamicConfigTemplate as ConfigTemplate, UserConfigInstance,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::D1Database;

/// Configuration for ConfigRepository
#[derive(Debug, Clone)]
pub struct ConfigRepositoryConfig {
    pub connection_pool_size: u32,
    pub batch_size: usize,
    pub cache_ttl_seconds: u64,
    pub enable_caching: bool,
    pub enable_metrics: bool,
    pub max_retries: u32,
    pub timeout_seconds: u64,
    pub enable_template_validation: bool,
    pub enable_preset_caching: bool,
}

impl Default for ConfigRepositoryConfig {
    fn default() -> Self {
        Self {
            connection_pool_size: 15, // Medium pool for config operations
            batch_size: 30,           // Moderate batches for config operations
            cache_ttl_seconds: 1800,  // 30 minutes - longer cache for config data
            enable_caching: true,
            enable_metrics: true,
            max_retries: 3,
            timeout_seconds: 30,
            enable_template_validation: true,
            enable_preset_caching: true,
        }
    }
}

impl RepositoryConfig for ConfigRepositoryConfig {
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
        if self.enable_caching && self.cache_ttl_seconds == 0 {
            return Err(validation_error(
                "cache_ttl_seconds",
                "must be greater than 0 when caching is enabled",
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

/// Configuration template summary for admin dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigTemplateSummary {
    pub template_id: String,
    pub name: String,
    pub category: String,
    pub version: String,
    pub is_active: bool,
    pub usage_count: u32,
    pub created_at: u64,
    pub last_updated: u64,
}

/// Configuration preset summary for user selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPresetSummary {
    pub preset_id: String,
    pub name: String,
    pub description: String,
    pub template_id: String,
    pub is_default: bool,
    pub usage_count: u32,
    pub created_at: u64,
}

/// User configuration instance with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfigInstanceWithMetadata {
    pub instance: UserConfigInstance,
    pub template_name: String,
    pub preset_name: Option<String>,
    pub last_applied: Option<u64>,
    pub is_active: bool,
}

/// Configuration repository for specialized configuration data operations
pub struct ConfigRepository {
    db: Arc<D1Database>,
    config: ConfigRepositoryConfig,
    metrics: Arc<std::sync::Mutex<RepositoryMetrics>>,
    cache: Option<worker::kv::KvStore>,
}

impl ConfigRepository {
    /// Create new ConfigRepository
    pub fn new(db: Arc<D1Database>, config: ConfigRepositoryConfig) -> Self {
        let metrics = RepositoryMetrics {
            repository_name: "config_repository".to_string(),
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

    // ============= CONFIG TEMPLATE OPERATIONS =============

    /// Store a configuration template
    pub async fn store_config_template(&self, template: &ConfigTemplate) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate template if enabled
        if self.config.enable_template_validation {
            self.validate_config_template(template)?;
        }

        let result = self.store_config_template_internal(template).await;

        // Cache the template if successful and caching is enabled
        if result.is_ok() && self.config.enable_caching {
            let _ = self.cache_config_template(template).await;
        }

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// Get configuration template by ID
    pub async fn get_config_template(
        &self,
        template_id: &str,
    ) -> ArbitrageResult<Option<ConfigTemplate>> {
        let start_time = current_timestamp_ms();

        // Try cache first if enabled
        if self.config.enable_caching {
            if let Some(cached_template) = self.get_cached_config_template(template_id).await? {
                self.update_metrics(start_time, true).await;
                return Ok(Some(cached_template));
            }
        }

        let result = self.get_config_template_from_db(template_id).await;

        // Cache result if successful and caching is enabled
        if let Ok(Some(ref template)) = result {
            if self.config.enable_caching {
                let _ = self.cache_config_template(template).await;
            }
        }

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// Store a configuration preset
    pub async fn store_config_preset(&self, preset: &ConfigPreset) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate preset
        self.validate_config_preset(preset)?;

        let result = self.store_config_preset_internal(preset).await;

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// Store user configuration instance
    pub async fn store_user_config_instance(
        &self,
        instance: &UserConfigInstance,
    ) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate instance
        self.validate_user_config_instance(instance)?;

        let result = self.store_user_config_instance_internal(instance).await;

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    // ============= INTERNAL HELPER METHODS =============

    async fn store_config_template_internal(
        &self,
        template: &ConfigTemplate,
    ) -> ArbitrageResult<()> {
        let template_data_json = serde_json::to_string(&template.parameters).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize template parameters: {}", e))
        })?;

        let category_str = match template.category {
            crate::services::core::user::dynamic_config::ConfigCategory::RiskManagement => {
                "risk_management"
            }
            crate::services::core::user::dynamic_config::ConfigCategory::TradingStrategy => {
                "trading_strategy"
            }
            crate::services::core::user::dynamic_config::ConfigCategory::Notification => {
                "notification"
            }
            crate::services::core::user::dynamic_config::ConfigCategory::AI => "ai",
            crate::services::core::user::dynamic_config::ConfigCategory::Performance => {
                "performance"
            }
            crate::services::core::user::dynamic_config::ConfigCategory::Exchange => "exchange",
            crate::services::core::user::dynamic_config::ConfigCategory::Advanced => "advanced",
        };

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO config_templates (
                template_id, name, description, category, version,
                template_data, is_active, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            template.template_id.clone().into(),
            template.name.clone().into(),
            template.description.clone().into(),
            category_str.into(),
            template.version.clone().into(),
            template_data_json.into(),
            true.into(), // is_active - default to true
            (template.created_at as i64).into(),
            (template.created_at as i64).into(), // updated_at same as created_at for new records
        ])
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    async fn get_config_template_from_db(
        &self,
        template_id: &str,
    ) -> ArbitrageResult<Option<ConfigTemplate>> {
        let stmt = self
            .db
            .prepare("SELECT * FROM config_templates WHERE template_id = ?");

        let result = stmt
            .bind(&[template_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        match result {
            Some(row) => {
                let template = self.row_to_config_template(row)?;
                Ok(Some(template))
            }
            None => Ok(None),
        }
    }

    async fn store_config_preset_internal(&self, preset: &ConfigPreset) -> ArbitrageResult<()> {
        let preset_data_json = serde_json::to_string(&preset.parameter_values).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to serialize preset parameter values: {}",
                e
            ))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO config_presets (
                preset_id, template_id, name, description, preset_data,
                is_default, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            preset.preset_id.clone().into(),
            preset.template_id.clone().into(),
            preset.name.clone().into(),
            preset.description.clone().into(),
            preset_data_json.into(),
            preset.is_system_preset.into(),
            (preset.created_at as i64).into(),
        ])
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    async fn store_user_config_instance_internal(
        &self,
        instance: &UserConfigInstance,
    ) -> ArbitrageResult<()> {
        let config_data_json = serde_json::to_string(&instance.parameter_values).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to serialize config parameter values: {}",
                e
            ))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO user_config_instances (
                user_id, template_id, preset_id, config_data, is_active,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            instance.user_id.clone().into(),
            instance.template_id.clone().into(),
            instance.preset_id.clone().unwrap_or_default().into(),
            config_data_json.into(),
            instance.is_active.into(),
            (instance.created_at as i64).into(),
            (instance.updated_at as i64).into(),
        ])
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    fn row_to_config_template(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<ConfigTemplate> {
        let template_id = row
            .get("template_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let name = row
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let description = row
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let version = row
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let category_str = row
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("risk_management");
        let category = match category_str {
            "risk_management" => {
                crate::services::core::user::dynamic_config::ConfigCategory::RiskManagement
            }
            "trading_strategy" => {
                crate::services::core::user::dynamic_config::ConfigCategory::TradingStrategy
            }
            "notification" => {
                crate::services::core::user::dynamic_config::ConfigCategory::Notification
            }
            "ai" => crate::services::core::user::dynamic_config::ConfigCategory::AI,
            "performance" => {
                crate::services::core::user::dynamic_config::ConfigCategory::Performance
            }
            "exchange" => crate::services::core::user::dynamic_config::ConfigCategory::Exchange,
            "advanced" => crate::services::core::user::dynamic_config::ConfigCategory::Advanced,
            _ => crate::services::core::user::dynamic_config::ConfigCategory::RiskManagement,
        };
        let parameters_json = row
            .get("template_data")
            .and_then(|v| v.as_str())
            .unwrap_or("[]");
        let parameters = serde_json::from_str(parameters_json)
            .map_err(|e| ArbitrageError::parse_error(format!("Invalid template_data: {e}")))?;
        let created_at = row.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

        Ok(ConfigTemplate {
            template_id,
            name,
            description,
            version,
            category,
            parameters,
            created_at,
            created_by: "system".to_string(),
            is_system_template: true,
            subscription_tier_required: crate::types::SubscriptionTier::Free,
        })
    }

    // ============= VALIDATION METHODS =============

    fn validate_config_template(&self, template: &ConfigTemplate) -> ArbitrageResult<()> {
        validate_required_string(&template.template_id, "template_id")?;
        validate_required_string(&template.name, "name")?;
        validate_required_string(&template.version, "version")?;

        if template.parameters.is_empty() {
            return Err(validation_error("parameters", "cannot be empty"));
        }

        Ok(())
    }

    fn validate_config_preset(&self, preset: &ConfigPreset) -> ArbitrageResult<()> {
        validate_required_string(&preset.preset_id, "preset_id")?;
        validate_required_string(&preset.template_id, "template_id")?;
        validate_required_string(&preset.name, "name")?;

        if preset.parameter_values.is_empty() {
            return Err(validation_error("parameter_values", "cannot be empty"));
        }

        Ok(())
    }

    fn validate_user_config_instance(&self, instance: &UserConfigInstance) -> ArbitrageResult<()> {
        validate_required_string(&instance.user_id, "user_id")?;
        validate_required_string(&instance.template_id, "template_id")?;

        if instance.parameter_values.is_empty() {
            return Err(validation_error("parameter_values", "cannot be empty"));
        }

        Ok(())
    }

    async fn cache_config_template(&self, template: &ConfigTemplate) -> ArbitrageResult<()> {
        if let Some(cache) = &self.cache {
            let cache_key = format!("config_template:{}", template.template_id);
            let serialized = serde_json::to_string(template)
                .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

            let _ = cache
                .put(&cache_key, serialized)?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await;
        }
        Ok(())
    }

    async fn get_cached_config_template(
        &self,
        template_id: &str,
    ) -> ArbitrageResult<Option<ConfigTemplate>> {
        if let Some(cache) = &self.cache {
            let cache_key = format!("config_template:{}", template_id);
            if let Ok(Some(cached_data)) = cache.get(&cache_key).text().await {
                if let Ok(template) = serde_json::from_str::<ConfigTemplate>(&cached_data) {
                    return Ok(Some(template));
                }
            }
        }
        Ok(None)
    }

    async fn update_metrics(&self, start_time: u64, success: bool) {
        if let Ok(mut metrics) = self.metrics.lock() {
            let end_time = current_timestamp_ms();
            let response_time = end_time - start_time;

            metrics.total_operations += 1;
            if success {
                metrics.successful_operations += 1;
            } else {
                metrics.failed_operations += 1;
            }

            // Update average response time (exponential moving average)
            let alpha = 0.1;
            metrics.avg_response_time_ms =
                alpha * response_time as f64 + (1.0 - alpha) * metrics.avg_response_time_ms;

            metrics.last_updated = end_time;
        }
    }
}

impl Repository for ConfigRepository {
    fn name(&self) -> &str {
        "config_repository"
    }

    async fn health_check(&self) -> ArbitrageResult<RepositoryHealth> {
        let start_time = current_timestamp_ms();

        // Test database connectivity
        let db_healthy = (self
            .db
            .prepare("SELECT 1")
            .first::<serde_json::Value>(None)
            .await)
            .is_ok();

        let cache_healthy = if self.cache.is_some() {
            // Test cache connectivity
            true // Assume healthy for now
        } else {
            true // No cache configured
        };

        let end_time = current_timestamp_ms();
        let response_time = end_time - start_time;

        Ok(RepositoryHealth {
            repository_name: "config_repository".to_string(),
            is_healthy: db_healthy && cache_healthy,
            database_healthy: db_healthy,
            cache_healthy,
            last_health_check: end_time,
            response_time_ms: response_time as f64,
            error_rate: 0.0, // Would be calculated from metrics
        })
    }

    async fn get_metrics(&self) -> RepositoryMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            RepositoryMetrics::default()
        }
    }

    async fn initialize(&self) -> ArbitrageResult<()> {
        // Create tables if they don't exist
        let create_templates_table = self.db.prepare(
            "CREATE TABLE IF NOT EXISTS config_templates (
                template_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                category TEXT NOT NULL,
                version TEXT NOT NULL,
                template_data TEXT NOT NULL,
                is_active BOOLEAN DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        );

        create_templates_table
            .run()
            .await
            .map_err(|e| database_error("create config_templates table", e))?;

        let create_presets_table = self.db.prepare(
            "CREATE TABLE IF NOT EXISTS config_presets (
                preset_id TEXT PRIMARY KEY,
                template_id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                preset_data TEXT NOT NULL,
                is_default BOOLEAN DEFAULT 0,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (template_id) REFERENCES config_templates(template_id)
            )",
        );

        create_presets_table
            .run()
            .await
            .map_err(|e| database_error("create config_presets table", e))?;

        let create_instances_table = self.db.prepare(
            "CREATE TABLE IF NOT EXISTS user_config_instances (
                user_id TEXT NOT NULL,
                template_id TEXT NOT NULL,
                preset_id TEXT,
                config_data TEXT NOT NULL,
                is_active BOOLEAN DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (user_id, template_id),
                FOREIGN KEY (template_id) REFERENCES config_templates(template_id),
                FOREIGN KEY (preset_id) REFERENCES config_presets(preset_id)
            )",
        );

        create_instances_table
            .run()
            .await
            .map_err(|e| database_error("create user_config_instances table", e))?;

        Ok(())
    }

    async fn shutdown(&self) -> ArbitrageResult<()> {
        // No specific cleanup needed for this repository
        Ok(())
    }
}
