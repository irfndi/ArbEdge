// src/types.rs

use crate::utils::now_system_time;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::UNIX_EPOCH;

pub use crate::services::core::analysis::market_analysis::OpportunityType;

/// Represents the type of an order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
    StopLossLimit,
    TakeProfitLimit,
    TrailingStop,
}

/// Represents the status of an order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OrderStatus {
    Open,
    Closed,
    Filled,
    PartiallyFilled,
    Canceled, // Consistent spelling with 'Cancelled' if used elsewhere, or 'Canceled'
    Rejected,
    PendingNew, // For orders that have been submitted but not yet acknowledged
    Expired,
    PendingCancel, // For orders that have a cancel request but not yet confirmed
}

/// Account status for user accounts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AccountStatus {
    #[default]
    Active,
    Inactive,
    Suspended,
    Pending,
    Banned,
    Verified,
    Unverified,
}
// UUID is used throughout the file as uuid::Uuid::new_v4()
// Keeping the full path for clarity

// use thiserror::Error; // TODO: Re-enable when implementing custom error types

/// Exchange identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::upper_case_acronyms)]
pub enum ExchangeIdEnum {
    Binance,
    Bybit,
    OKX,
    Bitget,
    Kucoin,
    Gate,
    Mexc,
    Huobi,
    Kraken,
    Coinbase,
    // Add other exchanges as needed
}

impl ExchangeIdEnum {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExchangeIdEnum::Binance => "binance",
            ExchangeIdEnum::Bybit => "bybit",
            ExchangeIdEnum::OKX => "okx",
            ExchangeIdEnum::Bitget => "bitget",
            ExchangeIdEnum::Kucoin => "kucoin",
            ExchangeIdEnum::Gate => "gate",
            ExchangeIdEnum::Mexc => "mexc",
            ExchangeIdEnum::Huobi => "huobi",
            ExchangeIdEnum::Kraken => "kraken",
            ExchangeIdEnum::Coinbase => "coinbase",
        }
    }

    pub fn from_string(s: &str) -> Result<Self, String> {
        s.parse()
    }

    /// Get all supported exchanges
    pub fn all_supported() -> Vec<ExchangeIdEnum> {
        vec![
            ExchangeIdEnum::Binance,
            ExchangeIdEnum::Bybit,
            ExchangeIdEnum::OKX,
            ExchangeIdEnum::Bitget,
            ExchangeIdEnum::Kucoin,
            ExchangeIdEnum::Gate,
            ExchangeIdEnum::Mexc,
            ExchangeIdEnum::Huobi,
            ExchangeIdEnum::Kraken,
            ExchangeIdEnum::Coinbase,
        ]
    }
}

impl std::fmt::Display for ExchangeIdEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ExchangeIdEnum {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "binance" => Ok(ExchangeIdEnum::Binance),
            "bybit" => Ok(ExchangeIdEnum::Bybit),
            "okx" => Ok(ExchangeIdEnum::OKX),
            "bitget" => Ok(ExchangeIdEnum::Bitget),
            "kucoin" => Ok(ExchangeIdEnum::Kucoin),
            "gate" => Ok(ExchangeIdEnum::Gate),
            "mexc" => Ok(ExchangeIdEnum::Mexc),
            "huobi" => Ok(ExchangeIdEnum::Huobi),
            "kraken" => Ok(ExchangeIdEnum::Kraken),
            "coinbase" => Ok(ExchangeIdEnum::Coinbase),
            _ => Err(format!("Unknown exchange: {}", s)),
        }
    }
}

impl PartialEq<String> for ExchangeIdEnum {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<ExchangeIdEnum> for String {
    fn eq(&self, other: &ExchangeIdEnum) -> bool {
        self.as_str() == other.as_str()
    }
}

/// User access levels for RBAC system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserAccessLevel {
    Guest,
    Free,
    Registered,
    Verified,
    Paid,
    Premium,
    Admin,
    SuperAdmin,
    BetaUser, // Added based on test errors
    FreeWithoutAPI,
    FreeWithAPI,
    SubscriptionWithAPI,
    Basic, // Added to resolve compilation error
    User,  // Added to resolve compilation error
}

/// Alias for UserAccessLevel for contexts where UserRole is more semantically appropriate.
/// Or for backward compatibility during refactoring.
pub type UserRole = UserAccessLevel;

impl UserAccessLevel {
    pub fn can_trade(&self) -> bool {
        matches!(
            self,
            UserAccessLevel::Verified
                | UserAccessLevel::Paid
                | UserAccessLevel::Premium
                | UserAccessLevel::Admin
                | UserAccessLevel::SuperAdmin
                | UserAccessLevel::FreeWithAPI
                | UserAccessLevel::SubscriptionWithAPI
        )
    }

    pub fn can_use_ai(&self) -> bool {
        matches!(
            self,
            UserAccessLevel::Premium
                | UserAccessLevel::Admin
                | UserAccessLevel::SuperAdmin
                | UserAccessLevel::SubscriptionWithAPI
        )
    }

    pub fn can_use_ai_analysis(&self) -> bool {
        self.can_use_ai()
    }

    pub fn get_daily_opportunity_limits(&self) -> (u32, u32) {
        match self {
            UserAccessLevel::FreeWithoutAPI => (0, 0),
            UserAccessLevel::FreeWithAPI => (10, 10),
            UserAccessLevel::SubscriptionWithAPI => (u32::MAX, u32::MAX),
            UserAccessLevel::Guest
            | UserAccessLevel::Free
            | UserAccessLevel::Registered
            | UserAccessLevel::Verified => (0, 0),
            UserAccessLevel::Paid
            | UserAccessLevel::Premium
            | UserAccessLevel::Admin
            | UserAccessLevel::SuperAdmin
            | UserAccessLevel::BetaUser => (u32::MAX, u32::MAX),
            UserAccessLevel::Basic => (10, 10), // Assuming Basic has similar limits to FreeWithAPI for now
            UserAccessLevel::User => (10, 10),  // Similar to Registered users
        }
    }

    pub fn gets_realtime_opportunities(&self) -> bool {
        matches!(
            self,
            UserAccessLevel::SubscriptionWithAPI
                | UserAccessLevel::Paid
                | UserAccessLevel::Premium
                | UserAccessLevel::Admin
                | UserAccessLevel::SuperAdmin
        )
    }

    pub fn can_access_feature(&self, feature: &str) -> bool {
        match feature {
            "basic_trading" => self.can_trade(),
            "ai_analysis_byok" => self.can_use_ai(),
            "ai_analysis_enhanced" => matches!(
                self,
                UserAccessLevel::Premium | UserAccessLevel::Admin | UserAccessLevel::SuperAdmin
            ),
            "advanced_analytics" => {
                matches!(self, UserAccessLevel::Admin | UserAccessLevel::SuperAdmin)
            }
            "system_admin" => matches!(self, UserAccessLevel::SuperAdmin),
            _ => false,
        }
    }

    pub fn max_opportunities_per_day(&self) -> u32 {
        match self {
            UserAccessLevel::Guest => 3,
            UserAccessLevel::Free | UserAccessLevel::FreeWithoutAPI => 5,
            UserAccessLevel::Registered => 10,
            UserAccessLevel::Verified => 20,
            UserAccessLevel::Paid | UserAccessLevel::FreeWithAPI => 50,
            UserAccessLevel::Premium => 100,
            UserAccessLevel::Admin | UserAccessLevel::SuperAdmin => u32::MAX,
            UserAccessLevel::SubscriptionWithAPI => 200,
            UserAccessLevel::BetaUser => 100, // Assuming Beta has similar limits to Premium
            UserAccessLevel::Basic => 20, // Assuming Basic has similar limits to Verified for now
            UserAccessLevel::User => 10,  // Similar to Registered users
        }
    }

    pub fn get_opportunity_delay_seconds(&self) -> u64 {
        match self {
            UserAccessLevel::Guest => 600,        // 10 minutes
            UserAccessLevel::Free => 300,         // 5 minutes
            UserAccessLevel::FreeWithoutAPI => 0, // No delay for FreeWithoutAPI
            UserAccessLevel::Registered => 120,   // 2 minutes
            UserAccessLevel::Verified => 60,      // 1 minute
            UserAccessLevel::Paid => 30,          // 30 seconds
            UserAccessLevel::FreeWithAPI => 300, // 5 minutes - free users with API get longer delay
            UserAccessLevel::Premium => 10,      // 10 seconds
            UserAccessLevel::Admin | UserAccessLevel::SuperAdmin => 0, // No delay
            UserAccessLevel::SubscriptionWithAPI => 0, // 5 seconds changed to 0 to match test
            UserAccessLevel::BetaUser => 10,     // Assuming Beta has similar delay to Premium
            UserAccessLevel::Basic => 60, // Assuming Basic has similar delay to Verified for now
            UserAccessLevel::User => 120, // Similar to Registered users
        }
    }

    pub fn get_access_level(&self) -> UserAccessLevel {
        self.clone()
    }

    pub fn get_ai_access_level(&self) -> UserAccessLevel {
        self.clone()
    }
}

/// Command permissions for RBAC
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CommandPermission {
    ViewOpportunities,
    BasicOpportunities,
    BasicTrading,
    AdvancedTrading,
    AdminAccess,
    SuperAdminAccess,
    ManageUsers,
    ViewAnalytics,
    ConfigureSystem,
    // Additional permissions used throughout the codebase
    TechnicalAnalysis,
    AIEnhancedOpportunities,
    AdvancedAnalytics,
    ManualTrading,
    AutomatedTrading,
    SystemAdministration,
    PremiumFeatures,     // Added based on test errors
    UserManagement,      // Added based on test errors
    GlobalConfiguration, // Added for test compatibility
    GroupAnalytics,      // Added for test compatibility
}

/// Exchange credentials structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeCredentials {
    pub exchange: ExchangeIdEnum,
    pub api_key: String,
    pub api_secret: String,
    pub secret: String,
    pub passphrase: Option<String>,
    pub sandbox: bool,
    pub is_testnet: bool,
    pub default_leverage: u32,
    pub exchange_type: String,
}

impl ExchangeCredentials {
    pub fn new(
        exchange: ExchangeIdEnum,
        api_key: String,
        api_secret: String,
        passphrase: Option<String>,
        is_testnet: bool,
    ) -> Self {
        Self {
            exchange,
            api_key,
            secret: api_secret.clone(),
            api_secret,
            passphrase,
            sandbox: false,
            is_testnet,
            default_leverage: 1,
            exchange_type: "spot".to_string(),
        }
    }
}

/// User profile structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub telegram_user_id: Option<i64>,
    pub telegram_username: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub access_level: UserAccessLevel,
    pub subscription_tier: SubscriptionTier,
    pub api_keys: Vec<UserApiKey>,
    pub preferences: UserPreferences,
    pub risk_profile: RiskProfile,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_active: u64,
    pub last_login: Option<u64>,
    pub is_active: bool,
    pub is_beta_active: bool, // Added for beta status
    pub invitation_code_used: Option<String>,
    pub invitation_code: Option<String>,
    pub invited_by: Option<String>,
    pub total_invitations_sent: u32,
    pub successful_invitations: u32,
    pub beta_expires_at: Option<u64>,
    pub total_trades: u32,
    pub total_pnl_usdt: f64,
    pub account_balance_usdt: f64,
    pub profile_metadata: Option<String>,
    pub subscription: Subscription, // Changed from SubscriptionTier to Subscription struct
    pub group_admin_roles: Vec<GroupAdminRole>,
    pub configuration: UserConfiguration,
}

impl UserProfile {
    pub fn get_user_role(&self) -> UserRole {
        self.access_level.clone()
    }
}

