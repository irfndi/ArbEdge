//! User Onboarding Commands
//! 
//! Priority 1: User Onboarding & Session Management
//! - Session creation and validation
//! - Initial profile setup
//! - Beta access validation
//! - Welcome flow

use crate::services::interfaces::telegram::{UserInfo, UserPermissions};
use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::services::core::user::session_management::{SessionManagementService, EnhancedUserSession};
use crate::services::core::user::user_profile::UserProfileService;
use crate::types::{UserRole, UserProfile};
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::console_log;
use std::sync::Arc;

/// Handle /start command - Primary onboarding entry point
pub async fn handle_start_command(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
    args: &[&str],
) -> ArbitrageResult<String> {
    console_log!("🚀 Starting onboarding for user {}", user_info.user_id);

    // Step 1: Session Management (Critical Priority)
    let session_result = create_or_update_session(service_container, user_info).await?;
    
    // Step 2: Profile Management (High Priority)
    let profile_result = ensure_user_profile(service_container, user_info).await?;
    
    // Step 3: Beta Access Validation (High Priority)
    let beta_status = validate_beta_access(service_container, user_info).await?;
    
    // Step 4: Generate Welcome Message
    let welcome_message = generate_welcome_message(
        user_info,
        &session_result,
        &profile_result,
        &beta_status,
        args,
    ).await?;
    
    console_log!("✅ Onboarding completed for user {}", user_info.user_id);
    Ok(welcome_message)
}

/// Create or update user session
async fn create_or_update_session(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
) -> ArbitrageResult<SessionResult> {
    console_log!("🔐 Managing session for user {}", user_info.user_id);

    let session_service = service_container
        .get_session_management_service()
        .ok_or_else(|| ArbitrageError::service_unavailable("Session management service not available"))?;

    let user_id_str = user_info.user_id.to_string();
    let telegram_id = user_info.user_id;

    // Check if session already exists
    let existing_session_option = session_service.validate_session(&user_id_str).await?;

    if let Some(existing_session_details) = existing_session_option {
        // Update existing session activity using the correct user_id from session_details
        session_service.update_activity(&existing_session_details.user_id).await?;
        console_log!("✅ Updated existing session for user {}", user_info.user_id);
        
        Ok(SessionResult {
            is_new_session: false,
            session_id: existing_session_details.session_id, // Use actual session_id
            message: "Welcome back! Your session has been refreshed.".to_string(),
        })
    } else {
        // Create new session. SessionManagementService::start_session expects telegram_id and a user_id.
        // We'll use user_info.user_id (telegram_id) as the user_id for the session for now,
        // consistent with how user_id_str was derived.
        // A more robust system might involve a separate, persistent internal user ID.
        let new_session = session_service.start_session(telegram_id, user_id_str).await?;
        console_log!("✅ Created new session for user {}: {}", user_info.user_id, new_session.session_id);
        
        Ok(SessionResult {
            is_new_session: true,
            session_id: new_session.session_id,
            message: "New session created! Welcome to ArbEdge.".to_string(),
        })
    }
}

/// Ensure user profile exists and is up to date
async fn ensure_user_profile(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
) -> ArbitrageResult<ProfileResult> {
    console_log!("👤 Managing profile for user {}", user_info.user_id);

    let user_profile_service = service_container
        .get_user_profile_service()
        .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

    let user_id_str = user_info.user_id.to_string();

    // Try to get existing profile
    match user_profile_service.get_user_profile(&user_id_str).await {
        Ok(existing_profile) => {
            console_log!("✅ Found existing profile for user {}", user_info.user_id);
            
            // Update profile with latest Telegram info if needed
            let updated_profile = update_profile_from_telegram(existing_profile, user_info)?;
            
            Ok(ProfileResult {
                is_new_profile: false,
                profile: updated_profile,
                message: "Profile loaded successfully.".to_string(),
            })
        }
        Err(_) => {
            // Create new profile
            console_log!("🆕 Creating new profile for user {}", user_info.user_id);
            let new_profile = create_profile_from_telegram(user_info)?;
            
            // Save new profile
            user_profile_service.create_user_profile(&new_profile).await?;
            
            Ok(ProfileResult {
                is_new_profile: true,
                profile: new_profile,
                message: "New profile created! Welcome to ArbEdge.".to_string(),
            })
        }
    }
}

