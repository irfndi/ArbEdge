// src/services/telegram.rs

use crate::services::core::ai::{
    // AiOpportunityEnhancement, AiPerformanceInsights, ParameterSuggestion,
    AiIntelligenceService,
};
use crate::services::core::analysis::market_analysis::MarketAnalysisService;
use crate::services::core::analysis::technical_analysis::TechnicalAnalysisService;
use crate::services::core::infrastructure::DatabaseManager;
// use crate::services::core::opportunities::opportunity_categorization::CategorizedOpportunity;
use crate::services::core::opportunities::opportunity_distribution::OpportunityDistributionService;
use crate::services::core::opportunities::opportunity_engine::OpportunityEngine;
use crate::services::core::trading::exchange::ExchangeService;
use crate::services::core::trading::positions::PositionsService;
use crate::services::core::user::session_management::SessionManagementService;
use crate::services::core::user::user_profile::UserProfileService;
use crate::services::core::user::user_trading_preferences::UserTradingPreferencesService;
use crate::types::{
    // AiInsightsSummary, CommandPermission,
    GroupRateLimitConfig,
    GroupRegistration,
    MessageAnalytics, // UserProfile, UserRole,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid;
use worker::console_log;

// ============= USER PREFERENCES AND PERSONALIZATION TYPES =============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub user_id: String,
    pub notification_settings: NotificationSettings,
    pub display_settings: DisplaySettings,
    pub alert_settings: AlertSettings,
    pub command_aliases: std::collections::HashMap<String, String>,
    pub dashboard_layout: DashboardLayout,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub opportunity_notifications: bool,
    pub price_alerts: bool,
    pub trading_updates: bool,
    pub system_notifications: bool,
    pub frequency: NotificationFrequency,
    pub quiet_hours: Option<QuietHours>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationFrequency {
    Immediate,
    Every5Minutes,
    Every15Minutes,
    Every30Minutes,
    Hourly,
    Daily,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHours {
    pub start_hour: u8, // 0-23
    pub end_hour: u8,   // 0-23
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub currency: String,
    pub timezone: String,
    pub language: String,
    pub number_format: NumberFormat,
    pub date_format: String,
    pub show_percentages: bool,
    pub compact_mode: bool,
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NumberFormat {
    Standard,    // 1,234.56
    European,    // 1.234,56
    Scientific,  // 1.23e+3
    Abbreviated, // 1.23K
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSettings {
    pub price_change_threshold: f64,
    pub volume_change_threshold: f64,
    pub opportunity_confidence_threshold: f64,
    pub portfolio_change_threshold: f64,
    pub custom_alerts: Vec<CustomAlert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAlert {
    pub id: String,
    pub name: String,
    pub condition: AlertCondition,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertCondition {
    PriceAbove { symbol: String, price: f64 },
    PriceBelow { symbol: String, price: f64 },
    VolumeAbove { symbol: String, volume: f64 },
    OpportunityFound { min_confidence: f64 },
    PortfolioChange { percentage: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardLayout {
    pub sections: Vec<DashboardSection>,
    pub quick_actions: Vec<String>,
    pub favorite_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DashboardSection {
    Portfolio,
    Opportunities,
    Alerts,
    RecentActivity,
    MarketOverview,
    Performance,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            notification_settings: NotificationSettings::default(),
            display_settings: DisplaySettings::default(),
            alert_settings: AlertSettings::default(),
            command_aliases: std::collections::HashMap::new(),
            dashboard_layout: DashboardLayout::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            opportunity_notifications: true,
            price_alerts: true,
            trading_updates: true,
            system_notifications: true,
            frequency: NotificationFrequency::Immediate,
            quiet_hours: None,
        }
    }
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            currency: "USD".to_string(),
            timezone: "UTC".to_string(),
            language: "en".to_string(),
            number_format: NumberFormat::Standard,
            date_format: "%Y-%m-%d %H:%M:%S".to_string(),
            show_percentages: true,
            compact_mode: false,
            theme: "light".to_string(),
        }
    }
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            price_change_threshold: 5.0,
            volume_change_threshold: 20.0,
            opportunity_confidence_threshold: 80.0,
            portfolio_change_threshold: 10.0,
            custom_alerts: Vec::new(),
        }
    }
}

impl Default for DashboardLayout {
    fn default() -> Self {
        Self {
            sections: vec![
                DashboardSection::Portfolio,
                DashboardSection::Opportunities,
                DashboardSection::Alerts,
                DashboardSection::RecentActivity,
            ],
            quick_actions: vec![
                "/balance".to_string(),
                "/opportunities".to_string(),
                "/status".to_string(),
            ],
            favorite_commands: Vec::new(),
        }
    }
}

// ============= PERFORMANCE AND RELIABILITY TYPES =============

#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    pub data: T,
    pub timestamp: Instant,
    pub ttl: Duration,
}

impl<T> CacheEntry<T> {
    pub fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            timestamp: Instant::now(),
            ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitEntry {
    pub count: u32,
    pub window_start: Instant,
    pub window_duration: Duration,
}

impl RateLimitEntry {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            count: 1,
            window_start: Instant::now(),
            window_duration,
        }
    }

    pub fn is_within_limit(&self, max_requests: u32) -> bool {
        if self.window_start.elapsed() > self.window_duration {
            true // Window expired, reset
        } else {
            self.count < max_requests
        }
    }

    pub fn increment(&mut self) {
        if self.window_start.elapsed() > self.window_duration {
            // Reset window
            self.count = 1;
            self.window_start = Instant::now();
        } else {
            self.count += 1;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    pub command_count: u64,
    pub total_response_time_ms: u64,
    pub error_count: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub retry_attempts: u64,
    pub fallback_activations: u64,
    pub rate_limit_hits: u64,
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
        }
    }
}

// ============= CHAT CONTEXT DETECTION TYPES =============

#[derive(Debug, Clone, PartialEq)]
pub enum ChatType {
    Private,
    Group,
    SuperGroup,
    Channel,
}

#[derive(Debug, Clone)]
pub struct ChatContext {
    pub chat_id: String,
    pub chat_type: ChatType,
    pub user_id: Option<String>,
    pub is_bot_admin: bool,
}

impl ChatContext {
    pub fn new(chat_id: String, chat_type: ChatType, user_id: Option<String>) -> Self {
        Self {
            chat_id,
            chat_type,
            user_id,
            is_bot_admin: false,
        }
    }

    pub fn is_private(&self) -> bool {
        matches!(self.chat_type, ChatType::Private)
    }

    pub fn is_group_or_channel(&self) -> bool {
        matches!(
            self.chat_type,
            ChatType::Group | ChatType::SuperGroup | ChatType::Channel
        )
    }

    pub fn from_telegram_update(update: &Value) -> ArbitrageResult<Self> {
        let message = update["message"].as_object().ok_or_else(|| {
            ArbitrageError::validation_error("Missing message in update".to_string())
        })?;

        let chat = message["chat"].as_object().ok_or_else(|| {
            ArbitrageError::validation_error("Missing chat in message".to_string())
        })?;

        let chat_id = chat
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ArbitrageError::validation_error("Missing chat ID".to_string()))?
            .to_string();

        let chat_type_str = chat
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ArbitrageError::validation_error("Missing chat type".to_string()))?;

        let chat_type = match chat_type_str {
            "private" => ChatType::Private,
            "group" => ChatType::Group,
            "supergroup" => ChatType::SuperGroup,
            "channel" => ChatType::Channel,
            _ => {
                return Err(ArbitrageError::validation_error(format!(
                    "Unknown chat type: {}",
                    chat_type_str
                )))
            }
        };

        let user_id = message
            .get("from")
            .and_then(|from| from.get("id"))
            .and_then(|id| id.as_u64())
            .map(|id| id.to_string());

        Ok(ChatContext::new(chat_id, chat_type, user_id))
    }
}

