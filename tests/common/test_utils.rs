use arb_edge::types::*;
use arb_edge::utils::feature_flags::FeatureFlagManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock KV store for testing
#[derive(Clone)]
pub struct MockKvStore {
    data: Arc<Mutex<HashMap<String, String>>>,
}

impl Default for MockKvStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MockKvStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn put(&self, key: &str, value: &str, _ttl: Option<u32>) -> Result<(), String> {
        let mut data = self.data.lock().unwrap();
        data.insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, String> {
        let data = self.data.lock().unwrap();
        Ok(data.get(key).cloned())
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        let mut data = self.data.lock().unwrap();
        data.remove(key);
        Ok(())
    }
}

/// Mock D1 database for testing
#[derive(Clone)]
pub struct MockD1Database {
    opportunities: Arc<Mutex<HashMap<String, ArbitrageOpportunity>>>,
    users: Arc<Mutex<HashMap<String, UserProfile>>>,
}

impl Default for MockD1Database {
    fn default() -> Self {
        Self::new()
    }
}

impl MockD1Database {
    pub fn new() -> Self {
        Self {
            opportunities: Arc::new(Mutex::new(HashMap::new())),
            users: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, String>>, String> {
        if sql.contains("opportunities") && sql.contains("SELECT") {
            let opportunities = self.opportunities.lock().unwrap();
            let results: Vec<HashMap<String, String>> = opportunities
                .values()
                .map(|opp| {
                    let mut row = HashMap::new();
                    row.insert("id".to_string(), opp.id.clone());
                    row.insert("trading_pair".to_string(), opp.trading_pair.clone());
                    row.insert(
                        "profit_percentage".to_string(),
                        opp.profit_percentage.to_string(),
                    );
                    row.insert(
                        "confidence_score".to_string(),
                        opp.confidence_score.to_string(),
                    );
                    row
                })
                .collect();
            Ok(results)
        } else if sql.contains("user_profiles") && sql.contains("SELECT") {
            let users = self.users.lock().unwrap();
            let results: Vec<HashMap<String, String>> = users
                .values()
                .map(|user| {
                    let mut row = HashMap::new();
                    row.insert("user_id".to_string(), user.user_id.clone());
                    row.insert(
                        "telegram_user_id".to_string(),
                        user.telegram_user_id.unwrap_or(0).to_string(),
                    );
                    row.insert(
                        "subscription_tier".to_string(),
                        user.subscription_tier.tier().to_string(),
                    );
                    row
                })
                .collect();
            Ok(results)
        } else {
            Ok(vec![])
        }
    }

    pub async fn execute(&self, sql: &str, _params: &[serde_json::Value]) -> Result<(), String> {
        if sql.contains("INSERT") || sql.contains("UPDATE") {
            // Mock successful execution
            Ok(())
        } else {
            Ok(())
        }
    }

    pub fn add_opportunity(&self, opportunity: ArbitrageOpportunity) {
        let mut opportunities = self.opportunities.lock().unwrap();
        opportunities.insert(opportunity.id.clone(), opportunity);
    }

    pub fn add_user(&self, user: UserProfile) {
        let mut users = self.users.lock().unwrap();
        users.insert(user.user_id.clone(), user);
    }

    pub fn get_opportunity(&self, id: &str) -> Option<ArbitrageOpportunity> {
        let opportunities = self.opportunities.lock().unwrap();
        opportunities.get(id).cloned()
    }

    pub fn get_user(&self, user_id: &str) -> Option<UserProfile> {
        let users = self.users.lock().unwrap();
        users.get(user_id).cloned()
    }

    pub fn get_opportunities_count(&self) -> usize {
        let opportunities = self.opportunities.lock().unwrap();
        opportunities.len()
    }
}

/// Test environment structure containing all necessary services and dependencies
pub struct TestEnvironment {
    pub d1_database: MockD1Database,
    pub kv_store: MockKvStore,
    pub feature_flags: Arc<FeatureFlagManager>,
}

/// Setup a complete test environment with all necessary services
pub async fn setup_test_environment() -> TestEnvironment {
    // Create mock services for testing
    let d1_database = MockD1Database::new();
    let kv_store = MockKvStore::new();
    let feature_flags = Arc::new(FeatureFlagManager::new("test".to_string()));

    TestEnvironment {
        d1_database,
        kv_store,
        feature_flags,
    }
}

/// Create a test user profile with realistic data
pub fn create_realistic_test_user(user_id: &str, tier: SubscriptionTier) -> UserProfile {
    let now = chrono::Utc::now().timestamp_millis() as u64;

    UserProfile {
        user_id: user_id.to_string(),
        telegram_user_id: Some(123456789),
        telegram_username: Some("testuser".to_string()),
        username: Some("testuser".to_string()),
        email: Some("test@example.com".to_string()),
        access_level: match tier {
            SubscriptionTier::Free => UserAccessLevel::Free,
            SubscriptionTier::Paid => UserAccessLevel::Paid,
            SubscriptionTier::Beta => UserAccessLevel::BetaUser,
            SubscriptionTier::Admin => UserAccessLevel::Admin,
            _ => UserAccessLevel::Free,
        },
        subscription_tier: tier.clone(),
        api_keys: vec![],
        preferences: UserPreferences {
            notification_enabled: true,
            preferred_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::OKX],
            risk_tolerance: 0.7,
            min_profit_threshold: 1.0,
            max_position_size: 10000.0,
            preferred_trading_pairs: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            timezone: "UTC".to_string(),
            language: "en".to_string(),
            applied_invitation_code: None,
            has_beta_features_enabled: Some(matches!(tier, SubscriptionTier::Beta)),
        },
        risk_profile: RiskProfile {
            risk_level: "medium".to_string(),
            max_leverage: 10,
            max_position_size_usd: 10000.0,
            stop_loss_percentage: 5.0,
            take_profit_percentage: 10.0,
            daily_loss_limit_usd: 1000.0,
        },
        created_at: now,
        updated_at: now,
        last_active: now,
        last_login: Some(now),
        is_active: true,
        is_beta_active: matches!(tier, SubscriptionTier::Beta),
        invitation_code_used: None,
        invitation_code: None,
        invited_by: None,
        total_invitations_sent: 0,
        successful_invitations: 0,
        beta_expires_at: None,
        total_trades: 0,
        total_pnl_usdt: 0.0,
        account_balance_usdt: 10000.0,
        profile_metadata: None,
        subscription: Subscription {
            tier: tier.clone(),
            is_active: true,
            expires_at: None,
            features: match tier {
                SubscriptionTier::Free => vec!["basic_opportunities".to_string()],
                SubscriptionTier::Paid => vec![
                    "real_time_opportunities".to_string(),
                    "advanced_analytics".to_string(),
                ],
                SubscriptionTier::Beta => {
                    vec!["beta_features".to_string(), "early_access".to_string()]
                }
                SubscriptionTier::Admin => {
                    vec!["admin_panel".to_string(), "user_management".to_string()]
                }
                _ => vec![],
            },
            daily_opportunity_limit: match tier {
                SubscriptionTier::Free => Some(10),
                SubscriptionTier::Paid => Some(100),
                SubscriptionTier::Beta => Some(50),
                SubscriptionTier::Admin => None,
                _ => Some(10),
            },
            created_at: now,
            updated_at: now,
        },
        group_admin_roles: vec![],
        configuration: UserConfiguration {
            preferred_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::OKX],
            preferred_pairs: vec!["BTC/USDT".to_string()],
            notification_settings: NotificationSettings {
                enabled: true,
                email_notifications: true,
                telegram_notifications: true,
                push_notifications: false,
                opportunity_alerts: true,
                price_alerts: true,
                system_alerts: true,
                quiet_hours_start: None,
                quiet_hours_end: None,
                timezone: "UTC".to_string(),
            },
            trading_settings: TradingSettings {
                auto_trading_enabled: false,
                max_position_size: 10000.0,
                risk_tolerance: 0.7,
                stop_loss_percentage: 5.0,
                take_profit_percentage: 10.0,
                preferred_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::OKX],
                preferred_trading_pairs: vec!["BTC/USDT".to_string()],
                min_profit_threshold: 1.0,
                max_leverage: 10,
                daily_loss_limit: 1000.0,
            },
            risk_tolerance_percentage: 70.0,
            max_entry_size_usdt: 5000.0,
        },
    }
}

