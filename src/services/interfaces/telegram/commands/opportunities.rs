//! Opportunities Commands
//! 
//! Priority 3: Global Opportunities & Beta Features
//! - Global opportunities using our keys
//! - Beta feature access
//! - User-specific opportunity filtering
//! - Opportunity analytics

use crate::services::interfaces::telegram::{UserInfo, UserPermissions};
use crate::services::core::infrastructure::service_container::ServiceContainer;
use crate::services::core::opportunities::opportunity_engine::OpportunityEngine;
use crate::services::core::opportunities::opportunity_distribution::OpportunityDistributionService;
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::console_log;
use std::sync::Arc;
use serde_json::Value;

/// Handle /opportunities command - View global trading opportunities
pub async fn handle_opportunities_command(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
    args: &[&str],
) -> ArbitrageResult<String> {
    console_log!("💰 Opportunities command for user {} with role {:?}", user_info.user_id, permissions.role);

    // Check user's daily limit
    let remaining_limit = check_daily_opportunity_limit(service_container, user_info, permissions).await?;
    
    if remaining_limit <= 0 {
        return Ok(format!(
            "📊 *Daily Limit Reached*\n\n\
            You've reached your daily opportunity limit of {}.\n\n\
            💎 Upgrade to Premium for unlimited opportunities!\n\
            Use `/subscription` to learn more.",
            permissions.daily_opportunity_limit
        ));
    }

    // Get global opportunities using our keys
    let opportunities = get_global_opportunities(service_container, user_info, permissions).await?;
    
    // Format opportunities message
    let message = format_opportunities_message(&opportunities, permissions, remaining_limit).await?;
    
    // Track opportunity access
    track_opportunity_access(service_container, user_info).await?;
    
    console_log!("✅ Opportunities delivered to user {}", user_info.user_id);
    Ok(message)
}

/// Handle /beta command - Access beta features
pub async fn handle_beta_command(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
    args: &[&str],
) -> ArbitrageResult<String> {
    console_log!("🧪 Beta command for user {} with beta access: {}", user_info.user_id, permissions.beta_access);

    if !permissions.beta_access {
        return Ok("❌ *Beta Access Required*\n\nYou don't have access to beta features.\nContact admin for beta invitation.".to_string());
    }

    // Handle beta subcommands
    let subcommand = args.get(0).unwrap_or(&"menu");
    
    match *subcommand {
        "menu" | "" => generate_beta_menu(permissions).await,
        "opportunities" => get_beta_opportunities(service_container, user_info, permissions).await,
        "ai" => get_beta_ai_features(service_container, user_info, permissions).await,
        "analytics" => get_beta_analytics(service_container, user_info, permissions).await,
        _ => Ok("❓ Unknown beta command. Use `/beta` to see available options.".to_string()),
    }
}

