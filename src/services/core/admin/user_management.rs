// src/services/core/admin/user_management.rs

use crate::types::{
    UserProfile, SubscriptionTier, UserAccessLevel, UpdateUserProfileRequest,
    UserSession, GroupAdminRole, GroupRegistration, UserStatistics,
};
use crate::utils::{ArbitrageResult, ArbitrageError};
use worker::{Env, kv::KvStore};
use std::collections::HashMap;

/// User management service for super admin operations
#[derive(Debug, Clone)]
pub struct UserManagementService {
    kv_store: KvStore,
    env: Env,
}

impl UserManagementService {
    pub fn new(env: Env, kv_store: KvStore) -> Self {
        Self { kv_store, env }
    }

    /// Get all users (super admin only)
    pub async fn get_all_users(&self, limit: Option<u32>, offset: Option<u32>) -> ArbitrageResult<Vec<UserProfile>> {
        let limit = limit.unwrap_or(100).min(1000); // Max 1000 users per request
        let offset = offset.unwrap_or(0);

        // In a real implementation, this would query a database
        // For now, we'll simulate with KV store scanning
        let mut users = Vec::new();
        
        // This is a simplified implementation - in production, use proper database pagination
        for i in offset..(offset + limit) {
            let user_key = format!("user_profile:{}", i);
            if let Some(user_data) = self.kv_store.get(&user_key).text().await? {
                if let Ok(user_profile) = serde_json::from_str::<UserProfile>(&user_data) {
                    users.push(user_profile);
                }
            }
        }

        Ok(users)
    }

