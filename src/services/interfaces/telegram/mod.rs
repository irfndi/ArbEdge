// src/services/interfaces/telegram/mod.rs

//! Telegram Interface Module
//! 
//! Modular Telegram bot interface with user journey prioritization:
//! - User Onboarding & Profile Management
//! - RBAC & Subscription Status
//! - Session Management
//! - Beta Features
//! - Global Opportunities

pub mod core;
pub mod commands;
pub mod features;
pub mod utils;

// Legacy telegram service (will be gradually replaced)
pub mod telegram;
pub mod telegram_keyboard;

// Export the new modular service and types
pub use self::{ModularTelegramService, UserInfo, UserPermissions};

use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::services::core::user::user_profile::UserProfileService;
use crate::services::core::user::session_management::SessionManagementService;
use crate::services::core::opportunities::opportunity_engine::OpportunityEngine;
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::{console_log, Env};
use serde_json::Value;
use std::sync::Arc;

use self::core::{TelegramBotClient, WebhookHandler, MessageHandler};

/// Modular Telegram Service
/// 
/// Focuses on user journey priorities:
/// 1. User Onboarding & Profile Management
/// 2. RBAC & Subscription Status  
/// 3. Session Management
/// 4. Beta Features
/// 5. Global Opportunities
pub struct ModularTelegramService {
    // Core telegram functionality
    bot_client: TelegramBotClient,
    webhook_handler: WebhookHandler,
    message_handler: MessageHandler,
    
    // Service dependencies
    service_container: Arc<ServiceContainer>,
    
    // Configuration
    is_test_mode: bool,
}