/// Subscription structure with detailed subscription information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub tier: SubscriptionTier,
    pub is_active: bool,
    pub expires_at: Option<u64>,
    pub features: Vec<String>,
    pub daily_opportunity_limit: Option<u32>, // Added for daily limits
    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for Subscription {
    fn default() -> Self {
        Self {
            tier: SubscriptionTier::Free,
            is_active: true,
            expires_at: None,
            features: vec!["basic_opportunities".to_string()],
            daily_opportunity_limit: Some(3), // Default for Free tier
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

impl Subscription {
    pub fn new(tier: SubscriptionTier) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let (features, daily_limit) = match tier {
            SubscriptionTier::Free => (vec!["basic_opportunities".to_string()], Some(3)),
            SubscriptionTier::Paid => (
                vec![
                    "basic_opportunities".to_string(),
                    "enhanced_opportunities".to_string(),
                    "real_time_notifications".to_string(),
                ],
                Some(10),
            ),

            SubscriptionTier::Admin | SubscriptionTier::SuperAdmin => (
                vec![
                    "basic_opportunities".to_string(),
                    "enhanced_opportunities".to_string(),
                    "real_time_notifications".to_string(),
                    "admin_features".to_string(),
                    "unlimited_access".to_string(),
                ],
                None, // Unlimited or managed elsewhere
            ),
            _ => (vec!["basic_opportunities".to_string()], Some(3)), // Fallback
        };

        Self {
            tier,
            is_active: true,
            expires_at: None,
            features,
            daily_opportunity_limit: daily_limit,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn tier(&self) -> String {
        self.tier.tier()
    }
}

/// User configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfiguration {
    pub preferred_exchanges: Vec<ExchangeIdEnum>,
    pub preferred_pairs: Vec<String>,
    pub notification_settings: NotificationSettings,
    pub trading_settings: TradingSettings,
    // Additional fields needed by the codebase
    pub risk_tolerance_percentage: f64,
    pub max_entry_size_usdt: f64,
}

impl Default for UserConfiguration {
    fn default() -> Self {
        Self {
            preferred_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::Bybit],
            preferred_pairs: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            notification_settings: NotificationSettings::default(),
            trading_settings: TradingSettings::default(),
            risk_tolerance_percentage: 0.02, // 2% default risk tolerance
            max_entry_size_usdt: 100.0,      // $100 default max entry size
        }
    }
}

impl UserProfile {
    pub fn new(telegram_user_id: Option<i64>, invitation_code: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            user_id: uuid::Uuid::new_v4().to_string(),
            telegram_user_id,
            telegram_username: None,
            username: None,
            email: None,
            access_level: UserAccessLevel::Free,
            subscription_tier: SubscriptionTier::Free,
            api_keys: Vec::new(),
            preferences: UserPreferences::default(),
            risk_profile: RiskProfile::default(),
            created_at: now,
            updated_at: now,
            last_active: now,
            last_login: None,
            is_active: true,
            is_beta_active: false, // Initialize new field
            invitation_code_used: invitation_code.clone(),
            invitation_code,
            invited_by: None,
            total_invitations_sent: 0,
            successful_invitations: 0,
            beta_expires_at: None,
            total_trades: 0,
            total_pnl_usdt: 0.0,
            account_balance_usdt: 0.0,
            profile_metadata: None,
            subscription: Subscription::default(),
            group_admin_roles: Vec::new(),
            configuration: UserConfiguration::default(),
        }
    }

    pub fn update_last_active(&mut self) {
        self.last_active = chrono::Utc::now().timestamp_millis() as u64;
        self.updated_at = self.last_active;
    }

    pub fn add_api_key(&mut self, api_key: UserApiKey) {
        // Remove existing key for the same exchange if it exists
        self.api_keys.retain(|key| {
            if let ApiKeyProvider::Exchange(exchange) = &key.provider {
                if let ApiKeyProvider::Exchange(new_exchange) = &api_key.provider {
                    exchange != new_exchange
                } else {
                    true
                }
            } else {
                true
            }
        });
        self.api_keys.push(api_key);
        self.updated_at = chrono::Utc::now().timestamp_millis() as u64;
    }

    pub fn remove_api_key(&mut self, exchange: &ExchangeIdEnum) -> bool {
        let initial_len = self.api_keys.len();
        self.api_keys.retain(|key| {
            if let ApiKeyProvider::Exchange(key_exchange) = &key.provider {
                key_exchange != exchange
            } else {
                true
            }
        });
        let removed = self.api_keys.len() < initial_len;
        if removed {
            self.updated_at = chrono::Utc::now().timestamp_millis() as u64;
        }
        removed
    }

    pub fn has_permission(&self, permission: CommandPermission) -> bool {
        // Check if user is beta user (has active beta status)
        let is_beta_user = self.beta_expires_at.is_some_and(|expires| {
            let now = chrono::Utc::now().timestamp_millis() as u64;
            expires > now
        });

        // SuperAdmin has access to everything
        if matches!(self.access_level, UserAccessLevel::SuperAdmin) {
            return true;
        }

        // Beta users get enhanced permissions
        if is_beta_user {
            match permission {
                CommandPermission::AIEnhancedOpportunities
                | CommandPermission::AdvancedAnalytics
                | CommandPermission::TechnicalAnalysis
                | CommandPermission::PremiumFeatures
                | CommandPermission::ViewOpportunities
                | CommandPermission::BasicOpportunities
                | CommandPermission::BasicTrading
                | CommandPermission::ManualTrading => return true,
                CommandPermission::SystemAdministration
                | CommandPermission::SuperAdminAccess
                | CommandPermission::AdminAccess => return false,
                _ => {} // Continue to regular permission checks
            }
        }

        // Regular permission checks based on access level
        matches!(
            (&self.access_level, permission),
            // SuperAdmin has access to everything
            (UserAccessLevel::SuperAdmin, _)
            // Admin permissions
            | (UserAccessLevel::Admin, CommandPermission::AdminAccess)
            | (UserAccessLevel::Admin, CommandPermission::ViewOpportunities)
            | (UserAccessLevel::Admin, CommandPermission::BasicTrading)
            | (UserAccessLevel::Admin, CommandPermission::BasicOpportunities)
            | (UserAccessLevel::Admin, CommandPermission::AdvancedAnalytics)
            | (UserAccessLevel::Admin, CommandPermission::ManualTrading)
            | (UserAccessLevel::Admin, CommandPermission::AutomatedTrading)
            | (UserAccessLevel::Admin, CommandPermission::SystemAdministration)
            // Premium permissions
            | (UserAccessLevel::Premium, CommandPermission::ViewOpportunities)
            | (UserAccessLevel::Premium, CommandPermission::BasicTrading)
            | (UserAccessLevel::Premium, CommandPermission::BasicOpportunities)
            | (UserAccessLevel::Premium, CommandPermission::AdvancedAnalytics)
            | (UserAccessLevel::Premium, CommandPermission::ManualTrading)
            | (UserAccessLevel::Premium, CommandPermission::AutomatedTrading)
            | (UserAccessLevel::Premium, CommandPermission::AIEnhancedOpportunities)
            | (UserAccessLevel::Premium, CommandPermission::TechnicalAnalysis)
            | (UserAccessLevel::Premium, CommandPermission::PremiumFeatures)
            // Verified permissions
            | (UserAccessLevel::Verified, CommandPermission::ViewOpportunities)
            | (UserAccessLevel::Verified, CommandPermission::BasicTrading)
            | (UserAccessLevel::Verified, CommandPermission::BasicOpportunities)
            | (UserAccessLevel::Verified, CommandPermission::ManualTrading)
            // Registered permissions
            | (UserAccessLevel::Registered, CommandPermission::ViewOpportunities)
            | (UserAccessLevel::Registered, CommandPermission::BasicOpportunities)
            | (UserAccessLevel::Registered, CommandPermission::BasicTrading)
            // API key users
            | (UserAccessLevel::FreeWithAPI, CommandPermission::ViewOpportunities)
            | (UserAccessLevel::FreeWithAPI, CommandPermission::BasicTrading)
            | (UserAccessLevel::FreeWithAPI, CommandPermission::BasicOpportunities)
            | (UserAccessLevel::FreeWithAPI, CommandPermission::ManualTrading)
            | (UserAccessLevel::SubscriptionWithAPI, _)
            // FREE USERS get basic access including basic trading
            | (UserAccessLevel::Free, CommandPermission::ViewOpportunities)
            | (UserAccessLevel::Free, CommandPermission::BasicOpportunities)
            | (UserAccessLevel::Free, CommandPermission::BasicTrading)
            // PAID users get trading access
            | (UserAccessLevel::Paid, CommandPermission::ViewOpportunities)
            | (UserAccessLevel::Paid, CommandPermission::BasicOpportunities)
            | (UserAccessLevel::Paid, CommandPermission::BasicTrading)
            | (UserAccessLevel::Paid, CommandPermission::ManualTrading)
            | (UserAccessLevel::Paid, CommandPermission::AdvancedAnalytics)
            // BASIC users get basic trading but NOT manual trading
            | (UserAccessLevel::Basic, CommandPermission::ViewOpportunities)
            | (UserAccessLevel::Basic, CommandPermission::BasicOpportunities)
            | (UserAccessLevel::Basic, CommandPermission::BasicTrading)
        )
    }

    pub fn has_trading_api_keys(&self) -> bool {
        self.api_keys.iter().any(|key| {
            matches!(
                key.provider,
                ApiKeyProvider::Exchange(ExchangeIdEnum::Binance)
                    | ApiKeyProvider::Exchange(ExchangeIdEnum::Bybit)
                    | ApiKeyProvider::Exchange(ExchangeIdEnum::OKX)
            )
        })
    }

    pub fn get_access_level(&self) -> UserAccessLevel {
        self.access_level.clone()
    }

    pub fn get_ai_access_level(&self) -> UserAccessLevel {
        self.access_level.clone()
    }

    pub fn has_compatible_exchanges(&self, required_exchanges: &[ExchangeIdEnum]) -> bool {
        // Check if user has API keys for the required exchanges
        for exchange in required_exchanges {
            let has_key = self.api_keys.iter().any(|key| {
                if let ApiKeyProvider::Exchange(key_exchange) = &key.provider {
                    key_exchange == exchange && key.is_active
                } else {
                    false
                }
            });
            if !has_key {
                return false;
            }
        }
        true
    }

    pub fn has_minimum_exchanges(&self) -> bool {
        // Define minimum number of exchanges required
        const MINIMUM_EXCHANGES: usize = 2;
        self.api_keys.iter().filter(|key| key.is_active).count() >= MINIMUM_EXCHANGES
    }
}

/// User session structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub session_id: String,
    pub user_id: String,
    pub telegram_user_id: i64,
    pub state: SessionState,
    pub created_at: u64,
    pub last_activity: u64,
    pub expires_at: u64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl UserSession {
    pub fn new(user_id: String, telegram_user_id: i64) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let expires_at = now + (24 * 60 * 60 * 1000); // 24 hours
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            user_id,
            telegram_user_id,
            state: SessionState::Active,
            created_at: now,
            last_activity: now,
            expires_at,
            metadata: HashMap::new(),
        }
    }

    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        now > self.expires_at
    }

    pub fn update_activity(&mut self) {
        self.last_activity = chrono::Utc::now().timestamp_millis() as u64;
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, SessionState::Active) && !self.is_expired()
    }
}

/// Session state enum
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Active,
    Idle,
    Expired,
    Terminated,
}

/// User API key structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserApiKey {
    pub key_id: String,
    pub user_id: String,
    pub provider: ApiKeyProvider, // Changed from String to ApiKeyProvider enum
    pub encrypted_key: String,
    pub encrypted_secret: Option<String>,
    pub permissions: Vec<String>,
    pub is_active: bool,
    pub is_read_only: bool, // Added missing field
    pub created_at: u64,
    pub last_used: Option<u64>,
    pub expires_at: Option<u64>,
    pub is_testnet: bool,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl UserApiKey {
    pub fn new_exchange_key(
        user_id: String,
        exchange: ExchangeIdEnum,
        encrypted_key: String,
        encrypted_secret: Option<String>,
        is_testnet: bool,
    ) -> Self {
        Self {
            key_id: uuid::Uuid::new_v4().to_string(),
            user_id,
            provider: ApiKeyProvider::Exchange(exchange),
            encrypted_key,
            encrypted_secret,
            permissions: vec!["trading".to_string()],
            is_active: true,
            is_read_only: false,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            last_used: None,
            expires_at: None,
            is_testnet,
            metadata: HashMap::new(),
        }
    }

    pub fn new_ai_key(
        user_id: String,
        provider: ApiKeyProvider,
        encrypted_key: String,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            key_id: uuid::Uuid::new_v4().to_string(),
            user_id,
            provider,
            encrypted_key,
            encrypted_secret: None,
            permissions: vec!["ai".to_string()],
            is_active: true,
            is_read_only: false,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            last_used: None,
            expires_at: None,
            is_testnet: false,
            metadata,
        }
    }

    pub fn is_ai_key(&self) -> bool {
        // Clippy suggests using matches! here, but it's already used.
        // This might be a case where the permissions check should also be part of the matches! macro,
        // or there's a misunderstanding of the clippy warning.
        // For now, leaving as is, as it's already using matches! for the provider check.
        self.permissions.contains(&"ai".to_string())
            || matches!(
                self.provider,
                ApiKeyProvider::OpenAI | ApiKeyProvider::Anthropic | ApiKeyProvider::AI
            )
    }

    pub fn update_last_used(&mut self) {
        self.last_used = Some(chrono::Utc::now().timestamp_millis() as u64);
    }

    pub fn passphrase(&self) -> Option<String> {
        self.metadata
            .get("passphrase")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

/// User preferences structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub notification_enabled: bool,
    pub preferred_exchanges: Vec<ExchangeIdEnum>,
    pub risk_tolerance: f64,
    pub min_profit_threshold: f64,
    pub max_position_size: f64,
    pub preferred_trading_pairs: Vec<String>,
    pub timezone: String,
    pub language: String,
    pub applied_invitation_code: Option<String>, // Added field
    pub has_beta_features_enabled: Option<bool>, // Added field
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            notification_enabled: true,
            preferred_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::Bybit],
            risk_tolerance: 0.5,
            min_profit_threshold: 0.1,
            max_position_size: 100.0,
            preferred_trading_pairs: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            timezone: "UTC".to_string(),
            language: "en".to_string(),
            applied_invitation_code: None, // Initialize new field
            has_beta_features_enabled: Some(false), // Initialize new field
        }
    }
}

/// Risk profile structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskProfile {
    pub risk_level: String,
    pub max_leverage: u32,
    pub max_position_size_usd: f64,
    pub stop_loss_percentage: f64,
    pub take_profit_percentage: f64,
    pub daily_loss_limit_usd: f64,
}

impl Default for RiskProfile {
    fn default() -> Self {
        Self {
            risk_level: "conservative".to_string(),
            max_leverage: 3,
            max_position_size_usd: 100.0,
            stop_loss_percentage: 2.0,
            take_profit_percentage: 5.0,
            daily_loss_limit_usd: 50.0,
        }
    }
}

/// Invitation code structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationCode {
    pub code_id: String,
    pub code: String,
    pub created_by: String,
    pub created_by_user_id: String,
    pub max_uses: Option<u32>,
    pub current_uses: u32,
    pub expires_at: Option<u64>,
    pub is_active: bool,
    pub created_at: u64,
    pub bonus_percentage: Option<f64>,
    pub purpose: String,
    pub invitation_type: String, // Added missing field
    pub metadata: HashMap<String, serde_json::Value>,
}

