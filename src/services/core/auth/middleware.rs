//! Authentication Middleware
//! 
//! Middleware for request authentication and authorization:
//! - Request authentication validation
//! - User context extraction
//! - Permission checking for endpoints
//! - Session validation

use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::types::UserProfile;
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::{console_log, Request, Headers};
use std::sync::Arc;

/// Authentication Middleware
pub struct AuthMiddleware {
    service_container: Arc<ServiceContainer>,
}

impl AuthMiddleware {
    /// Create new authentication middleware
    pub async fn new(service_container: Arc<ServiceContainer>) -> ArbitrageResult<Self> {
        console_log!("🔐 Initializing Authentication Middleware...");

        console_log!("✅ Authentication Middleware initialized successfully");

        Ok(Self {
            service_container,
        })
    }

    /// Authenticate request and extract user context
    pub async fn authenticate_request(&self, req: &Request) -> ArbitrageResult<AuthenticationResult> {
        console_log!("🔐 Authenticating request");

        // Extract authentication information from request
        let auth_info = self.extract_auth_info(req)?;

        match auth_info {
            AuthInfo::SessionId(session_id) => {
                self.authenticate_with_session(&session_id).await
            }
            AuthInfo::TelegramId(telegram_id) => {
                self.authenticate_with_telegram_id(telegram_id).await
            }
            AuthInfo::ApiKey(api_key) => {
                self.authenticate_with_api_key(&api_key).await
            }
            AuthInfo::None => {
                console_log!("❌ No authentication information found in request");
                Err(ArbitrageError::authentication_error("No authentication information provided"))
            }
        }
    }

    /// Authorize request for specific action
    pub async fn authorize_request(&self, user_profile: &UserProfile, action: &str) -> ArbitrageResult<AuthorizationResult> {
        console_log!("🔍 Authorizing request for user {} action '{}'", user_profile.user_id, action);

        // Get auth service from container
        let auth_service = self.service_container
            .get_auth_service()
            .ok_or_else(|| ArbitrageError::service_unavailable("Auth service not available"))?;

        // Check permission
        let has_permission = auth_service.check_permission(&user_profile.user_id, action).await?;

        if has_permission {
            console_log!("✅ Authorization granted for user {} action '{}'", user_profile.user_id, action);
            Ok(AuthorizationResult {
                is_authorized: true,
                user_profile: user_profile.clone(),
                action: action.to_string(),
                reason: None,
            })
        } else {
            console_log!("❌ Authorization denied for user {} action '{}'", user_profile.user_id, action);
            Ok(AuthorizationResult {
                is_authorized: false,
                user_profile: user_profile.clone(),
                action: action.to_string(),
                reason: Some("Insufficient permissions".to_string()),
            })
        }
    }

    /// Extract authentication information from request
    fn extract_auth_info(&self, req: &Request) -> ArbitrageResult<AuthInfo> {
        let headers = req.headers();

        // Check for session ID in headers
        if let Ok(Some(session_id)) = headers.get("X-Session-ID") {
            return Ok(AuthInfo::SessionId(session_id));
        }

        // Check for user ID in headers (legacy support)
        if let Ok(Some(user_id)) = headers.get("X-User-ID") {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                return Ok(AuthInfo::TelegramId(telegram_id));
            }
        }

        // Check for API key in Authorization header
        if let Ok(Some(auth_header)) = headers.get("Authorization") {
            if auth_header.starts_with("Bearer ") {
                let api_key = auth_header.strip_prefix("Bearer ").unwrap_or("").to_string();
                return Ok(AuthInfo::ApiKey(api_key));
            }
        }

        // Check for Telegram ID in custom header
        if let Ok(Some(telegram_id_str)) = headers.get("X-Telegram-ID") {
            if let Ok(telegram_id) = telegram_id_str.parse::<i64>() {
                return Ok(AuthInfo::TelegramId(telegram_id));
            }
        }