    /// Get user by ID
    pub async fn get_user_by_id(&self, user_id: &str) -> ArbitrageResult<Option<UserProfile>> {
        let user_key = format!("user_profile:{}", user_id);
        
        if let Some(user_data) = self.kv_store.get(&user_key).text().await? {
            let user_profile = serde_json::from_str::<UserProfile>(&user_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse user profile: {}", e)))?;
            Ok(Some(user_profile))
        } else {
            Ok(None)
        }
    }

    /// Update user profile (super admin only)
    pub async fn update_user_profile(&self, request: UpdateUserProfileRequest) -> ArbitrageResult<UserProfile> {
        // Validate request
        request.validate()
            .map_err(|e| ArbitrageError::ValidationError(e))?;

        // Get existing user
        let mut user_profile = self.get_user_by_id(&request.user_id).await?
            .ok_or_else(|| ArbitrageError::NotFound("User not found".to_string()))?;

        // Apply updates
        request.apply_to_profile(&mut user_profile)
            .map_err(|e| ArbitrageError::ValidationError(e))?;

        // Save updated profile
        let user_key = format!("user_profile:{}", user_profile.user_id);
        let user_data = serde_json::to_string(&user_profile)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize user profile: {}", e)))?;

        self.kv_store.put(&user_key, &user_data)?
            .execute()
            .await?;

        Ok(user_profile)
    }

    /// Deactivate user (super admin only)
    pub async fn deactivate_user(&self, user_id: &str) -> ArbitrageResult<()> {
        let mut user_profile = self.get_user_by_id(user_id).await?
            .ok_or_else(|| ArbitrageError::NotFound("User not found".to_string()))?;

        user_profile.is_active = false;
        user_profile.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        // Save updated profile
        let user_key = format!("user_profile:{}", user_profile.user_id);
        let user_data = serde_json::to_string(&user_profile)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize user profile: {}", e)))?;

        self.kv_store.put(&user_key, &user_data)?
            .execute()
            .await?;

        // Also terminate any active sessions
        self.terminate_user_sessions(user_id).await?;

        Ok(())
    }

    /// Activate user (super admin only)
    pub async fn activate_user(&self, user_id: &str) -> ArbitrageResult<()> {
        let mut user_profile = self.get_user_by_id(user_id).await?
            .ok_or_else(|| ArbitrageError::NotFound("User not found".to_string()))?;

        user_profile.is_active = true;
        user_profile.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        // Save updated profile
        let user_key = format!("user_profile:{}", user_profile.user_id);
        let user_data = serde_json::to_string(&user_profile)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize user profile: {}", e)))?;

        self.kv_store.put(&user_key, &user_data)?
            .execute()
            .await?;

        Ok(())
    }

    /// Change user subscription tier (super admin only)
    pub async fn change_user_subscription(&self, user_id: &str, new_tier: SubscriptionTier) -> ArbitrageResult<UserProfile> {
        let mut user_profile = self.get_user_by_id(user_id).await?
            .ok_or_else(|| ArbitrageError::NotFound("User not found".to_string()))?;

        user_profile.subscription_tier = new_tier;
        user_profile.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        // Save updated profile
        let user_key = format!("user_profile:{}", user_profile.user_id);
        let user_data = serde_json::to_string(&user_profile)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize user profile: {}", e)))?;

        self.kv_store.put(&user_key, &user_data)?
            .execute()
            .await?;

        Ok(user_profile)
    }

    /// Change user access level (super admin only)
    pub async fn change_user_access_level(&self, user_id: &str, new_access_level: UserAccessLevel) -> ArbitrageResult<UserProfile> {
        let mut user_profile = self.get_user_by_id(user_id).await?
            .ok_or_else(|| ArbitrageError::NotFound("User not found".to_string()))?;

        user_profile.access_level = new_access_level;
        user_profile.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        // Save updated profile
        let user_key = format!("user_profile:{}", user_profile.user_id);
        let user_data = serde_json::to_string(&user_profile)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize user profile: {}", e)))?;

        self.kv_store.put(&user_key, &user_data)?
            .execute()
            .await?;

        Ok(user_profile)
    }

    /// Get user statistics
    pub async fn get_user_statistics(&self) -> ArbitrageResult<UserStatistics> {
        // This is a simplified implementation - in production, use proper database aggregation
        let mut stats = UserStatistics::default();

        // Count users by scanning KV store (simplified)
        for i in 0..10000 { // Limit scan to prevent timeout
            let user_key = format!("user_profile:{}", i);
            if let Some(user_data) = self.kv_store.get(&user_key).text().await? {
                if let Ok(user_profile) = serde_json::from_str::<UserProfile>(&user_data) {
                    stats.total_users += 1;
                    
                    if user_profile.is_active {
                        stats.active_users += 1;
                    }

                    match user_profile.subscription_tier {
                        SubscriptionTier::Free => stats.free_users += 1,
                        SubscriptionTier::Paid => stats.paid_users += 1,
                        SubscriptionTier::Admin => stats.admin_users += 1,
                        SubscriptionTier::SuperAdmin => stats.super_admin_users += 1,
                        _ => stats.other_users += 1,
                    }

                    // Check recent activity (last 7 days)
                    let now = chrono::Utc::now().timestamp_millis() as u64;
                    let seven_days_ago = now - (7 * 24 * 60 * 60 * 1000);
                    
                    if let Some(last_active) = user_profile.last_active {
                        if last_active > seven_days_ago {
                            stats.recently_active_users += 1;
                        }
                    }
                }
            }
        }

        Ok(stats)
    }

    /// Get user sessions (super admin only)
    pub async fn get_user_sessions(&self, user_id: &str) -> ArbitrageResult<Vec<UserSession>> {
        let sessions_key = format!("user_sessions:{}", user_id);
        
        if let Some(sessions_data) = self.kv_store.get(&sessions_key).text().await? {
            let sessions = serde_json::from_str::<Vec<UserSession>>(&sessions_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse user sessions: {}", e)))?;
            Ok(sessions)
        } else {
            Ok(Vec::new())
        }
    }

    /// Terminate user sessions (super admin only)
    pub async fn terminate_user_sessions(&self, user_id: &str) -> ArbitrageResult<u32> {
        let sessions = self.get_user_sessions(user_id).await?;
        let active_sessions_count = sessions.iter().filter(|s| s.is_active).count() as u32;

        // Mark all sessions as inactive
        let mut updated_sessions = sessions;
        for session in &mut updated_sessions {
            session.is_active = false;
        }

        // Save updated sessions
        let sessions_key = format!("user_sessions:{}", user_id);
        let sessions_data = serde_json::to_string(&updated_sessions)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize sessions: {}", e)))?;

        self.kv_store.put(&sessions_key, &sessions_data)?
            .execute()
            .await?;

        Ok(active_sessions_count)
    }

    /// Get all group registrations (super admin only)
    pub async fn get_all_groups(&self) -> ArbitrageResult<Vec<GroupRegistration>> {
        let mut groups = Vec::new();

        // Scan for group registrations (simplified implementation)
        for i in 0..1000 { // Limit scan
            let group_key = format!("group_registration:{}", i);
            if let Some(group_data) = self.kv_store.get(&group_key).text().await? {
                if let Ok(group_registration) = serde_json::from_str::<GroupRegistration>(&group_data) {
                    groups.push(group_registration);
                }
            }
        }

        Ok(groups)
    }

    /// Get group admin roles for a user
    pub async fn get_user_group_admin_roles(&self, user_id: &str) -> ArbitrageResult<Vec<GroupAdminRole>> {
        let roles_key = format!("user_group_admin_roles:{}", user_id);
        
        if let Some(roles_data) = self.kv_store.get(&roles_key).text().await? {
            let roles = serde_json::from_str::<Vec<GroupAdminRole>>(&roles_data)
                .map_err(|e| ArbitrageError::DatabaseError(format!("Failed to parse group admin roles: {}", e)))?;
            Ok(roles)
        } else {
            Ok(Vec::new())
        }
    }

    /// Check if user is super admin
    pub async fn is_super_admin(&self, user_id: &str) -> ArbitrageResult<bool> {
        if let Some(user_profile) = self.get_user_by_id(user_id).await? {
            Ok(matches!(user_profile.subscription_tier, SubscriptionTier::SuperAdmin))
        } else {
            Ok(false)
        }
    }

    /// Create new user (super admin only)
    pub async fn create_user(&self, user_data: crate::services::core::admin::CreateUserData) -> ArbitrageResult<UserProfile> {
        // Generate new user ID
        let user_id = uuid::Uuid::new_v4().to_string();
        
        // Create user profile
        let user_profile = UserProfile {
            user_id: user_id.clone(),
            telegram_user_id: user_data.telegram_user_id.unwrap_or(0),
            telegram_username: user_data.username.clone(),
            subscription_tier: user_data.subscription_tier,
            access_level: user_data.access_level,
            is_active: true,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            last_active: Some(chrono::Utc::now().timestamp_millis() as u64),
            email: user_data.email,
            group_admin_roles: Vec::new(),
            api_keys: Vec::new(),
            invitation_code: user_data.invitation_code,
        };

        // Save user profile
        let user_key = format!("user_profile:{}", user_profile.user_id);
        let user_data_json = serde_json::to_string(&user_profile)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize user profile: {}", e)))?;

        self.kv_store.put(&user_key, &user_data_json)?
            .execute()
            .await?;

        Ok(user_profile)
    }

    /// Update user access level (super admin only)
    pub async fn update_user_access_level(&self, user_id: &str, access_level: UserAccessLevel) -> ArbitrageResult<()> {
        let mut user_profile = self.get_user_by_id(user_id).await?
            .ok_or_else(|| ArbitrageError::NotFound("User not found".to_string()))?;

        user_profile.access_level = access_level;
        user_profile.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        // Save updated profile
        let user_key = format!("user_profile:{}", user_profile.user_id);
        let user_data = serde_json::to_string(&user_profile)
            .map_err(|e| ArbitrageError::SerializationError(format!("Failed to serialize user profile: {}", e)))?;

        self.kv_store.put(&user_key, &user_data)?
            .execute()
            .await?;

        Ok(())
    }

    /// Health check for user management service
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Test KV store connectivity
        let test_key = "user_mgmt_health_check";
        let test_value = "test";

        match self.kv_store.put(test_key, test_value) {
            Ok(put_builder) => {
                match put_builder.execute().await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            }
            Err(_) => Ok(false),
        }
    }
} 