impl InvitationCode {
    pub fn new(
        purpose: String,
        max_uses: Option<u32>,
        expires_in_days: Option<u32>,
        created_by_user_id: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let expires_at = expires_in_days.map(|days| now + (days as u64 * 24 * 60 * 60 * 1000));

        Self {
            code_id: uuid::Uuid::new_v4().to_string(),
            code: uuid::Uuid::new_v4().to_string(),
            created_by: created_by_user_id.clone(),
            created_by_user_id,
            max_uses,
            current_uses: 0,
            expires_at,
            is_active: true,
            created_at: now,
            bonus_percentage: None,
            purpose,
            invitation_type: "beta".to_string(),
            metadata: HashMap::new(),
        }
    }

    pub fn can_be_used(&self) -> bool {
        if !self.is_active {
            return false;
        }
        if self.current_uses >= self.max_uses.unwrap_or(u32::MAX) {
            return false;
        }
        if let Some(expires_at) = self.expires_at {
            let now = chrono::Utc::now().timestamp_millis() as u64;
            if now > expires_at {
                return false;
            }
        }
        true
    }

    pub fn use_code(&mut self) -> bool {
        if self.can_be_used() {
            self.current_uses += 1;
            true
        } else {
            false
        }
    }
}

/// Notification settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub email_notifications: bool,
    pub telegram_notifications: bool,
    pub push_notifications: bool,
    pub opportunity_alerts: bool,
    pub price_alerts: bool,
    pub system_alerts: bool,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub timezone: String,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            email_notifications: false,
            telegram_notifications: true,
            push_notifications: false,
            opportunity_alerts: true,
            price_alerts: true,
            system_alerts: true,
            quiet_hours_start: None,
            quiet_hours_end: None,
            timezone: "UTC".to_string(),
        }
    }
}

/// Trading settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSettings {
    pub auto_trading_enabled: bool,
    pub max_position_size: f64,
    pub risk_tolerance: f64,
    pub stop_loss_percentage: f64,
    pub take_profit_percentage: f64,
    pub preferred_exchanges: Vec<ExchangeIdEnum>,
    pub preferred_trading_pairs: Vec<String>,
    pub min_profit_threshold: f64,
    pub max_leverage: u32,
    pub daily_loss_limit: f64,
}

impl Default for TradingSettings {
    fn default() -> Self {
        Self {
            auto_trading_enabled: false,
            max_position_size: 100.0,
            risk_tolerance: 0.5,
            stop_loss_percentage: 2.0,
            take_profit_percentage: 5.0,
            preferred_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::Bybit],
            preferred_trading_pairs: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            min_profit_threshold: 0.1,
            max_leverage: 3,
            daily_loss_limit: 50.0,
        }
    }
}

/// AI Provider enum for different AI services
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyProvider {
    OpenAI,
    Anthropic,
    Custom,
    AI,
    Exchange(ExchangeIdEnum), // For exchange API keys
}

impl std::fmt::Display for ApiKeyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiKeyProvider::OpenAI => write!(f, "openai"),
            ApiKeyProvider::Anthropic => write!(f, "anthropic"),
            ApiKeyProvider::Custom => write!(f, "custom"),
            ApiKeyProvider::AI => write!(f, "ai"),
            ApiKeyProvider::Exchange(exchange) => write!(f, "exchange_{}", exchange),
        }
    }
}

/// String alias for exchange identifiers (for compatibility with CCXT-like interface)
pub type ExchangeId = String;
pub type TradingPairSymbol = String;

/// Types of arbitrage opportunities
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArbitrageType {
    FundingRate,
    SpotFutures,
    CrossExchange,
    Price, // Price arbitrage between exchanges
}

/// Trading analytics data structure for tracking user trading performance
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradingAnalytics {
    // Core analytics fields
    pub analytics_id: String,
    pub user_id: String,
    pub metric_type: String,
    pub metric_value: f64,
    pub metric_data: serde_json::Value,

    // Trading context
    pub exchange_id: Option<String>,
    pub trading_pair: Option<String>,
    pub opportunity_type: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub session_id: Option<String>,
    pub analytics_metadata: serde_json::Value,

    // Aggregated analytics fields
    pub total_trades: u32,
    pub successful_trades: u32,
    pub total_pnl_usdt: f64,
    pub best_trade_pnl: f64,
    pub worst_trade_pnl: f64,
    pub average_trade_size: f64,
    pub total_volume_traded: f64,
    pub win_rate_percentage: f64,
    pub average_holding_time_hours: f64,
    pub risk_score: f64,
    pub last_updated: u64,
}

/// User invitation structure for tracking invitations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInvitation {
    pub invitation_id: String,
    pub inviter_user_id: String,
    pub invitee_identifier: String, // email, telegram username, or phone
    pub invitation_type: String,    // email, telegram, referral
    pub status: String,             // pending, accepted, expired, cancelled
    pub message: Option<String>,
    pub invitation_data: serde_json::Value,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub accepted_at: Option<u64>,
    // Additional fields needed by the codebase
    pub invitation_code: String,
    pub invited_user_id: String,
    pub invited_by: String,
    pub used_at: Option<u64>,
    pub invitation_metadata: Option<String>,
}

/// Enhanced session state for user sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EnhancedSessionState {
    Idle,
    Active,
    Expired,
    Terminated,
    AddingApiKey,
    ConfiguringLeverage,
    ConfiguringEntrySize,
    ConfiguringRisk,
    ExecutingTrade,
    ViewingOpportunities,
    AnalyzingPortfolio,
    OptimizingPositions,
}

impl EnhancedSessionState {
    pub fn to_db_string(&self) -> String {
        match self {
            EnhancedSessionState::Idle => "idle".to_string(),
            EnhancedSessionState::Active => "active".to_string(),
            EnhancedSessionState::Expired => "expired".to_string(),
            EnhancedSessionState::Terminated => "terminated".to_string(),
            EnhancedSessionState::AddingApiKey => "adding_api_key".to_string(),
            EnhancedSessionState::ConfiguringLeverage => "configuring_leverage".to_string(),
            EnhancedSessionState::ConfiguringEntrySize => "configuring_entry_size".to_string(),
            EnhancedSessionState::ConfiguringRisk => "configuring_risk".to_string(),
            EnhancedSessionState::ExecutingTrade => "executing_trade".to_string(),
            EnhancedSessionState::ViewingOpportunities => "viewing_opportunities".to_string(),
            EnhancedSessionState::AnalyzingPortfolio => "analyzing_portfolio".to_string(),
            EnhancedSessionState::OptimizingPositions => "optimizing_positions".to_string(),
        }
    }
}

/// Enhanced user session with additional analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedUserSession {
    pub session_id: String,
    pub user_id: String,
    pub telegram_chat_id: i64,
    pub telegram_id: i64,
    pub last_command: Option<String>,
    pub current_state: EnhancedSessionState,
    pub session_state: EnhancedSessionState,
    pub temporary_data: HashMap<String, String>,
    pub started_at: u64,
    pub last_activity_at: u64,
    pub expires_at: u64,
    pub onboarding_completed: bool,
    pub preferences_set: bool,
    pub metadata: serde_json::Value,
    pub created_at: u64,
    pub updated_at: u64,
    pub session_analytics: SessionAnalytics,
    pub config: SessionConfig,
}

impl EnhancedUserSession {
    pub fn new(user_id: String, telegram_chat_id: i64) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            session_id: format!("session_{}", telegram_chat_id),
            user_id,
            telegram_chat_id,
            telegram_id: telegram_chat_id,
            last_command: None,
            current_state: EnhancedSessionState::Active,
            session_state: EnhancedSessionState::Active,
            temporary_data: std::collections::HashMap::new(),
            started_at: now,
            last_activity_at: now,
            expires_at: now + (7 * 24 * 60 * 60 * 1000), // 7 days
            onboarding_completed: false,
            preferences_set: false,
            metadata: serde_json::Value::Null,
            created_at: now,
            updated_at: now,
            session_analytics: SessionAnalytics {
                commands_executed: 0,
                opportunities_viewed: 0,
                trades_executed: 0,
                session_duration_seconds: 0,
                session_duration_ms: 0,
                last_activity: now,
            },
            config: SessionConfig::default(),
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.current_state, EnhancedSessionState::Active)
    }

    pub fn update_activity(&mut self) {
        self.last_activity_at = chrono::Utc::now().timestamp_millis() as u64;
        self.session_analytics.last_activity = self.last_activity_at;
        self.updated_at = self.last_activity_at;
    }

    pub fn terminate(&mut self) {
        self.current_state = EnhancedSessionState::Terminated;
        self.session_state = EnhancedSessionState::Terminated;
        self.update_activity();
    }
}

/// Session analytics for tracking user behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAnalytics {
    pub commands_executed: u32,
    pub opportunities_viewed: u32,
    pub trades_executed: u32,
    pub session_duration_seconds: u64,
    pub session_duration_ms: u64,
    pub last_activity: u64,
}

/// Session configuration for user preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub auto_extend: bool,
    pub max_duration_hours: u32,
    pub idle_timeout_minutes: u32,
    pub enable_analytics: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            auto_extend: true,
            max_duration_hours: 24,
            idle_timeout_minutes: 30,
            enable_analytics: true,
        }
    }
}

/// Session outcome tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionOutcome {
    Completed,
    Timeout,
    UserTerminated,
    Error,
    Terminated,
    Expired,
}

impl SessionOutcome {
    pub fn to_stable_string(&self) -> String {
        match self {
            SessionOutcome::Completed => "completed".to_string(),
            SessionOutcome::Timeout => "timeout".to_string(),
            SessionOutcome::UserTerminated => "user_terminated".to_string(),
            SessionOutcome::Error => "error".to_string(),
            SessionOutcome::Terminated => "terminated".to_string(),
            SessionOutcome::Expired => "expired".to_string(),
        }
    }
}

/// Chat context for telegram interactions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContext {
    pub chat_id: i64,
    pub chat_type: String, // "private", "group", "supergroup", "channel"
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub is_group: bool,
    pub group_title: Option<String>,
    pub message_id: Option<i32>,
    pub reply_to_message_id: Option<i32>,
}

impl ChatContext {
    // Associated constants for common chat types
    pub const PRIVATE: &'static str = "private";
    pub const GROUP: &'static str = "group";
    pub const SUPERGROUP: &'static str = "supergroup";
    pub const CHANNEL: &'static str = "channel";

    pub fn get_group_id(&self) -> Option<String> {
        if self.is_group {
            Some(self.chat_id.to_string())
        } else {
            None
        }
    }

    pub fn is_group_context(&self) -> bool {
        matches!(self.chat_type.as_str(), "group" | "supergroup" | "channel")
    }

    pub fn get_context_id(&self) -> String {
        self.chat_type.clone()
    }

    pub fn is_group_or_channel(&self) -> bool {
        matches!(self.chat_type.as_str(), "group" | "supergroup" | "channel")
    }

    pub fn allows_manual_requests(&self) -> bool {
        // Only private chats allow manual requests
        self.chat_type == "private"
    }

    pub fn allows_direct_trading(&self) -> bool {
        // Only private chats allow direct trading
        self.chat_type == "private"
    }

    pub fn should_show_take_action_buttons(&self) -> bool {
        // Show take action buttons in groups/channels to redirect to private chat
        self.is_group_or_channel()
    }

    pub fn get_response_mode(&self) -> ChatResponseMode {
        match self.chat_type.as_str() {
            "private" => ChatResponseMode::FullInteractive,
            "group" | "supergroup" => ChatResponseMode::OpportunitiesOnly,
            "channel" => ChatResponseMode::BroadcastOnly,
            _ => ChatResponseMode::FullInteractive, // Default to full interactive
        }
    }

    // Helper methods for creating common chat contexts
    pub fn private_chat(chat_id: i64, user_id: String) -> Self {
        Self {
            chat_id,
            chat_type: "private".to_string(),
            user_id: Some(user_id),
            username: None,
            is_group: false,
            group_title: None,
            message_id: None,
            reply_to_message_id: None,
        }
    }

    pub fn group_chat(chat_id: i64, group_title: String, user_id: Option<String>) -> Self {
        Self {
            chat_id,
            chat_type: "group".to_string(),
            user_id,
            username: None,
            is_group: true,
            group_title: Some(group_title),
            message_id: None,
            reply_to_message_id: None,
        }
    }

    pub fn channel_chat(chat_id: i64, channel_title: String) -> Self {
        Self {
            chat_id,
            chat_type: "channel".to_string(),
            user_id: None,
            username: None,
            is_group: false,
            group_title: Some(channel_title),
            message_id: None,
            reply_to_message_id: None,
        }
    }
}

/// Chat response mode based on context
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatResponseMode {
    FullInteractive,   // Private chat - full bot functionality
    OpportunitiesOnly, // Group - only opportunities with take action buttons
    BroadcastOnly,     // Channel - broadcast opportunities only
}

/// User opportunity limits based on subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOpportunityLimits {
    pub daily_global_opportunities: u32,
    pub daily_technical_opportunities: u32,
    pub daily_ai_opportunities: u32,
    pub hourly_rate_limit: u32,
    pub can_receive_realtime: bool,
    pub delay_seconds: u64,
    // Track current usage
    pub arbitrage_received_today: u32,
    pub technical_received_today: u32,
    // Additional tracking fields
    pub current_arbitrage_count: u32,
    pub current_technical_count: u32,
}

