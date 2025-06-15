// src/services/interfaces/telegram/mod.rs

//! Telegram Interface Module
//!
//! Modular Telegram bot interface with user journey prioritization:
//! - User Onboarding & Profile Management
//! - RBAC & Subscription Status
//! - Session Management
//! - Beta Features
//! - Global Opportunities

pub mod commands;
pub mod core;
pub mod features;
pub mod utils;

pub mod telegram_keyboard;

// Export commands module publicly
pub use commands::CommandRouter;

// Export the new modular service and types - defined below in this file
// Note: These are defined later in this file

use crate::services::core::infrastructure::service_container::ServiceContainer;
// Imports removed as they are now used directly from ServiceContainer
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde_json::Value;
use std::sync::Arc;
use worker::{console_log, Env};

use self::core::{MessageHandler, TelegramBotClient, WebhookHandler};

/// Modular Telegram Service
///
/// Focuses on user journey priorities:
/// 1. User Onboarding & Profile Management
/// 2. RBAC & Subscription Status  
/// 3. Session Management
/// 4. Beta Features
/// 5. Global Opportunities
#[allow(dead_code)]
pub struct ModularTelegramService {
    // Core telegram functionality
    bot_client: TelegramBotClient,
    webhook_handler: WebhookHandler,
    message_handler: MessageHandler,

    // Service dependencies
    service_container: Arc<ServiceContainer>,

    // Configuration
    chat_id: String,
    is_test_mode: bool,
}

impl ModularTelegramService {
    /// Create new modular telegram service
    pub async fn new(env: &Env, service_container: Arc<ServiceContainer>) -> ArbitrageResult<Self> {
        console_log!("🚀 Initializing Modular Telegram Service...");

        // Get telegram configuration
        let bot_token = env
            .secret("TELEGRAM_BOT_TOKEN")
            .map_err(|_| {
                ArbitrageError::configuration_error("TELEGRAM_BOT_TOKEN secret not found")
            })?
            .to_string();

        let chat_id = env
            .var("TELEGRAM_CHAT_ID")
            .map_err(|_| ArbitrageError::configuration_error("TELEGRAM_CHAT_ID not found"))?
            .to_string();

        let is_test_mode = env
            .var("TELEGRAM_TEST_MODE")
            .map(|v| v.to_string() == "true")
            .unwrap_or(false);

        // Create telegram config
        let config = self::core::TelegramConfig {
            bot_token,
            chat_id: chat_id.clone(),
            is_test_mode,
        };

        // Initialize core components
        let bot_client = TelegramBotClient::new(config);
        let webhook_handler = WebhookHandler::new();
        let message_handler = MessageHandler::new();

        console_log!("✅ Modular Telegram Service initialized successfully");

        Ok(Self {
            bot_client,
            webhook_handler,
            message_handler,
            service_container,
            chat_id,
            is_test_mode,
        })
    }

    /// Send message to default chat
    pub async fn send_message(&self, message: &str) -> ArbitrageResult<()> {
        self.bot_client
            .send_message(&self.chat_id, message, None, None)
            .await
            .map(|_| ())
    }

    /// Cleanup session memory cache (should be called periodically)
    pub async fn cleanup_session_cache(&self) -> ArbitrageResult<()> {
        let optimization_enabled =
            crate::utils::feature_flags::is_feature_enabled("session_optimization.enabled")
                .unwrap_or(false);
        let cache_cleanup_enabled =
            crate::utils::feature_flags::is_feature_enabled("session_optimization.cache_cleanup")
                .unwrap_or(false);

        if optimization_enabled && cache_cleanup_enabled {
            console_log!("🧹 Cleaning up session memory cache");
            let session_service = self.service_container.session_service();
            session_service.cleanup_memory_cache();
            console_log!("✅ Session memory cache cleanup completed");
        }
        Ok(())
    }

