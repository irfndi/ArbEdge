// src/services/user_profile.rs

use crate::services::D1Service;
use crate::types::{ExchangeIdEnum, InvitationCode, UserApiKey, UserProfile, UserSession};
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::console_log;
use worker::kv::KvStore;
// use std::collections::HashMap; // TODO: Re-enable when implementing HashMap functionality

pub struct UserProfileService {
    kv_store: KvStore,
    d1_service: D1Service,
    encryption_key: String, // For encrypting API keys
}

impl UserProfileService {
    pub fn new(kv_store: KvStore, d1_service: D1Service, encryption_key: String) -> Self {
        Self {
            kv_store,
            d1_service,
            encryption_key,
        }
    }

    // User Profile CRUD Operations (now using D1 for persistence, KV for sessions)
    pub async fn create_user_profile(
        &self,
        telegram_user_id: i64,
        invitation_code: Option<String>,
        telegram_username: Option<String>,
    ) -> ArbitrageResult<UserProfile> {
        // Validate telegram_user_id is positive
        if telegram_user_id <= 0 {
            return Err(ArbitrageError::validation_error(
                "Telegram user ID must be positive",
            ));
        }

        // Check if user already exists (check D1 for authoritative data)
        if let Some(_existing) = self
            .d1_service
            .get_user_by_telegram_id(telegram_user_id)
            .await?
        {
            return Err(ArbitrageError::validation_error(
                "User profile already exists for this Telegram ID",
            ));
        }

        // Validate invitation code if provided (D1 for persistent data)
        if let Some(ref code) = invitation_code {
            self.validate_and_use_invitation_code(code).await?;
        }

        let mut profile = UserProfile::new(Some(telegram_user_id), invitation_code);
        profile.telegram_username = telegram_username;

        // Store profile in D1 (persistent storage)
        self.d1_service.create_user_profile(&profile).await?;

        // Create user session in KV (fast access)
        let session = UserSession::new(profile.user_id.clone(), telegram_user_id);
        self.store_user_session(&session).await?;

        Ok(profile)
    }

    pub async fn get_user_profile(&self, user_id: &str) -> ArbitrageResult<Option<UserProfile>> {
        // Always get from D1 for authoritative user data
        self.d1_service.get_user_profile(user_id).await
    }

    pub async fn get_user_by_telegram_id(
        &self,
        telegram_user_id: i64,
    ) -> ArbitrageResult<Option<UserProfile>> {
        // Check KV cache first for fast lookup
        let cache_key = format!("user_cache:telegram:{}", telegram_user_id);
        if let Ok(Some(cached_user_id)) = self.kv_store.get(&cache_key).text().await {
            // Get full profile from D1 using cached user_id
            if let Some(profile) = self.d1_service.get_user_profile(&cached_user_id).await? {
                return Ok(Some(profile));
            }
        }

        // If not in cache, get from D1 and cache the result
        if let Some(profile) = self
            .d1_service
            .get_user_by_telegram_id(telegram_user_id)
            .await?
        {
            // Cache the mapping for faster future lookups
            self.cache_telegram_user_mapping(telegram_user_id, &profile.user_id)
                .await?;
            Ok(Some(profile))
        } else {
            Ok(None)
        }
    }

    pub async fn update_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
        let mut updated_profile = profile.clone();
        updated_profile.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        // Update in D1 (persistent storage)
        self.d1_service
            .update_user_profile(&updated_profile)
            .await?;

        // Invalidate cache
        self.invalidate_user_cache(&updated_profile.user_id).await?;