impl UserOpportunityLimits {
    pub fn record_arbitrage_received(&mut self) -> bool {
        if self.arbitrage_received_today < self.daily_global_opportunities {
            self.arbitrage_received_today += 1;
            self.current_arbitrage_count += 1;
            true
        } else {
            false
        }
    }

    pub fn record_technical_received(&mut self) -> bool {
        if self.technical_received_today < self.daily_technical_opportunities {
            self.technical_received_today += 1;
            self.current_technical_count += 1;
            true
        } else {
            false
        }
    }

    pub fn can_receive_arbitrage(&self) -> bool {
        self.arbitrage_received_today < self.daily_global_opportunities
    }

    pub fn can_receive_technical(&self) -> bool {
        self.technical_received_today < self.daily_technical_opportunities
    }

    pub fn get_remaining_opportunities(&self) -> (u32, u32) {
        let remaining_arbitrage = self
            .daily_global_opportunities
            .saturating_sub(self.arbitrage_received_today);
        let remaining_technical = self
            .daily_technical_opportunities
            .saturating_sub(self.technical_received_today);
        (remaining_arbitrage, remaining_technical)
    }

    pub fn new(_user_id: String, access_level: &UserAccessLevel, is_group_context: bool) -> Self {
        let base_limits = match access_level {
            UserAccessLevel::Free => UserOpportunityLimits {
                daily_global_opportunities: 10,
                daily_technical_opportunities: 5,
                daily_ai_opportunities: 5, // AI opportunities with BYOK
                hourly_rate_limit: 3,
                can_receive_realtime: false,
                delay_seconds: 300, // 5 minutes delay
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::BetaUser => UserOpportunityLimits {
                // Assuming Beta has similar limits to Paid
                daily_global_opportunities: 100,
                daily_technical_opportunities: 50,
                daily_ai_opportunities: 25,
                hourly_rate_limit: 20,
                can_receive_realtime: true,
                delay_seconds: 60,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::Basic => UserOpportunityLimits {
                daily_global_opportunities: 10,
                daily_technical_opportunities: 5,
                daily_ai_opportunities: 5, // AI opportunities with BYOK
                hourly_rate_limit: 3,
                can_receive_realtime: false,
                delay_seconds: 300, // 5 minutes delay
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::Paid => UserOpportunityLimits {
                daily_global_opportunities: 100,
                daily_technical_opportunities: 50,
                daily_ai_opportunities: 25, // BYOK & AI enabled if admin provides keys
                hourly_rate_limit: 20,
                can_receive_realtime: true,
                delay_seconds: 60,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::Admin | UserAccessLevel::SuperAdmin => UserOpportunityLimits {
                daily_global_opportunities: u32::MAX,
                daily_technical_opportunities: u32::MAX,
                daily_ai_opportunities: u32::MAX,
                hourly_rate_limit: u32::MAX,
                can_receive_realtime: true,
                delay_seconds: 0,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },

            // Legacy support - map to simplified tiers
            UserAccessLevel::Guest => UserOpportunityLimits {
                daily_global_opportunities: 2,
                daily_technical_opportunities: 1,
                daily_ai_opportunities: 0,
                hourly_rate_limit: 1,
                can_receive_realtime: false,
                delay_seconds: 600,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::Registered => UserOpportunityLimits {
                daily_global_opportunities: 10,
                daily_technical_opportunities: 5,
                daily_ai_opportunities: 2,
                hourly_rate_limit: 3,
                can_receive_realtime: false,
                delay_seconds: 300,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::Verified => UserOpportunityLimits {
                daily_global_opportunities: 50,
                daily_technical_opportunities: 25,
                daily_ai_opportunities: 10,
                hourly_rate_limit: 10,
                can_receive_realtime: true,
                delay_seconds: 60,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::Premium => UserOpportunityLimits {
                daily_global_opportunities: 200,
                daily_technical_opportunities: 100,
                daily_ai_opportunities: 50,
                hourly_rate_limit: 30,
                can_receive_realtime: true,
                delay_seconds: 10,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::FreeWithoutAPI => UserOpportunityLimits {
                daily_global_opportunities: 5,
                daily_technical_opportunities: 2,
                daily_ai_opportunities: 0,
                hourly_rate_limit: 2,
                can_receive_realtime: false,
                delay_seconds: 300,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::FreeWithAPI => UserOpportunityLimits {
                daily_global_opportunities: 10,
                daily_technical_opportunities: 5,
                daily_ai_opportunities: 5,
                hourly_rate_limit: 5,
                can_receive_realtime: false,
                delay_seconds: 120,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::SubscriptionWithAPI => UserOpportunityLimits {
                daily_global_opportunities: 100,
                daily_technical_opportunities: 50,
                daily_ai_opportunities: 25,
                hourly_rate_limit: 20,
                can_receive_realtime: true,
                delay_seconds: 30,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            UserAccessLevel::User => UserOpportunityLimits {
                daily_global_opportunities: 10,
                daily_technical_opportunities: 5,
                daily_ai_opportunities: 2,
                hourly_rate_limit: 3,
                can_receive_realtime: false,
                delay_seconds: 300,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
        };

        // Reduce limits for group contexts to prevent spam
        if is_group_context {
            UserOpportunityLimits {
                daily_global_opportunities: base_limits.daily_global_opportunities / 2,
                daily_technical_opportunities: base_limits.daily_technical_opportunities / 2,
                daily_ai_opportunities: base_limits.daily_ai_opportunities / 2,
                hourly_rate_limit: base_limits.hourly_rate_limit / 2,
                can_receive_realtime: base_limits.can_receive_realtime,
                delay_seconds: base_limits.delay_seconds * 2,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            }
        } else {
            base_limits
        }
    }

    pub fn needs_daily_reset(&self) -> bool {
        // Check if it's a new day (simplified - in production, use proper timezone handling)
        let now = chrono::Utc::now();
        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let today_start_timestamp = today_start.and_utc().timestamp() as u64 * 1000;

        // If we have any usage and it's past midnight, we need a reset
        (self.arbitrage_received_today > 0 || self.technical_received_today > 0)
            && chrono::Utc::now().timestamp_millis() as u64 > today_start_timestamp
    }

    pub fn reset_daily_counters(&mut self) {
        self.arbitrage_received_today = 0;
        self.technical_received_today = 0;
        self.current_arbitrage_count = 0;
        self.current_technical_count = 0;
    }
}

/// Position optimization result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionOptimizationResult {
    pub position_id: String,
    pub optimization_score: f64,
    pub recommended_action: PositionAction,
    pub risk_assessment: RiskAssessment,
    pub expected_improvement: f64,
    pub confidence: f64,
    pub reasoning: String,
    // Additional fields needed by the codebase
    pub current_score: f64,
    pub optimized_score: f64,
    pub recommended_actions: Vec<String>,
    pub confidence_level: f64,
    pub suggested_stop_loss: f64,
    pub suggested_take_profit: f64,
    pub timestamp: u64,
}

/// Risk assessment structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAssessment {
    pub overall_risk_level: RiskLevel,
    pub risk_score: f64, // 0.0 to 1.0
    pub volatility_risk: f64,
    pub correlation_risk: f64,
    pub concentration_risk: f64,
    pub market_risk: f64,
    pub recommendations: Vec<String>,
    // Additional fields needed by the codebase
    pub max_position_size: f64,
    pub stop_loss_recommendation: f64,
    pub take_profit_recommendation: f64,
    pub risk_level: RiskLevel,
}

/// Risk level enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
            RiskLevel::Critical => write!(f, "critical"),
        }
    }
}

/// Risk management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskManagementConfig {
    pub max_position_size_percent: f64,
    pub max_correlation_threshold: f64,
    pub stop_loss_percentage: f64,
    pub take_profit_percentage: f64,
    pub max_drawdown_percentage: f64,
    pub risk_per_trade_percentage: f64,
    pub min_risk_reward_ratio: f64, // Added missing field
    // Additional fields needed by the codebase
    pub max_positions_per_exchange: u32,
    pub max_positions_per_pair: u32,
    pub max_position_size_usd: f64,
    pub max_total_exposure_usd: f64,
    // Fields from test_validate_position_risk
    pub volatility_threshold: f64,
    pub default_stop_loss_percentage: f64,
    pub default_take_profit_percentage: f64,
    pub max_portfolio_risk_percentage: f64,
    pub max_single_position_risk_percentage: f64,
    pub enable_stop_loss: bool,
    pub enable_take_profit: bool,
    pub enable_trailing_stop: bool,
    pub correlation_limit: f64,
}

/// Distribution strategy for opportunities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DistributionStrategy {
    Broadcast,           // Send to all eligible users
    Tiered,              // Send based on subscription tier
    Personalized,        // AI-personalized distribution
    RoundRobin,          // Rotate through users
    HighestBidder,       // Premium users first
    Immediate,           // Immediate distribution
    Batched,             // Batched distribution
    Prioritized,         // Prioritized distribution
    RateLimited,         // Rate-limited distribution
    FirstComeFirstServe, // First-come, first-serve distribution
    PriorityBased,       // Priority-based distribution
    Targeted,            // Targeted distribution
    Priority,            // Priority distribution
}

impl DistributionStrategy {
    pub fn to_stable_string(&self) -> String {
        match self {
            DistributionStrategy::Broadcast => "broadcast".to_string(),
            DistributionStrategy::Tiered => "tiered".to_string(),
            DistributionStrategy::Personalized => "personalized".to_string(),
            DistributionStrategy::RoundRobin => "round_robin".to_string(),
            DistributionStrategy::HighestBidder => "highest_bidder".to_string(),
            DistributionStrategy::Immediate => "immediate".to_string(),
            DistributionStrategy::Batched => "batched".to_string(),
            DistributionStrategy::Prioritized => "prioritized".to_string(),
            DistributionStrategy::RateLimited => "rate_limited".to_string(),
            DistributionStrategy::FirstComeFirstServe => "first_come_first_serve".to_string(),
            DistributionStrategy::PriorityBased => "priority_based".to_string(),
            DistributionStrategy::Targeted => "targeted".to_string(),
            DistributionStrategy::Priority => "priority".to_string(),
        }
    }
}

/// Fairness configuration for opportunity distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FairnessConfig {
    pub enable_rotation: bool,
    pub max_consecutive_opportunities: u32,
    pub cooldown_minutes: u32,
    pub priority_boost_for_inactive: bool,
    // Additional field needed by the codebase
    pub max_opportunities_per_user_per_hour: u32,
}

impl Default for FairnessConfig {
    fn default() -> Self {
        Self {
            enable_rotation: true,
            max_consecutive_opportunities: 3,
            cooldown_minutes: 5,
            priority_boost_for_inactive: true,
            max_opportunities_per_user_per_hour: 10,
        }
    }
}

/// Opportunity source enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunitySource {
    GlobalScanner,
    TechnicalAnalysis,
    AIGenerated,
    SystemGenerated,
    UserRequested,
    MarketMaker,
    External,
    UserSubmitted,
    ExternalAPI,
}

impl OpportunitySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            OpportunitySource::GlobalScanner => "global_scanner",
            OpportunitySource::TechnicalAnalysis => "technical_analysis",
            OpportunitySource::AIGenerated => "ai_generated",
            OpportunitySource::SystemGenerated => "system_generated",
            OpportunitySource::UserRequested => "user_requested",
            OpportunitySource::MarketMaker => "market_maker",
            OpportunitySource::External => "external",
            OpportunitySource::UserSubmitted => "user_submitted",
            OpportunitySource::ExternalAPI => "external_api",
        }
    }
}

/// Core arbitrage opportunity structure
/// **POSITION STRUCTURE**: Requires exactly 2 exchanges (long + short)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbitrageOpportunity {
    pub id: String,
    pub trading_pair: String,
    pub exchanges: Vec<String>,
    pub profit_percentage: f64,
    pub confidence_score: f64,
    pub risk_level: String,
    pub buy_exchange: String,
    pub sell_exchange: String,
    pub buy_price: f64,
    pub sell_price: f64,
    pub volume: f64,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    // Unified modular fields (legacy aliases removed)
    pub pair: String,                   // Unified pair field
    pub long_exchange: ExchangeIdEnum,  // Long position exchange
    pub short_exchange: ExchangeIdEnum, // Short position exchange
    pub long_rate: Option<f64>,
    pub short_rate: Option<f64>,
    pub rate_difference: f64,
    pub net_rate_difference: Option<f64>,
    pub potential_profit_value: Option<f64>,
    pub timestamp: u64,   // Unix timestamp in milliseconds
    pub detected_at: u64, // Detection timestamp
    pub r#type: ArbitrageType,
    pub details: Option<String>,
    pub min_exchanges_required: u8, // **ALWAYS 2** for arbitrage
}

impl Default for ArbitrageOpportunity {
    fn default() -> Self {
        let timestamp = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        const DEFAULT_TTL_MS: u64 = 60_000; // 60 seconds default TTL

        Self {
            id: String::new(),
            trading_pair: "BTC/USDT".to_string(),
            exchanges: vec!["binance".to_string(), "bybit".to_string()],
            profit_percentage: 0.0,
            confidence_score: 0.0,
            risk_level: "low".to_string(),
            buy_exchange: "binance".to_string(),
            sell_exchange: "bybit".to_string(),
            buy_price: 0.0,
            sell_price: 0.0,
            volume: 0.0,
            created_at: timestamp,
            expires_at: Some(timestamp + DEFAULT_TTL_MS),
            // Additional fields
            pair: "BTC/USDT".to_string(),
            long_exchange: ExchangeIdEnum::Binance,
            short_exchange: ExchangeIdEnum::Bybit,
            long_rate: None,
            short_rate: None,
            rate_difference: 0.0,
            net_rate_difference: None,
            potential_profit_value: None,

            timestamp,
            detected_at: timestamp,
            r#type: ArbitrageType::CrossExchange,
            details: None,
            min_exchanges_required: 2,
        }
    }
}

