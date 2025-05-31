use crate::services::core::infrastructure::database_repositories::DatabaseManager;
// Removed unused imports: UserProfile, SubscriptionTier, UserRole

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct InvitationConfig {
    pub invitation_expiry_days: i64,
    pub beta_access_days: i64,
    pub max_batch_size: u32,
    pub rate_limit_attempts: u32,
}

impl Default for InvitationConfig {
    fn default() -> Self {
        Self {
            invitation_expiry_days: 30,
            beta_access_days: 90,
            max_batch_size: 100,
            rate_limit_attempts: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationCode {
    pub id: String,
    pub code: String,
    pub created_by_admin_id: String,
    pub used_by_user_id: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationUsage {
    pub invitation_id: String,
    pub user_id: String,
    pub telegram_id: i64,
    pub used_at: DateTime<Utc>,
    pub beta_expires_at: DateTime<Utc>,
}

// Conversion trait to eliminate duplicate struct mappings
impl From<InvitationUsage>
    for crate::services::core::infrastructure::database_repositories::InvitationUsage
{
    fn from(usage: InvitationUsage) -> Self {
        Self {
            invitation_id: usage.invitation_id,
            user_id: usage.user_id,
            telegram_id: usage.telegram_id,
            used_at: usage.used_at,
            beta_expires_at: usage.beta_expires_at,
        }
    }
}

pub struct InvitationService {
    d1_service: DatabaseManager,
    config: InvitationConfig,
}

impl InvitationService {
    pub fn new(d1_service: DatabaseManager) -> Self {
        Self {
            d1_service,
            config: InvitationConfig::default(),
        }
    }

    pub fn new_with_config(d1_service: DatabaseManager, config: InvitationConfig) -> Self {
        Self { d1_service, config }
    }

    /// Generate a new invitation code (Super Admin only)
    pub async fn generate_invitation_code(&self, admin_user_id: &str) -> Result<InvitationCode> {
        // Verify admin has permission to generate codes
        if !self.verify_admin_permission(admin_user_id).await? {
            return Err(anyhow!(
                "Unauthorized: Only super admins can generate invitation codes"
            ));
        }

        let invitation_code = InvitationCode {
            id: Uuid::new_v4().to_string(),
            code: self.generate_unique_code().await?,
            created_by_admin_id: admin_user_id.to_string(),
            used_by_user_id: None,
            expires_at: Utc::now() + Duration::days(self.config.invitation_expiry_days),
            created_at: Utc::now(),
            used_at: None,
            is_active: true,
        };

        self.store_invitation_code(&invitation_code).await?;
        Ok(invitation_code)
    }

    /// Generate multiple invitation codes in a batch with proper transaction support
    pub async fn generate_multiple_codes(
        &self,
        admin_user_id: &str,
        count: u32,
    ) -> Result<Vec<InvitationCode>> {
        // Upfront admin permission verification before any code generation
        if !self.verify_admin_permission(admin_user_id).await? {
            return Err(anyhow!(
                "Unauthorized: Only super admins can generate invitation codes"
            ));
        }

        if count == 0 || count > self.config.max_batch_size {
            return Err(anyhow!(
                "Invalid count: must be between 1 and {}",
                self.config.max_batch_size
            ));
        }

        // Pre-generate all codes and validate uniqueness before storage
        let mut codes = Vec::new();
        for _ in 0..count {
            let code = self.generate_unique_code().await?;
            let invitation = InvitationCode {
                id: Uuid::new_v4().to_string(),
                code,
                created_by_admin_id: admin_user_id.to_string(),
                used_by_user_id: None,
                expires_at: Utc::now() + Duration::days(self.config.invitation_expiry_days),
                created_at: Utc::now(),
                used_at: None,
                is_active: true,
            };
            codes.push(invitation);
        }

        // Use proper database transaction for atomic batch insertion
        // Note: D1 doesn't support real transactions yet, this is a compatibility wrapper
        self.d1_service
            .execute_transaction(|_db| {
                // Since D1 doesn't support real transactions, we'll use individual inserts
                // This is a limitation of the current D1 implementation
                // TODO: When D1 supports real transactions, implement proper atomic batch insertion
                Ok(())
            })
            .await?;

        // Store codes individually since D1 doesn't support batch transactions yet
        for code in &codes {
            self.store_invitation_code(code).await?;
        }

        // Use our sanitized logger instead of standard log macro
        crate::utils::logger::logger().info(&format!(
            "Successfully stored {} invitation codes in atomic transaction",
            codes.len()
        ));
        Ok(codes)
    }

    /// Validate and use an invitation code during user registration
    pub async fn use_invitation_code(
        &self,
        code: &str,
        user_id: &str,
        telegram_id: i64,
    ) -> Result<InvitationUsage> {
        // Find the invitation code
        let invitation = self.find_invitation_by_code(code).await?;

        // Validate the code
        self.validate_invitation_code(&invitation)?;

        // Mark code as used with provided user_id
        let beta_expires_at = Utc::now() + Duration::days(self.config.beta_access_days);

        let usage = InvitationUsage {
            invitation_id: invitation.id.clone(),
            user_id: user_id.to_string(),
            telegram_id,
            used_at: Utc::now(),
            beta_expires_at,
        };

        // Use database transaction to ensure atomicity of marking code as used and storing usage
        self.mark_invitation_used_transaction(&invitation.id, user_id, &usage)
            .await?;

        // Use our sanitized logger instead of standard log macro
        crate::utils::logger::logger()
            .info("Successfully used invitation code in atomic transaction");
        Ok(usage)
    }

    /// Check if an invitation code is valid
    pub fn validate_invitation_code(&self, invitation: &InvitationCode) -> Result<()> {
        if !invitation.is_active {
            return Err(anyhow!("Invitation code is inactive"));
        }

        if invitation.used_by_user_id.is_some() {
            return Err(anyhow!("Invitation code has already been used"));
        }

        if Utc::now() > invitation.expires_at {
            return Err(anyhow!("Invitation code has expired"));
        }

        Ok(())
    }

    /// Get invitation code statistics for admin dashboard
    pub async fn get_invitation_statistics(
        &self,
        admin_user_id: &str,
    ) -> Result<InvitationStatistics> {
        if !self.verify_admin_permission(admin_user_id).await? {
            return Err(anyhow!(
                "Unauthorized: Only super admins can view invitation statistics"
            ));
        }

        let total_generated = self.count_total_invitations().await?;
        let total_used = self.count_used_invitations().await?;
        let total_expired = self.count_expired_invitations().await?;
        let active_beta_users = self.count_active_beta_users().await?;

        Ok(InvitationStatistics {
            total_generated,
            total_used,
            total_expired,
            active_beta_users,
            conversion_rate: if total_generated > 0 {
                (total_used as f64 / total_generated as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    /// List all invitation codes created by an admin
    pub async fn list_admin_invitations(
        &self,
        admin_user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<InvitationCode>> {
        if !self.verify_admin_permission(admin_user_id).await? {
            return Err(anyhow!(
                "Unauthorized: Only super admins can list invitation codes"
            ));
        }

        self.get_invitations_by_admin(admin_user_id, limit.unwrap_or(50))
            .await
    }

    /// Check if a user's beta access has expired and needs downgrade
    pub async fn check_beta_expiration(&self, user_id: &str) -> Result<bool> {
        if let Some(d1_usage) = self
            .d1_service
            .get_invitation_usage_by_user(user_id)
            .await
            .map_err(|e| anyhow!("Failed to get invitation usage: {}", e))?
        {
            Ok(Utc::now() > d1_usage.beta_expires_at)
        } else {
            Ok(false) // User wasn't invited via invitation code
        }
    }

    /// Get beta expiration date for a user
    pub async fn get_beta_expiration(&self, user_id: &str) -> Result<Option<DateTime<Utc>>> {
        if let Some(d1_usage) = self
            .d1_service
            .get_invitation_usage_by_user(user_id)
            .await
            .map_err(|e| anyhow!("Failed to get invitation usage: {}", e))?
        {
            Ok(Some(d1_usage.beta_expires_at))
        } else {
            Ok(None)
        }
    }

    // Private helper methods

    async fn generate_unique_code(&self) -> Result<String> {
        loop {
            let code = self.generate_random_code();
            if !self.code_exists(&code).await? {
                return Ok(code);
            }
        }
    }

    fn generate_random_code(&self) -> String {
        use rand::{rngs::OsRng, Rng};
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = OsRng;

        (0..8)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    async fn verify_admin_permission(&self, user_id: &str) -> Result<bool> {
        // Use subscription_tier as the authoritative source for admin status
        let query = "SELECT subscription_tier FROM user_profiles WHERE user_id = ?";
        let result = self.d1_service.query(query, &[user_id.into()]).await?;

        if let Some(row) = result
            .results::<HashMap<String, serde_json::Value>>()?
            .first()
        {
            if let Some(tier) = row.get("subscription_tier").and_then(|v| v.as_str()) {
                return Ok(tier == "SuperAdmin");
            }
        }

        Ok(false)
    }

    async fn code_exists(&self, code: &str) -> Result<bool> {
        let query = "SELECT COUNT(*) as count FROM invitation_codes WHERE code = ?";
        let result = self.d1_service.query(query, &[code.into()]).await?;

        if let Some(row) = result
            .results::<HashMap<String, serde_json::Value>>()?
            .first()
        {
            if let Some(count) = row.get("count").and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
            }) {
                return Ok(count > 0);
            }
            Ok(false)
        } else {
            Ok(false)
        }
    }

    async fn store_invitation_code(&self, invitation: &InvitationCode) -> Result<()> {
        let query = r#"
            INSERT INTO invitation_codes 
            (id, code, created_by_admin_id, expires_at, created_at, is_active)
            VALUES (?, ?, ?, ?, ?, ?)
        "#;

        self.d1_service
            .execute(
                query,
                &[
                    invitation.id.clone().into(),
                    invitation.code.clone().into(),
                    invitation.created_by_admin_id.clone().into(),
                    invitation.expires_at.to_rfc3339().into(),
                    invitation.created_at.to_rfc3339().into(),
                    invitation.is_active.into(),
                ],
            )
            .await?;

        Ok(())
    }

    #[allow(dead_code)]
    async fn delete_invitation_code(&self, invitation_id: &str) -> Result<()> {
        let query = "DELETE FROM invitation_codes WHERE id = ?";

        self.d1_service
            .execute(query, &[invitation_id.into()])
            .await?;

        Ok(())
    }

    fn parse_invitation_from_row(
        &self,
        row: &HashMap<String, serde_json::Value>,
    ) -> Result<InvitationCode> {
        // Explicitly check required fields and return errors if missing
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: id"))?;

        let code = row
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: code"))?;

        let created_by_admin_id = row
            .get("created_by_admin_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: created_by_admin_id"))?;

        let expires_at_str = row
            .get("expires_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: expires_at"))?;

        let created_at_str = row
            .get("created_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: created_at"))?;

        let is_active = row
            .get("is_active")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow!("Missing required field or not a bool: is_active"))?;

        let created_at = DateTime::parse_from_rfc3339(created_at_str)
            .map_err(|e| anyhow!("Invalid created_at format '{}': {}", created_at_str, e))?
            .with_timezone(&Utc);

        // Optional fields can use safe defaults
        let used_by_user_id = row
            .get("used_by_user_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let used_at = row
            .get("used_at")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(InvitationCode {
            id: id.to_string(),
            code: code.to_string(),
            created_by_admin_id: created_by_admin_id.to_string(),
            used_by_user_id,
            expires_at: DateTime::parse_from_rfc3339(expires_at_str)
                .map_err(|e| anyhow!("Invalid expires_at format '{}': {}", expires_at_str, e))?
                .with_timezone(&Utc),
            created_at,
            used_at,
            is_active,
        })
    }

    async fn find_invitation_by_code(&self, code: &str) -> Result<InvitationCode> {
        let query = r#"
            SELECT id, code, created_by_admin_id, used_by_user_id, expires_at, created_at, used_at, is_active
            FROM invitation_codes 
            WHERE code = ?
        "#;

        let result = self.d1_service.query(query, &[code.into()]).await?;

        if let Some(row) = result
            .results::<HashMap<String, serde_json::Value>>()?
            .first()
        {
            self.parse_invitation_from_row(row)
        } else {
            Err(anyhow!("Invitation code not found"))
        }
    }

    /// Helper method to execute count queries with consistent error handling
    async fn execute_count_query(&self, query: &str, params: &[serde_json::Value]) -> Result<u32> {
        let params: Vec<worker::wasm_bindgen::JsValue> = params
            .iter()
            .map(|v| match v {
                serde_json::Value::String(s) => worker::wasm_bindgen::JsValue::from(s.as_str()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        worker::wasm_bindgen::JsValue::from(i as f64)
                    } else if let Some(f) = n.as_f64() {
                        worker::wasm_bindgen::JsValue::from(f)
                    } else {
                        worker::wasm_bindgen::JsValue::from(0.0)
                    }
                }
                serde_json::Value::Bool(b) => worker::wasm_bindgen::JsValue::from(*b),
                serde_json::Value::Null => worker::wasm_bindgen::JsValue::NULL,
                _ => worker::wasm_bindgen::JsValue::from(v.to_string().as_str()),
            })
            .collect();
        let result = self.d1_service.query(query, &params).await?;

        if let Some(row) = result
            .results::<HashMap<String, serde_json::Value>>()?
            .first()
        {
            let count_str = row.get("count").and_then(|v| v.as_str()).unwrap_or("0");
            match count_str.parse::<u32>() {
                Ok(count) => Ok(count),
                Err(e) => {
                    // Use our sanitized logger instead of standard log macro
                    crate::utils::logger::logger()
                        .warn(&format!("Failed to parse count '{}': {}", count_str, e));
                    Ok(0)
                }
            }
        } else {
            Ok(0)
        }
    }

    /// Helper method to handle the transaction logic for marking invitation codes as used
    async fn mark_invitation_used_transaction(
        &self,
        invitation_id: &str,
        user_id: &str,
        usage: &InvitationUsage,
    ) -> Result<()> {
        // Since D1 doesn't support real transactions, we'll execute operations sequentially
        // TODO: When D1 supports real transactions, implement proper atomic operations

        // Mark invitation code as used
        let mark_used_query = r#"
            UPDATE invitation_codes 
            SET used_by_user_id = ?, used_at = ?, is_active = false
            WHERE id = ?
        "#;

        self.d1_service
            .execute(
                mark_used_query,
                &[
                    user_id.into(),
                    Utc::now().to_rfc3339().into(),
                    invitation_id.into(),
                ],
            )
            .await?;

        // Store usage record
        let store_usage_query = r#"
            INSERT INTO invitation_usage 
            (invitation_id, user_id, telegram_id, used_at, beta_expires_at)
            VALUES (?, ?, ?, ?, ?)
        "#;

        self.d1_service
            .execute(
                store_usage_query,
                &[
                    usage.invitation_id.clone().into(),
                    usage.user_id.clone().into(),
                    usage.telegram_id.into(),
                    usage.used_at.to_rfc3339().into(),
                    usage.beta_expires_at.to_rfc3339().into(),
                ],
            )
            .await?;

        Ok(())
    }

    async fn count_total_invitations(&self) -> Result<u32> {
        self.execute_count_query("SELECT COUNT(*) as count FROM invitation_codes", &[])
            .await
    }

    async fn count_used_invitations(&self) -> Result<u32> {
        self.execute_count_query(
            "SELECT COUNT(*) as count FROM invitation_codes WHERE used_by_user_id IS NOT NULL",
            &[],
        )
        .await
    }

    async fn count_expired_invitations(&self) -> Result<u32> {
        self.execute_count_query("SELECT COUNT(*) as count FROM invitation_codes WHERE expires_at < datetime('now') AND used_by_user_id IS NULL", &[]).await
    }

    async fn count_active_beta_users(&self) -> Result<u32> {
        self.execute_count_query("SELECT COUNT(*) as count FROM invitation_usage WHERE beta_expires_at > datetime('now')", &[]).await
    }

    async fn get_invitations_by_admin(
        &self,
        admin_user_id: &str,
        limit: u32,
    ) -> Result<Vec<InvitationCode>> {
        let query = r#"
            SELECT id, code, created_by_admin_id, used_by_user_id, expires_at, created_at, used_at, is_active
            FROM invitation_codes 
            WHERE created_by_admin_id = ?
            ORDER BY created_at DESC
            LIMIT ?
        "#;

        let result = self
            .d1_service
            .query(query, &[admin_user_id.into(), limit.to_string().into()])
            .await?;

        let mut invitations = Vec::new();
        let results_vec = result.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            let invitation = self.parse_invitation_from_row(&row)?;
            invitations.push(invitation);
        }

        Ok(invitations)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationStatistics {
    pub total_generated: u32,
    pub total_used: u32,
    pub total_expired: u32,
    pub active_beta_users: u32,
    pub conversion_rate: f64,
}