        Ok(AuthInfo::None)
    }

    /// Authenticate with session ID
    async fn authenticate_with_session(&self, session_id: &str) -> ArbitrageResult<AuthenticationResult> {
        console_log!("🔐 Authenticating with session ID: {}", session_id);

        // Get auth service
        let auth_service = self.service_container
            .get_auth_service()
            .ok_or_else(|| ArbitrageError::service_unavailable("Auth service not available"))?;

        // Validate session and get user context
        let user_context = auth_service.validate_session(session_id).await?;

        console_log!("✅ Session authentication successful for user: {}", user_context.user_profile.user_id);

        Ok(AuthenticationResult {
            is_authenticated: true,
            user_profile: Some(user_context.user_profile),
            auth_method: AuthMethod::Session,
            session_id: Some(session_id.to_string()),
            error_message: None,
        })
    }

    /// Authenticate with Telegram ID (legacy support)
    async fn authenticate_with_telegram_id(&self, telegram_id: i64) -> ArbitrageResult<AuthenticationResult> {
        console_log!("🔐 Authenticating with Telegram ID: {}", telegram_id);

        // Get user profile service
        let user_profile_service = self.service_container
            .get_user_profile_service()
            .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

        // Get user profile
        let user_id = telegram_id.to_string();
        match user_profile_service.get_user_profile(&user_id).await {
            Ok(user_profile) => {
                console_log!("✅ Telegram ID authentication successful for user: {}", user_profile.user_id);
                Ok(AuthenticationResult {
                    is_authenticated: true,
                    user_profile: Some(user_profile),
                    auth_method: AuthMethod::TelegramId,
                    session_id: None,
                    error_message: None,
                })
            }
            Err(_) => {
                console_log!("❌ Telegram ID authentication failed: user not found");
                Ok(AuthenticationResult {
                    is_authenticated: false,
                    user_profile: None,
                    auth_method: AuthMethod::TelegramId,
                    session_id: None,
                    error_message: Some("User not found".to_string()),
                })
            }
        }
    }

    /// Authenticate with API key
    async fn authenticate_with_api_key(&self, api_key: &str) -> ArbitrageResult<AuthenticationResult> {
        console_log!("🔐 Authenticating with API key: {}...", &api_key[..8.min(api_key.len())]);

        // TODO: Implement API key authentication
        // For now, reject all API key authentication
        console_log!("❌ API key authentication not implemented");

        Ok(AuthenticationResult {
            is_authenticated: false,
            user_profile: None,
            auth_method: AuthMethod::ApiKey,
            session_id: None,
            error_message: Some("API key authentication not implemented".to_string()),
        })
    }

    /// Check if request requires authentication
    pub fn requires_authentication(&self, path: &str, method: &str) -> bool {
        // Public endpoints that don't require authentication
        let public_endpoints = [
            "/health",
            "/health/detailed",
            "/telegram/webhook", // Telegram webhook is authenticated differently
        ];

        // Check if path is public
        for public_path in &public_endpoints {
            if path.starts_with(public_path) {
                return false;
            }
        }

        // All other endpoints require authentication
        true
    }

    /// Check if request requires specific permission
    pub fn get_required_permission(&self, path: &str, method: &str) -> Option<String> {
        match (method, path) {
            // User management endpoints
            ("GET", "/api/v1/user/profile") => Some("users.read".to_string()),
            ("PUT", "/api/v1/user/profile") => Some("users.update".to_string()),
            
            // Admin endpoints
            ("GET", "/api/v1/admin/users") => Some("users.read".to_string()),
            ("GET", "/api/v1/admin/system") => Some("system.admin".to_string()),
            ("GET", "/api/v1/admin/config") => Some("system.config".to_string()),
            ("PUT", "/api/v1/admin/config") => Some("system.config".to_string()),
            
            // Trading endpoints
            ("GET", "/api/v1/trading/balance") => Some("trading.manual".to_string()),
            ("POST", "/api/v1/trading/order") => Some("trading.manual".to_string()),
            
            // AI endpoints
            ("POST", "/api/v1/ai/analyze") => Some("ai.basic".to_string()),
            
            // Analytics endpoints
            ("GET", "/api/v1/analytics/dashboard") => Some("analytics.basic".to_string()),
            
            _ => None, // No specific permission required
        }
    }
}

/// Authentication information extracted from request
#[derive(Debug, Clone)]
enum AuthInfo {
    SessionId(String),
    TelegramId(i64),
    ApiKey(String),
    None,
}

/// Authentication method used
#[derive(Debug, Clone)]
pub enum AuthMethod {
    Session,
    TelegramId,
    ApiKey,
}

/// Authentication result
#[derive(Debug, Clone)]
pub struct AuthenticationResult {
    pub is_authenticated: bool,
    pub user_profile: Option<UserProfile>,
    pub auth_method: AuthMethod,
    pub session_id: Option<String>,
    pub error_message: Option<String>,
}

/// Authorization result
#[derive(Debug, Clone)]
pub struct AuthorizationResult {
    pub is_authorized: bool,
    pub user_profile: UserProfile,
    pub action: String,
    pub reason: Option<String>,
} 