impl ArbitrageOpportunity {
    pub fn new(
        pair: String,
        long_exchange: ExchangeIdEnum,
        short_exchange: ExchangeIdEnum,
        rate_difference: f64,
        volume: f64,
        confidence: f64,
    ) -> Self {
        let timestamp = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        const DEFAULT_TTL_MS: u64 = 60_000; // 60 seconds default TTL

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            trading_pair: pair.clone(),
            exchanges: vec![
                long_exchange.as_str().to_string(),
                short_exchange.as_str().to_string(),
            ],
            profit_percentage: rate_difference,
            confidence_score: confidence,
            risk_level: "medium".to_string(),
            buy_exchange: long_exchange.as_str().to_string(),
            sell_exchange: short_exchange.as_str().to_string(),
            buy_price: 0.0,
            sell_price: 0.0,
            volume,
            created_at: timestamp,
            expires_at: Some(timestamp + DEFAULT_TTL_MS),
            // Additional fields
            pair: pair.clone(),
            long_exchange,
            short_exchange,
            long_rate: None,
            short_rate: None,
            rate_difference,
            net_rate_difference: Some(rate_difference),
            potential_profit_value: None,

            timestamp,
            detected_at: timestamp,
            r#type: ArbitrageType::CrossExchange,
            details: None,
            min_exchanges_required: 2,
        }
    }
}

/// Position action recommendations
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionAction {
    Hold,
    IncreasePosition,
    DecreasePosition,
    ClosePosition,
    Close,        // Alias for ClosePosition
    DecreaseSize, // Alias for DecreasePosition
    Rebalance,
    HedgeRisk,
    TakeProfit,
    StopLoss,
}

impl fmt::Display for PositionAction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PositionAction::Hold => write!(f, "Hold"),
            PositionAction::IncreasePosition => write!(f, "IncreasePosition"),
            PositionAction::DecreasePosition => write!(f, "DecreasePosition"),
            PositionAction::ClosePosition => write!(f, "ClosePosition"),
            PositionAction::Close => write!(f, "Close"),
            PositionAction::DecreaseSize => write!(f, "DecreaseSize"),
            PositionAction::Rebalance => write!(f, "Rebalance"),
            PositionAction::HedgeRisk => write!(f, "HedgeRisk"),
            PositionAction::TakeProfit => write!(f, "TakeProfit"),
            PositionAction::StopLoss => write!(f, "StopLoss"),
        }
    }
}

/// User subscription tier - Simplified for group/channel focus
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SubscriptionTier {
    // Core subscription types
    #[default]
    Free, // Free tier - basic features
    Paid, // Paid tier - enhanced features
    Beta, // Beta tier for invited users

    // Admin levels
    Admin,      // Group/Channel admin
    SuperAdmin, // System admin

    // Legacy support (will be migrated)
    Basic,
    Premium,
    Pro,
    Enterprise,
}

impl SubscriptionTier {
    pub fn get_opportunity_limits(&self) -> UserOpportunityLimits {
        match self {
            SubscriptionTier::Beta => UserOpportunityLimits {
                // Assuming Beta has similar limits to Paid
                daily_global_opportunities: 100,
                daily_technical_opportunities: 50,
                daily_ai_opportunities: 25,
                hourly_rate_limit: 20,
                can_receive_realtime: true,
                delay_seconds: 60,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            SubscriptionTier::Free => UserOpportunityLimits {
                daily_global_opportunities: 10,
                daily_technical_opportunities: 5,
                daily_ai_opportunities: 0,
                hourly_rate_limit: 3,
                can_receive_realtime: false,
                delay_seconds: 300, // 5 minutes delay
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            SubscriptionTier::Paid => UserOpportunityLimits {
                daily_global_opportunities: 100,
                daily_technical_opportunities: 50,
                daily_ai_opportunities: 25, // AI enabled if admin provides keys
                hourly_rate_limit: 20,
                can_receive_realtime: true,
                delay_seconds: 60,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            SubscriptionTier::Admin | SubscriptionTier::SuperAdmin => UserOpportunityLimits {
                daily_global_opportunities: u32::MAX,
                daily_technical_opportunities: u32::MAX,
                daily_ai_opportunities: u32::MAX,
                hourly_rate_limit: u32::MAX,
                can_receive_realtime: true,
                delay_seconds: 0,
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },

            // Legacy support
            SubscriptionTier::Basic => UserOpportunityLimits {
                daily_global_opportunities: 20,
                daily_technical_opportunities: 10,
                daily_ai_opportunities: 5,
                hourly_rate_limit: 5,
                can_receive_realtime: false,
                delay_seconds: 60, // 1 minute delay
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            SubscriptionTier::Premium => UserOpportunityLimits {
                daily_global_opportunities: 100,
                daily_technical_opportunities: 50,
                daily_ai_opportunities: 25,
                hourly_rate_limit: 20,
                can_receive_realtime: true,
                delay_seconds: 10, // 10 seconds delay
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            SubscriptionTier::Pro => UserOpportunityLimits {
                daily_global_opportunities: 500,
                daily_technical_opportunities: 200,
                daily_ai_opportunities: 100,
                hourly_rate_limit: 50,
                can_receive_realtime: true,
                delay_seconds: 0, // No delay
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
            SubscriptionTier::Enterprise => UserOpportunityLimits {
                daily_global_opportunities: u32::MAX,
                daily_technical_opportunities: u32::MAX,
                daily_ai_opportunities: u32::MAX,
                hourly_rate_limit: u32::MAX,
                can_receive_realtime: true,
                delay_seconds: 0, // No delay
                arbitrage_received_today: 0,
                technical_received_today: 0,
                current_arbitrage_count: 0,
                current_technical_count: 0,
            },
        }
    }

    /// Get tier field for compatibility
    pub fn tier(&self) -> String {
        format!("{}", self)
    }
}

impl std::fmt::Display for SubscriptionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SubscriptionTier::Free => "Free",
            SubscriptionTier::Paid => "Paid",
            SubscriptionTier::Admin => "Admin",
            SubscriptionTier::SuperAdmin => "SuperAdmin",
            SubscriptionTier::Basic => "Basic",
            SubscriptionTier::Premium => "Premium",
            SubscriptionTier::Pro => "Pro",
            SubscriptionTier::Enterprise => "Enterprise",
            SubscriptionTier::Beta => "Beta",
        };
        write!(f, "{}", s)
    }
}

/// Opportunity data for distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityData {
    Arbitrage(ArbitrageOpportunity),
    Technical(TechnicalOpportunity),
    AI(AIOpportunity),
}

impl OpportunityData {
    pub fn as_arbitrage(&self) -> Option<&ArbitrageOpportunity> {
        match self {
            OpportunityData::Arbitrage(arb) => Some(arb),
            _ => None,
        }
    }

    pub fn get_id(&self) -> String {
        match self {
            OpportunityData::Arbitrage(arb) => arb.id.clone(),
            OpportunityData::Technical(tech) => tech.id.clone(),
            OpportunityData::AI(ai) => ai.id.clone(),
        }
    }

    pub fn get_pair(&self) -> String {
        match self {
            OpportunityData::Arbitrage(arb) => arb.pair.clone(),
            OpportunityData::Technical(tech) => tech.pair.clone(),
            OpportunityData::AI(ai) => ai.pair.clone(),
        }
    }

    pub fn rate_difference(&self) -> f64 {
        match self {
            OpportunityData::Arbitrage(arb) => arb.rate_difference,
            OpportunityData::Technical(tech) => tech.expected_return_percentage,
            OpportunityData::AI(ai) => ai.expected_return_percentage,
        }
    }
}

/// Technical signal type enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TechnicalSignalType {
    MovingAverageCrossover,
    RSIOverBought,
    RSIOverSold,
    MACDSignal,
    BollingerBands,
    SupportResistance,
    VolumeSpike,
    PriceBreakout,
    DivergencePattern,
    CandlestickPattern,
    Buy,
    Sell,
    Hold,
}

impl TechnicalSignalType {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            TechnicalSignalType::MovingAverageCrossover => "moving_average_crossover",
            TechnicalSignalType::RSIOverBought => "rsi_over_bought",
            TechnicalSignalType::RSIOverSold => "rsi_over_sold",
            TechnicalSignalType::MACDSignal => "macd_signal",
            TechnicalSignalType::BollingerBands => "bollinger_bands",
            TechnicalSignalType::SupportResistance => "support_resistance",
            TechnicalSignalType::VolumeSpike => "volume_spike",
            TechnicalSignalType::PriceBreakout => "price_breakout",
            TechnicalSignalType::DivergencePattern => "divergence_pattern",
            TechnicalSignalType::CandlestickPattern => "candlestick_pattern",
            TechnicalSignalType::Buy => "buy",
            TechnicalSignalType::Sell => "sell",
            TechnicalSignalType::Hold => "hold",
        }
    }
}

/// Account information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub account_id: String,
    pub exchange: ExchangeIdEnum,
    pub balances: Vec<AssetBalance>,
    pub total_balance_usd: f64,
    pub available_balance_usd: f64,
    pub used_balance_usd: f64,
    pub last_updated: u64,
}

/// Asset balance structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalance {
    pub asset: String,
    pub free: f64,
    pub used: f64,
    pub total: f64,
    pub usd_value: Option<f64>,
}

/// AI opportunity structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIOpportunity {
    pub id: String,
    pub trading_pair: String,
    pub exchanges: Vec<String>,
    pub ai_model: String,
    pub confidence_score: f64,
    pub risk_level: String,
    pub predicted_movement: f64,
    pub reasoning: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    // Additional fields needed by the codebase
    pub pair: String,
    pub expected_return_percentage: f64,
    pub details: Option<String>,
    pub confidence: f64,
    pub timestamp: u64,
    pub metadata: serde_json::Value,
}

impl Default for AIOpportunity {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            trading_pair: "BTC/USDT".to_string(),
            exchanges: vec!["binance".to_string(), "bybit".to_string()],
            ai_model: "gpt-4".to_string(),
            confidence_score: 0.5,
            risk_level: "medium".to_string(),
            predicted_movement: 0.0,
            reasoning: "AI analysis".to_string(),
            created_at: now,
            expires_at: Some(now + 3600000), // 1 hour
            pair: "BTC/USDT".to_string(),
            expected_return_percentage: 0.0,
            details: None,
            confidence: 0.5,
            timestamp: now,
            metadata: serde_json::Value::Null,
        }
    }
}

/// Technical opportunity structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalOpportunity {
    pub id: String,
    pub trading_pair: String,
    pub exchanges: Vec<String>,
    pub signal_type: TechnicalSignalType,
    pub confidence: f64,
    pub risk_level: String,
    pub entry_price: f64,
    pub target_price: f64,
    pub stop_loss: f64,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    // Additional fields needed by the codebase
    pub pair: String,
    pub expected_return_percentage: f64,
    pub details: Option<String>,
    pub timestamp: u64,
    pub metadata: serde_json::Value,
    // Unified fields (legacy aliases removed for clean modular architecture)
    pub timeframe: String,
    pub indicators: serde_json::Value,
}

impl Default for TechnicalOpportunity {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            trading_pair: "BTC/USDT".to_string(),
            exchanges: vec!["binance".to_string(), "bybit".to_string()],
            signal_type: TechnicalSignalType::Buy,
            confidence: 0.5,
            risk_level: "medium".to_string(),
            entry_price: 0.0,
            target_price: 0.0,
            stop_loss: 0.0,
            created_at: now,
            expires_at: Some(now + 3600000), // 1 hour
            pair: "BTC/USDT".to_string(),
            expected_return_percentage: 0.0,
            details: None,
            timestamp: now,
            metadata: serde_json::Value::Null,

            timeframe: "1h".to_string(),
            indicators: serde_json::Value::Null,
        }
    }
}

/// Global opportunity structure for distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalOpportunity {
    pub id: String,
    pub source: OpportunitySource,           // Added field
    pub opportunity_type: OpportunitySource, // Retained as it might be used for high-level filtering before accessing opportunity_data
    pub target_users: Vec<String>,
    pub opportunity_data: OpportunityData,
    pub created_at: u64,
    pub detection_timestamp: u64,
    pub expires_at: u64, // Retained as it's a primary field for opportunity lifecycle
    pub priority: u32,
    pub priority_score: f64,
    pub ai_enhanced: bool,
    pub ai_confidence_score: Option<f64>,
    pub ai_insights: Option<Vec<String>>,
    pub distributed_to: Vec<String>,
    pub max_participants: Option<u32>,
    pub current_participants: u32,
    pub distribution_strategy: DistributionStrategy,
}

impl Default for GlobalOpportunity {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source: OpportunitySource::SystemGenerated, // Added field
            opportunity_type: OpportunitySource::SystemGenerated,
            target_users: Vec::new(),
            opportunity_data: OpportunityData::Arbitrage(ArbitrageOpportunity::default()),
            created_at: now,
            detection_timestamp: now,
            expires_at: now + 300_000, // 5 minutes
            priority: 1,
            priority_score: 0.5,
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
            distributed_to: Vec::new(),
            max_participants: Some(100),
            current_participants: 0,
            distribution_strategy: DistributionStrategy::Broadcast,
            // opportunity_id, trading_pair, exchanges, profit_percentage, confidence_score, risk_level, and metadata
            // are now part of ArbitrageOpportunity::default() via opportunity_data
        }
    }
}