    /// Handle incoming webhook update
    ///
    /// Priority: User onboarding and session management first
    pub async fn handle_webhook(&self, update: Value) -> ArbitrageResult<String> {
        console_log!("📱 Modular Telegram: Processing webhook update");

        // Extract user information for session management
        let user_info = self.extract_user_info(&update)?;

        // Check if this is a /start command - if so, allow without session validation
        let is_start_command = self.is_start_command(&update);

        // Priority 1: Session Management (unless it's /start command)
        if !is_start_command {
            if let Err(e) = self.ensure_user_session(&user_info).await {
                console_log!("⚠️ Session management failed: {:?}", e);
                return self.handle_session_error(&user_info).await;
            }
        }

        // Priority 2: User Profile & RBAC - Check user permissions (handle gracefully for new users)
        let user_permissions = if is_start_command {
            // For /start command, use default permissions (will be properly set after user creation)
            self.get_default_permissions()
        } else {
            self.get_user_permissions(&user_info).await?
        };

        // Priority 3: Process update with user context
        let result = self
            .process_update_with_context(update, &user_info, &user_permissions)
            .await?;

        // Priority 4: After processing /start, ensure session is created
        if is_start_command {
            if let Err(e) = self.create_user_session(&user_info).await {
                console_log!("⚠️ Failed to create session after /start: {:?}", e);
                // Don't fail the request, just log the warning
            }
        }

        Ok(result)
    }

    /// Extract user information from update
    fn extract_user_info(&self, update: &Value) -> ArbitrageResult<UserInfo> {
        // Extract from message
        if let Some(message) = update.get("message") {
            return self.extract_user_from_message(message);
        }

        // Extract from callback query
        if let Some(callback_query) = update.get("callback_query") {
            return self.extract_user_from_callback(callback_query);
        }

        Err(ArbitrageError::validation_error(
            "No user information found in update",
        ))
    }

    /// Extract user info from message
    fn extract_user_from_message(&self, message: &Value) -> ArbitrageResult<UserInfo> {
        let user = message
            .get("from")
            .ok_or_else(|| ArbitrageError::validation_error("No user in message"))?;

        let chat = message
            .get("chat")
            .ok_or_else(|| ArbitrageError::validation_error("No chat in message"))?;

        Ok(UserInfo {
            user_id: user
                .get("id")
                .and_then(|id| id.as_i64())
                .ok_or_else(|| ArbitrageError::validation_error("Invalid user ID"))?,
            username: user
                .get("username")
                .and_then(|u| u.as_str())
                .map(|s| s.to_string()),
            first_name: user
                .get("first_name")
                .and_then(|f| f.as_str())
                .map(|s| s.to_string()),
            chat_id: chat
                .get("id")
                .and_then(|id| id.as_i64())
                .ok_or_else(|| ArbitrageError::validation_error("Invalid chat ID"))?,
            chat_type: chat
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("private")
                .to_string(),
        })
    }

    /// Extract user info from callback query
    fn extract_user_from_callback(&self, callback_query: &Value) -> ArbitrageResult<UserInfo> {
        let user = callback_query
            .get("from")
            .ok_or_else(|| ArbitrageError::validation_error("No user in callback query"))?;

        let message = callback_query
            .get("message")
            .ok_or_else(|| ArbitrageError::validation_error("No message in callback query"))?;

        let chat = message
            .get("chat")
            .ok_or_else(|| ArbitrageError::validation_error("No chat in callback message"))?;

        Ok(UserInfo {
            user_id: user
                .get("id")
                .and_then(|id| id.as_i64())
                .ok_or_else(|| ArbitrageError::validation_error("Invalid user ID"))?,
            username: user
                .get("username")
                .and_then(|u| u.as_str())
                .map(|s| s.to_string()),
            first_name: user
                .get("first_name")
                .and_then(|f| f.as_str())
                .map(|s| s.to_string()),
            chat_id: chat
                .get("id")
                .and_then(|id| id.as_i64())
                .ok_or_else(|| ArbitrageError::validation_error("Invalid chat ID"))?,
            chat_type: chat
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("private")
                .to_string(),
        })
    }