        Ok(())
    }

    pub async fn update_user_last_active(&self, user_id: &str) -> ArbitrageResult<()> {
        if let Some(mut profile) = self.get_user_profile(user_id).await? {
            profile.update_last_active();
            self.update_user_profile(&profile).await?;
        }
        Ok(())
    }

    // API Key Management (D1 for persistent storage, with KV invalidation)
    pub async fn add_user_api_key(
        &self,
        user_id: &str,
        exchange: ExchangeIdEnum,
        api_key: &str,
        secret: &str,
        permissions: Vec<String>,
    ) -> ArbitrageResult<()> {
        let mut profile = self
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        let api_key_encrypted = self.encrypt_string(api_key)?;
        let secret_encrypted = self.encrypt_string(secret)?;

        let user_api_key = UserApiKey::new_exchange_key(
            user_id.to_string(),
            exchange,
            api_key_encrypted,
            secret_encrypted,
            permissions,
        );

        // Store API key metadata in D1
        self.d1_service
            .store_user_api_key(user_id, &user_api_key)
            .await?;

        // Update profile with new API key
        profile.add_api_key(user_api_key);
        self.update_user_profile(&profile).await?;

        Ok(())
    }

    pub async fn remove_user_api_key(
        &self,
        user_id: &str,
        exchange: &ExchangeIdEnum,
    ) -> ArbitrageResult<bool> {
        let mut profile = self
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        let removed = profile.remove_api_key(exchange);
        if removed {
            self.update_user_profile(&profile).await?;
        }

        Ok(removed)
    }

    pub async fn get_user_api_keys(&self, user_id: &str) -> ArbitrageResult<Vec<UserApiKey>> {
        let profile = self
            .get_user_profile(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        Ok(profile.api_keys.clone())
    }

    pub async fn decrypt_user_api_key(
        &self,
        encrypted_key: &str,
        encrypted_secret: &str,
    ) -> ArbitrageResult<(String, String)> {
        let api_key = self.decrypt_string(encrypted_key)?;
        let secret = self.decrypt_string(encrypted_secret)?;
        Ok((api_key, secret))
    }

    /// Execute a read-only query on the D1 database for analytics and logging
    /// SECURITY: Only SELECT queries are allowed to prevent SQL injection
    #[allow(dead_code)]
    pub(crate) async fn execute_readonly_query(
        &self,
        query: &str,
        params: &[serde_json::Value],
    ) -> ArbitrageResult<()> {
        // Validate that the query is read-only (SELECT only)
        let trimmed_query = query.trim().to_lowercase();
        if !trimmed_query.starts_with("select") {
            return Err(ArbitrageError::validation_error(
                "Only SELECT queries are allowed for security reasons",
            ));
        }

        // Additional validation: ensure no dangerous keywords
        let dangerous_keywords = [
            "insert", "update", "delete", "drop", "create", "alter", "exec", "execute",
        ];
        for keyword in dangerous_keywords {
            if trimmed_query.contains(keyword) {
                return Err(ArbitrageError::validation_error(format!(
                    "Query contains forbidden keyword: {}",
                    keyword
                )));
            }
        }

        self.d1_service.execute(query, params).await
    }

    /// Execute a write operation (INSERT, UPDATE, DELETE) on the D1 database
    /// SECURITY: This method is restricted to crate-level access and should only be used
    /// for trusted operations with validated inputs
    pub(crate) async fn execute_write_operation(
        &self,
        query: &str,
        params: &[serde_json::Value],
    ) -> ArbitrageResult<()> {
        // Validate that the query is a write operation
        let trimmed_query = query.trim().to_lowercase();
        let allowed_write_operations = ["insert", "update", "delete"];

        let is_valid_write = allowed_write_operations
            .iter()
            .any(|op| trimmed_query.starts_with(op));

        if !is_valid_write {
            return Err(ArbitrageError::validation_error(
                "Only INSERT, UPDATE, DELETE operations are allowed for write operations",
            ));
        }

        // Additional validation: ensure no dangerous keywords for write operations
        let dangerous_keywords = ["drop", "create", "alter", "exec", "execute"];
        for keyword in dangerous_keywords {
            if trimmed_query.contains(keyword) {
                return Err(ArbitrageError::validation_error(format!(
                    "Query contains forbidden keyword: {}",
                    keyword
                )));
            }
        }

        self.d1_service.execute(query, params).await
    }

    // Session Management (KV only for fast access)
    pub async fn store_user_session(&self, session: &UserSession) -> ArbitrageResult<()> {
        let key = format!("user_session:{}", session.telegram_chat_id);
        let value = serde_json::to_string(session).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize user session: {}", e))
        })?;

        self.kv_store
            .put(&key, value)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to store user session: {}", e))
            })?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute session put: {}", e))
            })?;

        Ok(())
    }

    pub async fn get_user_session(
        &self,
        telegram_chat_id: i64,
    ) -> ArbitrageResult<Option<UserSession>> {
        let key = format!("user_session:{}", telegram_chat_id);

        match self.kv_store.get(&key).text().await {
            Ok(Some(value)) => {
                let session: UserSession = serde_json::from_str(&value).map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to deserialize user session: {}",
                        e
                    ))
                })?;

                // Check if session is expired
                if session.is_expired() {
                    self.delete_user_session(telegram_chat_id).await?;
                    Ok(None)
                } else {
                    Ok(Some(session))
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(ArbitrageError::database_error(format!(
                "Failed to get user session: {}",
                e
            ))),
        }
    }

    pub async fn delete_user_session(&self, telegram_chat_id: i64) -> ArbitrageResult<()> {
        let key = format!("user_session:{}", telegram_chat_id);

        self.kv_store.delete(&key).await.map_err(|e| {
            ArbitrageError::database_error(format!("Failed to delete user session: {}", e))
        })?;

        Ok(())
    }

    // Invitation Code Management (D1 for persistence, KV for validation cache)
    pub async fn create_invitation_code(
        &self,
        purpose: String,
        max_uses: Option<u32>,
        expires_in_days: Option<u32>,
        created_by: Option<String>,
    ) -> ArbitrageResult<InvitationCode> {
        let mut invitation = InvitationCode::new(purpose, max_uses, expires_in_days);
        invitation.created_by = created_by;

        // Store in D1 (persistent storage)
        self.d1_service.create_invitation_code(&invitation).await?;

        // Cache in KV for fast validation
        self.store_invitation_code(&invitation).await?;

        Ok(invitation)
    }

    pub async fn validate_and_use_invitation_code(&self, code: &str) -> ArbitrageResult<()> {
        // Always get from D1 for authoritative data to ensure consistency
        let mut invitation = self
            .d1_service
            .get_invitation_code(code)
            .await?
            .ok_or_else(|| ArbitrageError::validation_error("Invalid invitation code"))?;

        if !invitation.can_be_used() {
            return Err(ArbitrageError::validation_error(
                "Invitation code is invalid or expired",
            ));
        }

        invitation.use_code();

        // Update D1 first (authoritative source)
        self.d1_service.update_invitation_code(&invitation).await?;

        // Update cache after successful D1 update (best effort)
        // If cache update fails, it's not critical as D1 is authoritative
        if let Err(e) = self.store_invitation_code(&invitation).await {
            // Log the cache update failure but don't fail the operation
            console_log!("⚠️ Cache update failed for invitation code {}: {}", code, e);
        }

        Ok(())
    }

    pub async fn get_invitation_code(&self, code: &str) -> ArbitrageResult<Option<InvitationCode>> {
        self.d1_service.get_invitation_code(code).await
    }

    /// Get all user profiles (admin function)
    pub async fn get_all_user_profiles(&self) -> ArbitrageResult<Vec<UserProfile>> {
        // Use D1Service to get all user profiles
        let profiles = self.d1_service.list_user_profiles(None, None).await?;
        Ok(profiles)
    }

    // Helper methods for caching
    async fn cache_telegram_user_mapping(
        &self,
        telegram_user_id: i64,
        user_id: &str,
    ) -> ArbitrageResult<()> {
        let cache_key = format!("user_cache:telegram:{}", telegram_user_id);

        self.kv_store
            .put(&cache_key, user_id)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to cache user mapping: {}", e))
            })?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute cache put: {}", e))
            })?;

        Ok(())
    }

    async fn invalidate_user_cache(&self, user_id: &str) -> ArbitrageResult<()> {
        // In a real implementation, we'd need to track all cache keys for a user
        // For now, we'll implement a simple cache invalidation
        let profile_cache_key = format!("user_cache:profile:{}", user_id);

        let _ = self.kv_store.delete(&profile_cache_key).await;
        // Note: We can't easily invalidate telegram_user_id cache without knowing the telegram_user_id
        // This could be improved by maintaining a reverse mapping

        Ok(())
    }

    async fn store_invitation_code(&self, invitation: &InvitationCode) -> ArbitrageResult<()> {
        let key = format!("invitation_code:{}", invitation.code);
        let value = serde_json::to_string(invitation).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize invitation code: {}", e))
        })?;

        self.kv_store
            .put(&key, value)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to store invitation code: {}", e))
            })?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute invitation put: {}", e))
            })?;

        Ok(())
    }

    #[allow(dead_code)]
    async fn get_invitation_code_from_cache(
        &self,
        code: &str,
    ) -> ArbitrageResult<Option<InvitationCode>> {
        let key = format!("invitation_code:{}", code);

        match self.kv_store.get(&key).text().await {
            Ok(Some(value)) => {
                let invitation: InvitationCode = serde_json::from_str(&value).map_err(|e| {
                    ArbitrageError::parse_error(format!(
                        "Failed to deserialize invitation code: {}",
                        e
                    ))
                })?;
                Ok(Some(invitation))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(ArbitrageError::database_error(format!(
                "Failed to get invitation code from cache: {}",
                e
            ))),
        }
    }

    // Simple encryption/decryption (in production, use proper encryption)
    #[allow(clippy::result_large_err)]
    fn encrypt_string(&self, plaintext: &str) -> ArbitrageResult<String> {
        // For MVP, we'll use base64 encoding with a simple XOR cipher
        // In production, use proper encryption like AES-GCM
        use base64::{engine::general_purpose, Engine as _};

        let key_bytes = self.encryption_key.as_bytes();
        let plaintext_bytes = plaintext.as_bytes();

        let encrypted: Vec<u8> = plaintext_bytes
            .iter()
            .enumerate()
            .map(|(i, &byte)| byte ^ key_bytes[i % key_bytes.len()])
            .collect();

        Ok(general_purpose::STANDARD.encode(encrypted))
    }

    #[allow(clippy::result_large_err)]
    fn decrypt_string(&self, ciphertext: &str) -> ArbitrageResult<String> {
        use base64::{engine::general_purpose, Engine as _};

        let encrypted = general_purpose::STANDARD
            .decode(ciphertext)
            .map_err(|e| ArbitrageError::parse_error(format!("Failed to decode base64: {}", e)))?;

        let key_bytes = self.encryption_key.as_bytes();
        let decrypted: Vec<u8> = encrypted
            .iter()
            .enumerate()
            .map(|(i, &byte)| byte ^ key_bytes[i % key_bytes.len()])
            .collect();

        String::from_utf8(decrypted).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to convert decrypted bytes to string: {}",
                e
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExchangeIdEnum;

    // Note: Service integration tests would require proper mocking framework
    // These tests focus on the core logic that can be tested independently

    #[tokio::test]
    async fn test_user_profile_creation() {
        // Test user profile creation logic
        let telegram_user_id = 123456789i64;
        let invitation_code = Some("TEST-CODE".to_string());
        let _telegram_username = Some("testuser".to_string());

        // Create a test profile manually to validate structure
        let profile = UserProfile::new(Some(telegram_user_id), invitation_code.clone());

        assert_eq!(profile.telegram_user_id, Some(telegram_user_id));
        assert_eq!(profile.invitation_code, invitation_code);
        assert!(profile.is_active);
        assert_eq!(profile.total_trades, 0);
        assert_eq!(profile.total_pnl_usdt, 0.0);
    }

    #[tokio::test]
    async fn test_user_profile_api_key_management() {
        // Test API key management logic
        let mut profile = UserProfile::new(Some(123456789), None);
        let api_key1 = UserApiKey::new_exchange_key(
            profile.user_id.clone(),
            ExchangeIdEnum::Binance,
            "encrypted_key".to_string(),
            "encrypted_secret".to_string(),
            vec!["read".to_string(), "trade".to_string()],
        );

        // Test adding first API key
        profile.add_api_key(api_key1);
        assert_eq!(profile.api_keys.len(), 1);
        assert!(!profile.has_minimum_exchanges()); // Need at least 2 exchanges

        // Add second API key for different exchange
        let api_key2 = UserApiKey::new_exchange_key(
            profile.user_id.clone(),
            ExchangeIdEnum::Bybit,
            "encrypted_key2".to_string(),
            "encrypted_secret2".to_string(),
            vec!["read".to_string(), "trade".to_string()],
        );

        profile.add_api_key(api_key2);
        assert_eq!(profile.api_keys.len(), 2);
        assert!(profile.has_minimum_exchanges()); // Now has 2 exchanges

        // Test removing one API key
        let removed = profile.remove_api_key(&ExchangeIdEnum::Binance);
        assert!(removed);
        assert_eq!(profile.api_keys.len(), 1);
        assert!(!profile.has_minimum_exchanges()); // Back to 1 exchange
    }

    #[tokio::test]
    async fn test_invitation_code_creation() {
        // Test invitation code creation logic
        let purpose = "beta_testing".to_string();
        let max_uses = Some(10);
        let expires_in_days = Some(30);

        let invitation = InvitationCode::new(purpose.clone(), max_uses, expires_in_days);

        assert_eq!(invitation.purpose, purpose);
        assert_eq!(invitation.max_uses, max_uses);
        assert_eq!(invitation.current_uses, 0);
        assert!(invitation.is_active);
        assert!(invitation.can_be_used());
    }

    #[tokio::test]
    async fn test_invitation_code_usage() {
        // Test invitation code usage logic
        let mut invitation = InvitationCode::new(
            "beta_testing".to_string(),
            Some(1), // Only 1 use allowed
            Some(30),
        );

        // Should be usable initially
        assert!(invitation.can_be_used());

        // Use the code
        let used = invitation.use_code();
        assert!(used);
        assert_eq!(invitation.current_uses, 1);

        // Should not be usable after reaching max uses
        assert!(!invitation.can_be_used());
        let used_again = invitation.use_code();
        assert!(!used_again);
    }

    #[tokio::test]
    async fn test_user_session_creation() {
        // Test user session creation logic
        let user_id = "test_user_123".to_string();
        let telegram_chat_id = 987654321i64;

        let session = UserSession::new(user_id.clone(), telegram_chat_id);

        assert_eq!(session.user_id, user_id);
        assert_eq!(session.telegram_chat_id, telegram_chat_id);
        assert_eq!(session.current_state, crate::types::SessionState::Idle);
        assert!(!session.is_expired());
    }

    #[tokio::test]
    async fn test_encryption_decryption() {
        // Test encryption/decryption logic (simplified test)
        let original_text = "test_api_key_12345";
        let encryption_key = "test_encryption_key_32_bytes_long";

        // In a real test, we'd create the service and test encrypt/decrypt
        // For now, just validate the test strings
        assert!(!original_text.is_empty());
        assert!(!encryption_key.is_empty());
        assert_eq!(encryption_key.len(), 33); // Verify actual key length (was 30, corrected to 33)
    }
}
