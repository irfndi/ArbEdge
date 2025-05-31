//! Profile Management Commands
//! 
//! Priority 2: Profile Management & RBAC
//! - User profile display and management
//! - RBAC role and permission display
//! - Subscription status and management
//! - Profile configuration

use crate::services::interfaces::telegram::{UserInfo, UserPermissions};
use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::services::core::user::user_profile::UserProfileService;
use crate::types::{UserProfile, UserRole};
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::console_log;
use std::sync::Arc;

/// Handle /profile command - Display and manage user profile
pub async fn handle_profile_command(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
    args: &[&str],
) -> ArbitrageResult<String> {
    console_log!("👤 Profile command for user {} with role {:?}", user_info.user_id, permissions.role);

    // Handle profile subcommands
    let subcommand = args.get(0).unwrap_or(&"view");
    
    match *subcommand {
        "view" | "" => display_user_profile(service_container, user_info, permissions).await,
        "edit" => edit_user_profile(service_container, user_info, permissions, &args[1..]).await,
        "permissions" => display_user_permissions(service_container, user_info, permissions).await,
        "stats" => display_user_stats(service_container, user_info, permissions).await,
        _ => Ok("❓ Unknown profile command. Use `/profile` to see your profile.".to_string()),
    }
}

/// Handle /subscription command - Manage subscription status
pub async fn handle_subscription_command(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
    args: &[&str],
) -> ArbitrageResult<String> {
    console_log!("💎 Subscription command for user {} with tier: {}", user_info.user_id, permissions.subscription_tier);

    // Handle subscription subcommands
    let subcommand = args.get(0).unwrap_or(&"status");
    
    match *subcommand {
        "status" | "" => display_subscription_status(service_container, user_info, permissions).await,
        "upgrade" => display_upgrade_options(service_container, user_info, permissions).await,
        "benefits" => display_subscription_benefits(service_container, user_info, permissions).await,
        _ => Ok("❓ Unknown subscription command. Use `/subscription` to see your status.".to_string()),
    }
}

/// Display comprehensive user profile
async fn display_user_profile(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("👤 Displaying profile for user {}", user_info.user_id);

    // Get full user profile
    let user_profile_service = service_container
        .get_user_profile_service()
        .ok_or_else(|| ArbitrageError::service_unavailable("User profile service not available"))?;

    let user_id_str = user_info.user_id.to_string();
    let profile = user_profile_service.get_user_profile(&user_id_str).await?;

    let mut message = String::from("👤 *Your Profile*\n\n");
    
    // Basic Information
    message.push_str("📋 *Basic Information*\n");
    message.push_str(&format!("Name: {}\n", 
        profile.first_name.as_ref()
            .or(profile.username.as_ref())
            .unwrap_or(&"Not set".to_string())
    ));
    message.push_str(&format!("Username: @{}\n", 
        profile.username.as_ref().unwrap_or(&"Not set".to_string())
    ));
    message.push_str(&format!("User ID: `{}`\n", profile.user_id));
    message.push_str(&format!("Member Since: {}\n\n", 
        profile.created_at.format("%Y-%m-%d")
    ));

    // Account Status
    message.push_str("🔐 *Account Status*\n");
    message.push_str(&format!("Role: {:?}\n", profile.role));
    message.push_str(&format!("Status: {}\n", 
        if profile.is_active { "✅ Active" } else { "❌ Inactive" }
    ));
    message.push_str(&format!("Last Login: {}\n\n", 
        profile.last_login
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or("Never".to_string())
    ));

    // Subscription & Access
    message.push_str("💎 *Subscription & Access*\n");
    message.push_str(&format!("Tier: {}\n", profile.subscription_tier.to_uppercase()));
    message.push_str(&format!("Beta Access: {}\n", 
        if profile.beta_access { "✅ Active" } else { "❌ Not Available" }
    ));
    if let Some(beta_expires) = profile.beta_expires_at {
        message.push_str(&format!("Beta Expires: {}\n", beta_expires.format("%Y-%m-%d")));
    }
    message.push_str(&format!("Trading Enabled: {}\n", 
        if profile.can_trade { "✅ Yes" } else { "❌ No (Add API keys)" }
    ));
    message.push_str(&format!("Daily Limit: {}\n\n", 
        if profile.daily_opportunity_limit > 100 { 
            "Unlimited".to_string() 
        } else { 
            profile.daily_opportunity_limit.to_string() 
        }
    ));

    // Preferences
    message.push_str("⚙️ *Preferences*\n");
    if let Some(prefs) = profile.preferences.as_object() {
        for (key, value) in prefs {
            message.push_str(&format!("{}: {}\n", 
                key.replace('_', " ").to_title_case(),
                value.as_str().unwrap_or(&value.to_string())
            ));
        }
    }
    message.push_str("\n");

    // Quick Actions
    message.push_str("🎯 *Quick Actions*\n");
    message.push_str("• `/profile edit` - Edit profile information\n");
    message.push_str("• `/profile permissions` - View detailed permissions\n");
    message.push_str("• `/profile stats` - View usage statistics\n");
    message.push_str("• `/subscription` - Manage subscription\n");
    message.push_str("• `/settings` - Configure preferences");

    Ok(message)
}