impl GlobalOpportunity {
    pub fn get_opportunity_type(&self) -> String {
        self.opportunity_type.as_str().to_string()
    }

    pub fn get_pair(&self) -> String {
        match &self.opportunity_data {
            OpportunityData::Arbitrage(arb) => arb.pair.clone(),
            OpportunityData::Technical(tech) => tech.pair.clone(),
            OpportunityData::AI(ai) => ai.pair.clone(),
        }
    }

    pub fn from_arbitrage(
        arb_opp: ArbitrageOpportunity,
        source: OpportunitySource,
        expires_at: u64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source: source.clone(), // Added field
            opportunity_type: source.clone(),
            target_users: Vec::new(),
            opportunity_data: OpportunityData::Arbitrage(arb_opp.clone()),
            created_at: now,
            detection_timestamp: now,
            expires_at,
            priority: 1,
            priority_score: arb_opp.confidence_score,
            ai_enhanced: false,
            ai_confidence_score: Some(arb_opp.confidence_score),
            ai_insights: None,
            distributed_to: Vec::new(),
            max_participants: Some(100),
            current_participants: 0,
            distribution_strategy: DistributionStrategy::Broadcast,
            // opportunity_id, trading_pair, exchanges, profit_percentage, confidence_score, risk_level and metadata
            // are now part of opportunity_data via ArbitrageOpportunity
            // opportunity_id: arb_opp.id.clone(),
            // trading_pair: arb_opp.trading_pair.clone(),
            // exchanges: arb_opp.exchanges.clone(),
            // profit_percentage: arb_opp.profit_percentage,
            // confidence_score: arb_opp.confidence_score,
            // risk_level: arb_opp.risk_level.clone(),
            // metadata: serde_json::json!({}),
        }
    }
}

/// Grouped opportunity structure for smart sorting and deduplication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedOpportunity {
    pub symbol: String,
    pub best_opportunity: GlobalOpportunity,
    pub total_opportunities: usize,
    pub available_exchanges: Vec<String>,
    pub expires_in_minutes: u32,
}

/// Group admin role structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAdminRole {
    pub group_id: String,
    pub group_title: String,
    pub role: String, // "admin", "owner", "moderator"
    pub permissions: Vec<String>,
    pub granted_at: u64,
    pub granted_by: Option<String>,
}

impl Default for GroupAdminRole {
    fn default() -> Self {
        Self {
            group_id: String::new(),
            group_title: String::new(),
            role: "admin".to_string(),
            permissions: vec!["manage_group".to_string()],
            granted_at: chrono::Utc::now().timestamp_millis() as u64,
            granted_by: None,
        }
    }
}

/// AI access level enum for subscription-based AI access control
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AIAccessLevel {
    #[default]
    FreeWithoutAI,
    FreeWithAI,
    SubscriptionWithAI,
    PremiumAI,
    EnterpriseAI,
}

impl AIAccessLevel {
    pub fn get_template_access(&self) -> TemplateAccess {
        match self {
            AIAccessLevel::FreeWithoutAI => TemplateAccess::None,
            AIAccessLevel::FreeWithAI => TemplateAccess::DefaultOnly,
            AIAccessLevel::SubscriptionWithAI
            | AIAccessLevel::PremiumAI
            | AIAccessLevel::EnterpriseAI => TemplateAccess::Full,
        }
    }

    pub fn can_use_ai_analysis(&self) -> bool {
        match self {
            AIAccessLevel::FreeWithoutAI => false,
            AIAccessLevel::FreeWithAI
            | AIAccessLevel::SubscriptionWithAI
            | AIAccessLevel::PremiumAI
            | AIAccessLevel::EnterpriseAI => true,
        }
    }

    pub fn can_create_custom_templates(&self) -> bool {
        match self {
            AIAccessLevel::FreeWithoutAI | AIAccessLevel::FreeWithAI => false,
            AIAccessLevel::SubscriptionWithAI
            | AIAccessLevel::PremiumAI
            | AIAccessLevel::EnterpriseAI => true,
        }
    }

    pub fn can_generate_personal_ai_opportunities(&self) -> bool {
        match self {
            AIAccessLevel::FreeWithoutAI | AIAccessLevel::FreeWithAI => false,
            AIAccessLevel::SubscriptionWithAI
            | AIAccessLevel::PremiumAI
            | AIAccessLevel::EnterpriseAI => true,
        }
    }

    pub fn get_daily_ai_limits(&self) -> u32 {
        match self {
            AIAccessLevel::FreeWithoutAI => 0,
            AIAccessLevel::FreeWithAI => 5,
            AIAccessLevel::SubscriptionWithAI => 100,
            AIAccessLevel::PremiumAI => 500,
            AIAccessLevel::EnterpriseAI => u32::MAX,
        }
    }
}

/// Funding rate information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateInfo {
    pub symbol: String,
    pub funding_rate: f64,
    pub timestamp: u64,
    pub datetime: String,
    pub info: serde_json::Value,
    // Additional fields used throughout the codebase
    pub next_funding_time: Option<u64>,
    pub estimated_rate: Option<f64>,
    pub estimated_settle_price: Option<f64>,
    pub exchange: ExchangeIdEnum,
    pub funding_interval_hours: u32,
    pub mark_price: Option<f64>,
    pub index_price: Option<f64>,
    pub funding_countdown: Option<u64>,
}

impl Default for FundingRateInfo {
    fn default() -> Self {
        Self {
            symbol: "BTC/USDT".to_string(),
            funding_rate: 0.0,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            datetime: chrono::Utc::now().to_rfc3339(),
            info: serde_json::Value::Null,
            next_funding_time: None,
            estimated_rate: None,
            estimated_settle_price: None,
            exchange: ExchangeIdEnum::Binance,
            funding_interval_hours: 8,
            mark_price: None,
            index_price: None,
            funding_countdown: None,
        }
    }
}

/// User preferences update structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferencesUpdate {
    pub notification_settings: Option<NotificationSettings>,
    pub trading_settings: Option<TradingSettings>,
    pub risk_tolerance_percentage: Option<f64>,
    pub max_entry_size_usdt: Option<f64>,
    pub preferred_exchanges: Option<Vec<ExchangeIdEnum>>,
    pub auto_trading_enabled: Option<bool>,
    pub max_leverage: Option<f64>,
}

impl UserPreferencesUpdate {
    /// Validate the preferences update request
    pub fn validate(&self) -> Result<(), String> {
        // Validate risk tolerance
        if let Some(risk_tolerance) = self.risk_tolerance_percentage {
            if !(0.0..=100.0).contains(&risk_tolerance) {
                return Err("Risk tolerance must be between 0 and 100".to_string());
            }
        }

        // Validate max entry size
        if let Some(max_entry_size) = self.max_entry_size_usdt {
            if max_entry_size <= 0.0 {
                return Err("Max entry size must be positive".to_string());
            }
            if max_entry_size > 1_000_000.0 {
                return Err("Max entry size cannot exceed $1,000,000".to_string());
            }
        }

        // Validate max leverage
        if let Some(max_leverage) = self.max_leverage {
            if !(1.0..=100.0).contains(&max_leverage) {
                return Err("Max leverage must be between 1 and 100".to_string());
            }
        }

        // Validate preferred exchanges
        if let Some(ref exchanges) = self.preferred_exchanges {
            if exchanges.is_empty() {
                return Err("At least one preferred exchange must be specified".to_string());
            }
            if exchanges.len() > 10 {
                return Err("Cannot specify more than 10 preferred exchanges".to_string());
            }
        }

        // Validate trading settings if provided
        if let Some(ref trading_settings) = self.trading_settings {
            if trading_settings.max_position_size <= 0.0 {
                return Err("Max position size must be positive".to_string());
            }
            if !(0.0..=1.0).contains(&trading_settings.risk_tolerance) {
                return Err("Risk tolerance must be between 0.0 and 1.0".to_string());
            }
            if trading_settings.stop_loss_percentage <= 0.0
                || trading_settings.stop_loss_percentage > 50.0
            {
                return Err("Stop loss percentage must be between 0 and 50".to_string());
            }
            if trading_settings.take_profit_percentage <= 0.0
                || trading_settings.take_profit_percentage > 100.0
            {
                return Err("Take profit percentage must be between 0 and 100".to_string());
            }
        }

        Ok(())
    }

    /// Apply the preferences update to a user profile
    pub fn apply_to_profile(&self, profile: &mut UserProfile) -> Result<(), String> {
        // Update notification settings
        if let Some(ref notification_settings) = self.notification_settings {
            profile.configuration.notification_settings = notification_settings.clone();
        }

        // Update trading settings
        if let Some(ref trading_settings) = self.trading_settings {
            profile.configuration.trading_settings = trading_settings.clone();
        }

        // Update individual preference fields
        if let Some(risk_tolerance) = self.risk_tolerance_percentage {
            profile.configuration.risk_tolerance_percentage = risk_tolerance;
        }

        if let Some(max_entry_size) = self.max_entry_size_usdt {
            profile.configuration.max_entry_size_usdt = max_entry_size;
        }

        if let Some(ref preferred_exchanges) = self.preferred_exchanges {
            profile.configuration.preferred_exchanges = preferred_exchanges.clone();
        }

        if let Some(auto_trading) = self.auto_trading_enabled {
            profile.configuration.trading_settings.auto_trading_enabled = auto_trading;
        }

        if let Some(max_leverage) = self.max_leverage {
            profile.configuration.trading_settings.max_leverage = max_leverage as u32;
        }

        // Update timestamp
        profile.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        Ok(())
    }
}

/// Update user profile request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserProfileRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub timezone: Option<String>,
    pub language: Option<String>,
    #[serde(flatten)]
    pub preferences: UserPreferencesUpdate,
}

impl UpdateUserProfileRequest {
    /// Validate the update request
    pub fn validate(&self) -> Result<(), String> {
        // Validate display name length
        if let Some(ref display_name) = self.display_name {
            if display_name.len() > 100 {
                return Err("Display name must be 100 characters or less".to_string());
            }
            if display_name.trim().is_empty() {
                return Err("Display name cannot be empty".to_string());
            }
        }

        // Validate bio length
        if let Some(ref bio) = self.bio {
            if bio.len() > 500 {
                return Err("Bio must be 500 characters or less".to_string());
            }
        }

        // Validate avatar URL format
        if let Some(ref avatar_url) = self.avatar_url {
            if !avatar_url.starts_with("http://") && !avatar_url.starts_with("https://") {
                return Err("Avatar URL must be a valid HTTP/HTTPS URL".to_string());
            }
        }

        // Validate timezone
        if let Some(ref timezone) = self.timezone {
            // Basic timezone validation - could be enhanced with proper timezone library
            if timezone.is_empty() {
                return Err("Timezone cannot be empty".to_string());
            }
        }

        // Validate language code
        if let Some(ref language) = self.language {
            if language.len() != 2 && language.len() != 5 {
                return Err(
                    "Language must be a valid 2-letter or 5-character language code".to_string(),
                );
            }
        }

        // Validate preferences
        self.preferences.validate()
    }

    /// Apply the update request to a user profile
    pub fn apply_to_profile(&self, profile: &mut UserProfile) -> Result<(), String> {
        // Update basic profile fields
        if let Some(ref display_name) = self.display_name {
            profile.username = Some(display_name.clone());
        }

        if let Some(ref timezone) = self.timezone {
            profile.preferences.timezone = timezone.clone();
        }

        if let Some(ref language) = self.language {
            profile.preferences.language = language.clone();
        }

        // Apply preferences update
        self.preferences.apply_to_profile(profile)
    }
}

/// Update user preferences request (type alias for consistency)
pub type UpdateUserPreferencesRequest = UserPreferencesUpdate;

/// AI template structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AITemplate {
    pub template_id: String,
    pub template_name: String,
    pub template_type: AITemplateType,
    pub access_level: TemplateAccess,
    pub prompt_template: String,
    pub parameters: AITemplateParameters,
    pub created_by: Option<String>,
    pub is_system_default: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

/// AI template type enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AITemplateType {
    Analysis,
    Prediction,
    RiskAssessment,
    MarketInsight,
    Custom,
    PersonalOpportunityGeneration,
    TradingDecisionSupport,
    PositionSizing,
}

/// AI template parameters structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AITemplateParameters {
    pub model: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub prompt_template: String,
    pub variables: HashMap<String, String>,
}

/// Template access level enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TemplateAccess {
    None,
    DefaultOnly,
    Full,
}

/// Validation level enum for AI template validation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationLevel {
    FormatOnly,
    CachedResult,
    LiveValidation,
}

/// Template access control structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateAccessControl {
    pub access_level: AIAccessLevel,
    pub allowed_users: Option<Vec<String>>,
    pub allowed_groups: Option<Vec<String>>,
}

/// AI usage tracker structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIUsageTracker {
    pub user_id: String,
    pub date: String,
    pub ai_calls_used: u32,
    pub ai_calls_limit: u32,
    pub last_reset: u64,
    pub access_level: AIAccessLevel,
    pub total_cost_usd: f64,
    pub cost_breakdown_by_provider: HashMap<String, f64>,
    pub cost_breakdown_by_feature: HashMap<String, f64>,
}

impl AIUsageTracker {
    pub fn new(user_id: String, access_level: AIAccessLevel) -> Self {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let ai_calls_limit = access_level.get_daily_ai_limits();

        Self {
            user_id,
            date: today,
            ai_calls_used: 0,
            ai_calls_limit,
            last_reset: chrono::Utc::now().timestamp_millis() as u64,
            access_level,
            total_cost_usd: 0.0,
            cost_breakdown_by_provider: HashMap::new(),
            cost_breakdown_by_feature: HashMap::new(),
        }
    }

