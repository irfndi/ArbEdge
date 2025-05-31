//! Role-Based Access Control (RBAC) Service
//! 
//! Comprehensive RBAC system supporting:
//! - Role management (SuperAdmin, Admin, Premium, Basic, Free)
//! - Permission checking and validation
//! - Subscription tier integration
//! - Feature gating based on roles and subscriptions
//! - Dynamic permission assignment

use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::services::core::user::UserProfileService;
use crate::types::{UserRole, UserProfile};
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::console_log;
use std::sync::Arc;
use std::collections::HashMap;

/// RBAC Service for role and permission management
pub struct RBACService {
    role_manager: RoleManager,
    permission_manager: PermissionManager,
    // TODO: Add user profile service dependency injection when needed
}

impl RBACService {
    /// Create new RBAC service
    pub async fn new(service_container: &Arc<ServiceContainer>) -> ArbitrageResult<Self> {
        console_log!("👑 Initializing RBAC Service...");

        let role_manager = RoleManager::new();
        let permission_manager = PermissionManager::new();

        console_log!("✅ RBAC Service initialized successfully");

        Ok(Self {
            role_manager,
            permission_manager,
        })
    }

    /// Create new RBAC service without service container dependency
    pub async fn new_standalone() -> ArbitrageResult<Self> {
        console_log!("👑 Initializing Standalone RBAC Service...");

        let role_manager = RoleManager::new();
        let permission_manager = PermissionManager::new();

        console_log!("✅ Standalone RBAC Service initialized successfully");

        Ok(Self {
            role_manager,
            permission_manager,
        })
    }

    /// Get user permissions based on profile
    pub async fn get_user_permissions(&self, user_profile: &UserProfile) -> ArbitrageResult<super::UserPermissions> {
        console_log!("👑 Getting permissions for user {} with role {:?}", user_profile.user_id, user_profile.role);

        // Get role-based permissions
        let role_permissions = self.role_manager.get_role_permissions(&user_profile.role);

        // Get subscription-based permissions
        let subscription_permissions = self.permission_manager.get_subscription_permissions(&user_profile.subscription_tier);

        // Combine permissions
        let mut all_permissions = role_permissions;
        all_permissions.extend(subscription_permissions);

        // Check if user is admin
        let is_admin = matches!(user_profile.role, UserRole::SuperAdmin | UserRole::Admin);

        // Determine daily opportunity limit
        let daily_opportunity_limit = self.get_daily_opportunity_limit(user_profile);

        console_log!("✅ Loaded {} permissions for user {}", all_permissions.len(), user_profile.user_id);

        Ok(super::UserPermissions {
            role: user_profile.role.clone(),
            subscription_tier: user_profile.subscription_tier.clone(),
            can_trade: user_profile.can_trade,
            daily_opportunity_limit,
            permissions: all_permissions,
            is_admin,
        })
    }

    /// Check if user has specific permission
    pub async fn check_permission(&self, user_profile: &UserProfile, permission: &str) -> ArbitrageResult<bool> {
        console_log!("🔍 Checking permission '{}' for user {} with role {:?}", permission, user_profile.user_id, user_profile.role);

        // Super admin has all permissions
        if matches!(user_profile.role, UserRole::SuperAdmin) {
            console_log!("✅ Super admin access granted for permission '{}'", permission);
            return Ok(true);
        }

        // Get user permissions
        let user_permissions = self.get_user_permissions(user_profile).await?;

        // Check if permission exists
        let has_permission = user_permissions.permissions.contains(&permission.to_string());

        console_log!("✅ Permission check result: {}", has_permission);
        Ok(has_permission)
    }

    /// Get daily opportunity limit based on role and subscription
    fn get_daily_opportunity_limit(&self, user_profile: &UserProfile) -> i32 {
        // Role-based limits
        let role_limit = match user_profile.role {
            UserRole::SuperAdmin => 999, // Unlimited
            UserRole::Admin => 999,       // Unlimited
            UserRole::Premium => 999,     // Unlimited
            UserRole::Basic => 10,        // Basic limit
            UserRole::Free => 3,          // Free limit
        };

        // Subscription-based limits
        let subscription_limit = match user_profile.subscription_tier.as_str() {
            "enterprise" => 999,  // Unlimited
            "premium" => 999,     // Unlimited
            "basic" => 10,        // Basic limit
            "free" => 3,          // Free limit
            _ => 3,               // Default to free
        };

        // Use the higher of the two limits
        role_limit.max(subscription_limit).max(user_profile.daily_opportunity_limit)
    }
}