impl ModularTelegramService {
    /// Create new modular telegram service
    pub async fn new(env: &Env, service_container: Arc<ServiceContainer>) -> ArbitrageResult<Self> {
        console_log!("🚀 Initializing Modular Telegram Service...");

        // Get telegram configuration
        let bot_token = env
            .var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| ArbitrageError::configuration_error("TELEGRAM_BOT_TOKEN not found"))?
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
            chat_id,
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
            is_test_mode,
        })
    }

    /// Handle incoming webhook update
    /// 
    /// Priority: User onboarding and session management first
    pub async fn handle_webhook(&self, update: Value) -> ArbitrageResult<String> {
        console_log!("📱 Modular Telegram: Processing webhook update");

        // Extract user information for session management
        let user_info = self.extract_user_info(&update)?;
        
        // Priority 1: Session Management - Ensure user has active session
        if let Err(e) = self.ensure_user_session(&user_info).await {
            console_log!("⚠️ Session management failed: {:?}", e);
            return self.handle_session_error(&user_info).await;
        }

        // Priority 2: User Profile & RBAC - Check user permissions
        let user_permissions = self.get_user_permissions(&user_info).await?;
        
        // Priority 3: Process update with user context
        self.process_update_with_context(update, &user_info, &user_permissions).await
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

        Err(ArbitrageError::validation_error("No user information found in update"))
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
    async fn ensure_user_session(&self, user_info: &UserInfo) -> ArbitrageResult<()> {
        console_log!("🔐 Checking session for user {}", user_info.user_id);

        // Get session management service
        let session_service = self.service_container.session_service();

        // Check if user has active session
        let user_id_str = user_info.user_id.to_string();
        let has_session = session_service.validate_session(&user_id_str).await?;

        if !has_session {
            console_log!("⚠️ No active session for user {}", user_info.user_id);
            return Err(ArbitrageError::authentication_error("No active session"));
        }

        // Update session activity
        session_service.update_activity(&user_id_str).await?;
        console_log!("✅ Session validated for user {}", user_info.user_id);

        Ok(())
    }

    /// Get user permissions (Priority 2: RBAC & Subscription)
    async fn get_user_permissions(&self, user_info: &UserInfo) -> ArbitrageResult<UserPermissions> {
        console_log!("👑 Checking permissions for user {}", user_info.user_id);

        // Get user profile service
        let user_profile_service = self.service_container.user_profile_service()
            .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

        // Get user profile with RBAC info
        let user_id_str = user_info.user_id.to_string();
        let user_profile = user_profile_service.get_user_profile(&user_id_str).await?
            .ok_or_else(|| ArbitrageError::not_found("User profile not found"))?;

        // Extract permissions from profile
        let permissions = UserPermissions {
            access_level: user_profile.access_level.clone(),
            subscription_tier: user_profile.subscription_tier.clone(),
            beta_expires_at: user_profile.beta_expires_at,
            can_trade: user_profile.can_trade(),
            is_admin: matches!(user_profile.access_level, crate::types::UserAccessLevel::Admin | crate::types::UserAccessLevel::SuperAdmin),
        };

        console_log!("✅ Permissions loaded for user {}: {:?}", user_info.user_id, permissions.access_level);
        Ok(permissions)
    }

    /// Process update with user context (Priority 3: Feature Processing)
    async fn process_update_with_context(
        &self,
        update: Value,
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        console_log!("🎯 Processing update with user context for user {}", user_info.user_id);

        // Route to appropriate handler based on update type and permissions
        if let Some(message) = update.get("message") {
            return self.handle_message_with_context(message, user_info, permissions).await;
        }

        if let Some(callback_query) = update.get("callback_query") {
            return self.handle_callback_with_context(callback_query, user_info, permissions).await;
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
            return self.handle_command_with_context(text, user_info, permissions).await;
        }

        // Handle regular message
        self.handle_regular_message_with_context(message, user_info, permissions).await
    }

    /// Handle command with proper context (Priority 3: Command Processing)
    async fn handle_command_with_context(
        &self,
        command: &str,
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        console_log!("🎯 Processing command '{}' for user {} in chat type '{}'", command, user_info.user_id, user_info.chat_type);

        // Check if this is a group/channel context
        let is_group_context = matches!(user_info.chat_type.as_str(), "group" | "supergroup" | "channel");

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

        // Private chat - full command processing
        match command {
            "/start" => self.handle_start_command(user_info, permissions).await,
            "/help" => self.handle_help_command(user_info, permissions).await,
            "/profile" => self.handle_profile_command(user_info, permissions).await,
            "/opportunities" => self.handle_opportunities_command(user_info, permissions).await,
            "/settings" => self.handle_settings_command(user_info, permissions).await,
            "/admin" => self.handle_admin_command(user_info, permissions).await,
            "/groups" => self.handle_groups_command(user_info, permissions).await,
            "/addkey" => self.handle_add_key_command(user_info, permissions).await,
            "/trade" => self.handle_trade_command(user_info, permissions).await,
            _ => Ok(format!("❓ Unknown command: {}\n\nType /help to see available commands.", command)),
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
        self.bot_client.send_message(&chat_id, text, Some("Markdown"), None).await?;
        Ok(())
    }

    // Command handlers (to be implemented)
    async fn handle_start_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        console_log!("🚀 Start command for user {}", user_info.user_id);
        // TODO: Implement user onboarding flow
        let message = "🚀 *Welcome to ArbEdge!*\n\nYour trading assistant is ready.";
        self.send_message_to_user(user_info, message).await?;
        Ok("Start command handled".to_string())
    }

    async fn handle_help_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        console_log!("❓ Help command for user {}", user_info.user_id);
        let message = self.message_handler.format_help_message();
        self.send_message_to_user(user_info, &message).await?;
        Ok("Help command handled".to_string())
    }

    async fn handle_profile_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        console_log!("👤 Profile command for user {}", user_info.user_id);
        // TODO: Implement profile display
        let message = format!("👤 *Your Profile*\n\nRole: {:?}\nBeta Access: {}", permissions.access_level, permissions.beta_expires_at);
        self.send_message_to_user(user_info, &message).await?;
        Ok("Profile command handled".to_string())
    }

    async fn handle_opportunities_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        console_log!("💰 Opportunities command for user {}", user_info.user_id);
        // TODO: Implement global opportunities using our keys
        let message = "💰 *Global Opportunities*\n\nFetching latest opportunities...";
        self.send_message_to_user(user_info, message).await?;
        Ok("Opportunities command handled".to_string())
    }

    async fn handle_settings_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        console_log!("⚙️ Settings command for user {}", user_info.user_id);
        let message = "⚙️ *Settings*\n\nConfigure your preferences here.";
        self.send_message_to_user(user_info, message).await?;
        Ok("Settings command handled".to_string())
    }

    async fn handle_admin_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        if !permissions.is_admin {
            return Ok("❌ Admin access required.".to_string());
        }
        Ok("🔧 Admin panel - Feature coming soon!".to_string())
    }

    async fn handle_regular_message_with_context(
        &self,
        message: &Value,
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        console_log!("💭 Regular message from user {}", user_info.user_id);
        let response = "💭 I received your message. Use /help to see available commands.";
        self.send_message_to_user(user_info, response).await?;
        Ok("Regular message handled".to_string())
    }

    async fn handle_callback_with_context(
        &self,
        callback_query: &Value,
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        console_log!("🔘 Callback query from user {}", user_info.user_id);
        // TODO: Implement callback handling
        Ok("Callback handled".to_string())
    }

    async fn handle_groups_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        // Show groups where user is admin
        Ok("📊 **Your Groups**\n\n🔹 Group management coming soon!\n🔹 Add me to groups and I'll automatically detect admin status\n🔹 Configure AI and subscription settings here".to_string())
    }

    async fn handle_add_key_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        Ok("🔑 **Add API Key**\n\n🔹 Exchange API keys for trading\n🔹 AI API keys for enhanced analysis\n🔹 Key management interface coming soon!\n\n💡 Tip: Free tier users can use BYOK (Bring Your Own Key)".to_string())
    }

    async fn handle_trade_command(&self, user_info: &UserInfo, permissions: &UserPermissions) -> ArbitrageResult<String> {
        if !permissions.can_trade {
            return Ok("❌ Trading not available. Please add exchange API keys first.\n\nUse /addkey to add your exchange API keys.".to_string());
        }
        Ok("💹 **Trading Panel**\n\n🔹 Manual trading interface\n🔹 Position management\n🔹 Portfolio overview\n\n⚠️ Trading interface coming soon!".to_string())
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
    pub access_level: crate::types::UserAccessLevel,
    pub subscription_tier: crate::types::SubscriptionTier,
    pub beta_expires_at: Option<u64>,
    pub can_trade: bool,
    pub is_admin: bool,
} 