#[derive(Clone)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
    pub is_test_mode: bool,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: "test_token".to_string(),
            chat_id: "0".to_string(),
            is_test_mode: true,
        }
    }
}

// Simple InlineKeyboard placeholder for now
#[derive(Debug, Clone)]
pub struct InlineKeyboard {
    pub buttons: Vec<Vec<InlineKeyboardButton>>,
}

#[derive(Debug, Clone)]
pub struct InlineKeyboardButton {
    pub text: String,
    pub callback_data: Option<String>,
    pub url: Option<String>,
}

impl Default for InlineKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineKeyboard {
    pub fn new() -> Self {
        Self {
            buttons: Vec::new(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "inline_keyboard": self.buttons.iter().map(|row| {
                row.iter().map(|button| {
                    let mut btn = serde_json::json!({
                        "text": button.text
                    });
                    if let Some(ref callback_data) = button.callback_data {
                        btn["callback_data"] = serde_json::Value::String(callback_data.clone());
                    }
                    if let Some(ref url) = button.url {
                        btn["url"] = serde_json::Value::String(url.clone());
                    }
                    btn
                }).collect::<Vec<_>>()
            }).collect::<Vec<_>>()
        })
    }
}

pub struct TelegramService {
    config: TelegramConfig,
    http_client: Client,
    #[allow(dead_code)]
    analytics_enabled: bool,
    group_registrations: std::collections::HashMap<String, GroupRegistration>,
    // Core services - Optional for initialization, required for full functionality
    user_profile_service: Option<UserProfileService>,
    session_management_service: Option<SessionManagementService>,
    user_trading_preferences_service: Option<UserTradingPreferencesService>,
    // Infrastructure services
    d1_service: Option<DatabaseManager>,
    // Opportunity services
    global_opportunity_service: Option<OpportunityEngine>,
    opportunity_distribution_service: Option<OpportunityDistributionService>,
    // Analysis services
    #[allow(dead_code)]
    market_analysis_service: Option<MarketAnalysisService>,
    #[allow(dead_code)]
    technical_analysis_service: Option<TechnicalAnalysisService>,
    // AI services
    ai_integration_service: Option<AiIntelligenceService>,
    // Trading services
    exchange_service: Option<ExchangeService>,
    #[allow(dead_code)]
    positions_service: Option<PositionsService<worker::kv::KvStore>>,
}

