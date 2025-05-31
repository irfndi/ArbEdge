// AI Data Repository - Specialized AI Data Access Component
// Handles AI enhancements, insights, suggestions, and ML model data

use super::{utils::*, Repository, RepositoryConfig, RepositoryHealth, RepositoryMetrics};
use crate::services::core::ai::ai_intelligence::{
    AiOpportunityEnhancement, AiPerformanceInsights, ParameterSuggestion,
};
use crate::services::core::infrastructure::database_repositories::utils::{
    database_error, get_bool_field, get_f64_field, get_i64_field, get_json_field, get_string_field,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::{wasm_bindgen::JsValue, D1Database};

/// Configuration for AIDataRepository
#[derive(Debug, Clone)]
pub struct AIDataRepositoryConfig {
    pub connection_pool_size: u32,
    pub batch_size: usize,
    pub cache_ttl_seconds: u64,
    pub enable_caching: bool,
    pub enable_metrics: bool,
    pub max_retries: u32,
    pub timeout_seconds: u64,
    pub enable_ai_validation: bool,
    pub enable_insight_caching: bool,
}

impl Default for AIDataRepositoryConfig {
    fn default() -> Self {
        Self {
            connection_pool_size: 12, // Smaller pool for AI operations
            batch_size: 25,           // Smaller batches for AI data
            cache_ttl_seconds: 900,   // 15 minutes - medium cache for AI data
            enable_caching: true,
            enable_metrics: true,
            max_retries: 3,
            timeout_seconds: 30,
            enable_ai_validation: true,
            enable_insight_caching: true,
        }
    }
}

impl RepositoryConfig for AIDataRepositoryConfig {
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

/// AI enhancement summary for admin dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIEnhancementSummary {
    pub enhancement_id: String,
    pub user_id: String,
    pub enhancement_type: String,
    pub confidence_score: f64,
    pub is_active: bool,
    pub created_at: u64,
    pub last_applied: Option<u64>,
}

/// AI insight summary for user dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIInsightSummary {
    pub insight_id: String,
    pub user_id: String,
    pub insight_type: String,
    pub priority: String,
    pub confidence_score: f64,
    pub is_read: bool,
    pub created_at: u64,
}

/// AI data repository for specialized AI data operations
pub struct AIDataRepository {
    db: Arc<D1Database>,
    config: AIDataRepositoryConfig,
    metrics: Arc<std::sync::Mutex<RepositoryMetrics>>,
    cache: Option<worker::kv::KvStore>,
}

impl AIDataRepository {
    /// Create new AIDataRepository
    pub fn new(db: Arc<D1Database>, config: AIDataRepositoryConfig) -> Self {
        let metrics = RepositoryMetrics {
            repository_name: "ai_data_repository".to_string(),
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

    // ============= AI ENHANCEMENT OPERATIONS =============

    /// Store AI enhancement
    pub async fn store_ai_enhancement(
        &self,
        enhancement: &AiOpportunityEnhancement,
    ) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        if self.config.enable_ai_validation {
            self.validate_ai_enhancement(enhancement)?;
        }

        let result = self.store_ai_enhancement_internal(enhancement).await;

        if result.is_ok() && self.config.enable_caching {
            let _ = self.cache_ai_enhancement(enhancement).await;
        }

        self.update_metrics(start_time, result.is_ok()).await;
        result
    }

    /// Get AI enhancements for user
    pub async fn get_user_ai_enhancements(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> ArbitrageResult<Vec<AIEnhancementSummary>> {
        let start_time = current_timestamp_ms();
        let limit = limit.unwrap_or(50).min(200);

        let stmt = self.db.prepare(
            "SELECT enhancement_id, user_id, enhancement_type, confidence_score, 
                    is_active, created_at, last_applied_at
             FROM ai_enhancements 
             WHERE user_id = ? AND is_active = 1
             ORDER BY confidence_score DESC, created_at DESC 
             LIMIT ?",
        );

        let results = stmt
            .bind(&[user_id.into(), JsValue::from_f64(limit as f64)])
            .map_err(|e| database_error("bind parameters", e))?
            .all()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let mut enhancements = Vec::new();
        let results_vec = results.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            if let Ok(enhancement) = self.row_to_ai_enhancement_summary(row) {
                enhancements.push(enhancement);
            }
        }

        self.update_metrics(start_time, true).await;
        Ok(enhancements)
    }

    // ============= AI INSIGHT OPERATIONS =============

    /// Store AI insight
    pub async fn store_ai_insight(&self, insight: &AiPerformanceInsights) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        if self.config.enable_ai_validation {
            self.validate_ai_insight(insight)?;
        }

        let result = self.store_ai_insight_internal(insight).await;
        if result.is_ok() && self.config.enable_insight_caching {
            let _ = self.cache_ai_insight(insight).await;
        }

        self.update_metrics(start_time, result.is_ok()).await;
        result
    }