/// Display user permissions and RBAC details
async fn display_user_permissions(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("🔐 Displaying permissions for user {}", user_info.user_id);

    let mut message = String::from("🔐 *Your Permissions & Access*\n\n");
    
    // Role Information
    message.push_str("👑 *Role Information*\n");
    message.push_str(&format!("Current Role: {:?}\n", permissions.role));
    message.push_str(&format!("Admin Access: {}\n", 
        if permissions.is_admin { "✅ Yes" } else { "❌ No" }
    ));
    message.push_str("\n");

    // Access Levels
    message.push_str("🎯 *Access Levels*\n");
    message.push_str(&format!("Beta Features: {}\n", 
        if permissions.beta_access { "✅ Enabled" } else { "❌ Disabled" }
    ));
    message.push_str(&format!("Trading: {}\n", 
        if permissions.can_trade { "✅ Enabled" } else { "❌ Disabled" }
    ));
    message.push_str(&format!("Daily Opportunities: {}\n", 
        if permissions.daily_opportunity_limit > 100 { 
            "Unlimited".to_string() 
        } else { 
            permissions.daily_opportunity_limit.to_string() 
        }
    ));
    message.push_str("\n");

    // Feature Access by Role
    message.push_str("🚀 *Feature Access*\n");
    match permissions.role {
        UserRole::SuperAdmin => {
            message.push_str("✅ All features (Super Admin)\n");
            message.push_str("✅ System administration\n");
            message.push_str("✅ User management\n");
            message.push_str("✅ Configuration access\n");
        }
        UserRole::Admin => {
            message.push_str("✅ Admin features\n");
            message.push_str("✅ User support\n");
            message.push_str("✅ System monitoring\n");
            message.push_str("❌ System configuration\n");
        }
        UserRole::Premium => {
            message.push_str("✅ Premium features\n");
            message.push_str("✅ Unlimited opportunities\n");
            message.push_str("✅ Advanced analytics\n");
            message.push_str("✅ Priority support\n");
        }
        UserRole::Basic => {
            message.push_str("✅ Basic features\n");
            message.push_str("✅ Limited opportunities\n");
            message.push_str("❌ Advanced analytics\n");
            message.push_str("❌ Priority support\n");
        }
        UserRole::Free => {
            message.push_str("✅ Free features only\n");
            message.push_str("✅ 3 opportunities/day\n");
            message.push_str("❌ Real-time notifications\n");
            message.push_str("❌ Advanced features\n");
        }
    }
    message.push_str("\n");

    // Subscription Benefits
    message.push_str("💎 *Subscription Benefits*\n");
    message.push_str(&format!("Current Tier: {}\n", permissions.subscription_tier.to_uppercase()));
    if permissions.subscription_tier == "free" {
        message.push_str("💡 Upgrade to unlock:\n");
        message.push_str("• Unlimited opportunities\n");
        message.push_str("• Real-time notifications\n");
        message.push_str("• Advanced analytics\n");
        message.push_str("• Priority support\n");
        message.push_str("\nUse `/subscription upgrade` to learn more!");
    } else {
        message.push_str("✅ Premium benefits active\n");
        message.push_str("✅ All features unlocked\n");
    }

    Ok(message)
}

