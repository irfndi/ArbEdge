//! Permission Checking and Validation Service
//! 
//! Comprehensive permission checking system:
//! - Permission validation for user actions
//! - Subscription tier access control
//! - Feature gating based on roles and subscriptions
//! - Access validation for specific features

use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::types::{UserProfile, UserRole};
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::console_log;
use std::sync::Arc;

/// Permission Checker Service
pub struct PermissionChecker {
    // TODO: Add user profile service dependency injection when needed
}

impl PermissionChecker {
    /// Create new permission checker
    pub async fn new(service_container: &Arc<ServiceContainer>) -> ArbitrageResult<Self> {
        console_log!("🔍 Initializing Permission Checker...");

        console_log!("✅ Permission Checker initialized successfully");

        Ok(Self {
        })
    }

    /// Create new permission checker without service container dependency
    pub async fn new_standalone() -> ArbitrageResult<Self> {
        console_log!("🔍 Initializing Standalone Permission Checker...");

        console_log!("✅ Standalone Permission Checker initialized successfully");

        Ok(Self {
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

        // Check role-based permissions
        let has_role_permission = self.check_role_permission(&user_profile.role, permission);
        
        // Check subscription-based permissions
        let has_subscription_permission = self.check_subscription_permission(&user_profile.subscription_tier, permission);

        // Check beta access for beta features
        let has_beta_permission = if permission.starts_with("beta.") {
            user_profile.beta_access && !self.is_beta_expired(user_profile)
        } else {
            true // Non-beta permissions don't require beta access
        };

        let has_permission = has_role_permission || has_subscription_permission;
        let final_permission = has_permission && has_beta_permission;

        console_log!("✅ Permission check result: role={}, subscription={}, beta={}, final={}", 
                    has_role_permission, has_subscription_permission, has_beta_permission, final_permission);

        Ok(final_permission)
    }

    /// Check subscription access for specific feature
    pub async fn check_subscription_access(&self, user_profile: &UserProfile, feature: &str) -> ArbitrageResult<super::SubscriptionAccessResult> {
        console_log!("💎 Checking subscription access for user {} feature '{}'", user_profile.user_id, feature);

        let subscription_tier = &user_profile.subscription_tier;
        let has_access = self.check_feature_access(subscription_tier, feature);
        let limit_reached = self.check_daily_limit_reached(user_profile, feature).await;
        let upgrade_required = !has_access && !matches!(user_profile.role, UserRole::SuperAdmin | UserRole::Admin);

        console_log!("✅ Subscription access result: has_access={}, limit_reached={}, upgrade_required={}", 
                    has_access, limit_reached, upgrade_required);

        Ok(super::SubscriptionAccessResult {
            has_access: has_access && !limit_reached,
            subscription_tier: subscription_tier.clone(),
            feature: feature.to_string(),
            limit_reached,
            upgrade_required,
        })
    }

    /// Check role-based permission
    fn check_role_permission(&self, role: &UserRole, permission: &str) -> bool {
        match role {
            UserRole::SuperAdmin => true, // Super admin has all permissions
            UserRole::Admin => {
                // Admin permissions
                matches!(permission, 
                    "users.read" | "users.update" | "users.support" |
                    "system.monitoring" | "system.health" |
                    "trading.manual" | "trading.automated" | "trading.unlimited" |
                    "opportunities.unlimited" | "opportunities.realtime" | "opportunities.priority" |
                    "ai.enhanced" | "ai.unlimited" |
                    "analytics.advanced" | "analytics.support" |
                    "beta.access"
                )
            }
            UserRole::Premium => {
                // Premium user permissions
                matches!(permission,
                    "trading.manual" | "trading.automated" |
                    "opportunities.unlimited" | "opportunities.realtime" |
                    "ai.enhanced" | "ai.custom" |
                    "analytics.advanced" |
                    "beta.access"
                )
            }
            UserRole::Basic => {
                // Basic user permissions
                matches!(permission,
                    "trading.manual" |
                    "opportunities.limited" | "opportunities.delayed" |
                    "ai.basic" |
                    "analytics.basic"
                )
            }
            UserRole::Free => {
                // Free user permissions
                matches!(permission,
                    "opportunities.limited" | "opportunities.delayed" |
                    "analytics.basic"
                )
            }
        }
    }

    /// Check subscription-based permission
    fn check_subscription_permission(&self, subscription_tier: &str, permission: &str) -> bool {
        match subscription_tier {
            "enterprise" => {
                // Enterprise subscription permissions
                matches!(permission,
                    "subscription.enterprise" | "team.management" | "api.custom" |
                    "support.dedicated" | "sla.guaranteed" | "whitelabel.access"
                )
            }
            "premium" => {
                // Premium subscription permissions
                matches!(permission,
                    "subscription.premium" | "notifications.realtime" |
                    "analytics.advanced" | "support.priority" | "api.integrations"
                )
            }
            "basic" => {
                // Basic subscription permissions
                matches!(permission,
                    "subscription.basic" | "notifications.standard" |
                    "analytics.standard" | "support.standard"
                )
            }
            "free" => {
                // Free subscription permissions
                matches!(permission,
                    "subscription.free" | "notifications.limited" | "support.community"
                )
            }
            _ => false, // Unknown subscription tier
        }
    }

    /// Check if user has access to specific feature
    fn check_feature_access(&self, subscription_tier: &str, feature: &str) -> bool {
        match feature {
            "unlimited_opportunities" => {
                matches!(subscription_tier, "premium" | "enterprise")
            }
            "realtime_notifications" => {
                matches!(subscription_tier, "premium" | "enterprise")
            }
            "advanced_analytics" => {
                matches!(subscription_tier, "premium" | "enterprise")
            }
            "automated_trading" => {
                matches!(subscription_tier, "premium" | "enterprise")
            }
            "ai_enhanced" => {
                matches!(subscription_tier, "premium" | "enterprise")
            }
            "priority_support" => {
                matches!(subscription_tier, "premium" | "enterprise")
            }
            "team_management" => {
                matches!(subscription_tier, "enterprise")
            }
            "custom_integrations" => {
                matches!(subscription_tier, "enterprise")
            }
            "whitelabel" => {
                matches!(subscription_tier, "enterprise")
            }
            _ => true, // Default to allowing access for unknown features
        }
    }

    /// Check if daily limit is reached for feature
    async fn check_daily_limit_reached(&self, user_profile: &UserProfile, feature: &str) -> bool {
        // For now, implement basic limit checking
        // TODO: Integrate with actual usage tracking service
        
        match feature {
            "opportunities" => {
                // Check opportunity limit
                let daily_limit = user_profile.daily_opportunity_limit;
                if daily_limit >= 999 {
                    false // Unlimited
                } else {
                    // TODO: Check actual daily usage
                    false // For now, assume not reached
                }
            }
            _ => false, // No limits for other features
        }
    }

    /// Check if beta access is expired
    fn is_beta_expired(&self, user_profile: &UserProfile) -> bool {
        if let Some(expires_at) = user_profile.beta_expires_at {
            chrono::Utc::now() > expires_at
        } else {
            false // No expiration set
        }
    }
}

/// Access Validator for specific access patterns
pub struct AccessValidator {
    permission_checker: PermissionChecker,
}

impl AccessValidator {
    /// Create new access validator
    pub async fn new(service_container: &Arc<ServiceContainer>) -> ArbitrageResult<Self> {
        let permission_checker = PermissionChecker::new(service_container).await?;
        
        Ok(Self {
            permission_checker,
        })
    }

    /// Create new access validator without service container dependency
    pub async fn new_standalone() -> ArbitrageResult<Self> {
        let permission_checker = PermissionChecker::new_standalone().await?;
        
        Ok(Self {
            permission_checker,
        })
    }

    /// Validate trading access
    pub async fn validate_trading_access(&self, user_profile: &UserProfile, trading_type: &str) -> ArbitrageResult<bool> {
        console_log!("💰 Validating trading access for user {} type '{}'", user_profile.user_id, trading_type);

        // Check if user can trade
        if !user_profile.can_trade {
            console_log!("❌ User {} cannot trade - API keys not configured", user_profile.user_id);
            return Ok(false);
        }

        // Check trading permissions
        let permission = match trading_type {
            "manual" => "trading.manual",
            "automated" => "trading.automated",
            "advanced" => "trading.advanced",
            _ => return Err(ArbitrageError::validation_error(&format!("Unknown trading type: {}", trading_type))),
        };

        self.permission_checker.check_permission(user_profile, permission).await
    }

    /// Validate opportunity access
    pub async fn validate_opportunity_access(&self, user_profile: &UserProfile, opportunity_type: &str) -> ArbitrageResult<bool> {
        console_log!("💰 Validating opportunity access for user {} type '{}'", user_profile.user_id, opportunity_type);

        let permission = match opportunity_type {
            "limited" => "opportunities.limited",
            "unlimited" => "opportunities.unlimited",
            "realtime" => "opportunities.realtime",
            "priority" => "opportunities.priority",
            "global" => "opportunities.global",
            _ => return Err(ArbitrageError::validation_error(&format!("Unknown opportunity type: {}", opportunity_type))),
        };

        self.permission_checker.check_permission(user_profile, permission).await
    }

    /// Validate beta access
    pub async fn validate_beta_access(&self, user_profile: &UserProfile, beta_feature: &str) -> ArbitrageResult<bool> {
        console_log!("🧪 Validating beta access for user {} feature '{}'", user_profile.user_id, beta_feature);

        // Check if user has beta access
        if !user_profile.beta_access {
            console_log!("❌ User {} does not have beta access", user_profile.user_id);
            return Ok(false);
        }

        // Check if beta access is expired
        if self.permission_checker.is_beta_expired(user_profile) {
            console_log!("❌ User {} beta access is expired", user_profile.user_id);
            return Ok(false);
        }

        // Check specific beta permission
        let permission = format!("beta.{}", beta_feature);
        self.permission_checker.check_permission(user_profile, &permission).await
    }
}

/// Feature Gate for controlling access to features
pub struct FeatureGate {
    access_validator: AccessValidator,
}

impl FeatureGate {
    /// Create new feature gate
    pub async fn new(service_container: &Arc<ServiceContainer>) -> ArbitrageResult<Self> {
        let access_validator = AccessValidator::new(service_container).await?;
        
        Ok(Self {
            access_validator,
        })
    }

    /// Create new feature gate without service container dependency
    pub async fn new_standalone() -> ArbitrageResult<Self> {
        let access_validator = AccessValidator::new_standalone().await?;
        
        Ok(Self {
            access_validator,
        })
    }

    /// Check if feature is enabled for user
    pub async fn is_feature_enabled(&self, user_profile: &UserProfile, feature: &str) -> ArbitrageResult<bool> {
        console_log!("🚪 Checking feature gate for user {} feature '{}'", user_profile.user_id, feature);

        match feature {
            "trading_manual" => self.access_validator.validate_trading_access(user_profile, "manual").await,
            "trading_automated" => self.access_validator.validate_trading_access(user_profile, "automated").await,
            "opportunities_unlimited" => self.access_validator.validate_opportunity_access(user_profile, "unlimited").await,
            "opportunities_realtime" => self.access_validator.validate_opportunity_access(user_profile, "realtime").await,
            "beta_features" => self.access_validator.validate_beta_access(user_profile, "access").await,
            _ => {
                console_log!("❓ Unknown feature: {}", feature);
                Ok(false)
            }
        }
    }

    /// Get feature access summary for user
    pub async fn get_feature_access_summary(&self, user_profile: &UserProfile) -> ArbitrageResult<FeatureAccessSummary> {
        console_log!("📋 Getting feature access summary for user {}", user_profile.user_id);

        let trading_manual = self.is_feature_enabled(user_profile, "trading_manual").await?;
        let trading_automated = self.is_feature_enabled(user_profile, "trading_automated").await?;
        let opportunities_unlimited = self.is_feature_enabled(user_profile, "opportunities_unlimited").await?;
        let opportunities_realtime = self.is_feature_enabled(user_profile, "opportunities_realtime").await?;
        let beta_features = self.is_feature_enabled(user_profile, "beta_features").await?;

        Ok(FeatureAccessSummary {
            trading_manual,
            trading_automated,
            opportunities_unlimited,
            opportunities_realtime,
            beta_features,
            daily_opportunity_limit: user_profile.daily_opportunity_limit,
            can_trade: user_profile.can_trade,
        })
    }
}

/// Feature access summary
#[derive(Debug, Clone)]
pub struct FeatureAccessSummary {
    pub trading_manual: bool,
    pub trading_automated: bool,
    pub opportunities_unlimited: bool,
    pub opportunities_realtime: bool,
    pub beta_features: bool,
    pub daily_opportunity_limit: i32,
    pub can_trade: bool,
} 