    /// Ensure user has active session (Priority 1: Session Management)
    /// Optimized for high-performance with multi-tier caching
    async fn ensure_user_session(&self, user_info: &UserInfo) -> ArbitrageResult<()> {
        // Get session management service
        let session_service = self.service_container.session_service();

        // Check if session optimization is enabled
        let optimization_enabled =
            crate::utils::feature_flags::is_feature_enabled("session_optimization.enabled")
                .unwrap_or(false);

        if optimization_enabled {
            console_log!(
                "🔐 Checking session for user {} (optimized)",
                user_info.user_id
            );

            // Use optimized session validation (multi-tier caching)
            let has_session = session_service
                .validate_session_optimized(user_info.user_id)
                .await?;

            if has_session.is_none() {
                console_log!("⚠️ No active session for user {}", user_info.user_id);
                return Err(ArbitrageError::authentication_error("No active session"));
            }

            // Use optimized activity update (intelligent database writes)
            session_service
                .update_activity_optimized(user_info.user_id)
                .await?;
            console_log!(
                "✅ Session validated for user {} (optimized)",
                user_info.user_id
            );
        } else {
            console_log!(
                "🔐 Checking session for user {} (legacy)",
                user_info.user_id
            );

            // Use legacy session validation
            let user_id_str = user_info.user_id.to_string();
            let has_session = session_service.validate_session(&user_id_str).await?;

            if has_session.is_none() {
                console_log!("⚠️ No active session for user {}", user_info.user_id);
                return Err(ArbitrageError::authentication_error("No active session"));
            }

            // Use legacy activity update
            session_service.update_activity(&user_id_str).await?;
            console_log!(
                "✅ Session validated for user {} (legacy)",
                user_info.user_id
            );
        }

        Ok(())
    }