/// Role Manager for handling user roles
pub struct RoleManager {
    role_permissions: HashMap<UserRole, Vec<String>>,
}

impl RoleManager {
    /// Create new role manager with predefined permissions
    pub fn new() -> Self {
        let mut role_permissions = HashMap::new();

        // Super Admin - All permissions
        role_permissions.insert(UserRole::SuperAdmin, vec![
            // System administration
            "system.admin".to_string(),
            "system.config".to_string(),
            "system.monitoring".to_string(),
            "system.maintenance".to_string(),
            
            // User management
            "users.create".to_string(),
            "users.read".to_string(),
            "users.update".to_string(),
            "users.delete".to_string(),
            "users.manage_roles".to_string(),
            
            // Trading features
            "trading.manual".to_string(),
            "trading.automated".to_string(),
            "trading.advanced".to_string(),
            "trading.unlimited".to_string(),
            
            // Opportunities
            "opportunities.unlimited".to_string(),
            "opportunities.realtime".to_string(),
            "opportunities.priority".to_string(),
            "opportunities.global".to_string(),
            
            // AI features
            "ai.enhanced".to_string(),
            "ai.custom".to_string(),
            "ai.unlimited".to_string(),
            
            // Analytics
            "analytics.advanced".to_string(),
            "analytics.admin".to_string(),
            "analytics.export".to_string(),
            
            // Beta features
            "beta.access".to_string(),
            "beta.admin".to_string(),
        ]);

        // Admin - Administrative permissions
        role_permissions.insert(UserRole::Admin, vec![
            // User support
            "users.read".to_string(),
            "users.update".to_string(),
            "users.support".to_string(),
            
            // System monitoring
            "system.monitoring".to_string(),
            "system.health".to_string(),
            
            // Trading features
            "trading.manual".to_string(),
            "trading.automated".to_string(),
            "trading.unlimited".to_string(),
            
            // Opportunities
            "opportunities.unlimited".to_string(),
            "opportunities.realtime".to_string(),
            "opportunities.priority".to_string(),
            
            // AI features
            "ai.enhanced".to_string(),
            "ai.unlimited".to_string(),
            
            // Analytics
            "analytics.advanced".to_string(),
            "analytics.support".to_string(),
            
            // Beta features
            "beta.access".to_string(),
        ]);

        // Premium - Premium user permissions
        role_permissions.insert(UserRole::Premium, vec![
            // Trading features
            "trading.manual".to_string(),
            "trading.automated".to_string(),
            
            // Opportunities
            "opportunities.unlimited".to_string(),
            "opportunities.realtime".to_string(),
            
            // AI features
            "ai.enhanced".to_string(),
            "ai.custom".to_string(),
            
            // Analytics
            "analytics.advanced".to_string(),
            
            // Beta features
            "beta.access".to_string(),
        ]);

        // Basic - Basic user permissions
        role_permissions.insert(UserRole::Basic, vec![
            // Trading features
            "trading.manual".to_string(),
            
            // Opportunities
            "opportunities.limited".to_string(),
            "opportunities.delayed".to_string(),
            
            // AI features
            "ai.basic".to_string(),
            
            // Analytics
            "analytics.basic".to_string(),
        ]);

        // Free - Free user permissions
        role_permissions.insert(UserRole::Free, vec![
            // Opportunities
            "opportunities.limited".to_string(),
            "opportunities.delayed".to_string(),
            
            // Analytics
            "analytics.basic".to_string(),
        ]);

        Self { role_permissions }
    }

    /// Get permissions for a specific role
    pub fn get_role_permissions(&self, role: &UserRole) -> Vec<String> {
        self.role_permissions
            .get(role)
            .cloned()
            .unwrap_or_else(Vec::new)
    }

    /// Check if role has specific permission
    pub fn role_has_permission(&self, role: &UserRole, permission: &str) -> bool {
        if let Some(permissions) = self.role_permissions.get(role) {
            permissions.contains(&permission.to_string())
        } else {
            false
        }
    }
}

/// Permission Manager for handling subscription-based permissions
pub struct PermissionManager {
    subscription_permissions: HashMap<String, Vec<String>>,
}