    /// Get AI insights for user
    pub async fn get_user_ai_insights(
        &self,
        user_id: &str,
        unread_only: bool,
    ) -> ArbitrageResult<Vec<AIInsightSummary>> {
        let start_time = current_timestamp_ms();

        let query = if unread_only {
            "SELECT insight_id, user_id, insight_type, priority, confidence_score, 
                    is_read, created_at
             FROM ai_insights 
             WHERE user_id = ? AND is_read = 0
             ORDER BY priority DESC, confidence_score DESC, created_at DESC"
        } else {
            "SELECT insight_id, user_id, insight_type, priority, confidence_score, 
                    is_read, created_at
             FROM ai_insights 
             WHERE user_id = ?
             ORDER BY created_at DESC 
             LIMIT 100"
        };

        let stmt = self.db.prepare(query);
        let results = stmt
            .bind(&[user_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .all()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let mut insights = Vec::new();
        let results_vec = results.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            if let Ok(insight) = self.row_to_ai_insight_summary(row) {
                insights.push(insight);
            }
        }

        self.update_metrics(start_time, true).await;
        Ok(insights)
    }

    /// Mark insight as read
    pub async fn mark_insight_as_read(
        &self,
        insight_id: &str,
        user_id: &str,
    ) -> ArbitrageResult<bool> {
        let start_time = current_timestamp_ms();

        let stmt = self.db.prepare(
            "UPDATE ai_insights SET is_read = 1, read_at = ? WHERE insight_id = ? AND user_id = ?",
        );

        let now_ms = chrono::Utc::now().timestamp_millis();
        let result = stmt
            .bind(&[now_ms.into(), insight_id.into(), user_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .run()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let success = result
            .meta()
            .map(|meta| meta.unwrap().changes.unwrap_or(0) > 0)
            .unwrap_or(false);

        if success && self.config.enable_caching {
            let _ = self.invalidate_insight_cache(user_id).await;
        }

        self.update_metrics(start_time, success).await;
        Ok(success)
    }

    // ============= AI SUGGESTION OPERATIONS =============

    /// Store AI suggestion
    pub async fn store_ai_suggestion(
        &self,
        suggestion: &ParameterSuggestion,
    ) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        if self.config.enable_ai_validation {
            self.validate_ai_suggestion(suggestion)?;
        }

        let result = self.store_ai_suggestion_internal(suggestion).await;
        self.update_metrics(start_time, result.is_ok()).await;
        result
    }

    /// Get AI suggestions for user
    pub async fn get_user_ai_suggestions(
        &self,
        user_id: &str,
        suggestion_type: Option<&str>,
    ) -> ArbitrageResult<Vec<ParameterSuggestion>> {
        let start_time = current_timestamp_ms();

        let (query, params) = if let Some(stype) = suggestion_type {
            (
                "SELECT * FROM ai_suggestions WHERE user_id = ? AND suggestion_type = ? AND is_active = 1 ORDER BY confidence_score DESC, created_at DESC LIMIT 50",
                vec![user_id.into(), stype.into()]
            )
        } else {
            (
                "SELECT * FROM ai_suggestions WHERE user_id = ? AND is_active = 1 ORDER BY confidence_score DESC, created_at DESC LIMIT 50",
                vec![user_id.into()]
            )
        };

        let stmt = self.db.prepare(query);
        let results = stmt
            .bind(&params)
            .map_err(|e| database_error("bind parameters", e))?
            .all()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let mut suggestions = Vec::new();
        let results_vec = results.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            if let Ok(suggestion) = self.row_to_ai_suggestion(row) {
                suggestions.push(suggestion);
            }
        }

        self.update_metrics(start_time, true).await;
        Ok(suggestions)
    }

    // ============= BATCH OPERATIONS =============

    /// Create multiple AI insights in batch
    pub async fn create_ai_insights_batch(
        &self,
        insights: &[AiPerformanceInsights],
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        let start_time = current_timestamp_ms();

        if insights.len() > self.config.batch_size {
            return Err(validation_error(
                "batch_size",
                &format!("exceeds maximum batch size of {}", self.config.batch_size),
            ));
        }

        let results = Vec::new();
        for batch in insights.chunks(self.config.batch_size) {
            for insight in batch {
                self.validate_ai_insight(insight)?;
                self.store_ai_insight_internal(insight).await?;
            }
        }

        self.update_metrics(start_time, true).await;
        Ok(results)
    }

    // ============= INTERNAL HELPER METHODS =============

    async fn store_ai_enhancement_internal(
        &self,
        enhancement: &AiOpportunityEnhancement,
    ) -> ArbitrageResult<()> {
        let enhancement_data_json = serde_json::to_string(enhancement).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize enhancement data: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO ai_enhancements (
                enhancement_id, user_id, enhancement_type, enhancement_data,
                confidence_score, is_active, created_at, last_applied_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            enhancement.opportunity_id.clone().into(),
            enhancement.user_id.clone().into(),
            "opportunity_enhancement".into(),
            enhancement_data_json.into(),
            enhancement.ai_confidence_score.into(),
            true.into(),
            (enhancement.analysis_timestamp as i64).into(),
            (enhancement.analysis_timestamp as i64).into(),
        ])
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    async fn store_ai_insight_internal(
        &self,
        insight: &AiPerformanceInsights,
    ) -> ArbitrageResult<()> {
        let insight_data_json = serde_json::to_string(insight).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize insight data: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO ai_insights (
                insight_id, user_id, insight_type, title, description,
                insight_data, priority, confidence_score, is_read, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        let insight_id = format!("insight_{}_{}", insight.user_id, insight.generated_at);

        stmt.bind(&[
            insight_id.into(),
            insight.user_id.clone().into(),
            "performance_insights".into(),
            "AI Performance Insights".into(),
            "AI-generated performance analysis and recommendations".into(),
            insight_data_json.into(),
            "medium".into(),
            insight.performance_score.into(),
            false.into(),
            (insight.generated_at as i64).into(),
        ])
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    async fn store_ai_suggestion_internal(
        &self,
        suggestion: &ParameterSuggestion,
    ) -> ArbitrageResult<()> {
        let suggestion_data_json = serde_json::to_string(suggestion).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize suggestion data: {}", e))
        })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO ai_suggestions (
                suggestion_id, user_id, suggestion_type, title, description,
                suggestion_data, confidence_score, is_active, created_at, expires_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        let suggestion_id = format!(
            "param_{}_{}",
            suggestion.parameter_name,
            chrono::Utc::now().timestamp_millis()
        );
        let expires_at = chrono::Utc::now().timestamp_millis() + (7 * 24 * 60 * 60 * 1000); // 7 days

        stmt.bind(&[
            suggestion_id.into(),
            "system".into(), // ParameterSuggestion doesn't have user_id
            "parameter_optimization".into(),
            suggestion.parameter_name.clone().into(),
            suggestion.rationale.clone().into(),
            suggestion_data_json.into(),
            suggestion.confidence.into(),
            true.into(),
            chrono::Utc::now().timestamp_millis().into(),
            expires_at.into(),
        ])
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    fn row_to_ai_enhancement_summary(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<AIEnhancementSummary> {
        let enhancement_id = get_string_field(&row, "enhancement_id")?;
        let user_id = get_string_field(&row, "user_id")?;
        let enhancement_type = get_string_field(&row, "enhancement_type")?;
        let confidence_score = get_f64_field(&row, "confidence_score", 0.0);
        let is_active = get_bool_field(&row, "is_active", true);
        let created_at = get_i64_field(&row, "created_at", 0) as u64;
        let last_applied = if get_i64_field(&row, "last_applied_at", 0) > 0 {
            Some(get_i64_field(&row, "last_applied_at", 0) as u64)
        } else {
            None
        };

        Ok(AIEnhancementSummary {
            enhancement_id,
            user_id,
            enhancement_type,
            confidence_score,
            is_active,
            created_at,
            last_applied,
        })
    }

    fn row_to_ai_insight_summary(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<AIInsightSummary> {
        let insight_id = get_string_field(&row, "insight_id")?;
        let user_id = get_string_field(&row, "user_id")?;
        let insight_type = get_string_field(&row, "insight_type")?;
        let priority = get_string_field(&row, "priority")?;
        let confidence_score = get_f64_field(&row, "confidence_score", 0.0);
        let is_read = get_bool_field(&row, "is_read", false);
        let created_at = get_i64_field(&row, "created_at", 0) as u64;

        Ok(AIInsightSummary {
            insight_id,
            user_id,
            insight_type,
            priority,
            confidence_score,
            is_read,
            created_at,
        })
    }

    fn row_to_ai_suggestion(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<ParameterSuggestion> {
        let parameter_name = get_string_field(&row, "title")?; // Using title as parameter_name
        let rationale = get_string_field(&row, "description")?; // Using description as rationale
        let confidence = get_f64_field(&row, "confidence_score", 0.0);

        // Try to parse the suggestion_data to get current_value and suggested_value
        let suggestion_data = get_json_field(
            &row,
            "suggestion_data",
            serde_json::Value::Object(serde_json::Map::new()),
        );

        let current_value = suggestion_data
            .get("current_value")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let suggested_value = suggestion_data
            .get("suggested_value")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let impact_assessment = suggestion_data
            .get("impact_assessment")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);

        Ok(ParameterSuggestion {
            parameter_name,
            current_value,
            suggested_value,
            rationale,
            impact_assessment,
            confidence,
        })
    }

    // ============= VALIDATION METHODS =============

    fn validate_ai_enhancement(
        &self,
        enhancement: &AiOpportunityEnhancement,
    ) -> ArbitrageResult<()> {
        validate_required_string(&enhancement.opportunity_id, "opportunity_id")?;
        validate_required_string(&enhancement.user_id, "user_id")?;
        validate_required_string(&enhancement.ai_provider_used, "ai_provider_used")?;

        if enhancement.ai_confidence_score < 0.0 || enhancement.ai_confidence_score > 1.0 {
            return Err(validation_error(
                "ai_confidence_score",
                "must be between 0.0 and 1.0",
            ));
        }

        Ok(())
    }

    fn validate_ai_insight(&self, insight: &AiPerformanceInsights) -> ArbitrageResult<()> {
        validate_required_string(&insight.user_id, "user_id")?;

        if insight.performance_score < 0.0 || insight.performance_score > 1.0 {
            return Err(validation_error(
                "performance_score",
                "must be between 0.0 and 1.0",
            ));
        }

        if insight.automation_readiness_score < 0.0 || insight.automation_readiness_score > 1.0 {
            return Err(validation_error(
                "automation_readiness_score",
                "must be between 0.0 and 1.0",
            ));
        }

        Ok(())
    }

    fn validate_ai_suggestion(&self, suggestion: &ParameterSuggestion) -> ArbitrageResult<()> {
        validate_required_string(&suggestion.parameter_name, "parameter_name")?;
        validate_required_string(&suggestion.current_value, "current_value")?;
        validate_required_string(&suggestion.suggested_value, "suggested_value")?;
        validate_required_string(&suggestion.rationale, "rationale")?;

        if suggestion.impact_assessment < 0.0 || suggestion.impact_assessment > 1.0 {
            return Err(validation_error(
                "impact_assessment",
                "must be between 0.0 and 1.0",
            ));
        }

        if suggestion.confidence < 0.0 || suggestion.confidence > 1.0 {
            return Err(validation_error(
                "confidence",
                "must be between 0.0 and 1.0",
            ));
        }

        Ok(())
    }

    // ============= CACHING METHODS =============

    async fn cache_ai_enhancement(
        &self,
        enhancement: &AiOpportunityEnhancement,
    ) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("ai_enhancement:{}", enhancement.opportunity_id);
            let cache_data = serde_json::to_string(enhancement).map_err(|e| {
                ArbitrageError::serialization_error(format!(
                    "Failed to serialize enhancement: {}",
                    e
                ))
            })?;

            let _ = cache
                .put(&cache_key, cache_data)?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await;
        }
        Ok(())
    }

    async fn cache_ai_insight(&self, insight: &AiPerformanceInsights) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("ai_insight:{}", insight.user_id);
            let cache_data = serde_json::to_string(insight).map_err(|e| {
                ArbitrageError::serialization_error(format!("Failed to serialize insight: {}", e))
            })?;

            let _ = cache
                .put(&cache_key, cache_data)?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await;
        }
        Ok(())
    }

    async fn invalidate_insight_cache(&self, user_id: &str) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("user_insights:{}", user_id);
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

impl Repository for AIDataRepository {
    fn name(&self) -> &str {
        "ai_data_repository"
    }

    async fn health_check(&self) -> ArbitrageResult<RepositoryHealth> {
        let start_time = current_timestamp_ms();

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
        // Create AI tables
        let create_ai_enhancements_table = "
            CREATE TABLE IF NOT EXISTS ai_enhancements (
                enhancement_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                enhancement_type TEXT NOT NULL,
                enhancement_data TEXT NOT NULL,
                confidence_score REAL NOT NULL,
                is_active BOOLEAN DEFAULT true,
                created_at INTEGER NOT NULL,
                last_applied_at INTEGER
            )
        ";

        let create_ai_insights_table = "
            CREATE TABLE IF NOT EXISTS ai_insights (
                insight_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                insight_type TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                insight_data TEXT NOT NULL,
                priority TEXT NOT NULL,
                confidence_score REAL NOT NULL,
                is_read BOOLEAN DEFAULT false,
                created_at INTEGER NOT NULL,
                read_at INTEGER
            )
        ";

        let create_ai_suggestions_table = "
            CREATE TABLE IF NOT EXISTS ai_suggestions (
                suggestion_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                suggestion_type TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                suggestion_data TEXT NOT NULL,
                confidence_score REAL NOT NULL,
                is_active BOOLEAN DEFAULT true,
                created_at INTEGER NOT NULL,
                expires_at INTEGER
            )
        ";

        self.db
            .exec(create_ai_enhancements_table)
            .await
            .map_err(|e| database_error("create ai_enhancements table", e))?;

        self.db
            .exec(create_ai_insights_table)
            .await
            .map_err(|e| database_error("create ai_insights table", e))?;

        self.db
            .exec(create_ai_suggestions_table)
            .await
            .map_err(|e| database_error("create ai_suggestions table", e))?;

        // Create indexes
        let indexes = [
            "CREATE INDEX IF NOT EXISTS idx_ai_enhancements_user_id ON ai_enhancements(user_id)",
            "CREATE INDEX IF NOT EXISTS idx_ai_enhancements_type ON ai_enhancements(enhancement_type)",
            "CREATE INDEX IF NOT EXISTS idx_ai_insights_user_id ON ai_insights(user_id)",
            "CREATE INDEX IF NOT EXISTS idx_ai_insights_read ON ai_insights(is_read)",
            "CREATE INDEX IF NOT EXISTS idx_ai_suggestions_user_id ON ai_suggestions(user_id)",
            "CREATE INDEX IF NOT EXISTS idx_ai_suggestions_type ON ai_suggestions(suggestion_type)",
        ];

        for index in &indexes {
            self.db
                .exec(index)
                .await
                .map_err(|e| database_error("create index", e))?;
        }

        Ok(())
    }

    async fn shutdown(&self) -> ArbitrageResult<()> {
        Ok(())
    }
}
