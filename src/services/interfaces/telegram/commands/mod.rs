//! Telegram Command Handlers
//! 
//! Modular command handling with user journey prioritization:
//! - User Onboarding Commands
//! - Profile Management Commands  
//! - RBAC & Subscription Commands
//! - Beta Feature Commands
//! - Global Opportunity Commands

pub mod onboarding;
pub mod profile;
pub mod opportunities;
pub mod admin;
pub mod settings;

use crate::services::interfaces::telegram::{UserInfo, UserPermissions};
use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::utils::{ArbitrageError, ArbitrageResult};
use std::sync::Arc;

/// Command router for handling all telegram commands
pub struct CommandRouter {
    service_container: Arc<ServiceContainer>,
}

impl CommandRouter {
    pub fn new(service_container: Arc<ServiceContainer>) -> Self {
        Self { service_container }
    }

    /// Route command to appropriate handler
    pub async fn route_command(
        &self,
        command: &str,
        args: &[&str],
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let cmd = parts.get(0).unwrap_or(&"");

        match *cmd {
            // Priority 1: User Onboarding & Session
            "/start" => onboarding::handle_start_command(
                &self.service_container,
                user_info,
                permissions,
                args,
            ).await,
            
            // Priority 2: Profile Management & RBAC
            "/profile" => profile::handle_profile_command(
                &self.service_container,
                user_info,
                permissions,
                args,
            ).await,
            
            "/subscription" => profile::handle_subscription_command(
                &self.service_container,
                user_info,
                permissions,
                args,
            ).await,
            
            // Priority 3: Global Opportunities
            "/opportunities" => opportunities::handle_opportunities_command(
                &self.service_container,
                user_info,
                permissions,
                args,
            ).await,
            
            "/beta" if permissions.beta_access => opportunities::handle_beta_command(
                &self.service_container,
                user_info,
                permissions,
                args,
            ).await,
            
            // Settings & Configuration
            "/settings" => settings::handle_settings_command(
                &self.service_container,
                user_info,
                permissions,
                args,
            ).await,
            
            // Admin Commands (RBAC protected)
            "/admin" if permissions.is_admin => admin::handle_admin_command(
                &self.service_container,
                user_info,
                permissions,
                args,
            ).await,
            
            // Help and general commands
            "/help" => self.handle_help_command(user_info, permissions).await,
            
            _ => Err(ArbitrageError::validation_error(&format!("Unknown command: {}", cmd))),
        }
    }

    /// Handle help command
    async fn handle_help_command(
        &self,
        user_info: &UserInfo,
        permissions: &UserPermissions,
    ) -> ArbitrageResult<String> {
        let mut help_text = String::from("🤖 *ArbEdge Bot Commands*\n\n");
        
        // Basic commands available to all users
        help_text.push_str("*Basic Commands:*\n");
        help_text.push_str("🚀 `/start` - Start/restart your session\n");
        help_text.push_str("👤 `/profile` - View your profile\n");
        help_text.push_str("💰 `/opportunities` - View trading opportunities\n");
        help_text.push_str("⚙️ `/settings` - Configure preferences\n");
        help_text.push_str("❓ `/help` - Show this help\n\n");
        
        // Beta features (if user has beta access)
        if permissions.beta_access {
            help_text.push_str("*Beta Features:*\n");
            help_text.push_str("🧪 `/beta` - Access beta features\n");
            help_text.push_str("💎 `/subscription` - Manage subscription\n\n");
        }
        
        // Admin commands (if user is admin)
        if permissions.is_admin {
            help_text.push_str("*Admin Commands:*\n");
            help_text.push_str("👑 `/admin` - Admin panel\n\n");
        }
        
        help_text.push_str(&format!(
            "*Your Status:*\n\
            Role: {:?}\n\
            Beta Access: {}\n\
            Daily Limit: {}",
            permissions.role,
            if permissions.beta_access { "✅" } else { "❌" },
            permissions.daily_opportunity_limit
        ));
        
        Ok(help_text)
    }
} 