impl PermissionManager {
    /// Create new permission manager with subscription permissions
    pub fn new() -> Self {
        let mut subscription_permissions = HashMap::new();

        // Enterprise subscription
        subscription_permissions.insert("enterprise".to_string(), vec![
            "subscription.enterprise".to_string(),
            "team.management".to_string(),
            "api.custom".to_string(),
            "support.dedicated".to_string(),
            "sla.guaranteed".to_string(),
            "whitelabel.access".to_string(),
        ]);

        // Premium subscription
        subscription_permissions.insert("premium".to_string(), vec![
            "subscription.premium".to_string(),
            "notifications.realtime".to_string(),
            "analytics.advanced".to_string(),
            "support.priority".to_string(),
            "api.integrations".to_string(),
        ]);

        // Basic subscription
        subscription_permissions.insert("basic".to_string(), vec![
            "subscription.basic".to_string(),
            "notifications.standard".to_string(),
            "analytics.standard".to_string(),
            "support.standard".to_string(),
        ]);

        // Free subscription
        subscription_permissions.insert("free".to_string(), vec![
            "subscription.free".to_string(),
            "notifications.limited".to_string(),
            "support.community".to_string(),
        ]);

        Self { subscription_permissions }
    }

    /// Get permissions for a subscription tier
    pub fn get_subscription_permissions(&self, subscription_tier: &str) -> Vec<String> {
        self.subscription_permissions
            .get(subscription_tier)
            .cloned()
            .unwrap_or_else(|| {
                // Default to free permissions
                self.subscription_permissions
                    .get("free")
                    .cloned()
                    .unwrap_or_else(Vec::new)
            })
    }

    /// Check if subscription has specific permission
    pub fn subscription_has_permission(&self, subscription_tier: &str, permission: &str) -> bool {
        if let Some(permissions) = self.subscription_permissions.get(subscription_tier) {
            permissions.contains(&permission.to_string())
        } else {
            false
        }
    }
}

/// Permission constants for easy reference
pub mod permissions {
    // System permissions
    pub const SYSTEM_ADMIN: &str = "system.admin";
    pub const SYSTEM_CONFIG: &str = "system.config";
    pub const SYSTEM_MONITORING: &str = "system.monitoring";

    // User management permissions
    pub const USERS_CREATE: &str = "users.create";
    pub const USERS_READ: &str = "users.read";
    pub const USERS_UPDATE: &str = "users.update";
    pub const USERS_DELETE: &str = "users.delete";
    pub const USERS_MANAGE_ROLES: &str = "users.manage_roles";

    // Trading permissions
    pub const TRADING_MANUAL: &str = "trading.manual";
    pub const TRADING_AUTOMATED: &str = "trading.automated";
    pub const TRADING_ADVANCED: &str = "trading.advanced";
    pub const TRADING_UNLIMITED: &str = "trading.unlimited";

    // Opportunity permissions
    pub const OPPORTUNITIES_LIMITED: &str = "opportunities.limited";
    pub const OPPORTUNITIES_UNLIMITED: &str = "opportunities.unlimited";
    pub const OPPORTUNITIES_REALTIME: &str = "opportunities.realtime";
    pub const OPPORTUNITIES_PRIORITY: &str = "opportunities.priority";
    pub const OPPORTUNITIES_GLOBAL: &str = "opportunities.global";

    // AI permissions
    pub const AI_BASIC: &str = "ai.basic";
    pub const AI_ENHANCED: &str = "ai.enhanced";
    pub const AI_CUSTOM: &str = "ai.custom";
    pub const AI_UNLIMITED: &str = "ai.unlimited";

    // Analytics permissions
    pub const ANALYTICS_BASIC: &str = "analytics.basic";
    pub const ANALYTICS_ADVANCED: &str = "analytics.advanced";
    pub const ANALYTICS_ADMIN: &str = "analytics.admin";
    pub const ANALYTICS_EXPORT: &str = "analytics.export";

    // Beta permissions
    pub const BETA_ACCESS: &str = "beta.access";
    pub const BETA_ADMIN: &str = "beta.admin";

    // Subscription permissions
    pub const SUBSCRIPTION_FREE: &str = "subscription.free";
    pub const SUBSCRIPTION_BASIC: &str = "subscription.basic";
    pub const SUBSCRIPTION_PREMIUM: &str = "subscription.premium";
    pub const SUBSCRIPTION_ENTERPRISE: &str = "subscription.enterprise";
} 