    pub fn needs_daily_reset(&self) -> bool {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        self.date != today
    }

    pub fn reset_daily_counters(&mut self) {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        self.date = today;
        self.ai_calls_used = 0;
        self.last_reset = chrono::Utc::now().timestamp_millis() as u64;
        self.total_cost_usd = 0.0;
        self.cost_breakdown_by_provider.clear();
        self.cost_breakdown_by_feature.clear();
    }

    pub fn can_make_ai_call(&self) -> bool {
        self.ai_calls_used < self.ai_calls_limit
    }

    pub fn get_remaining_calls(&self) -> u32 {
        if self.ai_calls_limit == u32::MAX {
            u32::MAX
        } else {
            self.ai_calls_limit.saturating_sub(self.ai_calls_used)
        }
    }

    pub fn record_ai_call(&mut self, cost_usd: f64, provider: &str, feature: &str) {
        self.ai_calls_used += 1;
        self.total_cost_usd += cost_usd;

        *self
            .cost_breakdown_by_provider
            .entry(provider.to_string())
            .or_insert(0.0) += cost_usd;
        *self
            .cost_breakdown_by_feature
            .entry(feature.to_string())
            .or_insert(0.0) += cost_usd;
    }
}

/// Market data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub active: bool,
    pub type_: String, // spot, future, option
    pub spot: bool,
    pub margin: bool,
    pub future: bool,
    pub option: bool,
    pub contract: bool,
    pub settle: Option<String>,
    pub settle_id: Option<String>,
    pub contract_size: Option<f64>,
    pub linear: Option<bool>,
    pub inverse: Option<bool>,
    pub taker: f64,
    pub maker: f64,
    pub percentage: bool,
    pub tier_based: bool,
    pub limits: MarketLimits,
    pub precision: MarketPrecision,
    pub info: serde_json::Value,
}

/// Market limits structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketLimits {
    pub amount: Option<MinMax>,
    pub price: Option<MinMax>,
    pub cost: Option<MinMax>,
    pub leverage: Option<MinMax>,
}

/// Min/Max structure for limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinMax {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// Market precision structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketPrecision {
    pub amount: Option<i32>,
    pub price: Option<i32>,
    pub base: Option<i32>,
    pub quote: Option<i32>,
}

/// Order structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub client_order_id: Option<String>,
    pub datetime: String,
    pub timestamp: u64,
    pub last_trade_timestamp: Option<u64>,
    pub status: String, // open, closed, canceled, expired, rejected
    pub symbol: String,
    pub type_: String,                 // market, limit, stop, stop_limit
    pub time_in_force: Option<String>, // GTC, IOC, FOK, PO
    pub side: String,                  // buy, sell
    pub amount: f64,
    pub price: Option<f64>,
    pub average: Option<f64>,
    pub filled: f64,
    pub remaining: f64,
    pub cost: f64,
    pub trades: Vec<Trade>,
    pub fee: Option<TradingFee>,
    pub info: serde_json::Value,
}

/// Trade structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub order: Option<String>,
    pub info: serde_json::Value,
    pub timestamp: u64,
    pub datetime: String,
    pub symbol: String,
    pub type_: Option<String>,
    pub side: String,
    pub amount: f64,
    pub price: f64,
    pub cost: f64,
    pub fee: Option<TradingFee>,
}

/// Trading fee structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingFee {
    pub currency: String,
    pub cost: f64,
    pub rate: Option<f64>,
}

/// Trading fees structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingFees {
    pub trading: TradingFeeRates,
    pub funding: Option<TradingFeeRates>,
}

/// Trading fee rates structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingFeeRates {
    pub maker: f64,
    pub taker: f64,
    pub percentage: bool,
    pub tier_based: bool,
}

/// Represents a single entry (bid or ask) in an order book.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd)]
pub struct OrderBookEntry {
    pub price: f64,
    pub quantity: f64,
}

/// Order book structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<[f64; 2]>, // [price, amount]
    pub asks: Vec<[f64; 2]>, // [price, amount]
    pub timestamp: u64,
    pub datetime: String,
    pub nonce: Option<u64>,
}

/// Ticker structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Ticker {
    pub symbol: String,
    pub timestamp: u64,
    pub datetime: String,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub bid: Option<f64>,
    pub bid_volume: Option<f64>,
    pub ask: Option<f64>,
    pub ask_volume: Option<f64>,
    pub vwap: Option<f64>,
    pub open: Option<f64>,
    pub close: Option<f64>,
    pub last: Option<f64>,
    pub previous_close: Option<f64>,
    pub change: Option<f64>,
    pub percentage: Option<f64>,
    pub average: Option<f64>,
    pub base_volume: Option<f64>,
    pub quote_volume: Option<f64>,
    pub volume: Option<f64>, // Added missing volume field
    pub info: serde_json::Value,
}

/// Position structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub info: serde_json::Value,
    pub id: Option<String>,
    pub symbol: String,
    pub timestamp: u64,
    pub datetime: String,
    pub isolated: Option<bool>,
    pub hedged: Option<bool>,
    pub side: String, // long, short
    pub amount: f64,
    pub contracts: Option<f64>,
    pub contract_size: Option<f64>,
    pub entry_price: Option<f64>,
    pub mark_price: Option<f64>,
    pub notional: Option<f64>,
    pub leverage: Option<f64>,
    pub collateral: Option<f64>,
    pub initial_margin: Option<f64>,
    pub initial_margin_percentage: Option<f64>,
    pub maintenance_margin: Option<f64>,
    pub maintenance_margin_percentage: Option<f64>,
    pub unrealized_pnl: Option<f64>,
    pub realized_pnl: Option<f64>,
    pub percentage: Option<f64>,
}

/// Arbitrage position structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitragePosition {
    pub id: String,
    pub user_id: String,
    pub opportunity_id: String,
    pub long_position: Position,
    pub short_position: Position,
    pub status: PositionStatus,
    pub entry_time: u64,
    pub exit_time: Option<u64>,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub total_fees: f64,
    pub risk_score: f64,
    // Additional fields used in ai_intelligence.rs and positions.rs
    pub margin_used: f64,
    pub symbol: String,
    pub side: PositionSide,
    pub entry_price_long: f64,
    pub entry_price_short: f64,
    pub take_profit_price: Option<f64>,
    pub volatility_score: Option<f64>,
    pub calculated_size_usd: Option<f64>,
    pub long_exchange: ExchangeIdEnum,
    pub short_exchange: ExchangeIdEnum,
    // Fields required by positions.rs
    pub size: Option<f64>,
    pub pnl: Option<f64>,
    pub unrealized_pnl_percentage: Option<f64>,
    pub max_drawdown: Option<f64>,
    pub created_at: u64,
    pub holding_period_hours: Option<f64>,
    pub trailing_stop_distance: Option<f64>,
    pub stop_loss_price: Option<f64>,
    pub current_price: Option<f64>,
    pub current_price_long: Option<f64>,
    pub current_price_short: Option<f64>,
    pub max_loss_usd: Option<f64>,
    pub exchange: ExchangeIdEnum,
    pub pair: String,
    pub related_positions: Vec<String>,
    pub closed_at: Option<u64>,
    pub updated_at: u64,
    pub risk_reward_ratio: Option<f64>,
    pub last_optimization_check: Option<u64>,
    pub hedge_position_id: Option<String>,
    pub position_group_id: Option<String>,
    pub current_state: Option<String>, // Added for ai_intelligence.rs
    pub recommended_action: Option<String>, // Added for ai_intelligence.rs
    pub risk_percentage_applied: Option<f64>, // Added for ai_intelligence.rs
    pub optimization_score: Option<f64>, // Added for ai_intelligence.rs
}

/// Position side enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionSide {
    Long,
    Short,
    Both, // For hedge mode
}

/// Position status enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionStatus {
    Open,
    Closed,
    PartiallyFilled,
    Cancelled,
    Failed,
    Liquidated, // Added missing Liquidated variant
}

/// Technical risk level enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalRiskLevel {
    VeryLow,
    Low,
    Medium,
    High,
    VeryHigh,
}

impl TechnicalRiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            TechnicalRiskLevel::VeryLow => "very_low",
            TechnicalRiskLevel::Low => "low",
            TechnicalRiskLevel::Medium => "medium",
            TechnicalRiskLevel::High => "high",
            TechnicalRiskLevel::VeryHigh => "very_high",
        }
    }
}

/// Technical signal strength enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalSignalStrength {
    VeryWeak,
    Weak,
    Moderate,
    Strong,
    VeryStrong,
}

impl TechnicalSignalStrength {
    pub fn to_f64(&self) -> f64 {
        match self {
            TechnicalSignalStrength::VeryWeak => 0.2,
            TechnicalSignalStrength::Weak => 0.4,
            TechnicalSignalStrength::Moderate => 0.6,
            TechnicalSignalStrength::Strong => 0.8,
            TechnicalSignalStrength::VeryStrong => 1.0,
        }
    }
}

/// Balance structure for financial tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub currency: String,
    pub asset: String, // Alias for currency field for compatibility
    pub free: f64,
    pub used: f64,
    pub total: f64,
}

impl Balance {
    pub fn new(currency: String, free: f64, used: f64, total: f64) -> Self {
        Self {
            asset: currency.clone(),
            currency,
            free,
            used,
            total,
        }
    }
}

/// Balances structure (collection of balances)
pub type Balances = HashMap<String, Balance>;

/// Group rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRateLimitConfig {
    pub group_id: String,
    pub max_messages_per_minute: u32,
    pub max_opportunities_per_hour: u32,
    pub cooldown_seconds: u32,
    pub enabled: bool,
    // Additional fields used in telegram.rs
    pub max_commands_per_hour: u32,
    pub max_opportunities_per_day: u32,
    pub is_premium_group: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub max_technical_signals_per_hour: u32,
    pub max_broadcasts_per_day: u32,
    pub cooldown_between_messages_minutes: u32,
}

impl Default for GroupRateLimitConfig {
    fn default() -> Self {
        Self {
            group_id: String::new(),
            max_messages_per_minute: 10,
            max_opportunities_per_hour: 20,
            cooldown_seconds: 60,
            enabled: true,
            max_commands_per_hour: 20,
            max_opportunities_per_day: 50,
            is_premium_group: false,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            max_technical_signals_per_hour: 3,
            max_broadcasts_per_day: 10,
            cooldown_between_messages_minutes: 15,
        }
    }
}

/// Group registration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRegistration {
    pub group_id: String,
    pub group_title: String,
    pub group_type: String,    // "group", "supergroup", "channel"
    pub registered_by: String, // user_id who registered the group
    pub registered_at: u64,
    pub is_active: bool,
    pub settings: GroupSettings,
    pub rate_limit_config: GroupRateLimitConfig,
    // Additional fields used in telegram.rs
    pub group_name: String,
    pub registration_date: u64,
    pub subscription_tier: SubscriptionTier,
    pub registration_id: String,
    pub registered_by_user_id: String,
    pub group_username: Option<String>,
    pub member_count: Option<u32>,
    pub admin_user_ids: Vec<String>,
    pub bot_permissions: serde_json::Value,
    pub enabled_features: Vec<String>,
    pub last_activity: Option<u64>,
    pub total_messages_sent: u32,
    pub last_member_count_update: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for GroupRegistration {
    fn default() -> Self {
        Self {
            group_id: String::new(),
            group_title: String::new(),
            group_type: String::new(),
            registered_by: String::new(),
            registered_at: 0,
            is_active: false,
            settings: GroupSettings::default(),
            rate_limit_config: GroupRateLimitConfig::default(),
            group_name: String::new(),
            registration_date: 0,
            subscription_tier: SubscriptionTier::default(),
            registration_id: String::new(),
            registered_by_user_id: String::new(),
            group_username: None,
            member_count: None,
            admin_user_ids: Vec::new(),
            bot_permissions: serde_json::Value::Null,
            enabled_features: Vec::new(),
            last_activity: None,
            total_messages_sent: 0,
            last_member_count_update: None,
            created_at: 0,
            updated_at: 0,
        }
    }
}

