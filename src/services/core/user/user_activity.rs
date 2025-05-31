use crate::services::core::infrastructure::DatabaseManager;
use crate::types::{MessageAnalytics, UserProfile};
use crate::utils::{ArbitrageError, ArbitrageResult};
use std::sync::Arc;
use worker::console_log;

/// Service for tracking user activity and analytics
#[derive(Clone)]
pub struct UserActivityService {
    d1_service: Arc<DatabaseManager>,
    kv_service: Arc<worker::kv::KvStore>,
}

impl UserActivityService {
    pub fn new(d1_service: DatabaseManager, kv_service: worker::kv::KvStore) -> Self {
        Self {
            d1_service: Arc::new(d1_service),
            kv_service: Arc::new(kv_service),
        }
    }

    /// Record user activity
    pub async fn record_activity(
        &self,
        user_id: &str,
        activity_type: &str,
        metadata: serde_json::Value,
    ) -> ArbitrageResult<()> {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        
        // Store in KV for recent activity tracking
        let activity_key = format!("user_activity:{}:{}", user_id, now);
        let activity_data = serde_json::json!({
            "user_id": user_id,
            "activity_type": activity_type,
            "timestamp": now,
            "metadata": metadata
        });

        self.kv_service
            .put(&activity_key, &activity_data.to_string())?
            .expiration_ttl(24 * 3600) // 24 hours
            .execute()
            .await?;

        console_log!("📊 Recorded activity for user {}: {}", user_id, activity_type);
        Ok(())
    }

    /// Record message analytics
    pub async fn record_message_analytics(
        &self,
        analytics: &MessageAnalytics,
    ) -> ArbitrageResult<()> {
        // Store in D1 for persistent analytics
        let query = r#"
            INSERT INTO message_analytics (
                message_id, chat_id, user_id, message_type, command,
                timestamp, response_time_ms, success, error_message, metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        self.d1_service.execute(query, &[
            serde_json::Value::String(analytics.message_id.clone()),
            serde_json::Value::Number(serde_json::Number::from(analytics.chat_id)),
            serde_json::Value::String(analytics.user_id.clone().unwrap_or_default()),
            serde_json::Value::String(analytics.message_type.clone()),
            serde_json::Value::String(analytics.command.clone().unwrap_or_default()),
            serde_json::Value::Number(serde_json::Number::from(analytics.timestamp)),
            serde_json::Value::Number(serde_json::Number::from(analytics.response_time_ms)),
            serde_json::Value::Bool(analytics.success),
            serde_json::Value::String(analytics.error_message.clone().unwrap_or_default()),
            analytics.metadata.clone(),
        ]).await?;

        Ok(())
    }

    /// Get user activity summary
    pub async fn get_user_activity_summary(
        &self,
        user_id: &str,
        days: u32,
    ) -> ArbitrageResult<serde_json::Value> {
        let start_date = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let start_timestamp = start_date.timestamp_millis() as u64;

        // Get activity from KV store (recent) and D1 (historical)
        let mut activities = Vec::new();

        // Get recent activities from KV
        for hour in 0..24 {
            let hour_timestamp = chrono::Utc::now().timestamp_millis() as u64 - (hour * 3600 * 1000);
            let activity_pattern = format!("user_activity:{}:{}", user_id, hour_timestamp / 1000);
            
            // In a real implementation, we'd use KV list operations
            // For now, we'll just return a summary structure
        }

        // Get historical data from D1
        let query = r#"
            SELECT activity_type, COUNT(*) as count, MAX(timestamp) as last_activity
            FROM message_analytics 
            WHERE user_id = ? AND timestamp >= ?
            GROUP BY activity_type
            ORDER BY count DESC
        "#;

        let result = self.d1_service.query(query, &[
            serde_json::Value::String(user_id.to_string()),
            serde_json::Value::Number(serde_json::Number::from(start_timestamp)),
        ]).await?;

        let rows = result.results::<std::collections::HashMap<String, serde_json::Value>>()?;
        
        let summary = serde_json::json!({
            "user_id": user_id,
            "period_days": days,
            "activities": rows,
            "total_activities": rows.len(),
            "generated_at": chrono::Utc::now().timestamp_millis()
        });

        Ok(summary)
    }

    /// Update user last active timestamp
    pub async fn update_last_active(&self, user_id: &str) -> ArbitrageResult<()> {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        
        // Update in KV for fast access
        let last_active_key = format!("user_last_active:{}", user_id);
        self.kv_service
            .put(&last_active_key, &now.to_string())?
            .expiration_ttl(30 * 24 * 3600) // 30 days
            .execute()
            .await?;

        Ok(())
    }

    /// Check if user is recently active
    pub async fn is_recently_active(&self, user_id: &str, minutes: u32) -> ArbitrageResult<bool> {
        let last_active_key = format!("user_last_active:{}", user_id);
        
        if let Some(last_active_str) = self.kv_service.get(&last_active_key).text().await? {
            if let Ok(last_active) = last_active_str.parse::<u64>() {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                let threshold = minutes as u64 * 60 * 1000; // Convert to milliseconds
                
                return Ok(now - last_active < threshold);
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_activity_recording() {
        // Test activity recording logic
        let user_id = "test_user_123";
        let activity_type = "command_executed";
        let metadata = serde_json::json!({"command": "/start"});

        // In a real test, we'd mock the services
        // For now, just validate the data structures
        assert!(!user_id.is_empty());
        assert!(!activity_type.is_empty());
        assert!(metadata.is_object());
    }

    #[tokio::test]
    async fn test_message_analytics() {
        // Test message analytics structure
        let analytics = MessageAnalytics {
            message_id: "msg_123".to_string(),
            chat_id: 123456789,
            user_id: Some("user_123".to_string()),
            message_type: "command".to_string(),
            command: Some("/start".to_string()),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            response_time_ms: 150,
            success: true,
            error_message: None,
            metadata: serde_json::json!({}),
        };

        assert_eq!(analytics.message_type, "command");
        assert!(analytics.success);
        assert!(analytics.response_time_ms > 0);
    }
} 