/// Create a realistic test arbitrage opportunity
pub fn create_realistic_test_opportunity(
    id: &str,
    pair: &str,
    profit_pct: f64,
) -> ArbitrageOpportunity {
    let now = chrono::Utc::now().timestamp_millis() as u64;

    ArbitrageOpportunity {
        id: id.to_string(),
        trading_pair: pair.to_string(),
        exchanges: vec!["binance".to_string(), "okx".to_string()],
        profit_percentage: profit_pct,
        confidence_score: 85.0,
        risk_level: "medium".to_string(),
        buy_exchange: "binance".to_string(),
        sell_exchange: "okx".to_string(),
        buy_price: 45000.0,
        sell_price: 45000.0 * (1.0 + profit_pct / 100.0),
        volume: 1000.0,
        created_at: now,
        expires_at: Some(now + 300000), // 5 minutes
        pair: pair.to_string(),
        long_exchange: ExchangeIdEnum::Binance,
        short_exchange: ExchangeIdEnum::OKX,
        long_rate: Some(0.01),
        short_rate: Some(-0.015),
        rate_difference: profit_pct,
        net_rate_difference: Some(profit_pct * 0.95), // Account for fees
        potential_profit_value: Some(45000.0 * profit_pct / 100.0),
        timestamp: now,
        detected_at: now,
        r#type: ArbitrageType::CrossExchange,
        details: Some(format!("Cross-exchange arbitrage opportunity for {}", pair)),
        min_exchanges_required: 2,
    }
}