/// Display user usage statistics
async fn display_user_stats(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("📊 Displaying stats for user {}", user_info.user_id);

    let mut message = String::from("📊 *Your Usage Statistics*\n\n");
    
    // Today's Usage
    message.push_str("📅 *Today's Activity*\n");
    
    // Get opportunity distribution service for usage stats
    if let Some(distribution_service) = service_container.get_opportunity_distribution_service() {
        let user_id_str = user_info.user_id.to_string();
        let today_usage = distribution_service.get_daily_usage(&user_id_str).await.unwrap_or(0);
        let remaining = (permissions.daily_opportunity_limit - today_usage).max(0);
        
        message.push_str(&format!("Opportunities Viewed: {}\n", today_usage));
        message.push_str(&format!("Remaining Today: {}\n", 
            if remaining > 100 { "Unlimited".to_string() } else { remaining.to_string() }
        ));
    } else {
        message.push_str("Opportunities Viewed: Not available\n");
        message.push_str("Remaining Today: Not available\n");
    }
    
    message.push_str(&format!("Daily Limit: {}\n\n", 
        if permissions.daily_opportunity_limit > 100 { 
            "Unlimited".to_string() 
        } else { 
            permissions.daily_opportunity_limit.to_string() 
        }
    ));

    // Account Activity
    message.push_str("🎯 *Account Activity*\n");
    message.push_str("Commands Used: Coming soon\n");
    message.push_str("Features Accessed: Coming soon\n");
    message.push_str("Success Rate: Coming soon\n\n");

    // Performance Metrics (Beta)
    if permissions.beta_access {
        message.push_str("🧪 *Beta Analytics*\n");
        message.push_str("Advanced metrics available in beta!\n");
        message.push_str("Use `/beta analytics` for detailed insights.\n\n");
    }

    // Recommendations
    message.push_str("💡 *Recommendations*\n");
    if permissions.subscription_tier == "free" {
        message.push_str("• Consider upgrading for unlimited access\n");
        message.push_str("• Enable notifications for real-time alerts\n");
    }
    message.push_str("• Complete your profile for better experience\n");
    message.push_str("• Join our community for tips and updates");

    Ok(message)
}

/// Display subscription status
async fn display_subscription_status(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("💎 Displaying subscription status for user {}", user_info.user_id);

    let mut message = String::from("💎 *Subscription Status*\n\n");
    
    // Current Subscription
    message.push_str("📋 *Current Plan*\n");
    message.push_str(&format!("Tier: {}\n", permissions.subscription_tier.to_uppercase()));
    message.push_str(&format!("Status: {}\n", 
        if permissions.subscription_tier == "free" { "Free Plan" } else { "✅ Active" }
    ));
    message.push_str("\n");

    // Current Benefits
    message.push_str("🎯 *Current Benefits*\n");
    match permissions.subscription_tier.as_str() {
        "free" => {
            message.push_str("• 3 opportunities per day\n");
            message.push_str("• 5-minute delay on alerts\n");
            message.push_str("• Basic support\n");
            message.push_str("• Community access\n");
        }
        "premium" => {
            message.push_str("• ✅ Unlimited opportunities\n");
            message.push_str("• ✅ Real-time alerts\n");
            message.push_str("• ✅ Advanced analytics\n");
            message.push_str("• ✅ Priority support\n");
            message.push_str("• ✅ Beta access\n");
        }
        "enterprise" => {
            message.push_str("• ✅ All premium features\n");
            message.push_str("• ✅ Custom integrations\n");
            message.push_str("• ✅ Dedicated support\n");
            message.push_str("• ✅ Team management\n");
            message.push_str("• ✅ White-label options\n");
        }
        _ => {
            message.push_str("• Custom plan benefits\n");
        }
    }
    message.push_str("\n");

    // Usage Summary
    message.push_str("📊 *Usage Summary*\n");
    message.push_str(&format!("Daily Limit: {}\n", 
        if permissions.daily_opportunity_limit > 100 { 
            "Unlimited".to_string() 
        } else { 
            permissions.daily_opportunity_limit.to_string() 
        }
    ));
    message.push_str(&format!("Beta Access: {}\n", 
        if permissions.beta_access { "✅ Active" } else { "❌ Not Available" }
    ));
    message.push_str(&format!("Trading: {}\n", 
        if permissions.can_trade { "✅ Enabled" } else { "❌ Add API keys" }
    ));
    message.push_str("\n");

    // Actions
    message.push_str("🚀 *Actions*\n");
    if permissions.subscription_tier == "free" {
        message.push_str("• `/subscription upgrade` - View upgrade options\n");
    }
    message.push_str("• `/subscription benefits` - Compare all plans\n");
    message.push_str("• `/profile` - View full profile\n");
    message.push_str("• `/settings` - Configure preferences");

    Ok(message)
}

