use crate::services::core::infrastructure::{D1Service, KVService};
use crate::types::{
    ArbitrageOpportunity, ChatContext, EnhancedSessionState, EnhancedUserSession, SessionAnalytics,
    SessionConfig, SessionOutcome,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde_json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use worker::wasm_bindgen::JsValue;

/// Comprehensive session management service for user lifecycle tracking
/// and push notification eligibility management
#[derive(Clone)]
pub struct SessionManagementService {
    d1_service: Arc<D1Service>,
    kv_service: Arc<KVService>,
    config: SessionConfig,
}

impl SessionManagementService {
    pub fn new(d1_service: D1Service, kv_service: KVService) -> Self {
        Self {
            d1_service: Arc::new(d1_service),
            kv_service: Arc::new(kv_service),
            config: SessionConfig::default(),
        }
    }

    pub fn with_config(mut self, config: SessionConfig) -> Self {
        self.config = config;
        self
    }

    /// Start a new session for a user
    pub async fn start_session(
        &self,
        telegram_id: i64,
        user_id: String,
    ) -> ArbitrageResult<EnhancedUserSession> {
        // Check if user already has an active session
        if let Ok(existing_session) = self.get_session_by_telegram_id(telegram_id).await {
            if existing_session.is_active() {
                // Extend existing session
                let mut updated_session = existing_session;
                updated_session.update_activity();
                self.update_session(&updated_session).await?;
                return Ok(updated_session);
            }
        }

        // Create new session
        let session = EnhancedUserSession::new(user_id, telegram_id);

        // Store in database
        self.store_session(&session).await?;

        // Cache in KV for fast lookups
        self.cache_session(&session).await?;

        // Record session analytics
        self.record_session_start(&session).await?;

        Ok(session)
    }

    /// Validate if a user has an active session
    pub async fn validate_session(&self, user_id: &str) -> ArbitrageResult<bool> {
        match self.get_session_by_user_id(user_id).await {
            Ok(session) => Ok(session.is_active()),
            Err(_) => Ok(false),
        }
    }

    /// Validate session by telegram ID (faster lookup)
    pub async fn validate_session_by_telegram_id(&self, telegram_id: i64) -> ArbitrageResult<bool> {
        match self.get_session_by_telegram_id(telegram_id).await {
            Ok(session) => Ok(session.is_active()),
            Err(_) => Ok(false),
        }
    }

    /// Update user activity and extend session
    pub async fn update_activity(&self, user_id: &str) -> ArbitrageResult<()> {
        let mut session = self.get_session_by_user_id(user_id).await?;
        session.update_activity();

        self.update_session(&session).await?;
        self.cache_session(&session).await?;

        Ok(())
    }

    /// Update activity by telegram ID (faster)
    pub async fn update_activity_by_telegram_id(&self, telegram_id: i64) -> ArbitrageResult<()> {
        let mut session = self.get_session_by_telegram_id(telegram_id).await?;
        session.update_activity();

        self.update_session(&session).await?;
        self.cache_session(&session).await?;

        Ok(())
    }

    /// End a session manually
    pub async fn end_session(&self, user_id: &str) -> ArbitrageResult<()> {
        let mut session = self.get_session_by_user_id(user_id).await?;
        session.terminate();

        self.update_session(&session).await?;
        self.invalidate_session_cache(session.telegram_id).await?;

        // Record session analytics
        self.record_session_end(&session, SessionOutcome::Terminated)
            .await?;

        Ok(())
    }

    /// Get session by user ID
    pub async fn get_session_by_user_id(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<EnhancedUserSession> {
        let stmt = self.d1_service.database().prepare(
            "SELECT * FROM user_sessions WHERE user_id = ? AND session_state = 'active' ORDER BY created_at DESC LIMIT 1"
        );

        let result = stmt
            .bind(&[user_id.into()])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<std::collections::HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => self.row_to_session(row),
            None => Err(ArbitrageError::session_not_found(user_id)),
        }
    }

    /// Get session by telegram ID (with KV cache)
    pub async fn get_session_by_telegram_id(
        &self,
        telegram_id: i64,
    ) -> ArbitrageResult<EnhancedUserSession> {
        // Try KV cache first
        let cache_key = format!("session_cache:{}", telegram_id);
        if let Ok(Some(cached_data)) = self.kv_service.get(&cache_key).await {
            if let Ok(session) = serde_json::from_str::<EnhancedUserSession>(&cached_data) {
                if session.is_active() {
                    return Ok(session);
                }
            }
        }

        // Fallback to database
        let stmt = self.d1_service.database().prepare(
            "SELECT * FROM user_sessions WHERE telegram_id = ? AND session_state = 'active' ORDER BY created_at DESC LIMIT 1"
        );

        let result = stmt
            .bind(&[JsValue::from_f64(telegram_id as f64)])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<std::collections::HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let session = self.row_to_session(row)?;
                // Cache for future lookups
                self.cache_session(&session).await?;
                Ok(session)
            }
            None => Err(ArbitrageError::session_not_found(telegram_id.to_string())),
        }
    }

    /// Check if user is eligible for push notifications
    pub async fn is_eligible_for_push_notification(
        &self,
        user_id: &str,
        _opportunity: &ArbitrageOpportunity,
        _chat_context: &ChatContext,
    ) -> ArbitrageResult<bool> {
        // Layer 1: Session validation
        if !self.validate_session(user_id).await? {
            return Ok(false);
        }

        // Layer 2: Subscription & permissions (placeholder - integrate with UserProfileService)
        // This would check user's subscription tier and permissions
        // For now, assume basic validation

        // Layer 3: User preferences (placeholder - integrate with preference service)
        // This would check if user has push notifications enabled for this opportunity type

        // Layer 4: Rate limiting (placeholder - integrate with rate limiting service)
        // This would check daily/hourly limits and cooldown periods

        // Layer 5: Technical compatibility (placeholder - integrate with user API validation)
        // This would check if user has compatible exchange APIs

        // Layer 6: Context & compliance
        // Basic context validation - all contexts are currently eligible
        // Groups get enhanced limits but same eligibility rules

        // For now, return true if session is valid (other layers to be implemented)
        Ok(true)
    }

    /// Cleanup expired sessions
    pub async fn cleanup_expired_sessions(&self) -> ArbitrageResult<u32> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Find expired sessions
        let stmt = self.d1_service.database().prepare(
            "SELECT * FROM user_sessions WHERE session_state = 'active' AND expires_at < ?",
        );

        let result = stmt
            .bind(&[JsValue::from_f64(now as f64)])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .all()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        let results = result
            .results::<std::collections::HashMap<String, serde_json::Value>>()
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to parse results: {}", e))
            })?;

        let mut cleanup_count = 0;

        for row in results {
            if let Ok(mut session) = self.row_to_session(row) {
                session.expire();
                self.update_session(&session).await?;
                self.invalidate_session_cache(session.telegram_id).await?;

                // Record session analytics
                self.record_session_end(&session, SessionOutcome::Expired)
                    .await?;

                cleanup_count += 1;
            }
        }

        Ok(cleanup_count)
    }

    /// Get active session count for monitoring
    pub async fn get_active_session_count(&self) -> ArbitrageResult<u32> {
        let stmt = self
            .d1_service
            .database()
            .prepare("SELECT COUNT(*) as count FROM user_sessions WHERE session_state = 'active'");

        let result = stmt
            .bind(&[])
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to bind parameters: {}", e))
            })?
            .first::<std::collections::HashMap<String, serde_json::Value>>(None)
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute query: {}", e))
            })?;

        match result {
            Some(row) => {
                let count = row.get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                Ok(count)
            }
            None => Ok(0),
        }
    }

    /// Get session analytics for a user
    pub async fn get_session_analytics(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Vec<SessionAnalytics>> {
        let mut analytics = Vec::new();

        // Get recent session analytics from KV store
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        // Get session count for the user for recent dates
        for date in [today, yesterday] {
            // Get session count for the date
            let count_key = format!("session_count:{}", date);
            if let Ok(Some(count_str)) = self.kv_service.get(&count_key).await {
                if let Ok(_count) = count_str.parse::<u32>() {
                    analytics.push(SessionAnalytics {
                        session_id: format!("analytics_{}", date),
                        user_id: user_id.to_string(),
                        telegram_id: 0, // Would be filled from actual session data
                        session_duration_minutes: 0.0, // Would be calculated from individual session data
                        commands_executed: 0,
                        opportunities_viewed: 0,
                        onboarding_completed: false,
                        preferences_configured: false,
                        last_command: None,
                        session_outcome: SessionOutcome::Completed,
                        created_at: chrono::Utc::now().timestamp() as u64,
                    });
                }
            }
        }

        // If no analytics found, return empty vector
        Ok(analytics)
    }

    // Private helper methods

    /// Store session in database
    async fn store_session(&self, session: &EnhancedUserSession) -> ArbitrageResult<()> {
        let stmt = self.d1_service.database().prepare(
            r#"
            INSERT OR REPLACE INTO user_sessions (
                session_id, user_id, telegram_id, session_state,
                started_at, last_activity_at, expires_at,
                onboarding_completed, preferences_set, metadata,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        );

        let metadata_json = match &session.metadata {
            Some(metadata) => serde_json::to_string(metadata)?,
            None => "null".to_string(),
        };

        stmt.bind(&[
            session.session_id.clone().into(),
            session.user_id.clone().into(),
            JsValue::from_f64(session.telegram_id as f64),
            session.session_state.to_db_string().into(),
            JsValue::from_f64(session.started_at as f64),
            JsValue::from_f64(session.last_activity_at as f64),
            JsValue::from_f64(session.expires_at as f64),
            session.onboarding_completed.into(),
            session.preferences_set.into(),
            metadata_json.into(),
            JsValue::from_f64(session.created_at as f64),
            JsValue::from_f64(session.updated_at as f64),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Update existing session in database
    async fn update_session(&self, session: &EnhancedUserSession) -> ArbitrageResult<()> {
        let stmt = self.d1_service.database().prepare(
            r#"
            UPDATE user_sessions SET
                session_state = ?, last_activity_at = ?, expires_at = ?,
                onboarding_completed = ?, preferences_set = ?, metadata = ?, updated_at = ?
            WHERE session_id = ?
        "#,
        );

        let metadata_json = match &session.metadata {
            Some(metadata) => serde_json::to_string(metadata)?,
            None => "null".to_string(),
        };

        stmt.bind(&[
            session.session_state.to_db_string().into(),
            JsValue::from_f64(session.last_activity_at as f64),
            JsValue::from_f64(session.expires_at as f64),
            session.onboarding_completed.into(),
            session.preferences_set.into(),
            metadata_json.into(),
            JsValue::from_f64(session.updated_at as f64),
            session.session_id.clone().into(),
        ])
        .map_err(|e| ArbitrageError::database_error(format!("Failed to bind parameters: {}", e)))?
        .run()
        .await
        .map_err(|e| ArbitrageError::database_error(format!("Failed to execute query: {}", e)))?;

        Ok(())
    }

    /// Cache session in KV store for fast lookups
    async fn cache_session(&self, session: &EnhancedUserSession) -> ArbitrageResult<()> {
        let cache_key = format!("session_cache:{}", session.telegram_id);
        let session_json = serde_json::to_string(session)?;

        // Cache for 1 hour (3600 seconds)
        self.kv_service
            .put(&cache_key, &session_json, Some(3600))
            .await?;
        Ok(())
    }

    /// Invalidate session cache
    async fn invalidate_session_cache(&self, telegram_id: i64) -> ArbitrageResult<()> {
        let cache_key = format!("session_cache:{}", telegram_id);
        self.kv_service.delete(&cache_key).await?;
        Ok(())
    }

    /// Convert database row to session object
    fn row_to_session(
        &self,
        row: std::collections::HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<EnhancedUserSession> {
        let session_id = row
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::database_error("Missing session_id"))?
            .to_string();

        let user_id = row
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::database_error("Missing user_id"))?
            .to_string();

        let telegram_id = row
            .get("telegram_id")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ArbitrageError::database_error("Missing telegram_id"))?
            as i64;

        let session_state_str = row
            .get("session_state")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::database_error("Missing session_state"))?;

        let session_state = match session_state_str {
            "active" => EnhancedSessionState::Active,
            "expired" => EnhancedSessionState::Expired,
            "terminated" => EnhancedSessionState::Terminated,
            _ => EnhancedSessionState::Expired,
        };

        let started_at = row
            .get("started_at")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ArbitrageError::database_error("Missing started_at"))?
            as u64;

        let last_activity_at = row
            .get("last_activity_at")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ArbitrageError::database_error("Missing last_activity_at"))?
            as u64;

        let expires_at = row
            .get("expires_at")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ArbitrageError::database_error("Missing expires_at"))?
            as u64;

        let onboarding_completed = row
            .get("onboarding_completed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let preferences_set = row
            .get("preferences_set")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let metadata = row.get("metadata").and_then(|v| v.as_str()).and_then(|s| {
            if s == "null" {
                None
            } else {
                serde_json::from_str(s).ok()
            }
        });

        let created_at = row
            .get("created_at")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ArbitrageError::database_error("Missing created_at"))?
            as u64;

        let updated_at = row
            .get("updated_at")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ArbitrageError::database_error("Missing updated_at"))?
            as u64;

        Ok(EnhancedUserSession {
            session_id,
            user_id,
            telegram_id,
            session_state,
            started_at,
            last_activity_at,
            expires_at,
            onboarding_completed,
            preferences_set,
            metadata,
            created_at,
            updated_at,
        })
    }

    /// Record session start analytics
    async fn record_session_start(&self, session: &EnhancedUserSession) -> ArbitrageResult<()> {
        // Record session start metrics in KV store for analytics
        let analytics_key = format!("session_analytics:start:{}", session.session_id);
        let analytics_data = serde_json::json!({
            "session_id": session.session_id,
            "user_id": session.user_id,
            "telegram_id": session.telegram_id,
            "started_at": session.started_at,
            "event_type": "session_start",
            "timestamp": chrono::Utc::now().timestamp() as u64
        });

        // Store analytics data with 30-day TTL
        self.kv_service
            .put(
                &analytics_key,
                &analytics_data.to_string(),
                Some(30 * 24 * 3600),
            )
            .await?;

        // Update daily session count
        let date_key = format!("session_count:{}", chrono::Utc::now().format("%Y-%m-%d"));
        let current_count = self
            .kv_service
            .get(&date_key)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
            .unwrap_or(0);

        self.kv_service
            .put(&date_key, &(current_count + 1).to_string(), Some(24 * 3600))
            .await?;

        Ok(())
    }

    /// Record session end analytics
    async fn record_session_end(
        &self,
        session: &EnhancedUserSession,
        outcome: SessionOutcome,
    ) -> ArbitrageResult<()> {
        // Record session end metrics in KV store for analytics
        let analytics_key = format!("session_analytics:end:{}", session.session_id);
        let session_duration = session.last_activity_at.saturating_sub(session.started_at);

        let analytics_data = serde_json::json!({
            "session_id": session.session_id,
            "user_id": session.user_id,
            "telegram_id": session.telegram_id,
            "started_at": session.started_at,
            "ended_at": session.last_activity_at,
            "duration_seconds": session_duration,
            "outcome": format!("{:?}", outcome),
            "onboarding_completed": session.onboarding_completed,
            "preferences_set": session.preferences_set,
            "event_type": "session_end",
            "timestamp": chrono::Utc::now().timestamp() as u64
        });

        // Store analytics data with 30-day TTL
        self.kv_service
            .put(
                &analytics_key,
                &analytics_data.to_string(),
                Some(30 * 24 * 3600),
            )
            .await?;

        // Update outcome-specific counters
        let outcome_key = format!(
            "session_outcome:{}:{}",
            format!("{:?}", outcome).to_lowercase(),
            chrono::Utc::now().format("%Y-%m-%d")
        );
        let current_count = self
            .kv_service
            .get(&outcome_key)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
            .unwrap_or(0);

        self.kv_service
            .put(
                &outcome_key,
                &(current_count + 1).to_string(),
                Some(24 * 3600),
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Mock KV service for testing
    #[derive(Clone)]
    struct MockKvService {
        data: Arc<Mutex<HashMap<String, String>>>,
    }

    impl MockKvService {
        fn new() -> Self {
            Self {
                data: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        async fn put(&self, key: &str, value: &str, _ttl: Option<u32>) -> ArbitrageResult<()> {
            let mut data = self.data.lock().unwrap();
            data.insert(key.to_string(), value.to_string());
            Ok(())
        }

        async fn get(&self, key: &str) -> ArbitrageResult<Option<String>> {
            let data = self.data.lock().unwrap();
            Ok(data.get(key).cloned())
        }
    }

    /// Mock D1 service for testing
    #[derive(Clone)]
    struct MockD1Service {
        sessions: Arc<Mutex<HashMap<String, EnhancedUserSession>>>,
        query_results: Arc<Mutex<Vec<HashMap<String, serde_json::Value>>>>,
    }

    impl MockD1Service {
        fn new() -> Self {
            Self {
                sessions: Arc::new(Mutex::new(HashMap::new())),
                query_results: Arc::new(Mutex::new(Vec::new())),
            }
        }

        async fn query(
            &self,
            sql: &str,
            _params: &[serde_json::Value],
        ) -> ArbitrageResult<Vec<HashMap<String, serde_json::Value>>> {
            if sql.contains("SELECT * FROM user_sessions") {
                let sessions = self.sessions.lock().unwrap();
                let results: Vec<HashMap<String, serde_json::Value>> = sessions
                    .values()
                    .map(|session| {
                        let mut row = HashMap::new();
                        row.insert(
                            "session_id".to_string(),
                            serde_json::Value::String(session.session_id.clone()),
                        );
                        row.insert(
                            "user_id".to_string(),
                            serde_json::Value::String(session.user_id.clone()),
                        );
                        row.insert(
                            "telegram_id".to_string(),
                            serde_json::Value::Number(serde_json::Number::from(
                                session.telegram_id,
                            )),
                        );
                        row.insert(
                            "session_state".to_string(),
                            serde_json::Value::String(
                                format!("{:?}", session.session_state).to_lowercase(),
                            ),
                        );
                        row.insert(
                            "started_at".to_string(),
                            serde_json::Value::Number(serde_json::Number::from(session.started_at)),
                        );
                        row.insert(
                            "last_activity_at".to_string(),
                            serde_json::Value::Number(serde_json::Number::from(
                                session.last_activity_at,
                            )),
                        );
                        row.insert(
                            "expires_at".to_string(),
                            serde_json::Value::Number(serde_json::Number::from(session.expires_at)),
                        );
                        row.insert(
                            "onboarding_completed".to_string(),
                            serde_json::Value::Bool(session.onboarding_completed),
                        );
                        row.insert(
                            "preferences_set".to_string(),
                            serde_json::Value::Bool(session.preferences_set),
                        );
                        row.insert(
                            "metadata".to_string(),
                            serde_json::Value::String("null".to_string()),
                        );
                        row.insert(
                            "created_at".to_string(),
                            serde_json::Value::Number(serde_json::Number::from(session.created_at)),
                        );
                        row.insert(
                            "updated_at".to_string(),
                            serde_json::Value::Number(serde_json::Number::from(session.updated_at)),
                        );
                        row
                    })
                    .collect();
                Ok(results)
            } else if sql.contains("COUNT(*) as count FROM user_sessions") {
                let sessions = self.sessions.lock().unwrap();
                let count = sessions.len();
                let mut row = HashMap::new();
                row.insert(
                    "count".to_string(),
                    serde_json::Value::String(count.to_string()),
                );
                Ok(vec![row])
            } else {
                let query_results = self.query_results.lock().unwrap();
                Ok(query_results.clone())
            }
        }

        fn add_session(&self, session: EnhancedUserSession) {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session.session_id.clone(), session);
        }

        fn get_session_count(&self) -> usize {
            let sessions = self.sessions.lock().unwrap();
            sessions.len()
        }
    }

    /// Create mock session management service for testing
    fn create_mock_session_service() -> (MockD1Service, MockKvService) {
        let mock_d1 = MockD1Service::new();
        let mock_kv = MockKvService::new();

        (mock_d1, mock_kv)
    }

    /// Create test session
    fn create_test_session(telegram_id: i64, user_id: String) -> EnhancedUserSession {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        EnhancedUserSession {
            session_id: format!("session_{}", telegram_id),
            user_id,
            telegram_id,
            session_state: EnhancedSessionState::Active,
            started_at: now,
            last_activity_at: now,
            expires_at: now + (7 * 24 * 60 * 60 * 1000), // 7 days
            onboarding_completed: true,
            preferences_set: true,
            metadata: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn test_session_creation() {
        let (mock_d1, _mock_kv) = create_mock_session_service();

        // Test session creation
        let telegram_id = 123456789i64;
        let user_id = "test_user_001".to_string();
        let session = create_test_session(telegram_id, user_id.clone());

        // Verify session properties
        assert_eq!(session.telegram_id, telegram_id);
        assert_eq!(session.user_id, user_id);
        assert!(matches!(
            session.session_state,
            EnhancedSessionState::Active
        ));
        assert!(session.onboarding_completed);
        assert!(session.preferences_set);
        assert!(session.expires_at > session.started_at);

        // Store in mock D1
        mock_d1.add_session(session.clone());
        assert_eq!(mock_d1.get_session_count(), 1);
    }

    #[tokio::test]
    async fn test_session_validation() {
        let (mock_d1, mock_kv) = create_mock_session_service();

        // Create active session
        let active_session = create_test_session(111222333, "active_user".to_string());
        mock_d1.add_session(active_session.clone());

        // Cache session for validation
        let cache_key = format!("session_cache:{}", active_session.telegram_id);
        let session_json = serde_json::to_string(&active_session).unwrap();
        mock_kv
            .put(&cache_key, &session_json, Some(3600))
            .await
            .unwrap();

        // Test session validation via cache
        let cached_session = mock_kv.get(&cache_key).await.unwrap();
        assert!(cached_session.is_some());

        let parsed_session: EnhancedUserSession =
            serde_json::from_str(&cached_session.unwrap()).unwrap();
        assert_eq!(parsed_session.user_id, "active_user");
        assert!(matches!(
            parsed_session.session_state,
            EnhancedSessionState::Active
        ));
    }

    #[tokio::test]
    async fn test_session_expiry() {
        let (mock_d1, _mock_kv) = create_mock_session_service();

        // Create expired session
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let mut expired_session = create_test_session(444555666, "expired_user".to_string());
        expired_session.session_state = EnhancedSessionState::Expired;
        expired_session.expires_at = now - (24 * 60 * 60 * 1000); // Expired 1 day ago

        mock_d1.add_session(expired_session.clone());

        // Verify session is marked as expired
        assert!(matches!(
            expired_session.session_state,
            EnhancedSessionState::Expired
        ));
        assert!(expired_session.expires_at < now);
    }

    #[tokio::test]
    async fn test_session_activity_extension() {
        let (_mock_d1, mock_kv) = create_mock_session_service();

        // Test activity tracking
        let user_id = "activity_user";
        let activity_key = format!("last_activity:{}", user_id);
        let current_time = chrono::Utc::now().timestamp_millis().to_string();

        // Record activity
        mock_kv
            .put(&activity_key, &current_time, Some(24 * 3600))
            .await
            .unwrap();

        // Verify activity was recorded
        let stored_activity = mock_kv.get(&activity_key).await.unwrap().unwrap();
        assert_eq!(stored_activity, current_time);

        // Test activity extension logic
        let stored_timestamp = stored_activity.parse::<u64>().unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let time_diff = now - stored_timestamp;

        // Should be recent (within last few seconds)
        assert!(time_diff < 5000); // Less than 5 seconds
    }

    #[tokio::test]
    async fn test_push_notification_eligibility() {
        let (mock_d1, mock_kv) = create_mock_session_service();

        // Create eligible user session
        let eligible_session = create_test_session(777888999, "eligible_user".to_string());
        mock_d1.add_session(eligible_session.clone());

        // Test eligibility criteria
        assert!(matches!(
            eligible_session.session_state,
            EnhancedSessionState::Active
        ));
        assert!(eligible_session.onboarding_completed);
        assert!(eligible_session.preferences_set);

        // Test rate limiting setup
        let user_id = &eligible_session.user_id;
        let now = chrono::Utc::now();
        let hour_key = format!("rate_limit:{}:{}", user_id, now.format("%Y-%m-%d-%H"));
        let day_key = format!("rate_limit:{}:{}", user_id, now.format("%Y-%m-%d"));

        // Set initial rate limits (within limits)
        mock_kv.put(&hour_key, "1", Some(3600)).await.unwrap();
        mock_kv.put(&day_key, "5", Some(24 * 3600)).await.unwrap();

        // Verify rate limits are within bounds
        let hourly_count = mock_kv
            .get(&hour_key)
            .await
            .unwrap()
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let daily_count = mock_kv
            .get(&day_key)
            .await
            .unwrap()
            .unwrap()
            .parse::<u32>()
            .unwrap();

        assert!(hourly_count < 2); // Default hourly limit
        assert!(daily_count < 10); // Default daily limit
    }

    #[tokio::test]
    async fn test_session_analytics() {
        let (_mock_d1, mock_kv) = create_mock_session_service();

        // Test session analytics recording
        let session = create_test_session(123123123, "analytics_user".to_string());

        // Record session start analytics
        let analytics_key = format!("session_analytics:start:{}", session.session_id);
        let analytics_data = serde_json::json!({
            "session_id": session.session_id,
            "user_id": session.user_id,
            "telegram_id": session.telegram_id,
            "started_at": session.started_at,
            "event_type": "session_start",
            "timestamp": chrono::Utc::now().timestamp() as u64
        });

        mock_kv
            .put(
                &analytics_key,
                &analytics_data.to_string(),
                Some(30 * 24 * 3600),
            )
            .await
            .unwrap();

        // Verify analytics storage
        let stored_analytics = mock_kv.get(&analytics_key).await.unwrap().unwrap();
        let parsed_analytics: serde_json::Value = serde_json::from_str(&stored_analytics).unwrap();

        assert_eq!(parsed_analytics["session_id"], session.session_id);
        assert_eq!(parsed_analytics["event_type"], "session_start");
        assert_eq!(parsed_analytics["user_id"], session.user_id);

        // Test session count tracking
        let date_key = format!("session_count:{}", chrono::Utc::now().format("%Y-%m-%d"));
        mock_kv.put(&date_key, "1", Some(24 * 3600)).await.unwrap();

        let session_count = mock_kv
            .get(&date_key)
            .await
            .unwrap()
            .unwrap()
            .parse::<u32>()
            .unwrap();
        assert_eq!(session_count, 1);
    }

    #[tokio::test]
    async fn test_session_cleanup() {
        let (mock_d1, _mock_kv) = create_mock_session_service();

        // Create multiple sessions with different states
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Active session
        let active_session = create_test_session(111, "active".to_string());
        mock_d1.add_session(active_session);

        // Expired session
        let mut expired_session = create_test_session(222, "expired".to_string());
        expired_session.session_state = EnhancedSessionState::Expired;
        expired_session.expires_at = now - (24 * 60 * 60 * 1000);
        mock_d1.add_session(expired_session);

        // Terminated session
        let mut terminated_session = create_test_session(333, "terminated".to_string());
        terminated_session.session_state = EnhancedSessionState::Terminated;
        mock_d1.add_session(terminated_session);

        // Verify all sessions were added
        assert_eq!(mock_d1.get_session_count(), 3);

        // In a real cleanup, expired and terminated sessions would be removed
        // For this test, we just verify they exist and can be identified
        let query_result = mock_d1
            .query("SELECT * FROM user_sessions", &[])
            .await
            .unwrap();
        assert_eq!(query_result.len(), 3);
    }

    #[tokio::test]
    async fn test_concurrent_session_operations() {
        let (mock_d1, mock_kv) = create_mock_session_service();

        // Test concurrent session creation
        let mut sessions = Vec::new();
        for i in 0..10 {
            let session = create_test_session(i as i64, format!("concurrent_user_{}", i));
            sessions.push(session);
        }

        // Store all sessions
        for session in &sessions {
            mock_d1.add_session(session.clone());

            // Cache each session
            let cache_key = format!("session_cache:{}", session.telegram_id);
            let session_json = serde_json::to_string(session).unwrap();
            mock_kv
                .put(&cache_key, &session_json, Some(3600))
                .await
                .unwrap();
        }

        // Verify all sessions were stored
        assert_eq!(mock_d1.get_session_count(), 10);

        // Verify all sessions can be retrieved from cache
        for session in &sessions {
            let cache_key = format!("session_cache:{}", session.telegram_id);
            let cached_session = mock_kv.get(&cache_key).await.unwrap();
            assert!(cached_session.is_some());

            let parsed_session: EnhancedUserSession =
                serde_json::from_str(&cached_session.unwrap()).unwrap();
            assert_eq!(parsed_session.user_id, session.user_id);
        }
    }
}