    /// Get user permissions (RBAC check) (Priority 2)
    async fn get_user_permissions(&self, user_info: &UserInfo) -> ArbitrageResult<UserPermissions> {
        console_log!("👑 Checking permissions for user {}", user_info.user_id);

        let user_profile_service =
            self.service_container
                .user_profile_service()
                .ok_or_else(|| {
                    ArbitrageError::service_unavailable("User profile service not available")
                })?;

        // Get user profile using telegram_user_id lookup instead of user_id string conversion
        let user_profile = user_profile_service
            .get_user_by_telegram_id(user_info.user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        // Extract permissions from profile
        let permissions = UserPermissions {
            role: user_profile.access_level.clone(),
            subscription_tier: user_profile.subscription_tier.to_string(),
            daily_opportunity_limit: user_profile
                .subscription
                .daily_opportunity_limit
                .unwrap_or(5),
            beta_access: user_profile.is_beta_active,
            is_admin: matches!(
                user_profile.access_level,
                crate::types::UserAccessLevel::Admin | crate::types::UserAccessLevel::SuperAdmin
            ),
            can_trade: user_profile.access_level.can_trade(),
        };

        console_log!(
            "✅ Permissions loaded for user {}: {:?}",
            user_info.user_id,
            permissions.role
        );
        Ok(permissions)
    }

    /// Process update with user context (Priority 3: Feature Processing)
    async fn process_update_with_context(
        &self,
        update: Value,
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        console_log!(
            "🎯 Processing update with user context for user {}",
            user_info.user_id
        );

        // Route to appropriate handler based on update type and permissions
        if let Some(message) = update.get("message") {
            return self
                .handle_message_with_context(message, user_info, permissions)
                .await;
        }

        if let Some(callback_query) = update.get("callback_query") {
            return self
                .handle_callback_with_context(callback_query, user_info, permissions)
                .await;
        }

        Ok("Update processed".to_string())
    }

    /// Handle message with user context
    async fn handle_message_with_context(
        &self,
        message: &Value,
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        // Check if it's a command
        if let Some(text) = message.get("text").and_then(|t| t.as_str()) {
            return self
                .handle_command_with_context(text, user_info, permissions)
                .await;
        }

        // Handle regular message
        self.handle_regular_message_with_context(message, user_info, permissions)
            .await
    }

    /// Handle command with proper context (Priority 3: Command Processing)
    async fn handle_command_with_context(
        &self,
        command: &str,
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        console_log!(
            "🎯 Processing command '{}' for user {} in chat type '{}'",
            command,
            user_info.user_id,
            user_info.chat_type
        );

        // Check if this is a group/channel context
        let is_group_context = matches!(
            user_info.chat_type.as_str(),
            "group" | "supergroup" | "channel"
        );

        // In group/channel contexts, only allow specific commands
        if is_group_context {
            match command {
                "/start" => {
                    // In groups, /start should explain how to use the bot
                    return Ok("👋 Hi! I'm ArbEdge Bot. I'll send arbitrage opportunities to this group.\n\n🔹 To trade or manage settings, please start a private chat with me: @ArbEdgeBot\n🔹 Group admins can manage AI and subscription settings in private chat.".to_string());
                }
                "/help" => {
                    return Ok("📖 **ArbEdge Bot Help (Group/Channel)**\n\n🔹 I send arbitrage opportunities to this group\n🔹 Click 'Take Action' buttons to trade in private chat\n🔹 No manual commands work in groups\n🔹 Admins: manage settings in private chat with @ArbEdgeBot".to_string());
                }
                _ => {
                    // All other commands are not allowed in groups
                    return Ok("⚠️ Commands are not available in groups/channels.\n\n🔹 Please start a private chat with me: @ArbEdgeBot\n🔹 I'll send opportunities here, use 'Take Action' buttons to trade.".to_string());
                }
            }
        }

        // Private chat - use CommandRouter for full command processing
        let parts: Vec<&str> = command.split_whitespace().collect();
        let base_command = parts.first().unwrap_or(&"");
        let args: Vec<&str> = if parts.len() > 1 {
            parts[1..].to_vec()
        } else {
            vec![]
        };

        match commands::CommandRouter::route_command(
            base_command,
            &args,
            user_info,
            permissions,
            &self.service_container,
        )
        .await
        {
            Ok(response) => {
                // Send the response to the user
                self.send_message_to_user(user_info, &response).await?;
                Ok("Command processed successfully".to_string())
            }
            Err(e) => {
                console_log!("❌ Command processing failed: {:?}", e);
                let error_message = format!(
                    "❓ Unknown command: {}\n\nType /help to see available commands.",
                    command
                );
                self.send_message_to_user(user_info, &error_message).await?;
                Ok("Unknown command handled".to_string())
            }
        }
    }

    /// Check if the update contains a /start command
    fn is_start_command(&self, update: &Value) -> bool {
        if let Some(message) = update.get("message") {
            if let Some(text) = message.get("text").and_then(|t| t.as_str()) {
                return text.trim().starts_with("/start");
            }
        }
        false
    }

    /// Get default permissions for new users (used during /start)
    fn get_default_permissions(&self) -> UserPermissions {
        UserPermissions {
            role: crate::types::UserAccessLevel::User,
            subscription_tier: "free".to_string(),
            daily_opportunity_limit: 3, // Free tier default
            beta_access: false,
            is_admin: false,
            can_trade: true,
        }
    }

    /// Create user session after successful /start command processing
    async fn create_user_session(&self, user_info: &UserInfo) -> ArbitrageResult<()> {
        console_log!("🔐 Creating session for new user {}", user_info.user_id);

        let session_service = self.service_container.session_service();
        let user_id_str = user_info.user_id.to_string();

        // Create new session using start_session method
        match session_service
            .start_session(user_info.user_id, user_id_str)
            .await
        {
            Ok(_) => {
                console_log!(
                    "✅ Session created successfully for user {}",
                    user_info.user_id
                );
                Ok(())
            }
            Err(e) => {
                console_log!(
                    "❌ Failed to create session for user {}: {:?}",
                    user_info.user_id,
                    e
                );
                Err(e)
            }
        }
    }

    /// Handle session error
    async fn handle_session_error(&self, user_info: &UserInfo) -> ArbitrageResult<String> {
        let message = "🔐 *Session Required*\n\nPlease start a new session with /start to continue using the bot.";
        self.send_message_to_user(user_info, message).await?;
        Ok("Session error handled".to_string())
    }

    /// Send message to user
    async fn send_message_to_user(&self, user_info: &UserInfo, text: &str) -> ArbitrageResult<()> {
        let chat_id = user_info.chat_id.to_string();
        self.bot_client
            .send_message(&chat_id, text, Some("HTML"), None)
            .await?;
        Ok(())
    }

    async fn handle_regular_message_with_context(
        &self,
        _message: &Value,
        user_info: &UserInfo,
        _permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        console_log!("💭 Regular message from user {}", user_info.user_id);
        let response = "💭 I received your message. Use /help to see available commands.";
        self.send_message_to_user(user_info, response).await?;
        Ok("Regular message handled".to_string())
    }

    async fn handle_callback_with_context(
        &self,
        _callback_query: &Value,
        user_info: &UserInfo,
        _permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        console_log!("🔘 Callback query from user {}", user_info.user_id);
        // TODO: Implement callback handling
        Ok("Callback handled".to_string())
    }
}

/// User information extracted from Telegram update
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub user_id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub chat_id: i64,
    pub chat_type: String,
}