/// Display upgrade options
async fn display_upgrade_options(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("⬆️ Displaying upgrade options for user {}", user_info.user_id);

    let mut message = String::from("⬆️ *Upgrade Your Plan*\n\n");
    
    if permissions.subscription_tier != "free" {
        message.push_str("✅ You already have a premium subscription!\n\n");
        message.push_str("🎯 *Your Current Benefits*\n");
        message.push_str("• Unlimited opportunities\n");
        message.push_str("• Real-time notifications\n");
        message.push_str("• Advanced analytics\n");
        message.push_str("• Priority support\n\n");
        message.push_str("Thank you for being a premium member! 🙏");
        return Ok(message);
    }

    // Premium Plan
    message.push_str("💎 *Premium Plan - $29/month*\n");
    message.push_str("✅ Unlimited opportunities\n");
    message.push_str("✅ Real-time notifications\n");
    message.push_str("✅ Advanced analytics\n");
    message.push_str("✅ Priority support\n");
    message.push_str("✅ Beta access\n");
    message.push_str("✅ API integrations\n\n");

    // Enterprise Plan
    message.push_str("🏢 *Enterprise Plan - $99/month*\n");
    message.push_str("✅ All premium features\n");
    message.push_str("✅ Team management\n");
    message.push_str("✅ Custom integrations\n");
    message.push_str("✅ Dedicated support\n");
    message.push_str("✅ White-label options\n");
    message.push_str("✅ SLA guarantees\n\n");

    // Special Offers
    message.push_str("🎁 *Special Offers*\n");
    message.push_str("• 🎓 Student discount: 50% off\n");
    message.push_str("• 📅 Annual plans: 2 months free\n");
    message.push_str("• 🎯 First month: 50% off\n\n");

    // Next Steps
    message.push_str("🚀 *Ready to Upgrade?*\n");
    message.push_str("Contact our team to get started:\n");
    message.push_str("• Email: upgrade@arbedge.com\n");
    message.push_str("• Telegram: @arbedge_support\n\n");
    message.push_str("💡 Questions? Use `/subscription benefits` to compare plans!");

    Ok(message)
}

/// Display subscription benefits comparison
async fn display_subscription_benefits(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("📋 Displaying subscription benefits for user {}", user_info.user_id);

    let message = String::from(
        "📋 *Plan Comparison*\n\n\
        🆓 *Free Plan*\n\
        • 3 opportunities/day\n\
        • 5-minute delay\n\
        • Basic support\n\
        • Community access\n\n\
        💎 *Premium Plan - $29/month*\n\
        • ✅ Unlimited opportunities\n\
        • ✅ Real-time alerts\n\
        • ✅ Advanced analytics\n\
        • ✅ Priority support\n\
        • ✅ Beta access\n\
        • ✅ API integrations\n\n\
        🏢 *Enterprise Plan - $99/month*\n\
        • ✅ All premium features\n\
        • ✅ Team management (5+ users)\n\
        • ✅ Custom integrations\n\
        • ✅ Dedicated support\n\
        • ✅ White-label options\n\
        • ✅ SLA guarantees\n\n\
        🎯 *Which Plan is Right for You?*\n\
        • Individual traders → Premium\n\
        • Trading teams → Enterprise\n\
        • Just starting → Free (then upgrade)\n\n\
        💡 All plans include our core arbitrage detection!"
    );

    Ok(message)
}

/// Edit user profile (placeholder for future implementation)
async fn edit_user_profile(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
    args: &[&str],
) -> ArbitrageResult<String> {
    console_log!("✏️ Edit profile request for user {}", user_info.user_id);

    let message = String::from(
        "✏️ *Profile Editing*\n\n\
        Profile editing is coming soon!\n\n\
        🎯 *What you'll be able to edit*\n\
        • Display name\n\
        • Email address\n\
        • Notification preferences\n\
        • Trading preferences\n\
        • API configurations\n\n\
        💡 For now, use `/settings` to configure preferences."
    );

    Ok(message)
}

// Helper trait for string formatting
trait ToTitleCase {
    fn to_title_case(&self) -> String;
}

impl ToTitleCase for str {
    fn to_title_case(&self) -> String {
        self.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
} 