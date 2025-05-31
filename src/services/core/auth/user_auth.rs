//! User Authentication Service
//! 
//! Handles user authentication and profile creation:
//! - User onboarding and registration
//! - Profile creation with default settings
//! - Invitation code validation
//! - Beta access assignment

use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::services::core::user::UserProfileService;
use crate::types::{UserProfile, UserRole};
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::console_log;
use std::sync::Arc;

/// User Authentication Service
pub struct UserAuthService {
    service_container: Arc<ServiceContainer>,
}

impl UserAuthService {
    /// Create new user authentication service
    pub async fn new(service_container: &Arc<ServiceContainer>) -> ArbitrageResult<Self> {
        console_log!("👤 Initializing User Authentication Service...");

        console_log!("✅ User Authentication Service initialized successfully");

        Ok(Self {
            service_container: service_container.clone(),
        })
    }

    /// Create new user profile during authentication
    pub async fn create_new_user_profile(&self, telegram_id: i64, invitation_code: Option<String>) -> ArbitrageResult<UserProfile> {
        console_log!("🆕 Creating new user profile for telegram_id: {}", telegram_id);

        // Get user profile service
        let user_profile_service = self.service_container
            .get_user_profile_service()
            .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

        // Create profile with default settings
        let mut new_profile = self.create_default_profile(telegram_id)?;

        // Apply invitation code benefits if provided
        if let Some(code) = invitation_code {
            self.apply_invitation_benefits(&mut new_profile, &code)?;
        }

        // Save the new profile
        user_profile_service.create_user_profile(&new_profile).await?;

        console_log!("✅ New user profile created for telegram_id: {} with role: {:?}", telegram_id, new_profile.role);
        Ok(new_profile)
    }

    /// Authenticate existing user
    pub async fn authenticate_existing_user(&self, user_profile: &UserProfile) -> ArbitrageResult<LoginResult> {
        console_log!("🔐 Authenticating existing user: {}", user_profile.user_id);

        // Update last login time
        let user_profile_service = self.service_container
            .get_user_profile_service()
            .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

        let mut updated_profile = user_profile.clone();
        updated_profile.last_login = Some(chrono::Utc::now());
        updated_profile.updated_at = chrono::Utc::now();

        user_profile_service.update_user_profile(&updated_profile).await?;

        console_log!("✅ User authenticated successfully: {}", user_profile.user_id);

        Ok(LoginResult {
            user_profile: updated_profile,
            is_new_user: false,
            login_time: chrono::Utc::now(),
        })
    }

    /// Create default user profile
    fn create_default_profile(&self, telegram_id: i64) -> ArbitrageResult<UserProfile> {
        let now = chrono::Utc::now();
        
        Ok(UserProfile {
            user_id: telegram_id.to_string(),
            telegram_id: Some(telegram_id),
            username: None, // Will be updated from Telegram info
            first_name: None, // Will be updated from Telegram info
            last_name: None,
            email: None,
            role: UserRole::Basic, // Default role for new users
            subscription_tier: "free".to_string(), // Default to free tier
            beta_access: false, // Default: no beta access
            beta_expires_at: None,
            can_trade: false, // Default: no trading until API keys added
            daily_opportunity_limit: 3, // Free tier limit
            created_at: now,
            updated_at: now,
            last_login: Some(now),
            is_active: true,
            preferences: serde_json::json!({
                "notifications": true,
                "language": "en",
                "timezone": "UTC",
                "trading_focus": "arbitrage", // Default to arbitrage (safer)
                "automation_level": "manual" // Default to manual
            }),
        })
    }

    /// Apply invitation code benefits to profile
    fn apply_invitation_benefits(&self, profile: &mut UserProfile, invitation_code: &str) -> ArbitrageResult<()> {
        console_log!("🎫 Applying invitation benefits for code: {}", invitation_code);

        // TODO: Implement proper invitation code validation and benefits
        // For now, apply basic beta access for any valid invitation code
        
        if !invitation_code.is_empty() {
            // Grant beta access for 180 days
            profile.beta_access = true;
            profile.beta_expires_at = Some(chrono::Utc::now() + chrono::Duration::days(180));
            
            // Increase daily opportunity limit for beta users
            profile.daily_opportunity_limit = 10;
            
            // Update preferences for beta users
            if let Some(prefs) = profile.preferences.as_object_mut() {
                prefs.insert("beta_user".to_string(), serde_json::Value::Bool(true));
                prefs.insert("invitation_code".to_string(), serde_json::Value::String(invitation_code.to_string()));
            }

            console_log!("✅ Beta access granted to user {} via invitation code", profile.user_id);
        }

        Ok(())
    }

    /// Update user profile from Telegram information
    pub async fn update_profile_from_telegram(&self, user_profile: &mut UserProfile, telegram_info: &TelegramUserInfo) -> ArbitrageResult<()> {
        console_log!("📱 Updating profile from Telegram info for user: {}", user_profile.user_id);

        // Update Telegram-specific fields
        user_profile.telegram_id = Some(telegram_info.telegram_id);
        user_profile.username = telegram_info.username.clone();
        user_profile.first_name = telegram_info.first_name.clone();
        user_profile.last_name = telegram_info.last_name.clone();
        user_profile.updated_at = chrono::Utc::now();

        // Save updated profile
        let user_profile_service = self.service_container
            .get_user_profile_service()
            .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

        user_profile_service.update_user_profile(user_profile).await?;

        console_log!("✅ Profile updated from Telegram info for user: {}", user_profile.user_id);
        Ok(())
    }