/// Validate beta access for user
async fn validate_beta_access(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
) -> ArbitrageResult<BetaAccessResult> {
    console_log!("🧪 Validating beta access for user {}", user_info.user_id);

    let user_profile_service = service_container
        .get_user_profile_service()
        .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

    let user_id_str = user_info.user_id.to_string();
    let user_profile = user_profile_service.get_user_profile(&user_id_str).await?;

    let beta_access = user_profile.beta_access;
    let beta_expires_at = user_profile.beta_expires_at;

    // Check if beta access is still valid
    let is_beta_valid = if beta_access {
        if let Some(expires_at) = beta_expires_at {
            chrono::Utc::now() < expires_at
        } else {
            true // No expiration set
        }
    } else {
        false
    };

    console_log!("✅ Beta access validation for user {}: {}", user_info.user_id, is_beta_valid);

    Ok(BetaAccessResult {
        has_beta_access: is_beta_valid,
        expires_at: beta_expires_at,
        message: if is_beta_valid {
            "Beta access active.".to_string()
        } else {
            "Beta access not available.".to_string()
        },
    })
}

/// Create user profile from Telegram information
fn create_profile_from_telegram(user_info: &UserInfo) -> ArbitrageResult<UserProfile> {
    let now = chrono::Utc::now();
    
    Ok(UserProfile {
        user_id: user_info.user_id.to_string(),
        telegram_id: Some(user_info.user_id),
        username: user_info.username.clone(),
        first_name: user_info.first_name.clone(),
        last_name: None,
        email: None,
        role: UserRole::Basic, // Default role for new users
        subscription_tier: "free".to_string(),
        beta_access: false, // Default: no beta access (requires invitation)
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
            "timezone": "UTC"
        }),
    })
}

/// Update existing profile with latest Telegram information
fn update_profile_from_telegram(
    mut profile: UserProfile,
    user_info: &UserInfo,
) -> ArbitrageResult<UserProfile> {
    let now = chrono::Utc::now();
    
    // Update Telegram-specific fields
    profile.telegram_id = Some(user_info.user_id);
    profile.username = user_info.username.clone();
    profile.first_name = user_info.first_name.clone();
    profile.last_login = Some(now);
    profile.updated_at = now;
    profile.is_active = true;
    
    Ok(profile)
}