impl GroupRegistration {
    // Helper function to extract a string value from the D1 row
    fn get_string_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<String, String> {
        row.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("Missing or invalid type for key: {}", key))
    }

    // Helper function to extract a u64 value from the D1 row
    fn get_u64_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<u64, String> {
        row.get(key)
            .and_then(|v| v.as_u64())
            .ok_or_else(|| format!("Missing or invalid type for key: {}", key))
    }

    // Helper function to extract a boolean value from the D1 row
    fn get_bool_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<bool, String> {
        row.get(key)
            .and_then(|v| v.as_bool())
            .ok_or_else(|| format!("Missing or invalid type for key: {}", key))
    }

    // Helper function to extract an optional string value from the D1 row
    fn get_optional_string_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<Option<String>, String> {
        match row.get(key) {
            Some(v) if v.is_null() => Ok(None),
            Some(v) => v
                .as_str()
                .map(|s| Some(s.to_string()))
                .ok_or_else(|| format!("Invalid type for optional key: {}", key)),
            None => Ok(None), // Key not present is also considered as None
        }
    }

    // Helper function to extract an optional u64 value from the D1 row
    fn get_optional_u64_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<Option<u64>, String> {
        match row.get(key) {
            Some(v) if v.is_null() => Ok(None),
            Some(v) => v
                .as_u64()
                .map(Some)
                .ok_or_else(|| format!("Invalid type for optional key: {}", key)),
            None => Ok(None),
        }
    }

    // Helper function to extract an optional u32 value from the D1 row
    fn get_optional_u32_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<Option<u32>, String> {
        match row.get(key) {
            Some(v) if v.is_null() => Ok(None),
            Some(v) => v
                .as_u64()
                .map(|n| Some(n as u32))
                .ok_or_else(|| format!("Invalid type for optional key: {}", key)),
            None => Ok(None),
        }
    }

    // Helper function to extract a u32 value from the D1 row
    fn get_u32_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<u32, String> {
        row.get(key)
            .and_then(|v| v.as_u64().map(|n| n as u32))
            .ok_or_else(|| format!("Missing or invalid type for key: {}", key))
    }

    // Helper function to extract a Vec<String> value from the D1 row
    fn get_vec_string_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<Vec<String>, String> {
        row.get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .ok_or_else(|| format!("Missing or invalid type for key: {}", key))
    }

    // Helper function to extract a serde_json::Value from the D1 row
    fn get_json_value_from_row(
        row: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> Result<serde_json::Value, String> {
        row.get(key)
            .cloned()
            .ok_or_else(|| format!("Missing key: {}", key))
    }

    pub fn from_d1_row(row: HashMap<String, serde_json::Value>) -> Result<Self, String> {
        let group_id = Self::get_string_from_row(&row, "group_id")?;
        let group_title = Self::get_string_from_row(&row, "group_title")?;
        let group_type = Self::get_string_from_row(&row, "group_type")?;
        let registered_by = Self::get_string_from_row(&row, "registered_by")?;
        let registered_at = Self::get_u64_from_row(&row, "registered_at")?;
        let is_active = Self::get_bool_from_row(&row, "is_active")?;

        // Assuming settings and rate_limit_config are stored as JSON strings in D1
        // and need to be deserialized. If they are stored as separate columns,
        // this part needs adjustment.
        let settings_str = Self::get_string_from_row(&row, "settings")?;
        let settings: GroupSettings = serde_json::from_str(&settings_str)
            .map_err(|e| format!("Failed to parse settings: {}", e))?;

        let rate_limit_config_str = Self::get_string_from_row(&row, "rate_limit_config")?;
        let rate_limit_config: GroupRateLimitConfig = serde_json::from_str(&rate_limit_config_str)
            .map_err(|e| format!("Failed to parse rate_limit_config: {}", e))?;

        let group_name = Self::get_string_from_row(&row, "group_name")?;
        let registration_date = Self::get_u64_from_row(&row, "registration_date")?;

        let subscription_tier_str = Self::get_string_from_row(&row, "subscription_tier")?;
        let subscription_tier: SubscriptionTier =
            serde_json::from_str(&format!("\"{}\"", subscription_tier_str)).map_err(|e| {
                format!(
                    "Failed to parse subscription_tier: {}. Input: '{}'",
                    e, subscription_tier_str
                )
            })?;

        let registration_id = Self::get_string_from_row(&row, "registration_id")?;
        let registered_by_user_id = Self::get_string_from_row(&row, "registered_by_user_id")?;
        let group_username = Self::get_optional_string_from_row(&row, "group_username")?;
        let member_count = Self::get_optional_u32_from_row(&row, "member_count")?;
        let admin_user_ids = Self::get_vec_string_from_row(&row, "admin_user_ids")?;
        let bot_permissions = Self::get_json_value_from_row(&row, "bot_permissions")?;
        let enabled_features = Self::get_vec_string_from_row(&row, "enabled_features")?;
        let last_activity = Self::get_optional_u64_from_row(&row, "last_activity")?;
        let total_messages_sent = Self::get_u32_from_row(&row, "total_messages_sent")?;
        let last_member_count_update =
            Self::get_optional_u64_from_row(&row, "last_member_count_update")?;
        let created_at = Self::get_u64_from_row(&row, "created_at")?;
        let updated_at = Self::get_u64_from_row(&row, "updated_at")?;

        Ok(Self {
            group_id,
            group_title,
            group_type,
            registered_by,
            registered_at,
            is_active,
            settings,
            rate_limit_config,
            group_name,
            registration_date,
            subscription_tier,
            registration_id,
            registered_by_user_id,
            group_username,
            member_count,
            admin_user_ids,
            bot_permissions,
            enabled_features,
            last_activity,
            total_messages_sent,
            last_member_count_update,
            created_at,
            updated_at,
        })
    }
}

/// Group settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSettings {
    pub auto_opportunities: bool,
    pub opportunity_types: Vec<String>, // ["arbitrage", "technical", "ai"]
    pub min_profit_threshold: f64,
    pub max_opportunities_per_day: u32,
    pub notification_settings: NotificationSettings,
}

/// AI Enhancement Mode for groups
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AIEnhancementMode {
    #[default]
    Disabled,
    Basic,
    BYOKOnly,
    AdminProvided,
    Mixed,
    Advanced,
    Premium,
}

impl std::fmt::Display for AIEnhancementMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AIEnhancementMode::Disabled => write!(f, "disabled"),
            AIEnhancementMode::Basic => write!(f, "basic"),
            AIEnhancementMode::BYOKOnly => write!(f, "byok_only"),
            AIEnhancementMode::AdminProvided => write!(f, "admin_provided"),
            AIEnhancementMode::Mixed => write!(f, "mixed"),
            AIEnhancementMode::Advanced => write!(f, "advanced"),
            AIEnhancementMode::Premium => write!(f, "premium"),
        }
    }
}

/// Group Channel Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupChannelConfig {
    pub group_id: String,
    pub admin_user_id: String,
    pub group_type: String, // "group", "supergroup", "channel"
    pub is_active: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub settings: serde_json::Value,
    // Additional fields for group configuration
    pub opportunities_enabled: bool,
    pub manual_requests_enabled: bool,
    pub trading_enabled: bool,
    pub ai_enhancement_enabled: bool,
    pub take_action_buttons: bool,
    pub managed_by_admins: Vec<String>,
}

impl GroupChannelConfig {
    pub fn is_admin(&self, user_id: &str) -> bool {
        self.admin_user_id == user_id || self.managed_by_admins.contains(&user_id.to_string())
    }

    pub fn new_group(group_id: String, admin_user_id: String) -> Self {
        Self {
            group_id,
            admin_user_id: admin_user_id.clone(),
            group_type: "group".to_string(),
            is_active: true,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            settings: serde_json::json!({}),
            opportunities_enabled: true,
            manual_requests_enabled: false, // Fixed: Default to false for new groups
            trading_enabled: false,         // Also fixing to production defaults
            ai_enhancement_enabled: false,  // Also fixing to production defaults
            take_action_buttons: true,
            managed_by_admins: vec![admin_user_id],
        }
    }

    pub fn new_channel(group_id: String, admin_user_id: String) -> Self {
        Self {
            group_id,
            admin_user_id: admin_user_id.clone(),
            group_type: "channel".to_string(),
            is_active: true,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            settings: serde_json::json!({}),
            opportunities_enabled: true,
            manual_requests_enabled: true,
            trading_enabled: true,
            ai_enhancement_enabled: true,
            take_action_buttons: true,
            managed_by_admins: vec![admin_user_id],
        }
    }
}

/// Group AI Settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAISettings {
    pub group_id: String,
    pub admin_user_id: String,
    pub enhancement_mode: AIEnhancementMode,
    pub byok_enabled: bool,
    pub ai_enabled: bool, // Added missing field
    pub group_ai_key_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub settings_metadata: serde_json::Value,
}

impl GroupAISettings {
    pub fn get_ai_enhancement_mode(&self) -> AIEnhancementMode {
        self.enhancement_mode.clone()
    }

    pub fn new(group_id: String, admin_user_id: String) -> Self {
        Self {
            group_id,
            admin_user_id,
            enhancement_mode: AIEnhancementMode::Disabled,
            byok_enabled: false,
            ai_enabled: false, // Initialize as false
            group_ai_key_id: None,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            settings_metadata: serde_json::json!({}),
        }
    }

    /// Enable AI with the specified provider and model
    pub fn enable_ai(&mut self, provider: ApiKeyProvider, model: Option<String>) {
        self.ai_enabled = true;
        self.byok_enabled = true; // Enable BYOK when enabling AI
        self.enhancement_mode = AIEnhancementMode::BYOKOnly;
        self.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        // Store provider and model in metadata
        if let Some(model_name) = model {
            let mut metadata = self
                .settings_metadata
                .as_object()
                .cloned()
                .unwrap_or_default();
            metadata.insert(
                "provider".to_string(),
                serde_json::Value::String(provider.to_string()),
            );
            metadata.insert("model".to_string(), serde_json::Value::String(model_name));
            self.settings_metadata = serde_json::Value::Object(metadata);
        }
    }
}

/// Group Subscription Settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSubscriptionSettings {
    pub group_id: String,
    pub admin_user_id: String,
    pub subscription_tier: SubscriptionTier,
    pub is_active: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub settings: serde_json::Value,
}

impl GroupSubscriptionSettings {
    pub fn new(group_id: String, admin_user_id: String) -> Self {
        Self {
            group_id,
            admin_user_id,
            subscription_tier: SubscriptionTier::Free,
            is_active: true,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
            settings: serde_json::json!({}),
        }
    }
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            auto_opportunities: true,
            opportunity_types: vec!["arbitrage".to_string()],
            min_profit_threshold: 0.1,
            max_opportunities_per_day: 50,
            notification_settings: NotificationSettings::default(),
        }
    }
}

/// Message analytics structure for tracking telegram interactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAnalytics {
    pub message_id: String,
    pub chat_id: i64,
    pub user_id: Option<String>,
    pub message_type: String, // "command", "text", "callback"
    pub command: Option<String>,
    pub timestamp: u64,
    pub response_time_ms: u64,
    pub success: bool,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
}

/// User statistics structure for admin dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStatistics {
    pub total_users: u32,
    pub active_users: u32,
    pub free_users: u32,
    pub paid_users: u32,
    pub admin_users: u32,
    pub super_admin_users: u32,
    pub other_users: u32,
    pub recently_active_users: u32,
    pub total_trades: u32,
    pub total_volume_usdt: f64,
    pub generated_at: u64,
}

impl Default for UserStatistics {
    fn default() -> Self {
        Self {
            total_users: 0,
            active_users: 0,
            free_users: 0,
            paid_users: 0,
            admin_users: 0,
            super_admin_users: 0,
            other_users: 0,
            recently_active_users: 0,
            total_trades: 0,
            total_volume_usdt: 0.0,
            generated_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Environment configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Env {
    pub database_url: String,
    pub telegram_bot_token: String,
    pub encryption_key: String,
    pub redis_url: Option<String>,
    pub environment: String, // "development", "staging", "production"
    pub log_level: String,
    pub api_keys: HashMap<String, String>,
}

impl Default for Env {
    fn default() -> Self {
        Self {
            database_url: "sqlite://arb_edge.db".to_string(),
            telegram_bot_token: String::new(),
            encryption_key: String::new(),
            redis_url: None,
            environment: "development".to_string(),
            log_level: "info".to_string(),
            api_keys: HashMap::new(),
        }
    }
}

/// Notification preferences for users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub enabled: bool,
    pub email_notifications: bool,
    pub telegram_notifications: bool,
    pub push_notifications: bool,
    pub opportunity_alerts: bool,
    pub price_alerts: bool,
    pub system_alerts: bool,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub timezone: String,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            email_notifications: false,
            telegram_notifications: true,
            push_notifications: false,
            opportunity_alerts: true,
            price_alerts: true,
            system_alerts: true,
            quiet_hours_start: None,
            quiet_hours_end: None,
            timezone: "UTC".to_string(),
        }
    }
}

/// Subscription information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    pub tier: SubscriptionTier,
    pub is_active: bool,
    pub expires_at: Option<u64>,
    pub features: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for SubscriptionInfo {
    fn default() -> Self {
        Self {
            tier: SubscriptionTier::Free,
            is_active: true,
            expires_at: None,
            features: vec!["basic_opportunities".to_string()],
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

// Add Default implementations for missing types
impl Default for SessionAnalytics {
    fn default() -> Self {
        Self {
            commands_executed: 0,
            opportunities_viewed: 0,
            trades_executed: 0,
            session_duration_seconds: 0,
            session_duration_ms: 0,
            last_activity: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

impl Default for AITemplateParameters {
    fn default() -> Self {
        Self {
            model: "gpt-3.5-turbo".to_string(),
            max_tokens: Some(1000),
            temperature: Some(0.7),
            prompt_template: String::new(),
            variables: HashMap::new(),
        }
    }
}

impl AITemplate {
    pub fn new_system_template(
        template_name: String,
        template_type: AITemplateType,
        prompt_template: String,
        parameters: AITemplateParameters,
    ) -> Self {
        Self {
            template_id: uuid::Uuid::new_v4().to_string(),
            template_name,
            template_type,
            access_level: TemplateAccess::DefaultOnly,
            prompt_template,
            parameters,
            created_by: None,
            is_system_default: true,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn new_user_template(
        template_name: String,
        template_type: AITemplateType,
        prompt_template: String,
        parameters: AITemplateParameters,
        created_by: String,
    ) -> Self {
        Self {
            template_id: uuid::Uuid::new_v4().to_string(),
            template_name,
            template_type,
            access_level: TemplateAccess::Full,
            prompt_template,
            parameters,
            created_by: Some(created_by),
            is_system_default: false,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            updated_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

// Add missing AI template type variants
impl AITemplateType {
    pub fn global_opportunity_analysis() -> Self {
        Self::Analysis
    }
}
