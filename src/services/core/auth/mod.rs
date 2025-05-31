//! Authentication & Authorization Module
//! 
//! Comprehensive authentication and authorization system with:
//! - User authentication and session management
//! - Role-Based Access Control (RBAC)
//! - Permission checking and validation
//! - Subscription tier management
//! - Beta access control
//! - API key authentication

pub mod rbac;
pub mod session;
pub mod permissions;
pub mod middleware;
pub mod user_auth;

// Re-export main services and types
pub use rbac::{RBACService, RoleManager, PermissionManager};
pub use session::{AuthSessionService, SessionValidator, SessionManager, SessionStats, SessionValidationResult};
pub use permissions::{PermissionChecker, AccessValidator, FeatureGate, FeatureAccessSummary};
pub use middleware::{AuthMiddleware, AuthenticationResult, AuthorizationResult, AuthMethod};
pub use user_auth::{
    UserAuthService, LoginResult, AuthCredentials, TelegramUserInfo, 
    InvitationValidationResult, InvitationBenefits, OnboardingStatus
};

use crate::types::UserProfile;
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::console_log;
use std::sync::Arc;

/// Trait for user profile operations
pub trait UserProfileProvider: Send + Sync {
    async fn get_user_profile(&self, user_id: &str) -> ArbitrageResult<UserProfile>;
    async fn create_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()>;
    async fn update_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()>;
}

/// Trait for session management operations
pub trait SessionProvider: Send + Sync {
    async fn validate_session(&self, session_id: &str) -> ArbitrageResult<bool>;
    async fn create_session(&self, user_id: &str) -> ArbitrageResult<String>;
    async fn update_session_activity(&self, session_id: &str) -> ArbitrageResult<()>;
    async fn end_session(&self, session_id: &str) -> ArbitrageResult<()>;
}

/// Main Authentication & Authorization Service
/// 
/// Production-ready service with proper dependency injection
pub struct AuthService {
    rbac_service: RBACService,
    permission_checker: PermissionChecker,
    user_profile_provider: Option<Arc<dyn UserProfileProvider>>,
    session_provider: Option<Arc<dyn SessionProvider>>,
}

impl AuthService {
    /// Create new authentication service with dependency injection
    pub async fn new() -> ArbitrageResult<Self> {
        console_log!("🔐 Initializing Production Authentication Service...");

        // Create services without external dependencies
        let rbac_service = RBACService::new_standalone().await?;
        let permission_checker = PermissionChecker::new_standalone().await?;

        console_log!("✅ Production Authentication Service initialized successfully");

        Ok(Self {
            rbac_service,
            permission_checker,
            user_profile_provider: None,
            session_provider: None,
        })
    }

    /// Set user profile provider for dependency injection
    pub fn set_user_profile_provider(&mut self, provider: Arc<dyn UserProfileProvider>) {
        self.user_profile_provider = Some(provider);
        console_log!("✅ User profile provider injected into auth service");
    }

    /// Set session provider for dependency injection
    pub fn set_session_provider(&mut self, provider: Arc<dyn SessionProvider>) {
        self.session_provider = Some(provider);
        console_log!("✅ Session provider injected into auth service");
    }

    /// Check user permission with real user profile lookup
    pub async fn check_permission(&self, user_id: &str, permission: &str) -> ArbitrageResult<bool> {
        console_log!("🔍 Checking permission '{}' for user: {}", permission, user_id);

        // Get user profile from provider
        let user_profile = if let Some(provider) = &self.user_profile_provider {
            provider.get_user_profile(user_id).await?
        } else {
            return Err(ArbitrageError::service_unavailable("User profile provider not configured"));
        };

        self.permission_checker.check_permission(&user_profile, permission).await
    }

    /// Get user permissions based on real user profile
    pub async fn get_user_permissions(&self, user_id: &str) -> ArbitrageResult<Vec<String>> {
        console_log!("👑 Getting permissions for user: {}", user_id);

        // Get user profile from provider
        let user_profile = if let Some(provider) = &self.user_profile_provider {
            provider.get_user_profile(user_id).await?
        } else {
            return Err(ArbitrageError::service_unavailable("User profile provider not configured"));
        };

        let user_permissions = self.rbac_service.get_user_permissions(&user_profile).await?;
        Ok(user_permissions.permissions)
    }

    /// Validate user access with real data
    pub async fn validate_user_access(&self, user_id: &str, required_permission: &str) -> ArbitrageResult<bool> {
        console_log!("🔍 Validating access for user {} permission '{}'", user_id, required_permission);

        self.check_permission(user_id, required_permission).await
    }

    /// Get feature access summary with real user profile
    pub async fn get_feature_access_summary(&self, user_id: &str) -> ArbitrageResult<FeatureAccessSummary> {
        console_log!("📋 Getting feature access summary for user: {}", user_id);

        // Get user profile from provider
        let user_profile = if let Some(provider) = &self.user_profile_provider {
            provider.get_user_profile(user_id).await?
        } else {
            return Err(ArbitrageError::service_unavailable("User profile provider not configured"));
        };

        let feature_gate = FeatureGate::new_standalone().await?;
        feature_gate.get_feature_access_summary(&user_profile).await
    }