/// User permissions and subscription info
#[derive(Debug, Clone)]
pub struct UserPermissions {
    pub role: crate::types::UserAccessLevel,
    pub subscription_tier: String,
    pub daily_opportunity_limit: u32,
    pub beta_access: bool,
    pub is_admin: bool,
    pub can_trade: bool,
}

impl UserPermissions {
    /// Check if user can access trading features
    pub fn can_access_trading_features(&self) -> bool {
        matches!(
            self.role,
            crate::types::UserAccessLevel::Paid
                | crate::types::UserAccessLevel::Premium
                | crate::types::UserAccessLevel::Admin
                | crate::types::UserAccessLevel::SuperAdmin
        ) || self.can_trade
    }

    /// Check if user can automate trading
    pub fn can_automate_trading(&self) -> bool {
        matches!(
            self.role,
            crate::types::UserAccessLevel::Premium
                | crate::types::UserAccessLevel::Admin
                | crate::types::UserAccessLevel::SuperAdmin
        )
    }

    /// Check if user can request manual scans
    pub fn can_request_manual_scans(&self) -> bool {
        matches!(
            self.role,
            crate::types::UserAccessLevel::Paid
                | crate::types::UserAccessLevel::Premium
                | crate::types::UserAccessLevel::Admin
                | crate::types::UserAccessLevel::SuperAdmin
        )
    }

    /// Check if user can access API features
    pub fn can_access_api_features(&self) -> bool {
        matches!(
            self.role,
            crate::types::UserAccessLevel::Paid
                | crate::types::UserAccessLevel::Premium
                | crate::types::UserAccessLevel::Admin
                | crate::types::UserAccessLevel::SuperAdmin
        )
    }

    /// Check if user can access AI features
    pub fn can_access_ai_features(&self) -> bool {
        matches!(
            self.role,
            crate::types::UserAccessLevel::Paid
                | crate::types::UserAccessLevel::Premium
                | crate::types::UserAccessLevel::Admin
                | crate::types::UserAccessLevel::SuperAdmin
                | crate::types::UserAccessLevel::BetaUser
        ) && self.beta_access
    }

    /// Get access level as string for display
    pub fn access_level(&self) -> String {
        match self.role {
            crate::types::UserAccessLevel::Guest => "Guest".to_string(),
            crate::types::UserAccessLevel::Free => "Free".to_string(),
            crate::types::UserAccessLevel::Registered => "Registered".to_string(),
            crate::types::UserAccessLevel::Verified => "Verified".to_string(),
            crate::types::UserAccessLevel::Paid => "Paid".to_string(),
            crate::types::UserAccessLevel::Premium => "Premium".to_string(),
            crate::types::UserAccessLevel::Admin => "Admin".to_string(),
            crate::types::UserAccessLevel::SuperAdmin => "Super Admin".to_string(),
            crate::types::UserAccessLevel::BetaUser => "Beta User".to_string(),
            crate::types::UserAccessLevel::FreeWithoutAPI => "Free (No API)".to_string(),
            crate::types::UserAccessLevel::FreeWithAPI => "Free (With API)".to_string(),
            crate::types::UserAccessLevel::SubscriptionWithAPI => {
                "Subscription (With API)".to_string()
            }
            crate::types::UserAccessLevel::Basic => "Basic".to_string(),
            crate::types::UserAccessLevel::User => "User".to_string(),
        }
    }
}
