use crate::types;
use worker::{Env, Result};

/// Get development user tier for testing purposes
pub async fn get_development_user_tier(
    user_id: &str,
    env: &Env,
) -> Option<types::SubscriptionTier> {
    // Check if fallback permissions are enabled
    let fallback_enabled = env
        .var("ENABLE_FALLBACK_PERMISSIONS")
        .map(|v| v.to_string() == "true")
        .unwrap_or(false);

    if !fallback_enabled {
        return None;
    }

    // Additional production safeguards
    let is_production = env
        .var("ENVIRONMENT")
        .map(|v| v.to_string() == "production")
        .unwrap_or(false);

    if is_production {
        // Require explicit production override flags
        let allow_fallback_in_prod = env
            .var("ALLOW_FALLBACK_IN_PRODUCTION")
            .map(|v| v.to_string() == "true")
            .unwrap_or(false);

        let confirm_security_risk = env
            .var("CONFIRM_FALLBACK_SECURITY_RISK")
            .map(|v| v.to_string() == "true")
            .unwrap_or(false);

        if !allow_fallback_in_prod || !confirm_security_risk {
            worker::console_log!(
                "⚠️ SECURITY WARNING: Fallback permissions blocked in production environment. User: {}",
                user_id
            );
            return None;
        }

        worker::console_log!(
            "🚨 PRODUCTION SECURITY OVERRIDE: Using fallback permissions for user: {}",
            user_id
        );
    }

    // Development user seeding with secure patterns
    match user_id {
        id if id.starts_with("dev_admin_") => Some(types::SubscriptionTier::Admin),
        id if id.starts_with("dev_enterprise_") => Some(types::SubscriptionTier::Enterprise),
        id if id.starts_with("dev_pro_") => Some(types::SubscriptionTier::Pro),
        id if id.starts_with("dev_premium_") => Some(types::SubscriptionTier::Premium),
        id if id.starts_with("dev_basic_") => Some(types::SubscriptionTier::Basic),
        _ => None,
    }
}

/// Check if user has required permissions for an endpoint
pub async fn check_user_permissions(user_id: &str, required_tier: &str, env: &Env) -> Result<bool> {
    // Try to get user tier from KV store first
    let kv = env.kv("ArbEdgeKV")?;
    let user_tier_key = format!("user_tier:{}", user_id);

    let user_tier = if let Some(tier_str) = kv.get(&user_tier_key).text().await? {
        match tier_str.as_str() {
            "Free" => types::SubscriptionTier::Free,
            "Basic" => types::SubscriptionTier::Basic,
            "Premium" => types::SubscriptionTier::Premium,
            "Enterprise" => types::SubscriptionTier::Enterprise,
            "Pro" => types::SubscriptionTier::Pro,
            "Admin" => types::SubscriptionTier::Admin,
            "SuperAdmin" => types::SubscriptionTier::SuperAdmin,
            _ => types::SubscriptionTier::Free,
        }
    } else if let Some(dev_tier) = get_development_user_tier(user_id, env).await {
        // Store development tier in KV for consistency
        let tier_str = match dev_tier {
            types::SubscriptionTier::Free => "Free",
            types::SubscriptionTier::Basic => "Basic",
            types::SubscriptionTier::Premium => "Premium",
            types::SubscriptionTier::Enterprise => "Enterprise",
            types::SubscriptionTier::Pro => "Pro",
            types::SubscriptionTier::Admin => "Admin",
            types::SubscriptionTier::SuperAdmin => "SuperAdmin",
        };
        kv.put(&user_tier_key, tier_str)?
            .expiration_ttl(3600)
            .execute()
            .await?;
        dev_tier
    } else {
        types::SubscriptionTier::Free
    };

    Ok(check_subscription_tier_permission(
        &user_tier,
        required_tier,
    ))
}

/// Check if subscription tier has permission for required tier
pub fn check_subscription_tier_permission(
    user_tier: &types::SubscriptionTier,
    required_tier: &str,
) -> bool {
    let user_level = match user_tier {
        types::SubscriptionTier::Free => 0,
        types::SubscriptionTier::Basic => 1,
        types::SubscriptionTier::Premium => 2,
        types::SubscriptionTier::Enterprise => 3,
        types::SubscriptionTier::Pro => 4,
        types::SubscriptionTier::Admin => 5,
        types::SubscriptionTier::SuperAdmin => 6,
    };

    let required_level = match required_tier {
        "free" => 0,
        "basic" => 1,
        "premium" => 2,
        "enterprise" => 3,
        "pro" => 4,
        "admin" => 5,
        "superadmin" => 6,
        _ => 0,
    };

    user_level >= required_level
}
