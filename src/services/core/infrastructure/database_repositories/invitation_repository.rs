// Invitation Repository - Specialized Invitation Data Access Component
// Handles invitation codes, invitation usage tracking, and user invitations

use super::{utils::*, Repository, RepositoryConfig, RepositoryHealth, RepositoryMetrics};
use crate::types::{InvitationCode, UserInvitation};
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::{wasm_bindgen::JsValue, D1Database};

/// Configuration for InvitationRepository
#[derive(Debug, Clone)]
pub struct InvitationRepositoryConfig {
    pub connection_pool_size: u32,
    pub batch_size: usize,
    pub cache_ttl_seconds: u64,
    pub enable_caching: bool,
    pub enable_metrics: bool,
    pub max_retries: u32,
    pub timeout_seconds: u64,
    pub invitation_expiry_days: i64,
    pub beta_access_days: i64,
}

impl Default for InvitationRepositoryConfig {
    fn default() -> Self {
        Self {
            connection_pool_size: 10, // Invitations are less frequent than users
            batch_size: 25,           // Smaller batches for invitations
            cache_ttl_seconds: 600,   // 10 minutes - longer cache for invitations
            enable_caching: true,
            enable_metrics: true,
            max_retries: 3,
            timeout_seconds: 30,
            invitation_expiry_days: 30, // Default invitation expiry
            beta_access_days: 90,       // Default beta access period
        }
    }
}