#[allow(dead_code)]
impl TelegramService {
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
            analytics_enabled: true,
            group_registrations: std::collections::HashMap::new(),
            // Core services - Optional for initialization, required for full functionality
            user_profile_service: None,
            session_management_service: None,
            user_trading_preferences_service: None,
            // Infrastructure services
            d1_service: None,
            // Opportunity services
            global_opportunity_service: None,
            opportunity_distribution_service: None,
            // Analysis services
            market_analysis_service: None,
            technical_analysis_service: None,
            // AI services
            ai_integration_service: None,
            // Trading services
            exchange_service: None,
            positions_service: None,
        }
    }

    /// Create TelegramService from environment variables
    pub fn from_env(env: &worker::Env) -> ArbitrageResult<Self> {
        let bot_token = env
            .var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| ArbitrageError::configuration_error("TELEGRAM_BOT_TOKEN not found"))?
            .to_string();

        // Chat ID is not required as env var since it's dynamic per user/group
        let chat_id = env
            .var("TELEGRAM_CHAT_ID")
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "0".to_string()); // Default placeholder

        let is_test_mode = env
            .var("TELEGRAM_TEST_MODE")
            .map(|v| v.to_string() == "true")
            .unwrap_or(false);

        let config = TelegramConfig {
            bot_token,
            chat_id, // This will be overridden per message
            is_test_mode,
        };

        Ok(Self::new(config))
    }

    /// Set the UserProfile service for database-based RBAC
    pub fn set_user_profile_service(&mut self, user_profile_service: UserProfileService) {
        self.user_profile_service = Some(user_profile_service);
    }

    /// Set the SessionManagement service for session-first architecture
    pub fn set_session_management_service(
        &mut self,
        session_management_service: SessionManagementService,
    ) {
        self.session_management_service = Some(session_management_service);
    }

    pub fn set_opportunity_distribution_service(
        &mut self,
        opportunity_distribution_service: OpportunityDistributionService,
    ) {
        self.opportunity_distribution_service = Some(opportunity_distribution_service);
    }

    /// Set the D1 database service for database operations
    pub fn set_d1_service(&mut self, d1_service: DatabaseManager) {
        self.d1_service = Some(d1_service);
    }

    /// Set the GlobalOpportunity service for opportunity management
    pub fn set_global_opportunity_service(
        &mut self,
        global_opportunity_service: OpportunityEngine,
    ) {
        self.global_opportunity_service = Some(global_opportunity_service);
    }

    /// Set the AiIntegration service for AI analysis
    pub fn set_ai_integration_service(&mut self, ai_integration_service: AiIntelligenceService) {
        self.ai_integration_service = Some(ai_integration_service);
    }

    /// Set the Exchange service for trading operations
    pub fn set_exchange_service(&mut self, exchange_service: ExchangeService) {
        self.exchange_service = Some(exchange_service);
    }

    /// Set the MarketAnalysis service for market data
    pub fn set_market_analysis_service(&mut self, market_analysis_service: MarketAnalysisService) {
        self.market_analysis_service = Some(market_analysis_service);
    }

    /// Set the TechnicalAnalysis service for technical analysis
    pub fn set_technical_analysis_service(
        &mut self,
        technical_analysis_service: TechnicalAnalysisService,
    ) {
        self.technical_analysis_service = Some(technical_analysis_service);
    }

    /// Set the UserTradingPreferences service for user preferences
    pub fn set_user_trading_preferences_service(
        &mut self,
        user_trading_preferences_service: UserTradingPreferencesService,
    ) {
        self.user_trading_preferences_service = Some(user_trading_preferences_service);
    }

    /// Load group registrations from database into memory
    pub async fn load_group_registrations_from_database(&mut self) -> ArbitrageResult<()> {
        if let Some(ref d1_service) = self.d1_service {
            // Query group registrations from database
            let query = "SELECT group_id, group_type, group_title, member_count, registered_at, is_active, rate_limit_config FROM group_registrations WHERE is_active = 1 ORDER BY registered_at DESC";

            match d1_service.query(query, &[]).await {
                Ok(rows) => {
                    let mut loaded_count = 0;
                    for row in rows.results::<HashMap<String, serde_json::Value>>()? {
                        // Convert string_row to Value row for GroupRegistration::from_d1_row
                        let value_row: HashMap<String, serde_json::Value> = row
                            .into_iter()
                            .map(|(k, v)| {
                                (
                                    k.clone(),
                                    serde_json::Value::String(v.as_str().unwrap_or("").to_string()),
                                )
                            })
                            .collect();

                        match GroupRegistration::from_d1_row(&value_row) {
                            Ok(group_registration) => {
                                self.group_registrations.insert(
                                    group_registration.group_id.to_string(),
                                    group_registration,
                                );
                                loaded_count += 1;
                            }
                            Err(e) => {
                                console_log!("⚠️ Failed to parse group registration row: {}", e);
                            }
                        }
                    }
                    console_log!(
                        "✅ Loaded {} group registrations from database",
                        loaded_count
                    );
                }
                Err(e) => {
                    console_log!("⚠️ Failed to load group registrations from database: {}", e);
                    // Initialize empty HashMap on error
                    self.group_registrations = std::collections::HashMap::new();
                }
            }
        } else {
            console_log!("⚠️ D1Service not available - using empty group registrations HashMap");
            self.group_registrations = std::collections::HashMap::new();
        }
        Ok(())
    }

    /// Parse group registration from database row
    fn parse_group_registration_from_row(
        &self,
        row: &std::collections::HashMap<String, String>,
    ) -> ArbitrageResult<GroupRegistration> {
        let group_id = row
            .get("group_id")
            .ok_or_else(|| ArbitrageError::parse_error("Missing group_id"))?
            .clone();

        let group_type = row
            .get("group_type")
            .ok_or_else(|| ArbitrageError::parse_error("Missing group_type"))?
            .clone();

        let group_title = row.get("group_title").cloned();

        let group_username = row.get("group_username").cloned();

        let member_count = row.get("member_count").and_then(|s| s.parse::<u32>().ok());

        let admin_user_ids: Vec<String> = row
            .get("admin_user_ids")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let bot_permissions: Vec<String> = row
            .get("bot_permissions")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let enabled_features: Vec<String> = row
            .get("enabled_features")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let global_opportunities_enabled = row
            .get("global_opportunities_enabled")
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);

        let technical_analysis_enabled = row
            .get("technical_analysis_enabled")
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(false);

        let rate_limit_config: GroupRateLimitConfig = row
            .get("rate_limit_config")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(GroupRateLimitConfig {
                group_id: group_id.clone(),
                max_messages_per_minute: 10,
                max_commands_per_hour: 20,
                max_opportunities_per_day: 50,
                cooldown_seconds: 900, // 15 minutes
                is_premium_group: false,
                created_at: chrono::Utc::now().timestamp_millis() as u64,
                updated_at: chrono::Utc::now().timestamp_millis() as u64,
                max_opportunities_per_hour: 5,
                max_technical_signals_per_hour: 3,
                max_broadcasts_per_day: 10,
                cooldown_between_messages_minutes: 15,
            });

        let registered_at = row
            .get("registered_at")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let last_activity = row
            .get("last_activity")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let total_messages_sent = row
            .get("total_messages_sent")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        let last_member_count_update = row
            .get("last_member_count_update")
            .and_then(|s| s.parse::<u64>().ok());

        Ok(GroupRegistration {
            group_id,
            group_name: group_title
                .clone()
                .unwrap_or_else(|| "Unknown Group".to_string()),
            registered_by: "system".to_string(),
            registration_date: registered_at,
            is_active: true,
            subscription_tier: crate::types::SubscriptionTier::Free,
            registration_id: uuid::Uuid::new_v4().to_string(),
            group_type,
            group_title: group_title
                .clone()
                .unwrap_or_else(|| "Unknown Group".to_string()),
            registered_by_user_id: "system".to_string(),
            group_username,
            member_count,
            admin_user_ids,
            bot_permissions,
            enabled_features,
            global_opportunities_enabled,
            technical_analysis_enabled,
            rate_limit_config,
            registered_at,
            last_activity: Some(last_activity),
            total_messages_sent,
            last_member_count_update,
            created_at: registered_at,
            updated_at: registered_at,
        })
    }

    /// Track message analytics for analysis
    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code)]
    async fn track_message_analytics(
        &self,
        message_id: String,
        user_id: Option<String>,
        chat_context: &ChatContext,
        message_type: &str,
        command: Option<String>,
        content_type: &str,
        delivery_status: &str,
        response_time_ms: Option<u64>,
        metadata: serde_json::Value,
    ) -> ArbitrageResult<()> {
        if !self.analytics_enabled {
            return Ok(());
        }

        let analytics = MessageAnalytics {
            message_id,
            chat_id: chat_context.chat_id.parse::<i64>().unwrap_or(0),
            user_id,
            message_type: message_type.to_string(),
            command,
            response_time_ms: response_time_ms.unwrap_or(0),
            success: delivery_status == "success",
            error_message: if delivery_status == "success" {
                None
            } else {
                Some(delivery_status.to_string())
            },
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            metadata,
        };

        // Store analytics in database if user profile service is available
        if let Some(ref user_profile_service) = self.user_profile_service {
            // Use the D1 service from user profile service to store analytics
            let analytics_json = serde_json::to_value(&analytics)?;
            let query = "INSERT INTO message_analytics (message_id, user_id, chat_id, chat_type, message_type, command, content_type, delivery_status, response_time_ms, timestamp, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
            let params = vec![
                serde_json::Value::String(analytics.message_id),
                analytics
                    .user_id
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
                serde_json::Value::String(analytics.chat_id.to_string()),
                serde_json::Value::String(format!("{:?}", chat_context.chat_type).to_lowercase()),
                serde_json::Value::String(analytics.message_type),
                analytics
                    .command
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
                serde_json::Value::String(content_type.to_string()),
                serde_json::Value::String(delivery_status.to_string()),
                serde_json::Value::Number(analytics.response_time_ms.into()),
                serde_json::Value::Number(analytics.timestamp.into()),
                analytics_json,
            ];

            // Execute the query (ignore errors to not break message flow)
            let _ = user_profile_service
                .execute_write_operation(query, &params)
                .await;
        }

        Ok(())
    }

    /// Register group/channel when bot is added
    pub async fn register_group(
        &mut self,
        chat_context: &ChatContext,
        group_title: Option<String>,
        member_count: Option<u32>,
    ) -> ArbitrageResult<()> {
        if chat_context.is_private() {
            return Ok(()); // Not a group/channel
        }

        let current_time = chrono::Utc::now().timestamp_millis() as u64;
        let group_id_i64 = chat_context.chat_id.parse::<i64>().unwrap_or(0);

        let default_rate_limit = GroupRateLimitConfig {
            group_id: group_id_i64.to_string(),
            max_messages_per_minute: 10,
            max_commands_per_hour: 20,
            max_opportunities_per_day: 50,
            cooldown_seconds: 900, // 15 minutes
            is_premium_group: false,
            created_at: current_time,
            updated_at: current_time,
            max_opportunities_per_hour: 5,
            max_technical_signals_per_hour: 3,
            max_broadcasts_per_day: 10,
            cooldown_between_messages_minutes: 15,
        };

        let registration = GroupRegistration {
            registration_id: uuid::Uuid::new_v4().to_string(),
            group_id: group_id_i64.to_string(),
            group_name: group_title
                .clone()
                .unwrap_or_else(|| "Unknown Group".to_string()),
            registered_by: chat_context
                .user_id
                .clone()
                .unwrap_or_else(|| "system".to_string()),
            registration_date: current_time,
            is_active: true,
            subscription_tier: crate::types::SubscriptionTier::Free,
            group_title: group_title
                .clone()
                .unwrap_or_else(|| "Unknown Group".to_string()),
            group_type: format!("{:?}", chat_context.chat_type).to_lowercase(),
            registered_by_user_id: chat_context
                .user_id
                .clone()
                .unwrap_or_else(|| "system".to_string()),
            group_username: None,
            member_count,
            admin_user_ids: vec![],
            bot_permissions: vec!["read_messages".to_string(), "send_messages".to_string()],
            enabled_features: vec!["global_opportunities".to_string()],
            global_opportunities_enabled: true,
            technical_analysis_enabled: false,
            rate_limit_config: default_rate_limit,
            registered_at: current_time,
            last_activity: Some(current_time),
            total_messages_sent: 0,
            last_member_count_update: Some(current_time),
            created_at: current_time,
            updated_at: current_time,
        };

        // Store in memory for fast access
        self.group_registrations
            .insert(chat_context.chat_id.clone(), registration.clone());

        // Store in database for persistence
        if let Some(ref user_profile_service) = self.user_profile_service {
            let query = "
                INSERT OR REPLACE INTO telegram_group_registrations 
                (group_id, group_type, group_title, group_username, member_count, 
                 admin_user_ids, bot_permissions, enabled_features, 
                 global_opportunities_enabled, technical_analysis_enabled, 
                 rate_limit_config, registered_at, last_activity, 
                 total_messages_sent, last_member_count_update)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ";

            let params = vec![
                serde_json::Value::Number(serde_json::Number::from(
                    registration.group_id.parse::<i64>().unwrap_or(0),
                )),
                serde_json::Value::String(registration.group_type.clone()),
                serde_json::Value::String(registration.group_title.clone()),
                registration
                    .group_username
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
                registration
                    .member_count
                    .map(|c| serde_json::Value::Number(c.into()))
                    .unwrap_or(serde_json::Value::Null),
                serde_json::Value::String(
                    serde_json::to_string(&registration.admin_user_ids)
                        .unwrap_or_else(|_| "[]".to_string()),
                ),
                serde_json::Value::String(
                    serde_json::to_string(&registration.bot_permissions)
                        .unwrap_or_else(|_| "{}".to_string()),
                ),
                serde_json::Value::String(
                    serde_json::to_string(&registration.enabled_features)
                        .unwrap_or_else(|_| "[]".to_string()),
                ),
                serde_json::Value::Bool(registration.global_opportunities_enabled),
                serde_json::Value::Bool(registration.technical_analysis_enabled),
                serde_json::Value::String(
                    serde_json::to_string(&registration.rate_limit_config)
                        .unwrap_or_else(|_| "{}".to_string()),
                ),
                serde_json::Value::Number(registration.registered_at.into()),
                serde_json::Value::Number(registration.last_activity.unwrap_or(0).into()),
                serde_json::Value::Number(registration.total_messages_sent.into()),
                registration
                    .last_member_count_update
                    .map(|t| serde_json::Value::Number(t.into()))
                    .unwrap_or(serde_json::Value::Null),
            ];

            if let Err(e) = user_profile_service
                .execute_write_operation(query, &params)
                .await
            {
                console_log!("❌ Failed to store group registration in database: {}", e);
                // Don't fail the registration if database storage fails
            } else {
                console_log!(
                    "✅ Group registration stored in database: {}",
                    chat_context.chat_id
                );
            }
        }

        console_log!(
            "✅ Registered group: {} ({})",
            chat_context.chat_id,
            group_title.unwrap_or_else(|| "No title".to_string())
        );
        Ok(())
    }

    /// Extract group username from chat context using Telegram API
    async fn extract_group_username_from_context(
        &self,
        chat_context: &ChatContext,
    ) -> Option<String> {
        // In test mode, return a mock username
        if self.config.is_test_mode {
            return Some("test_group".to_string());
        }

        // Only try to get username for groups and channels
        if !chat_context.is_group_or_channel() {
            return None;
        }

        // Call Telegram API to get chat information
        match self.get_chat_info(&chat_context.chat_id).await {
            Ok(chat_info) => {
                // Extract username from chat info
                chat_info
                    .get("username")
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string())
            }
            Err(_) => {
                // If API call fails, return None
                None
            }
        }
    }

    /// Get chat information from Telegram API
    async fn get_chat_info(&self, chat_id: &str) -> ArbitrageResult<serde_json::Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/getChat",
            self.config.bot_token
        );

        let payload = json!({
            "chat_id": chat_id
        });

        let response = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!("Failed to get chat info: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error getting chat info: {}",
                error_text
            )));
        }

        let result: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse chat info response: {}", e))
        })?;

        if !result["ok"].as_bool().unwrap_or(false) {
            let error_description = result["description"].as_str().unwrap_or("Unknown error");
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error: {}",
                error_description
            )));
        }

        Ok(result["result"].clone())
    }

    /// Extract admin user IDs from chat context using Telegram API
    async fn extract_admin_user_ids_from_context(&self, chat_context: &ChatContext) -> Vec<String> {
        // In test mode, return mock admin IDs
        if self.config.is_test_mode {
            return vec!["123456789".to_string()];
        }

        // Only try to get admins for groups and channels
        if !chat_context.is_group_or_channel() {
            return vec![];
        }

        // Call Telegram API to get chat administrators
        match self.get_chat_administrators(&chat_context.chat_id).await {
            Ok(admins) => {
                // Extract user IDs from administrators list
                admins
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|admin| {
                        admin
                            .get("user")
                            .and_then(|user| user.get("id"))
                            .and_then(|id| id.as_i64())
                            .map(|id| id.to_string())
                    })
                    .collect()
            }
            Err(_) => {
                // If API call fails, return empty vector
                vec![]
            }
        }
    }

    /// Get chat administrators from Telegram API
    async fn get_chat_administrators(&self, chat_id: &str) -> ArbitrageResult<serde_json::Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/getChatAdministrators",
            self.config.bot_token
        );

        let payload = json!({
            "chat_id": chat_id
        });

        let response = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!("Failed to get chat administrators: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error getting chat administrators: {}",
                error_text
            )));
        }

        let result: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse chat administrators response: {}",
                e
            ))
        })?;

        if !result["ok"].as_bool().unwrap_or(false) {
            let error_description = result["description"].as_str().unwrap_or("Unknown error");
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error: {}",
                error_description
            )));
        }

        Ok(result["result"].clone())
    }

    /// Update member count for a group/channel
    pub async fn update_group_member_count(
        &mut self,
        chat_id: &str,
        member_count: u32,
    ) -> ArbitrageResult<()> {
        let current_time = chrono::Utc::now().timestamp_millis() as u64;

        // Update in memory
        if let Some(registration) = self.group_registrations.get_mut(chat_id) {
            registration.member_count = Some(member_count);
            registration.last_member_count_update = Some(current_time);
            registration.last_activity = Some(current_time);
        }

        // Update in database
        if let Some(ref user_profile_service) = self.user_profile_service {
            let query = "
                UPDATE telegram_group_registrations 
                SET member_count = ?, last_member_count_update = ?, last_activity = ?, updated_at = datetime('now')
                WHERE group_id = ?
            ";

            let params = vec![
                serde_json::Value::Number(member_count.into()),
                serde_json::Value::Number(current_time.into()),
                serde_json::Value::Number(current_time.into()),
                serde_json::Value::String(chat_id.to_string()),
            ];

            if let Err(e) = user_profile_service
                .execute_write_operation(query, &params)
                .await
            {
                console_log!("❌ Failed to update group member count in database: {}", e);
                // Don't fail the update if database storage fails
            } else {
                console_log!("✅ Updated member count for {}: {}", chat_id, member_count);
            }
        }

        Ok(())
    }

    pub async fn send_message(&self, text: &str) -> ArbitrageResult<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );

        let payload = json!({
            "chat_id": self.config.chat_id,
            "text": text,
            "parse_mode": "MarkdownV2"
        });

        let response = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!("Failed to send Telegram message: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error: {}",
                error_text
            )));
        }

        let result: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse Telegram response: {}", e))
        })?;

        if !result["ok"].as_bool().unwrap_or(false) {
            let error_description = result["description"].as_str().unwrap_or("Unknown error");
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error: {}",
                error_description
            )));
        }

        Ok(())
    }

    /// Send message to specific chat (helper for callback queries)
    async fn send_message_to_chat(&self, chat_id: &str, text: &str) -> ArbitrageResult<()> {
        let empty_keyboard = InlineKeyboard::new();
        self.send_message_with_keyboard(chat_id, text, &empty_keyboard)
            .await
    }

    /// Send message with inline keyboard to specific chat
    pub async fn send_message_with_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        keyboard: &InlineKeyboard,
    ) -> ArbitrageResult<()> {
        // In test mode, just return success without making HTTP requests
        if self.config.is_test_mode {
            return Ok(());
        }

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );

        let mut payload = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "MarkdownV2"
        });

        // Add inline keyboard if it has buttons
        if !keyboard.buttons.is_empty() {
            payload["reply_markup"] = keyboard.to_json();
        }

        let response = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!(
                    "Failed to send Telegram message with keyboard: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error: {}",
                error_text
            )));
        }

        let result: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse Telegram response: {}", e))
        })?;

        if !result["ok"].as_bool().unwrap_or(false) {
            let error_description = result["description"].as_str().unwrap_or("Unknown error");
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error: {}",
                error_description
            )));
        }

        Ok(())
    }

    // ============= SECURE NOTIFICATION METHODS =============

    /// Handle incoming webhook from Telegram
    pub async fn handle_webhook(&self, update: serde_json::Value) -> ArbitrageResult<String> {
        // Basic webhook handling - extract message and respond
        if let Some(message) = update.get("message") {
            if let Some(text) = message.get("text").and_then(|t| t.as_str()) {
                // Simple echo response for now
                return Ok(format!("Received: {}", text));
            }
        }

        if let Some(_callback_query) = update.get("callback_query") {
            // Handle callback queries
            return Ok("Callback query handled".to_string());
        }

        Ok("Webhook processed".to_string())
    }

    /// Format user preferences for display
    pub fn format_user_preferences(&self, preferences: &UserPreferences) -> String {
        let mut message = String::new();

        message.push_str("⚙️ *User Preferences*\n\n");

        // Notifications section
        message.push_str("🔔 *Notifications*\n");
        message.push_str(&format!(
            "• Enabled: {}\n",
            if preferences.notification_settings.enabled {
                "✅"
            } else {
                "❌"
            }
        ));
        message.push_str(&format!(
            "• Opportunities: {}\n",
            if preferences.notification_settings.opportunity_notifications {
                "✅"
            } else {
                "❌"
            }
        ));
        message.push_str(&format!(
            "• Price Alerts: {}\n",
            if preferences.notification_settings.price_alerts {
                "✅"
            } else {
                "❌"
            }
        ));
        message.push_str(&format!(
            "• Trading Updates: {}\n",
            if preferences.notification_settings.trading_updates {
                "✅"
            } else {
                "❌"
            }
        ));
        message.push_str(&format!(
            "• System Notifications: {}\n\n",
            if preferences.notification_settings.system_notifications {
                "✅"
            } else {
                "❌"
            }
        ));

        // Display section
        message.push_str("🎨 *Display*\n");
        message.push_str(&format!(
            "• Currency: {}\n",
            preferences.display_settings.currency
        ));
        message.push_str(&format!(
            "• Language: {}\n",
            preferences.display_settings.language
        ));
        message.push_str(&format!(
            "• Theme: {}\n",
            preferences.display_settings.theme
        ));
        message.push_str(&format!(
            "• Timezone: {}\n",
            preferences.display_settings.timezone
        ));
        message.push_str(&format!(
            "• Show Percentages: {}\n",
            if preferences.display_settings.show_percentages {
                "✅"
            } else {
                "❌"
            }
        ));
        message.push_str(&format!(
            "• Compact Mode: {}\n\n",
            if preferences.display_settings.compact_mode {
                "✅"
            } else {
                "❌"
            }
        ));

        // Alert thresholds section
        message.push_str("🚨 *Alert Thresholds*\n");
        message.push_str(&format!(
            "• Price Change: {:.1}%\n",
            preferences.alert_settings.price_change_threshold
        ));
        message.push_str(&format!(
            "• Volume Change: {:.1}%\n",
            preferences.alert_settings.volume_change_threshold
        ));
        message.push_str(&format!(
            "• Opportunity Confidence: {:.1}%\n",
            preferences.alert_settings.opportunity_confidence_threshold
        ));
        message.push_str(&format!(
            "• Portfolio Change: {:.1}%\n\n",
            preferences.alert_settings.portfolio_change_threshold
        ));

        // Dashboard sections
        message.push_str("🎯 *Dashboard Sections*\n");
        for section in &preferences.dashboard_layout.sections {
            message.push_str(&format!("• {:?}\n", section));
        }
        message.push('\n');

        // Quick actions
        message.push_str("⚡ *Quick Actions*\n");
        for action in &preferences.dashboard_layout.quick_actions {
            message.push_str(&format!("• {}\n", action));
        }
        message.push('\n');

        // Favorites
        message.push_str("⭐ *Favorites*\n");
        if preferences.dashboard_layout.favorite_commands.is_empty() {
            message.push_str("• None set\n");
        } else {
            for favorite in &preferences.dashboard_layout.favorite_commands {
                message.push_str(&format!("• {}\n", favorite));
            }
        }
        message.push('\n');

        // Command aliases
        message.push_str("🔗 *Command Aliases*\n");
        if preferences.command_aliases.is_empty() {
            message.push_str("• None set\n");
        } else {
            for (alias, command) in &preferences.command_aliases {
                message.push_str(&format!("• {} → {}\n", alias, command));
            }
        }
        message.push('\n');

        // Commands to modify preferences
        message.push_str("*Available Commands:*\n");
        message.push_str("• `/set_notifications` - Configure notifications\n");
        message.push_str("• `/set_display` - Configure display settings\n");
        message.push_str("• `/set_alerts` - Configure alert thresholds\n");
        message.push_str("• `/set_dashboard` - Configure dashboard layout\n");
        message.push_str("• `/add_alias` - Add command alias\n");
        message.push_str("• `/reset_preferences` - Reset to defaults\n");

        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_format_user_preferences() {
        let service = TelegramService::new(TelegramConfig::default());
        let mut preferences = UserPreferences::default();
        preferences.notification_settings.enabled = true;
        preferences.notification_settings.price_alerts = true;
        preferences.display_settings.theme = "dark".to_string();
        preferences.display_settings.language = "en".to_string();

        let message = service.format_user_preferences(&preferences);

        // Test message structure
        assert!(message.contains("⚙️ *User Preferences*"));
        assert!(message.contains("🔔 *Notifications*"));
        assert!(message.contains("🎨 *Display*"));
        assert!(message.contains("🚨 *Alert Thresholds*"));
        assert!(message.contains("🎯 *Dashboard Sections*"));
        assert!(message.contains("⚡ *Quick Actions*"));
        assert!(message.contains("⭐ *Favorites*"));
        assert!(message.contains("🔗 *Command Aliases*"));

        // Test command suggestions
        assert!(message.contains("/set_notifications"));
        assert!(message.contains("/set_display"));
        assert!(message.contains("/set_alerts"));
        assert!(message.contains("/set_dashboard"));
        assert!(message.contains("/add_alias"));
        assert!(message.contains("/reset_preferences"));
    }
}