/// Validate opportunity data integrity
pub fn validate_opportunity_integrity(opportunity: &ArbitrageOpportunity) -> Result<(), String> {
    if opportunity.id.is_empty() {
        return Err("Opportunity ID cannot be empty".to_string());
    }

    if opportunity.trading_pair.is_empty() {
        return Err("Trading pair cannot be empty".to_string());
    }

    if opportunity.exchanges.is_empty() {
        return Err("Exchanges list cannot be empty".to_string());
    }

    if opportunity.profit_percentage <= 0.0 {
        return Err("Profit percentage must be positive".to_string());
    }

    if opportunity.confidence_score < 0.0 || opportunity.confidence_score > 100.0 {
        return Err("Confidence score must be between 0-100".to_string());
    }

    if opportunity.volume <= 0.0 {
        return Err("Volume must be positive".to_string());
    }

    if opportunity.created_at == 0 {
        return Err("Created timestamp must be valid".to_string());
    }

    Ok(())
}

/// Validate user profile data integrity
pub fn validate_user_profile_integrity(profile: &UserProfile) -> Result<(), String> {
    if profile.user_id.is_empty() {
        return Err("User ID cannot be empty".to_string());
    }

    if profile.telegram_user_id.is_none() {
        return Err("Telegram user ID must be present".to_string());
    }

    if !profile.is_active {
        return Err("User should be active".to_string());
    }

    if !profile.subscription.is_active {
        return Err("Subscription should be active".to_string());
    }

    if profile.preferences.preferred_exchanges.is_empty() {
        return Err("User should have preferred exchanges".to_string());
    }

    if profile.preferences.risk_tolerance < 0.0 || profile.preferences.risk_tolerance > 1.0 {
        return Err("Risk tolerance should be between 0-1".to_string());
    }

    Ok(())
}

/// Create test API key for user
pub fn create_test_api_key(user_id: &str, exchange: ExchangeIdEnum) -> UserApiKey {
    let now = chrono::Utc::now().timestamp_millis() as u64;

    UserApiKey {
        key_id: format!("test_key_{}_{}", user_id, exchange.as_str()),
        user_id: user_id.to_string(),
        provider: ApiKeyProvider::Exchange(exchange),
        encrypted_key: "encrypted_test_key".to_string(),
        encrypted_secret: Some("encrypted_test_secret".to_string()),
        permissions: vec!["read".to_string(), "trade".to_string()],
        is_active: true,
        is_read_only: false,
        created_at: now,
        last_used: Some(now),
        expires_at: None,
        is_testnet: false,
        metadata: std::collections::HashMap::new(),
    }
}

/// Format opportunity for testing display validation
pub fn format_opportunity_display(opportunity: &ArbitrageOpportunity) -> String {
    let timestamp = chrono::DateTime::from_timestamp_millis(opportunity.timestamp as i64)
        .unwrap_or_else(chrono::Utc::now)
        .format("%H:%M:%S UTC");

    format!(
        "🚀 *{}* Arbitrage\n\
        💰 Profit: *{:.1}%* | Confidence: *{:.0}%*\n\
        📊 Buy: {} @ ${:.2} | Sell: {} @ ${:.2}\n\
        📈 Volume: ${:.0} | Risk: {}\n\
        ⏰ Detected: {}\n\
        🔗 Type: {:?}",
        opportunity.trading_pair,
        opportunity.profit_percentage,
        opportunity.confidence_score,
        opportunity.buy_exchange,
        opportunity.buy_price,
        opportunity.sell_exchange,
        opportunity.sell_price,
        opportunity.volume,
        opportunity.risk_level,
        timestamp,
        opportunity.r#type
    )
}