impl RepositoryConfig for InvitationRepositoryConfig {
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
        if self.invitation_expiry_days <= 0 {
            return Err(validation_error(
                "invitation_expiry_days",
                "must be greater than 0",
            ));
        }
        if self.beta_access_days <= 0 {
            return Err(validation_error(
                "beta_access_days",
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

/// Invitation usage record for beta tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationUsage {
    pub invitation_id: String,
    pub user_id: String,
    pub telegram_id: i64,
    pub used_at: DateTime<Utc>,
    pub beta_expires_at: DateTime<Utc>,
}

/// Invitation statistics for admin dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationStatistics {
    pub total_generated: u32,
    pub total_used: u32,
    pub total_expired: u32,
    pub active_beta_users: u32,
    pub conversion_rate: f64,
}

/// Invitation repository for specialized invitation data operations
pub struct InvitationRepository {
    db: Arc<D1Database>,
    config: InvitationRepositoryConfig,
    metrics: Arc<std::sync::Mutex<RepositoryMetrics>>,
    cache: Option<worker::kv::KvStore>,
}

impl InvitationRepository {
    /// Create new InvitationRepository
    pub fn new(db: Arc<D1Database>, config: InvitationRepositoryConfig) -> Self {
        let metrics = RepositoryMetrics {
            repository_name: "invitation_repository".to_string(),
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

    // ============= INVITATION CODE OPERATIONS =============

    /// Create a new invitation code
    pub async fn create_invitation_code(&self, invitation: &InvitationCode) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate invitation code
        self.validate_invitation_code(invitation)?;

        let result = self.store_invitation_code_internal(invitation).await;

        // Cache the invitation if successful and caching is enabled
        if result.is_ok() && self.config.enable_caching {
            let _ = self.cache_invitation_code(invitation).await;
        }

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// Get invitation code by code
    pub async fn get_invitation_code(&self, code: &str) -> ArbitrageResult<Option<InvitationCode>> {
        let start_time = current_timestamp_ms();

        // Try cache first if enabled
        if self.config.enable_caching {
            if let Some(cached_invitation) = self.get_cached_invitation_code(code).await? {
                self.update_metrics(start_time, true).await;
                return Ok(Some(cached_invitation));
            }
        }

        let result = self.get_invitation_code_from_db(code).await;

        // Cache result if successful and caching is enabled
        if let Ok(Some(ref invitation)) = result {
            if self.config.enable_caching {
                let _ = self.cache_invitation_code(invitation).await;
            }
        }

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// Update invitation code
    pub async fn update_invitation_code(&self, invitation: &InvitationCode) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate invitation code
        self.validate_invitation_code(invitation)?;

        let result = self.update_invitation_code_internal(invitation).await;

        // Invalidate cache if enabled
        if self.config.enable_caching && self.cache.is_some() {
            let _ = self.invalidate_invitation_cache(&invitation.code).await;
        }

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result
    }

    /// List invitation codes with pagination
    pub async fn list_invitation_codes(
        &self,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ArbitrageResult<Vec<InvitationCode>> {
        let start_time = current_timestamp_ms();
        let limit = limit.unwrap_or(50).min(1000); // Cap at 1000 for safety
        let offset = offset.unwrap_or(0).max(0);

        let stmt = self
            .db
            .prepare("SELECT * FROM invitation_codes ORDER BY created_at DESC LIMIT ? OFFSET ?");

        let results = stmt
            .bind(&[
                JsValue::from_f64(limit as f64),
                JsValue::from_f64(offset as f64),
            ])
            .map_err(|e| database_error("bind parameters", e))?
            .all()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let mut invitations = Vec::new();
        let results_vec = results.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            match self.row_to_invitation_code(row) {
                Ok(invitation) => invitations.push(invitation),
                Err(e) => {
                    // Log error but continue processing other invitations
                    eprintln!("Error parsing invitation code: {}", e);
                }
            }
        }

        // Update metrics
        self.update_metrics(start_time, true).await;

        Ok(invitations)
    }

    /// Delete invitation code
    pub async fn delete_invitation_code(&self, code: &str) -> ArbitrageResult<bool> {
        let start_time = current_timestamp_ms();

        let stmt = self
            .db
            .prepare("DELETE FROM invitation_codes WHERE code = ?");

        let result = stmt
            .bind(&[code.into()])
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
            let _ = self.invalidate_invitation_cache(code).await;
        }

        // Update metrics
        self.update_metrics(start_time, success).await;

        Ok(success)
    }

    // ============= INVITATION USAGE OPERATIONS =============

    /// Store invitation usage record for beta tracking
    pub async fn store_invitation_usage(&self, usage: &InvitationUsage) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate usage
        self.validate_invitation_usage(usage)?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO invitation_usage (
                invitation_id, user_id, telegram_id, used_at, beta_expires_at
            ) VALUES (?, ?, ?, ?, ?)",
        );

        let result = stmt
            .bind(&[
                usage.invitation_id.clone().into(),
                usage.user_id.clone().into(),
                JsValue::from_f64(usage.telegram_id as f64),
                usage.used_at.timestamp_millis().into(),
                usage.beta_expires_at.timestamp_millis().into(),
            ])
            .map_err(|e| database_error("bind parameters", e))?
            .run()
            .await
            .map_err(|e| database_error("execute query", e));

        // Invalidate cache for the invitation code when it's first used to maintain consistency
        if result.is_ok() && self.config.enable_caching {
            let _ = self.invalidate_invitation_cache(&usage.invitation_id).await;
        }

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result.map(|_| ())
    }

    /// Get invitation usage by user
    pub async fn get_invitation_usage_by_user(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<
        Option<crate::services::core::invitation::invitation_service::InvitationUsage>,
    > {
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

            let used_at =
                chrono::DateTime::from_timestamp_millis(used_at_ms).unwrap_or(chrono::Utc::now());

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

    /// Check if user has active beta access
    pub async fn has_active_beta_access(&self, user_id: &str) -> ArbitrageResult<bool> {
        let start_time = current_timestamp_ms();
        let now_ms = chrono::Utc::now().timestamp_millis();

        let stmt = self.db.prepare(
            "SELECT beta_expires_at FROM invitation_usage WHERE user_id = ? AND (
                (typeof(beta_expires_at) = 'integer' AND beta_expires_at > ?) OR
                (typeof(beta_expires_at) = 'text' AND beta_expires_at > datetime('now'))
            ) LIMIT 1",
        );

        let result = stmt
            .bind(&[user_id.into(), now_ms.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        let has_access = result.is_some();

        // Update metrics
        self.update_metrics(start_time, true).await;

        Ok(has_access)
    }

    // ============= USER INVITATION OPERATIONS =============

    /// Store a user invitation
    pub async fn store_user_invitation(&self, invitation: &UserInvitation) -> ArbitrageResult<()> {
        let start_time = current_timestamp_ms();

        // Validate user invitation
        self.validate_user_invitation(invitation)?;

        let invitation_data_json =
            serde_json::to_string(&invitation.invitation_data).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize invitation data: {}", e))
            })?;

        let stmt = self.db.prepare(
            "INSERT OR REPLACE INTO user_invitations (
                invitation_id, inviter_user_id, invitee_identifier,
                invitation_type, status, message, invitation_data,
                created_at, expires_at, accepted_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        );

        let expires_at = invitation.expires_at.unwrap_or(0);
        let accepted_at = invitation.accepted_at.unwrap_or(0);
        let created_at = invitation.created_at;

        let result = stmt
            .bind(&[
                invitation.invitation_id.clone().into(),
                invitation.inviter_user_id.clone().into(),
                invitation.invitee_identifier.clone().into(),
                invitation.invitation_type.clone().into(),
                invitation.status.clone().into(),
                invitation.message.clone().unwrap_or_default().into(),
                invitation_data_json.into(),
                created_at.into(),
                expires_at.into(),
                accepted_at.into(),
            ])
            .map_err(|e| database_error("bind parameters", e))?
            .run()
            .await
            .map_err(|e| database_error("execute query", e));

        // Update metrics
        self.update_metrics(start_time, result.is_ok()).await;

        result.map(|_| ())
    }

    /// Get user invitations by user ID
    pub async fn get_user_invitations(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Vec<UserInvitation>> {
        let start_time = current_timestamp_ms();

        let stmt = self.db.prepare(
            "SELECT * FROM user_invitations WHERE inviter_user_id = ? OR invitee_identifier = ?",
        );

        let results = stmt
            .bind(&[user_id.into(), user_id.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .all()
            .await
            .map_err(|e| database_error("execute query", e))?;

        let mut invitations = Vec::new();
        let results_vec = results.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            if let Ok(invitation) = self.row_to_user_invitation(row) {
                invitations.push(invitation);
            }
        }

        // Update metrics
        self.update_metrics(start_time, true).await;

        Ok(invitations)
    }

    // ============= INVITATION STATISTICS =============

    /// Get invitation statistics for admin dashboard
    pub async fn get_invitation_statistics(&self) -> ArbitrageResult<InvitationStatistics> {
        let start_time = current_timestamp_ms();

        let total_generated = self.count_total_invitations().await?;
        let total_used = self.count_used_invitations().await?;
        let total_expired = self.count_expired_invitations().await?;
        let active_beta_users = self.count_active_beta_users().await?;

        let conversion_rate = if total_generated > 0 {
            (total_used as f64 / total_generated as f64) * 100.0
        } else {
            0.0
        };

        let stats = InvitationStatistics {
            total_generated,
            total_used,
            total_expired,
            active_beta_users,
            conversion_rate,
        };

        // Update metrics
        self.update_metrics(start_time, true).await;

        Ok(stats)
    }

    // ============= BATCH OPERATIONS =============

    /// Create multiple invitation codes in batch
    pub async fn create_invitation_codes_batch(
        &self,
        invitations: &[InvitationCode],
    ) -> ArbitrageResult<Vec<ArbitrageResult<()>>> {
        let start_time = current_timestamp_ms();

        if invitations.len() > self.config.batch_size {
            return Err(validation_error(
                "batch_size",
                &format!("exceeds maximum batch size of {}", self.config.batch_size),
            ));
        }

        let mut results = Vec::with_capacity(invitations.len());
        for batch in invitations.chunks(self.config.batch_size) {
            for invitation in batch {
                let res = async {
                    self.validate_invitation_code(invitation)?;
                    self.store_invitation_code_internal(invitation).await
                };
                results.push(res.await);
            }
        }

        // Update metrics
        self.update_metrics(start_time, true).await;

        Ok(results)
    }

    // ============= INTERNAL HELPER METHODS =============

    async fn store_invitation_code_internal(
        &self,
        invitation: &InvitationCode,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "INSERT INTO invitation_codes (
                code, created_by, created_at, expires_at, max_uses,
                current_uses, is_active, purpose
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        );

        stmt.bind(&[
            invitation.code.clone().into(),
            invitation.created_by_user_id.clone().into(),
            (invitation.created_at as i64).into(),
            invitation.expires_at.map(|t| t as i64).unwrap_or(0).into(),
            invitation.max_uses.map(|u| u as i64).unwrap_or(0).into(),
            (invitation.current_uses as i64).into(),
            invitation.is_active.into(),
            invitation.invitation_type.clone().into(),
        ])
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    async fn get_invitation_code_from_db(
        &self,
        code: &str,
    ) -> ArbitrageResult<Option<InvitationCode>> {
        let stmt = self
            .db
            .prepare("SELECT * FROM invitation_codes WHERE code = ?");

        let result = stmt
            .bind(&[code.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        match result {
            Some(row) => {
                let invitation = self.row_to_invitation_code(row)?;
                Ok(Some(invitation))
            }
            None => Ok(None),
        }
    }

    async fn update_invitation_code_internal(
        &self,
        invitation: &InvitationCode,
    ) -> ArbitrageResult<()> {
        let stmt = self.db.prepare(
            "UPDATE invitation_codes SET 
                current_uses = ?, is_active = ?
            WHERE code = ?",
        );

        stmt.bind(&[
            (invitation.current_uses as i64).into(),
            invitation.is_active.into(),
            invitation.code.clone().into(),
        ])
        .map_err(|e| database_error("bind parameters", e))?
        .run()
        .await
        .map_err(|e| database_error("execute query", e))?;

        Ok(())
    }

    fn row_to_invitation_code(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<InvitationCode> {
        let code = get_string_field(&row, "code")?;
        let created_by =
            get_optional_string_field(&row, "created_by").unwrap_or_else(|| "system".to_string());
        let created_at = get_i64_field(&row, "created_at", 0) as u64;
        let expires_at = get_optional_i64_field(&row, "expires_at").map(|v| v as u64);
        let max_uses = {
            let uses = get_i64_field(&row, "max_uses", 0);
            if uses > 0 {
                Some(uses as u32)
            } else {
                None
            }
        };
        let current_uses = get_i64_field(&row, "current_uses", 0) as u32;
        let is_active = get_bool_field(&row, "is_active", true);
        let purpose = get_string_field(&row, "purpose").unwrap_or_else(|_| "general".to_string());

        // Required fields for the new struct definition
        let code_id =
            get_string_field(&row, "code_id").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());
        let created_by_user_id =
            get_string_field(&row, "created_by_user_id").unwrap_or_else(|_| "system".to_string());
        let bonus_percentage =
            get_optional_string_field(&row, "bonus_percentage").and_then(|s| s.parse::<f64>().ok());
        let invitation_type =
            get_string_field(&row, "invitation_type").unwrap_or_else(|_| "referral".to_string());

        Ok(InvitationCode {
            code_id,
            code,
            created_by_user_id,
            max_uses,
            current_uses,
            created_at,
            expires_at,
            is_active,
            bonus_percentage,
            invitation_type,
            created_by,
            purpose,
        })
    }

    fn row_to_invitation_usage(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<InvitationUsage> {
        let invitation_id = get_string_field(&row, "invitation_id")?;
        let user_id = get_string_field(&row, "user_id")?;
        let telegram_id = get_i64_field(&row, "telegram_id", 0);

        let used_at = {
            let timestamp = get_i64_field(&row, "used_at", 0);
            chrono::DateTime::from_timestamp_millis(timestamp)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or(chrono::Utc::now())
        };

        let beta_expires_at = {
            let timestamp = get_i64_field(&row, "beta_expires_at", 0);
            chrono::DateTime::from_timestamp_millis(timestamp)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or(chrono::Utc::now())
        };

        Ok(InvitationUsage {
            invitation_id,
            user_id,
            telegram_id,
            used_at,
            beta_expires_at,
        })
    }

    fn row_to_user_invitation(
        &self,
        row: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<UserInvitation> {
        let invitation_id = get_string_field(&row, "invitation_id")?;
        let inviter_user_id = get_string_field(&row, "inviter_user_id")?;
        let invitee_identifier = get_string_field(&row, "invitee_identifier")?;
        let invitation_type = get_string_field(&row, "invitation_type")?;
        let status = get_string_field(&row, "status")?;
        let message = get_optional_string_field(&row, "message");

        let invitation_data = get_json_field(&row, "invitation_data", serde_json::Value::Null);

        let expires_at = get_optional_i64_field(&row, "expires_at").map(|v| v as u64);
        let accepted_at = get_optional_i64_field(&row, "accepted_at").map(|v| v as u64);
        let created_at_timestamp = get_i64_field(&row, "created_at", 0) as u64;

        // Legacy fields for backward compatibility
        let invitation_code =
            get_string_field(&row, "invitation_code").unwrap_or_else(|_| invitation_id.clone());
        let invited_user_id = get_string_field(&row, "invited_user_id")
            .unwrap_or_else(|_| invitee_identifier.clone());
        let invited_by =
            get_optional_string_field(&row, "invited_by").unwrap_or_else(|| "system".to_string());
        let used_at = accepted_at;
        let invitation_metadata = invitation_data.as_str().map(|s| s.to_string());

        Ok(UserInvitation {
            invitation_code,
            invited_user_id,
            invited_by,
            used_at,
            invitation_metadata,
            invitation_id,
            inviter_user_id,
            invitee_identifier,
            invitation_type,
            status,
            message,
            invitation_data,
            created_at: created_at_timestamp,
            expires_at,
            accepted_at,
        })
    }

    // ============= VALIDATION METHODS =============

    fn validate_invitation_code(&self, invitation: &InvitationCode) -> ArbitrageResult<()> {
        validate_required_string(&invitation.code, "code")?;
        validate_required_string(&invitation.purpose, "purpose")?;

        if invitation.current_uses > invitation.max_uses.unwrap_or(u32::MAX) {
            return Err(validation_error("current_uses", "cannot exceed max_uses"));
        }

        Ok(())
    }

    fn validate_invitation_usage(&self, usage: &InvitationUsage) -> ArbitrageResult<()> {
        validate_required_string(&usage.invitation_id, "invitation_id")?;
        validate_required_string(&usage.user_id, "user_id")?;

        if usage.telegram_id <= 0 {
            return Err(validation_error("telegram_id", "must be positive"));
        }

        Ok(())
    }

    fn validate_user_invitation(&self, invitation: &UserInvitation) -> ArbitrageResult<()> {
        validate_required_string(&invitation.invitation_id, "invitation_id")?;
        validate_required_string(&invitation.inviter_user_id, "inviter_user_id")?;
        validate_required_string(&invitation.invitee_identifier, "invitee_identifier")?;
        validate_required_string(&invitation.invitation_type, "invitation_type")?;
        validate_required_string(&invitation.status, "status")?;

        Ok(())
    }

    // ============= CACHING METHODS =============

    async fn cache_invitation_code(&self, invitation: &InvitationCode) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("invitation_code:{}", invitation.code);
            let invitation_json = serde_json::to_string(invitation).map_err(|e| {
                ArbitrageError::parse_error(format!(
                    "Failed to serialize invitation for cache: {}",
                    e
                ))
            })?;

            cache
                .put(&cache_key, &invitation_json)
                .map_err(|e| {
                    ArbitrageError::cache_error(format!("Failed to cache invitation: {}", e))
                })?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await
                .map_err(|e| {
                    ArbitrageError::cache_error(format!("Failed to cache invitation: {}", e))
                })?;
        }
        Ok(())
    }

    async fn get_cached_invitation_code(
        &self,
        code: &str,
    ) -> ArbitrageResult<Option<InvitationCode>> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("invitation_code:{}", code);

            match cache.get(&cache_key).text().await {
                Ok(Some(invitation_json)) => {
                    let invitation: InvitationCode = serde_json::from_str(&invitation_json)
                        .map_err(|e| {
                            ArbitrageError::parse_error(format!(
                                "Failed to deserialize cached invitation: {}",
                                e
                            ))
                        })?;
                    Ok(Some(invitation))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(ArbitrageError::cache_error(format!(
                    "Failed to get cached invitation: {}",
                    e
                ))),
            }
        } else {
            Ok(None)
        }
    }

    async fn invalidate_invitation_cache(&self, code: &str) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let cache_key = format!("invitation_code:{}", code);
            cache.delete(&cache_key).await.map_err(|e| {
                ArbitrageError::cache_error(format!("Failed to invalidate cache: {}", e))
            })?;
        }
        Ok(())
    }

    // ============= STATISTICS HELPER METHODS =============

    async fn count_total_invitations(&self) -> ArbitrageResult<u32> {
        self.execute_count_query("SELECT COUNT(*) as count FROM invitation_codes")
            .await
    }

    async fn count_used_invitations(&self) -> ArbitrageResult<u32> {
        self.execute_count_query(
            "SELECT COUNT(*) as count FROM invitation_codes WHERE current_uses > 0",
        )
        .await
    }

    async fn count_expired_invitations(&self) -> ArbitrageResult<u32> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let stmt = self.db.prepare("SELECT COUNT(*) as count FROM invitation_codes WHERE expires_at > 0 AND expires_at < ?");

        let result = stmt
            .bind(&[now_ms.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        if let Some(row) = result {
            let count = get_i64_field(&row, "count", 0) as u32;
            Ok(count)
        } else {
            Ok(0)
        }
    }

    async fn count_active_beta_users(&self) -> ArbitrageResult<u32> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let stmt = self
            .db
            .prepare("SELECT COUNT(*) as count FROM invitation_usage WHERE beta_expires_at > ?");

        let result = stmt
            .bind(&[now_ms.into()])
            .map_err(|e| database_error("bind parameters", e))?
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        if let Some(row) = result {
            let count = get_i64_field(&row, "count", 0) as u32;
            Ok(count)
        } else {
            Ok(0)
        }
    }

    async fn execute_count_query(&self, query: &str) -> ArbitrageResult<u32> {
        let stmt = self.db.prepare(query);

        let result = stmt
            .first::<HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| database_error("execute query", e))?;

        if let Some(row) = result {
            let count = get_i64_field(&row, "count", 0) as u32;
            Ok(count)
        } else {
            Ok(0)
        }
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

impl Repository for InvitationRepository {
    fn name(&self) -> &str {
        "invitation_repository"
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
        let success_rate = if metrics.total_operations > 0 {
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
            error_rate: if is_healthy {
                (1.0 - success_rate) * 100.0
            } else {
                100.0
            },
        })
    }

    async fn get_metrics(&self) -> RepositoryMetrics {
        self.metrics.lock().unwrap().clone()
    }

    async fn initialize(&self) -> ArbitrageResult<()> {
        // Create tables if they don't exist
        let create_invitation_codes_table = "
            CREATE TABLE IF NOT EXISTS invitation_codes (
                code TEXT PRIMARY KEY,
                created_by TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER,
                max_uses INTEGER,
                current_uses INTEGER DEFAULT 0,
                is_active BOOLEAN DEFAULT true,
                purpose TEXT NOT NULL
            )
        ";

        let create_invitation_usage_table = "
            CREATE TABLE IF NOT EXISTS invitation_usage (
                id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
                invitation_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                telegram_id INTEGER NOT NULL,
                used_at INTEGER NOT NULL,
                beta_expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
                UNIQUE(user_id)
            )
        ";

        let create_user_invitations_table = "
            CREATE TABLE IF NOT EXISTS user_invitations (
                invitation_id TEXT PRIMARY KEY,
                inviter_user_id TEXT NOT NULL,
                invitee_identifier TEXT NOT NULL,
                invitation_type TEXT NOT NULL,
                status TEXT NOT NULL,
                message TEXT,
                invitation_data TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER,
                accepted_at INTEGER
            )
        ";

        // Execute table creation
        self.db
            .exec(create_invitation_codes_table)
            .await
            .map_err(|e| database_error("create invitation_codes table", e))?;

        self.db
            .exec(create_invitation_usage_table)
            .await
            .map_err(|e| database_error("create invitation_usage table", e))?;

        self.db
            .exec(create_user_invitations_table)
            .await
            .map_err(|e| database_error("create user_invitations table", e))?;

        // Create indexes for better performance
        let create_code_index =
            "CREATE INDEX IF NOT EXISTS idx_invitation_codes_code ON invitation_codes(code)";
        let create_expires_index = "CREATE INDEX IF NOT EXISTS idx_invitation_codes_expires_at ON invitation_codes(expires_at)";
        let create_active_index = "CREATE INDEX IF NOT EXISTS idx_invitation_codes_is_active ON invitation_codes(is_active)";
        let create_usage_user_index =
            "CREATE INDEX IF NOT EXISTS idx_invitation_usage_user_id ON invitation_usage(user_id)";
        let create_usage_beta_index = "CREATE INDEX IF NOT EXISTS idx_invitation_usage_beta_expires_at ON invitation_usage(beta_expires_at)";

        self.db
            .exec(create_code_index)
            .await
            .map_err(|e| database_error("create code index", e))?;

        self.db
            .exec(create_expires_index)
            .await
            .map_err(|e| database_error("create expires index", e))?;

        self.db
            .exec(create_active_index)
            .await
            .map_err(|e| database_error("create active index", e))?;

        self.db
            .exec(create_usage_user_index)
            .await
            .map_err(|e| database_error("create usage user index", e))?;

        self.db
            .exec(create_usage_beta_index)
            .await
            .map_err(|e| database_error("create usage beta index", e))?;

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
    fn test_invitation_repository_config_validation() {
        let mut config = InvitationRepositoryConfig::default();
        assert!(config.validate().is_ok());

        config.connection_pool_size = 0;
        assert!(config.validate().is_err());

        config.connection_pool_size = 10;
        config.batch_size = 0;
        assert!(config.validate().is_err());

        config.batch_size = 25;
        config.cache_ttl_seconds = 0;
        assert!(config.validate().is_err());

        config.cache_ttl_seconds = 600;
        config.invitation_expiry_days = 0;
        assert!(config.validate().is_err());

        config.invitation_expiry_days = 30;
        config.beta_access_days = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invitation_code_validation() {
        let config = InvitationRepositoryConfig::default();
        let db = Arc::new(unsafe { std::mem::zeroed() }); // Mock for testing
        let repo = InvitationRepository::new(db, config);

        let mut invitation = InvitationCode {
            code: "TEST123".to_string(),
            code_id: "test_code_id".to_string(),
            purpose: "testing".to_string(),
            max_uses: Some(5),
            current_uses: 0,
            expires_at: Some(Utc::now().timestamp_millis() as u64 + (30 * 24 * 60 * 60 * 1000)), // 30 days from now
            is_active: true,
            created_at: Utc::now().timestamp_millis() as u64,
            created_by: "admin".to_string(),
            created_by_user_id: "admin_user_id".to_string(),
            bonus_percentage: Some(0.1),
            invitation_type: "beta".to_string(),
        };

        assert!(repo.validate_invitation_code(&invitation).is_ok());

        invitation.code = "".to_string();
        assert!(repo.validate_invitation_code(&invitation).is_err());

        invitation.code = "TEST123".to_string();
        invitation.purpose = "".to_string();
        assert!(repo.validate_invitation_code(&invitation).is_err());

        invitation.purpose = "testing".to_string();
        invitation.current_uses = 15; // Exceeds max_uses
        assert!(repo.validate_invitation_code(&invitation).is_err());
    }

    #[test]
    fn test_invitation_usage_validation() {
        let config = InvitationRepositoryConfig::default();
        let db = Arc::new(unsafe { std::mem::zeroed() }); // Mock for testing
        let repo = InvitationRepository::new(db, config);

        let mut usage = InvitationUsage {
            invitation_id: "inv123".to_string(),
            user_id: "user123".to_string(),
            telegram_id: 123456789,
            used_at: chrono::Utc::now(),
            beta_expires_at: chrono::Utc::now() + chrono::Duration::days(90),
        };

        assert!(repo.validate_invitation_usage(&usage).is_ok());

        usage.invitation_id = "".to_string();
        assert!(repo.validate_invitation_usage(&usage).is_err());

        usage.invitation_id = "inv123".to_string();
        usage.user_id = "".to_string();
        assert!(repo.validate_invitation_usage(&usage).is_err());

        usage.user_id = "user123".to_string();
        usage.telegram_id = 0;
        assert!(repo.validate_invitation_usage(&usage).is_err());
    }
}