    /// Validate invitation code
    pub async fn validate_invitation_code(&self, invitation_code: &str) -> ArbitrageResult<InvitationValidationResult> {
        console_log!("🎫 Validating invitation code: {}", invitation_code);

        // TODO: Implement proper invitation code validation
        // For now, accept any non-empty code as valid
        
        if invitation_code.is_empty() {
            return Ok(InvitationValidationResult {
                is_valid: false,
                benefits: None,
                error_message: Some("Invitation code cannot be empty".to_string()),
            });
        }

        // Mock validation - in real implementation, check against database
        let benefits = InvitationBenefits {
            beta_access: true,
            beta_duration_days: 180,
            daily_opportunity_limit: 10,
            special_features: vec!["beta_access".to_string(), "priority_support".to_string()],
        };

        console_log!("✅ Invitation code validated: {}", invitation_code);

        Ok(InvitationValidationResult {
            is_valid: true,
            benefits: Some(benefits),
            error_message: None,
        })
    }

    /// Get user onboarding status
    pub async fn get_onboarding_status(&self, user_id: &str) -> ArbitrageResult<OnboardingStatus> {
        console_log!("📋 Getting onboarding status for user: {}", user_id);

        let user_profile_service = self.service_container
            .get_user_profile_service()
            .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

        let user_profile = user_profile_service.get_user_profile(user_id).await?;

        // Check onboarding completion status
        let profile_complete = user_profile.first_name.is_some() && user_profile.username.is_some();
        let preferences_set = user_profile.preferences.as_object().map_or(false, |prefs| {
            prefs.contains_key("trading_focus") && prefs.contains_key("automation_level")
        });
        let api_keys_configured = user_profile.can_trade;

        let completion_percentage = calculate_onboarding_completion(profile_complete, preferences_set, api_keys_configured);

        console_log!("✅ Onboarding status retrieved for user: {} ({}% complete)", user_id, completion_percentage);

        Ok(OnboardingStatus {
            is_complete: completion_percentage >= 100,
            completion_percentage,
            profile_complete,
            preferences_set,
            api_keys_configured,
            beta_access: user_profile.beta_access,
            next_steps: get_next_onboarding_steps(profile_complete, preferences_set, api_keys_configured),
        })
    }
}

/// Authentication Provider trait for different auth methods
pub trait AuthenticationProvider {
    async fn authenticate(&self, credentials: &AuthCredentials) -> ArbitrageResult<LoginResult>;
    async fn validate_credentials(&self, credentials: &AuthCredentials) -> ArbitrageResult<bool>;
}

/// Calculate onboarding completion percentage
fn calculate_onboarding_completion(profile_complete: bool, preferences_set: bool, api_keys_configured: bool) -> u8 {
    let mut completion = 0u8;
    
    if profile_complete { completion += 40; }
    if preferences_set { completion += 30; }
    if api_keys_configured { completion += 30; }
    
    completion
}

/// Get next onboarding steps
fn get_next_onboarding_steps(profile_complete: bool, preferences_set: bool, api_keys_configured: bool) -> Vec<String> {
    let mut steps = Vec::new();
    
    if !profile_complete {
        steps.push("Complete your profile information".to_string());
    }
    if !preferences_set {
        steps.push("Set your trading preferences".to_string());
    }
    if !api_keys_configured {
        steps.push("Configure exchange API keys for trading".to_string());
    }
    
    if steps.is_empty() {
        steps.push("Onboarding complete! Start exploring opportunities".to_string());
    }
    
    steps
}

/// Login result
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub user_profile: UserProfile,
    pub is_new_user: bool,
    pub login_time: chrono::DateTime<chrono::Utc>,
}

/// Authentication credentials
#[derive(Debug, Clone)]
pub struct AuthCredentials {
    pub telegram_id: i64,
    pub invitation_code: Option<String>,
    pub telegram_info: Option<TelegramUserInfo>,
}

/// Telegram user information
#[derive(Debug, Clone)]
pub struct TelegramUserInfo {
    pub telegram_id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

/// Invitation validation result
#[derive(Debug, Clone)]
pub struct InvitationValidationResult {
    pub is_valid: bool,
    pub benefits: Option<InvitationBenefits>,
    pub error_message: Option<String>,
}

/// Invitation benefits
#[derive(Debug, Clone)]
pub struct InvitationBenefits {
    pub beta_access: bool,
    pub beta_duration_days: i32,
    pub daily_opportunity_limit: i32,
    pub special_features: Vec<String>,
}

/// Onboarding status
#[derive(Debug, Clone)]
pub struct OnboardingStatus {
    pub is_complete: bool,
    pub completion_percentage: u8,
    pub profile_complete: bool,
    pub preferences_set: bool,
    pub api_keys_configured: bool,
    pub beta_access: bool,
    pub next_steps: Vec<String>,
} 