    /// Validate session using session provider
    pub async fn validate_session(&self, session_id: &str) -> ArbitrageResult<UserContext> {
        console_log!("🔍 Validating session: {}", session_id);

        // Validate session using provider
        let is_valid = if let Some(provider) = &self.session_provider {
            provider.validate_session(session_id).await?
        } else {
            return Err(ArbitrageError::service_unavailable("Session provider not configured"));
        };

        if !is_valid {
            return Err(ArbitrageError::authentication_error("Invalid session"));
        }

        // Extract user ID from session (assuming session_id format)
        let user_id = session_id.to_string(); // TODO: Implement proper session-to-user mapping

        // Get user profile
        let user_profile = if let Some(provider) = &self.user_profile_provider {
            provider.get_user_profile(&user_id).await?
        } else {
            return Err(ArbitrageError::service_unavailable("User profile provider not configured"));
        };

        // Get user permissions
        let permissions = self.get_user_permissions(&user_id).await?;

        console_log!("✅ Session validation successful: {}", session_id);

        Ok(UserContext {
            user_profile,
            session_info: SessionInfo {
                session_id: session_id.to_string(),
                user_id,
                created_at: chrono::Utc::now(), // TODO: Get actual creation time
                expires_at: chrono::Utc::now() + chrono::Duration::days(7),
                last_activity: chrono::Utc::now(),
            },
            permissions,
        })
    }

    /// Create user session
    pub async fn create_user_session(&self, user_id: &str) -> ArbitrageResult<String> {
        console_log!("🆕 Creating session for user: {}", user_id);

        if let Some(provider) = &self.session_provider {
            let session_id = provider.create_session(user_id).await?;
            console_log!("✅ Session created: {}", session_id);
            Ok(session_id)
        } else {
            Err(ArbitrageError::service_unavailable("Session provider not configured"))
        }
    }

    /// End user session
    pub async fn end_user_session(&self, session_id: &str) -> ArbitrageResult<()> {
        console_log!("🚪 Ending session: {}", session_id);

        if let Some(provider) = &self.session_provider {
            provider.end_session(session_id).await?;
            console_log!("✅ Session ended: {}", session_id);
            Ok(())
        } else {
            Err(ArbitrageError::service_unavailable("Session provider not configured"))
        }
    }

    /// Health check for auth service
    pub async fn health_check(&self) -> ArbitrageResult<AuthHealthStatus> {
        console_log!("🏥 Performing auth service health check");

        let user_profile_healthy = self.user_profile_provider.is_some();
        let session_provider_healthy = self.session_provider.is_some();
        let rbac_healthy = true; // RBAC is always healthy as it's stateless
        let permission_checker_healthy = true; // Permission checker is always healthy

        let overall_healthy = user_profile_healthy && session_provider_healthy && rbac_healthy && permission_checker_healthy;

        let status = AuthHealthStatus {
            overall_healthy,
            user_profile_provider_healthy: user_profile_healthy,
            session_provider_healthy,
            rbac_service_healthy: rbac_healthy,
            permission_checker_healthy,
        };

        console_log!("✅ Auth service health check complete: {}", if overall_healthy { "HEALTHY" } else { "UNHEALTHY" });

        Ok(status)
    }
}

/// User permissions structure
#[derive(Debug, Clone)]
pub struct UserPermissions {
    pub role: crate::types::UserRole,
    pub subscription_tier: String,
    pub can_trade: bool,
    pub daily_opportunity_limit: i32,
    pub permissions: Vec<String>,
    pub is_admin: bool,
}

/// User authentication result
#[derive(Debug, Clone)]
pub struct UserAuthenticationResult {
    pub user_profile: UserProfile,
    pub session_id: String,
    pub is_new_user: bool,
    pub authentication_time: chrono::DateTime<chrono::Utc>,
}

/// User context with session and permissions
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_profile: UserProfile,
    pub session_info: SessionInfo,
    pub permissions: Vec<String>,
}

/// Session information
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub user_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

/// Session creation result
#[derive(Debug, Clone)]
pub struct SessionCreationResult {
    pub session_id: String,
    pub is_new_session: bool,
}

/// Subscription access result
#[derive(Debug, Clone)]
pub struct SubscriptionAccessResult {
    pub has_access: bool,
    pub subscription_tier: String,
    pub feature: String,
    pub limit_reached: bool,
    pub upgrade_required: bool,
}

/// Auth service health status
#[derive(Debug, Clone)]
pub struct AuthHealthStatus {
    pub overall_healthy: bool,
    pub user_profile_provider_healthy: bool,
    pub session_provider_healthy: bool,
    pub rbac_service_healthy: bool,
    pub permission_checker_healthy: bool,
} 