/// Check user's daily opportunity limit
async fn check_daily_opportunity_limit(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<i32> {
    console_log!("📊 Checking daily limit for user {}", user_info.user_id);

    // For premium users, return unlimited
    if permissions.subscription_tier != "free" {
        return Ok(999); // Effectively unlimited
    }

    // Get opportunity distribution service
    let distribution_service = service_container
        .get_opportunity_distribution_service()
        .ok_or_else(|| ArbitrageError::service_unavailable("Opportunity distribution service not available"))?;

    // Check today's usage
    let user_id_str = user_info.user_id.to_string();
    let today_usage = distribution_service.get_daily_usage(&user_id_str).await.unwrap_or(0);
    
    let remaining = permissions.daily_opportunity_limit - today_usage;
    console_log!("📊 User {} has {} opportunities remaining today", user_info.user_id, remaining);
    
    Ok(remaining.max(0))
}

/// Get global opportunities using our keys
async fn get_global_opportunities(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<Vec<GlobalOpportunity>> {
    console_log!("🌍 Fetching global opportunities for user {}", user_info.user_id);

    // Get opportunity engine
    let opportunity_engine = service_container
        .get_opportunity_engine()
        .ok_or_else(|| ArbitrageError::service_unavailable("Opportunity engine not available"))?;

    // Get opportunities based on user's subscription tier
    let limit = if permissions.subscription_tier == "free" { 3 } else { 10 };
    
    // Fetch opportunities using our global API keys
    let opportunities = opportunity_engine
        .get_global_opportunities(limit)
        .await
        .unwrap_or_else(|e| {
            console_log!("⚠️ Failed to fetch global opportunities: {:?}", e);
            vec![]
        });

    console_log!("✅ Found {} global opportunities for user {}", opportunities.len(), user_info.user_id);
    Ok(opportunities)
}

/// Format opportunities message
async fn format_opportunities_message(
    opportunities: &[GlobalOpportunity],
    permissions: &UserPermissions,
    remaining_limit: i32,
) -> ArbitrageResult<String> {
    let mut message = String::from("💰 *Global Trading Opportunities*\n\n");
    
    // Add user status
    message.push_str(&format!(
        "👤 *Your Status*: {} | Remaining: {}\n\n",
        permissions.subscription_tier.to_uppercase(),
        if remaining_limit > 100 { "Unlimited".to_string() } else { remaining_limit.to_string() }
    ));

    if opportunities.is_empty() {
        message.push_str("📭 *No opportunities available right now*\n\n");
        message.push_str("🔄 Opportunities are updated every few minutes.\n");
        message.push_str("Try again shortly or enable notifications in `/settings`.");
        return Ok(message);
    }

    // Add opportunities
    for (index, opportunity) in opportunities.iter().enumerate() {
        message.push_str(&format!("🚀 *Opportunity {}*\n", index + 1));
        message.push_str(&format!("💰 Symbol: `{}`\n", opportunity.symbol));
        message.push_str(&format!("📊 Profit: `{:.2}%`\n", opportunity.profit_percentage));
        message.push_str(&format!("🏪 Exchanges: {} ↔️ {}\n", opportunity.buy_exchange, opportunity.sell_exchange));
        message.push_str(&format!("💵 Min Amount: `${:.2}`\n", opportunity.min_amount));
        message.push_str(&format!("⏰ Updated: {} ago\n\n", format_time_ago(opportunity.updated_at)));
    }

    // Add action buttons info
    message.push_str("🎯 *Quick Actions*\n");
    message.push_str("• Use inline buttons below for quick actions\n");
    message.push_str("• `/opportunities` to refresh\n");
    message.push_str("• `/settings` to configure notifications\n\n");

    // Premium upsell for free users
    if permissions.subscription_tier == "free" {
        message.push_str("💎 *Upgrade to Premium*\n");
        message.push_str("• Unlimited opportunities\n");
        message.push_str("• Real-time notifications\n");
        message.push_str("• Advanced analytics\n");
        message.push_str("Use `/subscription` to upgrade!");
    }

    Ok(message)
}

/// Track opportunity access for analytics
async fn track_opportunity_access(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
) -> ArbitrageResult<()> {
    console_log!("📈 Tracking opportunity access for user {}", user_info.user_id);

    // Get opportunity distribution service
    let distribution_service = service_container
        .get_opportunity_distribution_service()
        .ok_or_else(|| ArbitrageError::service_unavailable("Opportunity distribution service not available"))?;

    // Track the access
    let user_id_str = user_info.user_id.to_string();
    distribution_service.track_opportunity_access(&user_id_str).await?;

    Ok(())
}

/// Generate beta menu
async fn generate_beta_menu(permissions: &UserPermissions) -> ArbitrageResult<String> {
    let mut message = String::from("🧪 *Beta Features Menu*\n\n");
    
    message.push_str("Welcome to ArbEdge Beta! You have access to:\n\n");
    
    message.push_str("🚀 *Available Beta Features*\n");
    message.push_str("• `/beta opportunities` - Enhanced opportunity analysis\n");
    message.push_str("• `/beta ai` - Advanced AI features\n");
    message.push_str("• `/beta analytics` - Detailed performance analytics\n\n");
    
    message.push_str("🎯 *Beta Benefits*\n");
    message.push_str("• Priority access to new features\n");
    message.push_str("• Enhanced opportunity scoring\n");
    message.push_str("• Advanced AI analysis\n");
    message.push_str("• Detailed performance tracking\n\n");
    
    message.push_str("💡 *Feedback*\n");
    message.push_str("Help us improve! Share your feedback with the admin team.\n");
    message.push_str("Your input shapes the future of ArbEdge.");
    
    Ok(message)
}

/// Get beta opportunities with enhanced analysis
async fn get_beta_opportunities(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("🧪 Beta opportunities for user {}", user_info.user_id);

    // Get enhanced opportunities for beta users
    let opportunities = get_global_opportunities(service_container, user_info, permissions).await?;
    
    let mut message = String::from("🧪 *Beta Enhanced Opportunities*\n\n");
    
    if opportunities.is_empty() {
        message.push_str("📭 No opportunities available right now.\n");
        message.push_str("Beta users get priority access when opportunities are available!");
        return Ok(message);
    }

    // Enhanced formatting for beta users
    for (index, opportunity) in opportunities.iter().enumerate() {
        message.push_str(&format!("🚀 *Enhanced Opportunity {}*\n", index + 1));
        message.push_str(&format!("💰 Symbol: `{}`\n", opportunity.symbol));
        message.push_str(&format!("📊 Profit: `{:.2}%`\n", opportunity.profit_percentage));
        message.push_str(&format!("🏪 Exchanges: {} ↔️ {}\n", opportunity.buy_exchange, opportunity.sell_exchange));
        message.push_str(&format!("💵 Min Amount: `${:.2}`\n", opportunity.min_amount));
        message.push_str(&format!("🎯 Risk Score: `{}/10`\n", opportunity.risk_score.unwrap_or(5)));
        message.push_str(&format!("📈 Confidence: `{:.1}%`\n", opportunity.confidence.unwrap_or(75.0)));
        message.push_str(&format!("⏰ Updated: {} ago\n\n", format_time_ago(opportunity.updated_at)));
    }

    message.push_str("🧪 *Beta Enhancement*\n");
    message.push_str("• Risk scoring and confidence levels\n");
    message.push_str("• Priority opportunity access\n");
    message.push_str("• Enhanced market analysis");

    Ok(message)
}

/// Get beta AI features
async fn get_beta_ai_features(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("🤖 Beta AI features for user {}", user_info.user_id);

    let message = String::from(
        "🤖 *Beta AI Features*\n\n\
        🧠 *Available AI Tools*\n\
        • Advanced market sentiment analysis\n\
        • Predictive opportunity scoring\n\
        • Risk assessment algorithms\n\
        • Portfolio optimization suggestions\n\n\
        🎯 *Coming Soon*\n\
        • Custom AI model integration\n\
        • Automated trading strategies\n\
        • Real-time market predictions\n\
        • Personalized trading insights\n\n\
        💡 *Beta Status*\n\
        AI features are currently in development.\n\
        Beta users will get first access when ready!"
    );

    Ok(message)
}

/// Get beta analytics
async fn get_beta_analytics(
    service_container: &Arc<ServiceContainer>,
    user_info: &UserInfo,
    permissions: &UserPermissions,
) -> ArbitrageResult<String> {
    console_log!("📊 Beta analytics for user {}", user_info.user_id);

    let message = String::from(
        "📊 *Beta Analytics Dashboard*\n\n\
        📈 *Performance Metrics*\n\
        • Opportunity success rate tracking\n\
        • Profit/loss analysis\n\
        • Market timing insights\n\
        • Risk-adjusted returns\n\n\
        🎯 *Advanced Features*\n\
        • Multi-timeframe analysis\n\
        • Comparative performance\n\
        • Market correlation insights\n\
        • Predictive analytics\n\n\
        🧪 *Beta Access*\n\
        Enhanced analytics are being developed.\n\
        You'll get early access as features become available!"
    );

    Ok(message)
}

/// Format time ago helper
fn format_time_ago(timestamp: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(timestamp);
    
    if duration.num_minutes() < 1 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{}m", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h", duration.num_hours())
    } else {
        format!("{}d", duration.num_days())
    }
}

// Temporary structure for global opportunities (will be replaced with actual types)
#[derive(Debug, Clone)]
pub struct GlobalOpportunity {
    pub symbol: String,
    pub profit_percentage: f64,
    pub buy_exchange: String,
    pub sell_exchange: String,
    pub min_amount: f64,
    pub risk_score: Option<u8>,
    pub confidence: Option<f64>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
} 