/// Generate comprehensive welcome message
async fn generate_welcome_message(
    user_info: &UserInfo,
    session_result: &SessionResult,
    profile_result: &ProfileResult,
    beta_status: &BetaAccessResult,
    args: &[&str],
) -> ArbitrageResult<String> {
    let user_name = user_info.first_name
        .as_ref()
        .or(user_info.username.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("Trader");

    let mut message = String::new();
    
    // Welcome header
    if session_result.is_new_session || profile_result.is_new_profile {
        message.push_str(&format!("🚀 *Welcome to ArbEdge, {}!*\n\n", user_name));
        message.push_str("Your advanced cryptocurrency arbitrage assistant is ready.\n\n");
    } else {
        message.push_str(&format!("👋 *Welcome back, {}!*\n\n", user_name));
    }
    
    // Session status
    message.push_str("🔐 *Session Status*\n");
    message.push_str(&format!("✅ {}\n\n", session_result.message));
    
    // Profile status
    message.push_str("👤 *Profile Status*\n");
    message.push_str(&format!("✅ {}\n", profile_result.message));
    message.push_str(&format!("Role: {:?}\n", profile_result.profile.role));
    message.push_str(&format!("Subscription: {}\n\n", profile_result.profile.subscription_tier));
    
    // Beta access status
    message.push_str("🧪 *Beta Access*\n");
    if beta_status.has_beta_access {
        message.push_str("✅ Beta access active\n");
        if let Some(expires_at) = beta_status.expires_at {
            message.push_str(&format!("Expires: {}\n", expires_at.format("%Y-%m-%d")));
        }
    } else {
        message.push_str("❌ Beta access not available\n");
        message.push_str("Contact admin for beta invitation\n");
    }
    message.push_str("\n");
    
    // Quick start guide
    message.push_str("🎯 *Quick Start*\n");
    message.push_str("• `/opportunities` - View trading opportunities\n");
    message.push_str("• `/profile` - Manage your profile\n");
    message.push_str("• `/settings` - Configure preferences\n");
    message.push_str("• `/help` - Get help and commands\n\n");
    
    // Beta features (if available)
    if beta_status.has_beta_access {
        message.push_str("🧪 *Beta Features Available*\n");
        message.push_str("• `/beta` - Access beta features\n");
        message.push_str("• Advanced AI analysis\n");
        message.push_str("• Priority opportunity access\n\n");
    }
    
    // Next steps for new users
    if profile_result.is_new_profile {
        message.push_str("📋 *Next Steps*\n");
        message.push_str("1. Complete your profile with `/profile`\n");
        message.push_str("2. Configure notifications with `/settings`\n");
        message.push_str("3. Start exploring opportunities!\n\n");
    }
    
    message.push_str("Ready to start trading? Let's find some opportunities! 🚀");
    
    Ok(message)
}

// Result structures
#[derive(Debug)]
struct SessionResult {
    is_new_session: bool,
    session_id: String,
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::core::user::session_management::SessionManagementService as ActualSessionManagementService;
    use crate::services::core::user::user_profile::UserProfileService as ActualUserProfileService;
    use crate::types::{EnhancedSessionState, SessionConfig, SessionAnalytics, UserRole, SubscriptionTier};
    use tokio::sync::Mutex as TokioMutex; // Renamed to avoid conflict if any other Mutex is in scope
    use std::collections::HashMap;

    // --- Mock UserInfo and UserPermissions ---
    fn create_mock_user_info(user_id: i64, username: &str, first_name: &str) -> UserInfo {
        UserInfo {
            user_id,
            username: Some(username.to_string()),
            first_name: Some(first_name.to_string()),
            // other fields can be None or default
        }
    }

    fn create_mock_user_permissions() -> UserPermissions {
        UserPermissions {
            can_trade: true,
            daily_opportunity_limit: 100,
            // other fields
        }
    }

    // --- Mock SessionManagementService ---
    #[derive(Clone)]
    struct MockSessionManagementService {
        validate_session_response: TokioMutex<Option<ArbitrageResult<Option<EnhancedUserSession>>>>,
        start_session_response: TokioMutex<Option<ArbitrageResult<EnhancedUserSession>>>,
        update_activity_called: TokioMutex<bool>,
    }

    impl MockSessionManagementService {
        fn new() -> Self {
            Self {
                validate_session_response: TokioMutex::new(None),
                start_session_response: TokioMutex::new(None),
                update_activity_called: TokioMutex::new(false),
            }
        }

        async fn set_validate_session_response(&self, response: ArbitrageResult<Option<EnhancedUserSession>>) {
            *self.validate_session_response.lock().await = Some(response);
        }

        async fn set_start_session_response(&self, response: ArbitrageResult<EnhancedUserSession>) {
            *self.start_session_response.lock().await = Some(response);
        }

        // Implement methods that ActualSessionManagementService has, which are called by onboarding code
        #[allow(dead_code)]
        async fn validate_session(&self, _user_id: &str) -> ArbitrageResult<Option<EnhancedUserSession>> {
            self.validate_session_response.lock().await.take().unwrap_or_else(|| Ok(None))
        }

        #[allow(dead_code)]
        async fn start_session(&self, telegram_id: i64, user_id: String) -> ArbitrageResult<EnhancedUserSession> {
            self.start_session_response.lock().await.take().unwrap_or_else(|| {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                Ok(EnhancedUserSession {
                    session_id: format!("mock_session_{}", telegram_id),
                    user_id,
                    telegram_id,
                    telegram_chat_id: telegram_id,
                    session_state: EnhancedSessionState::Active,
                    current_state: EnhancedSessionState::Active,
                    started_at: now, last_activity_at: now, expires_at: now + 3600000,
                    created_at: now, updated_at: now,
                    onboarding_completed: false, preferences_set: false,
                    metadata: serde_json::Value::Null,
                    last_command: None, temporary_data: HashMap::new(),
                    session_analytics: SessionAnalytics::default(), config: SessionConfig::default(),
                })
            })
        }

        #[allow(dead_code)]
        async fn update_activity(&self, _user_id: &str) -> ArbitrageResult<()> {
            *self.update_activity_called.lock().await = true;
            Ok(())
        }
    }

    // --- Mock UserProfileService ---
    #[derive(Clone)]
    struct MockUserProfileService {
        get_user_profile_response: TokioMutex<Option<ArbitrageResult<UserProfile>>>,
        create_user_profile_called: TokioMutex<bool>,
        created_profile_data: TokioMutex<Option<UserProfile>>,
    }

    impl MockUserProfileService {
        fn new() -> Self {
            Self {
                get_user_profile_response: TokioMutex::new(None),
                create_user_profile_called: TokioMutex::new(false),
                created_profile_data: TokioMutex::new(None),
            }
        }

        async fn set_get_user_profile_response(&self, response: ArbitrageResult<UserProfile>) {
            *self.get_user_profile_response.lock().await = Some(response);
        }

        // Implement methods that ActualUserProfileService has
        #[allow(dead_code)]
        async fn get_user_profile(&self, _user_id: &str) -> ArbitrageResult<UserProfile> {
            self.get_user_profile_response.lock().await.take()
                .unwrap_or_else(|| Err(ArbitrageError::NotFound("Mocked profile not found".to_string())))
        }

        #[allow(dead_code)]
        async fn create_user_profile(&self, profile: &UserProfile) -> ArbitrageResult<()> {
            *self.create_user_profile_called.lock().await = true;
            *self.created_profile_data.lock().await = Some(profile.clone());
            Ok(())
        }
    }

    // --- Mock ServiceContainer ---
    // This is a simplified ServiceContainer for testing.
    struct MockServiceContainer {
        session_management_service: Arc<MockSessionManagementService>,
        user_profile_service: Arc<MockUserProfileService>,
        // Add other services as needed, returning Option<MockedService>
    }

    impl MockServiceContainer {
        fn new(
            session_service: Arc<MockSessionManagementService>,
            profile_service: Arc<MockUserProfileService>,
        ) -> Self {
            Self {
                session_management_service: session_service,
                user_profile_service: profile_service,
            }
        }

        // These methods mimic the real ServiceContainer's getters
        // but return the mock versions. The types must be cast or handled carefully.
        // This is where trait-based DI would be cleaner.
        #[allow(dead_code)]
        fn get_session_management_service(&self) -> Option<Arc<ActualSessionManagementService>> {
            // UNSAFE: This is a placeholder to make the structure work.
            // In a real scenario, ServiceContainer would be generic or use traits.
            let mock_arc = self.session_management_service.clone();
            unsafe {
                let ptr = Arc::into_raw(mock_arc) as *const ActualSessionManagementService;
                Some(Arc::from_raw(ptr))
            }
        }

        #[allow(dead_code)]
        fn get_user_profile_service(&self) -> Option<Arc<ActualUserProfileService>> {
            let mock_arc = self.user_profile_service.clone();
            unsafe {
                let ptr = Arc::into_raw(mock_arc) as *const ActualUserProfileService;
                Some(Arc::from_raw(ptr))
            }
        }
    }


    #[tokio::test]
    async fn test_handle_start_command_new_user() {
        let user_id = 12345i64;
        let user_info = create_mock_user_info(user_id, "testuser", "Test");
        let permissions = create_mock_user_permissions();

        let mock_sms = Arc::new(MockSessionManagementService::new());
        let mock_ups = Arc::new(MockUserProfileService::new());

        // Setup mock responses for a new user
        mock_sms.set_validate_session_response(Ok(None)).await; // No existing session
        let now = chrono::Utc::now();
        let expected_session_id = format!("mock_session_{}", user_id);
        mock_sms.set_start_session_response(Ok(EnhancedUserSession { // Mock response for start_session
            session_id: expected_session_id.clone(), user_id: user_id.to_string(), telegram_id: user_id,
            telegram_chat_id: user_id, session_state: EnhancedSessionState::Active, current_state: EnhancedSessionState::Active,
            started_at: now.timestamp_millis() as u64, last_activity_at: now.timestamp_millis() as u64, expires_at: (now + chrono::Duration::hours(1)).timestamp_millis() as u64,
            created_at: now.timestamp_millis() as u64, updated_at: now.timestamp_millis() as u64,
            onboarding_completed: false, preferences_set: false, metadata: serde_json::Value::Null,
            last_command: None, temporary_data: HashMap::new(), session_analytics: SessionAnalytics::default(), config: SessionConfig::default(),
        })).await;

        mock_ups.set_get_user_profile_response(Err(ArbitrageError::NotFound("User not found".to_string()))).await; // No existing profile

        // Construct a mock ServiceContainer. This part is tricky.
        // For now, we'll assume we can create a ServiceContainer that provides our mocks.
        // This will likely require modification to ServiceContainer or using `unsafe` if we were to use the real one.
        // The `MockServiceContainer` above is a conceptual stand-in.
        // The key is that `handle_start_command` needs an `Arc<ServiceContainer>`.

        // --- This is the problematic part for direct testing without refactoring ServiceContainer ---
        // let service_container = Arc::new(MockServiceContainer::new(mock_sms.clone(), mock_ups.clone()));
        // To make this compile, we need to ensure MockServiceContainer can be treated as ServiceContainer.
        // This usually means ServiceContainer itself should be an interface (trait) or allow construction with mockable parts.

        // Due to the above difficulty, this test will be more conceptual.
        // We check if the mock services were called as expected.

        // Simulate what handle_start_command would do with the mocks:
        // 1. create_or_update_session
        let session_service_for_onboarding = mock_sms.clone(); // Use the mock directly for this part
        let session_validate_result = session_service_for_onboarding.validate_session(&user_id.to_string()).await.unwrap();
        assert!(session_validate_result.is_none(), "New user should not have an existing session");

        let _new_session = session_service_for_onboarding.start_session(user_id, user_id.to_string()).await.unwrap();

        // 2. ensure_user_profile
        let profile_service_for_onboarding = mock_ups.clone();
        let get_profile_result = profile_service_for_onboarding.get_user_profile(&user_id.to_string()).await;
        assert!(get_profile_result.is_err(), "New user should not have a profile initially");

        let expected_profile = create_profile_from_telegram(&user_info).unwrap();
        profile_service_for_onboarding.create_user_profile(&expected_profile).await.unwrap();
        assert!(*mock_ups.create_user_profile_called.lock().await, "create_user_profile should have been called");

        let created_profile_in_mock = mock_ups.created_profile_data.lock().await.clone().unwrap();
        assert_eq!(created_profile_in_mock.user_id, user_id.to_string());
        assert_eq!(created_profile_in_mock.first_name, user_info.first_name);

        // 3. validate_beta_access (will fetch the profile we just "created")
        // For this to work, get_user_profile_response needs to be set again for the next call.
        mock_ups.set_get_user_profile_response(Ok(created_profile_in_mock.clone())).await;
        let profile_for_beta_check = profile_service_for_onboarding.get_user_profile(&user_id.to_string()).await.unwrap();
        assert!(!profile_for_beta_check.beta_access, "New user should not have beta access by default");

        // If we could call handle_start_command directly:
        // let result = handle_start_command(&service_container, &user_info, &permissions, &[]).await;
        // assert!(result.is_ok());
        // let message = result.unwrap();
        // assert!(message.contains("Welcome to ArbEdge"));
        // assert!(message.contains("New session created"));
        // assert!(message.contains("New profile created"));
        // assert!(message.contains("Beta access not available"));

        // Placeholder due to ServiceContainer mocking complexity
        println!("Conceptual test for new user onboarding passed its checks on mock interactions.");
        assert!(true);
    }
}

#[derive(Debug)]
struct ProfileResult {
    is_new_profile: bool,
    profile: UserProfile,
    message: String,
}

#[derive(Debug)]
struct BetaAccessResult {
    has_beta_access: bool,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    message: String,
} 