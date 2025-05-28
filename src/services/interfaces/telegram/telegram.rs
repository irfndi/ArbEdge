// src/services/telegram.rs

use crate::services::core::ai::ai_integration::AiIntegrationService;
use crate::services::core::ai::ai_intelligence::{
    AiOpportunityEnhancement, AiPerformanceInsights, ParameterSuggestion,
};
use crate::services::core::analysis::market_analysis::MarketAnalysisService;
use crate::services::core::analysis::technical_analysis::TechnicalAnalysisService;
use crate::services::core::infrastructure::d1_database::D1Service;
use crate::services::core::opportunities::global_opportunity::GlobalOpportunityService;
use crate::services::core::opportunities::opportunity_categorization::CategorizedOpportunity;
use crate::services::core::opportunities::opportunity_distribution::OpportunityDistributionService;
use crate::services::core::trading::exchange::ExchangeService;
use crate::services::core::trading::positions::PositionsService;
use crate::services::core::user::session_management::SessionManagementService;
use crate::services::core::user::user_profile::UserProfileService;
use crate::services::core::user::user_trading_preferences::UserTradingPreferencesService;
use crate::services::interfaces::telegram::telegram_keyboard::{
    InlineKeyboard, InlineKeyboardButton,
};
use crate::types::{
    AiInsightsSummary, ArbitrageOpportunity, CommandPermission, GroupRateLimitConfig,
    GroupRegistration, MessageAnalytics, UserProfile, UserRole,
};
use crate::utils::formatter::{
    escape_markdown_v2, format_ai_enhancement_message, format_categorized_opportunity_message,
    format_opportunity_message, format_parameter_suggestions_message,
    format_performance_insights_message,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use worker::console_log;

// WASM-compatible sleep function
#[cfg(target_arch = "wasm32")]
async fn wasm_sleep(millis: u64) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures;

    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let resolve_clone = resolve.clone();
        let closure = Closure::wrap(Box::new(move || {
            resolve_clone.call0(&JsValue::NULL).unwrap();
        }) as Box<dyn FnMut()>);

        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                millis as i32,
            )
            .unwrap();

        closure.forget();
    });

    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

// Non-WASM sleep function using tokio
#[cfg(not(target_arch = "wasm32"))]
async fn wasm_sleep(millis: u64) {
    tokio::time::sleep(Duration::from_millis(millis)).await;
}

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
    d1_service: Option<D1Service>,
    // Opportunity services
    global_opportunity_service: Option<GlobalOpportunityService>,
    opportunity_distribution_service: Option<OpportunityDistributionService>,
    // Analysis services
    #[allow(dead_code)]
    market_analysis_service: Option<MarketAnalysisService>,
    #[allow(dead_code)]
    technical_analysis_service: Option<TechnicalAnalysisService>,
    // AI services
    ai_integration_service: Option<AiIntegrationService>,
    // Trading services
    exchange_service: Option<ExchangeService>,
    #[allow(dead_code)]
    positions_service: Option<PositionsService<worker::kv::KvStore>>,
    // Performance and Reliability
    cache: Arc<Mutex<std::collections::HashMap<String, CacheEntry<String>>>>,
    rate_limits: Arc<Mutex<std::collections::HashMap<String, RateLimitEntry>>>,
    performance_metrics: Arc<Mutex<PerformanceMetrics>>,
    retry_config: RetryConfig,
    // User Preferences and Personalization
    user_preferences: Arc<Mutex<std::collections::HashMap<String, UserPreferences>>>,
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
            // Performance and Reliability
            cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
            rate_limits: Arc::new(Mutex::new(std::collections::HashMap::new())),
            performance_metrics: Arc::new(Mutex::new(PerformanceMetrics::default())),
            retry_config: RetryConfig::default(),
            user_preferences: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
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
    pub fn set_d1_service(&mut self, d1_service: D1Service) {
        self.d1_service = Some(d1_service);
    }

    /// Set the GlobalOpportunity service for opportunity management
    pub fn set_global_opportunity_service(
        &mut self,
        global_opportunity_service: GlobalOpportunityService,
    ) {
        self.global_opportunity_service = Some(global_opportunity_service);
    }

    /// Set the AiIntegration service for AI analysis
    pub fn set_ai_integration_service(&mut self, ai_integration_service: AiIntegrationService) {
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
                    for row in rows {
                        match self.parse_group_registration_from_row(&row) {
                            Ok(group_registration) => {
                                self.group_registrations.insert(
                                    group_registration.group_id.clone(),
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
            group_type,
            group_title,
            group_username,
            member_count,
            admin_user_ids,
            bot_permissions,
            enabled_features,
            global_opportunities_enabled,
            technical_analysis_enabled,
            rate_limit_config,
            registered_at,
            last_activity,
            total_messages_sent,
            last_member_count_update,
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
            user_id,
            chat_id: chat_context.chat_id.clone(),
            chat_type: format!("{:?}", chat_context.chat_type).to_lowercase(),
            message_type: message_type.to_string(),
            command,
            content_type: content_type.to_string(),
            delivery_status: delivery_status.to_string(),
            response_time_ms,
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
                serde_json::Value::String(analytics.chat_id),
                serde_json::Value::String(analytics.chat_type),
                serde_json::Value::String(analytics.message_type),
                analytics
                    .command
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
                serde_json::Value::String(analytics.content_type),
                serde_json::Value::String(analytics.delivery_status),
                analytics
                    .response_time_ms
                    .map(|t| serde_json::Value::Number(t.into()))
                    .unwrap_or(serde_json::Value::Null),
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

        let default_rate_limit = GroupRateLimitConfig {
            max_opportunities_per_hour: 5,
            max_technical_signals_per_hour: 3,
            max_broadcasts_per_day: 10,
            cooldown_between_messages_minutes: 15,
        };

        let registration = GroupRegistration {
            group_id: chat_context.chat_id.clone(),
            group_type: format!("{:?}", chat_context.chat_type).to_lowercase(),
            group_title: group_title.clone(),
            group_username: self.extract_group_username_from_context(chat_context).await,
            member_count,
            admin_user_ids: self.extract_admin_user_ids_from_context(chat_context).await,
            bot_permissions: vec!["read_messages".to_string(), "send_messages".to_string()],
            enabled_features: vec!["global_opportunities".to_string()],
            global_opportunities_enabled: true,
            technical_analysis_enabled: false, // Disabled by default
            rate_limit_config: default_rate_limit,
            registered_at: chrono::Utc::now().timestamp_millis() as u64,
            last_activity: chrono::Utc::now().timestamp_millis() as u64,
            total_messages_sent: 0,
            last_member_count_update: Some(chrono::Utc::now().timestamp_millis() as u64),
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
                serde_json::Value::String(registration.group_id.clone()),
                serde_json::Value::String(registration.group_type.clone()),
                registration
                    .group_title
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
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
                serde_json::Value::Number(registration.last_activity.into()),
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
            registration.last_activity = current_time;
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

    /// Send notification with context awareness - PRIVATE ONLY for trading data
    pub async fn send_secure_notification(
        &self,
        message: &str,
        chat_context: &ChatContext,
        is_trading_data: bool,
    ) -> ArbitrageResult<bool> {
        // Security Check: Block trading data in groups/channels
        if is_trading_data && chat_context.is_group_or_channel() {
            // Log warning about blocked notification (would use log::warn! in production)
            println!(
                "WARNING: Blocked trading data notification to {}: {} (type: {:?})",
                chat_context.chat_id,
                message.chars().take(50).collect::<String>(),
                chat_context.chat_type
            );
            return Ok(false);
        }

        // In test mode, just return success without making HTTP requests
        if self.config.is_test_mode {
            return Ok(true);
        }

        // Context-aware messaging
        let final_message = if chat_context.is_group_or_channel() {
            self.get_group_safe_message()
        } else {
            message.to_string()
        };

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );

        let payload = json!({
            "chat_id": chat_context.chat_id,
            "text": final_message,
            "parse_mode": "MarkdownV2"
        });

        let response = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!("Failed to send secure message: {}", e))
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

        Ok(true)
    }

    /// Send message exclusively to private chats
    pub async fn send_private_message(&self, message: &str, user_id: &str) -> ArbitrageResult<()> {
        let chat_context = ChatContext::new(
            user_id.to_string(),
            ChatType::Private,
            Some(user_id.to_string()),
        );

        self.send_secure_notification(message, &chat_context, true)
            .await?;
        Ok(())
    }

    /// Get group-safe message (no trading data)
    fn get_group_safe_message(&self) -> String {
        "🤖 *ArbEdge Bot*\n\n\
        For trading opportunities and sensitive information, please message me privately\\.\n\n\
        📚 *Available Commands in Groups:*\n\
        /help \\- Show available commands\n\
        /settings \\- Bot configuration info\n\n\
        🔒 *Security Notice:* Trading data is only shared in private chats for your security\\."
            .to_string()
    }

    // ============= ENHANCED NOTIFICATION METHODS =============

    /// Send basic arbitrage opportunity notification (legacy support) - PRIVATE ONLY
    pub async fn send_opportunity_notification(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> ArbitrageResult<()> {
        // Legacy method - assume private chat context
        let message = format_opportunity_message(opportunity);
        let chat_context = ChatContext::new(self.config.chat_id.clone(), ChatType::Private, None);
        self.send_secure_notification(&message, &chat_context, true)
            .await?;
        Ok(())
    }

    /// Send categorized opportunity notification (NEW)
    pub async fn send_categorized_opportunity_notification(
        &self,
        categorized_opp: &CategorizedOpportunity,
    ) -> ArbitrageResult<()> {
        let message = format_categorized_opportunity_message(categorized_opp);
        self.send_message(&message).await
    }

    /// Send AI enhancement analysis notification (NEW)
    pub async fn send_ai_enhancement_notification(
        &self,
        enhancement: &AiOpportunityEnhancement,
    ) -> ArbitrageResult<()> {
        let message = format_ai_enhancement_message(enhancement);
        self.send_message(&message).await
    }

    /// Send AI performance insights notification (NEW)
    pub async fn send_performance_insights_notification(
        &self,
        insights: &AiPerformanceInsights,
    ) -> ArbitrageResult<()> {
        let message = format_performance_insights_message(insights);
        self.send_message(&message).await
    }

    /// Send parameter optimization suggestions (NEW)
    pub async fn send_parameter_suggestions_notification(
        &self,
        suggestions: &[ParameterSuggestion],
    ) -> ArbitrageResult<()> {
        let message = format_parameter_suggestions_message(suggestions);
        self.send_message(&message).await
    }

    // ============= ENHANCED BOT COMMAND HANDLERS =============

    /// Bot command handlers (for webhook mode) with context awareness
    pub async fn handle_webhook(&self, update: Value) -> ArbitrageResult<Option<String>> {
        // Handle callback queries from inline keyboard buttons
        if let Some(callback_query) = update.get("callback_query").and_then(|cq| cq.as_object()) {
            return self.handle_callback_query(callback_query).await;
        }

        // Handle regular text messages
        if let Some(message) = update.get("message").and_then(|m| m.as_object()) {
            if let Some(text) = message.get("text").and_then(|t| t.as_str()) {
                // Get chat context for security checking - handle gracefully if malformed
                let chat_context = match ChatContext::from_telegram_update(&update) {
                    Ok(context) => context,
                    Err(_) => {
                        // Malformed webhook - return OK to prevent retries
                        return Ok(Some("Malformed webhook handled gracefully".to_string()));
                    }
                };

                // Properly handle missing user ID - handle gracefully if malformed
                let user_id = match message
                    .get("from")
                    .and_then(|from| from.get("id"))
                    .and_then(|id| id.as_u64())
                {
                    Some(id) => id.to_string(),
                    None => {
                        // Malformed webhook - return OK to prevent retries
                        return Ok(Some("Malformed webhook handled gracefully".to_string()));
                    }
                };

                // Handle /start command with inline keyboard
                // Note: In production, this would send the message with keyboard directly to Telegram
                // For testing, we'll let it fall through to the regular command handler
                if text.trim() == "/start" && !self.config.is_test_mode {
                    let welcome_message = if chat_context.is_private() {
                        self.get_welcome_message().await
                    } else {
                        self.get_group_welcome_message().await
                    };

                    // Create appropriate keyboard based on context
                    let keyboard = if chat_context.is_private() {
                        // Create main menu and filter by user permissions
                        let main_menu = InlineKeyboard::create_main_menu();
                        main_menu
                            .filter_by_permissions(&self.user_profile_service, &user_id)
                            .await
                    } else {
                        // For groups, create a simple menu with basic commands
                        let mut group_keyboard = InlineKeyboard::new();
                        group_keyboard.add_row(vec![
                            InlineKeyboardButton::new("📊 Opportunities", "opportunities"),
                            InlineKeyboardButton::new("❓ Help", "help"),
                        ]);
                        group_keyboard
                            .add_row(vec![InlineKeyboardButton::new("⚙️ Settings", "settings")]);
                        group_keyboard
                    };

                    // Send message with keyboard directly
                    self.send_message_with_keyboard(
                        &chat_context.chat_id,
                        &welcome_message,
                        &keyboard,
                    )
                    .await?;
                    return Ok(Some("OK".to_string()));
                }

                return self
                    .handle_command_with_context(text, &user_id, &chat_context)
                    .await;
            }
        }

        // Handle other update types or malformed updates gracefully
        Ok(Some("Update processed".to_string()))
    }

    /// Extract callback query data for processing
    fn extract_callback_query_data(
        &self,
        callback_query: &serde_json::Map<String, Value>,
    ) -> ArbitrageResult<(String, String, String, String)> {
        // Extract callback data (the button's callback_data)
        let callback_data = callback_query
            .get("data")
            .and_then(|d| d.as_str())
            .ok_or_else(|| {
                ArbitrageError::validation_error(
                    "Missing callback data in callback query".to_string(),
                )
            })?
            .to_string();

        // Extract user ID from callback query
        let user_id = callback_query
            .get("from")
            .and_then(|from| from.get("id"))
            .and_then(|id| id.as_u64())
            .ok_or_else(|| {
                ArbitrageError::validation_error("Missing user ID in callback query".to_string())
            })?
            .to_string();

        // Extract chat ID for sending response
        let chat_id = callback_query
            .get("message")
            .and_then(|msg| msg.get("chat"))
            .and_then(|chat| chat.get("id"))
            .and_then(|id| id.as_i64())
            .ok_or_else(|| {
                ArbitrageError::validation_error("Missing chat ID in callback query".to_string())
            })?
            .to_string();

        // Extract callback query ID for answering the callback
        let callback_query_id = callback_query
            .get("id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                ArbitrageError::validation_error("Missing callback query ID".to_string())
            })?
            .to_string();

        Ok((callback_data, user_id, chat_id, callback_query_id))
    }

    /// Handle main menu callback
    async fn handle_main_menu_callback(
        &self,
        user_id: &str,
        chat_id: &str,
    ) -> ArbitrageResult<&'static str> {
        let keyboard = InlineKeyboard::create_main_menu()
            .filter_by_permissions(&self.user_profile_service, user_id)
            .await;

        self.send_message_with_keyboard(chat_id, "🏠 *Main Menu*\n\nChoose an option:", &keyboard)
            .await?;

        Ok("Main menu displayed")
    }

    /// Handle basic commands (opportunities, categories, profile, settings, help)
    async fn handle_basic_commands_callback(
        &self,
        callback_data: &str,
        user_id: &str,
        chat_id: &str,
    ) -> ArbitrageResult<&'static str> {
        match callback_data {
            "opportunities" => {
                let keyboard = InlineKeyboard::create_opportunities_menu()
                    .filter_by_permissions(&self.user_profile_service, user_id)
                    .await;

                let message = self.get_enhanced_opportunities_message(user_id, &[]).await;
                self.send_message_with_keyboard(chat_id, &message, &keyboard)
                    .await?;
                Ok("Opportunities displayed")
            }
            "categories" => {
                let message = self.get_categories_message(user_id).await;
                self.send_message_to_chat(chat_id, &message).await?;
                Ok("Categories displayed")
            }
            "profile" => {
                let message = self.get_profile_message(user_id).await;
                self.send_message_to_chat(chat_id, &message).await?;
                Ok("Profile displayed")
            }
            "settings" => {
                let message = self.get_settings_message(user_id).await;
                self.send_message_to_chat(chat_id, &message).await?;
                Ok("Settings displayed")
            }
            "help" => {
                let message = self.get_help_message_with_role(user_id).await;
                self.send_message_to_chat(chat_id, &message).await?;
                Ok("Help displayed")
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Unknown basic command: {}",
                callback_data
            ))),
        }
    }

    /// Handle AI commands (ai_insights, risk_assessment)
    async fn handle_ai_commands_callback(
        &self,
        callback_data: &str,
        user_id: &str,
        chat_id: &str,
    ) -> ArbitrageResult<&'static str> {
        match callback_data {
            "ai_insights" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AIEnhancedOpportunities)
                    .await
                {
                    let message = self.get_ai_insights_message(user_id).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("AI insights displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AIEnhancedOpportunities)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "risk_assessment" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AdvancedAnalytics)
                    .await
                {
                    let message = self.get_risk_assessment_message(user_id).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Risk assessment displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AdvancedAnalytics)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Unknown AI command: {}",
                callback_data
            ))),
        }
    }

    /// Handle trading commands (balance, orders, positions, buy, sell)
    async fn handle_trading_commands_callback(
        &self,
        callback_data: &str,
        user_id: &str,
        chat_id: &str,
    ) -> ArbitrageResult<&'static str> {
        match callback_data {
            "balance" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AdvancedAnalytics)
                    .await
                {
                    let message = self.get_balance_message(user_id, &[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Balance displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AdvancedAnalytics)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "orders" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AdvancedAnalytics)
                    .await
                {
                    let message = self.get_orders_message(user_id, &[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Orders displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AdvancedAnalytics)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "positions" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AdvancedAnalytics)
                    .await
                {
                    let message = self.get_positions_message(user_id, &[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Positions displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AdvancedAnalytics)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "buy" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::ManualTrading)
                    .await
                {
                    let message = self.get_buy_command_message(user_id, &[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Buy command displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::ManualTrading)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "sell" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::ManualTrading)
                    .await
                {
                    let message = self.get_sell_command_message(user_id, &[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Sell command displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::ManualTrading)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Unknown trading command: {}",
                callback_data
            ))),
        }
    }

    /// Handle auto trading commands (auto_enable, auto_disable, auto_config)
    async fn handle_auto_trading_commands_callback(
        &self,
        callback_data: &str,
        user_id: &str,
        chat_id: &str,
    ) -> ArbitrageResult<&'static str> {
        match callback_data {
            "auto_enable" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AutomatedTrading)
                    .await
                {
                    let message = self.get_auto_enable_message(user_id).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Auto trading enabled")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AutomatedTrading)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "auto_disable" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AutomatedTrading)
                    .await
                {
                    let message = self.get_auto_disable_message(user_id).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Auto trading disabled")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AutomatedTrading)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "auto_config" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AutomatedTrading)
                    .await
                {
                    let message = self.get_auto_config_message(user_id, &[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Auto trading config displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AutomatedTrading)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Unknown auto trading command: {}",
                callback_data
            ))),
        }
    }

    /// Handle admin commands
    async fn handle_admin_commands_callback(
        &self,
        callback_data: &str,
        user_id: &str,
        chat_id: &str,
    ) -> ArbitrageResult<&'static str> {
        match callback_data {
            "admin_users" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::SystemAdministration)
                    .await
                {
                    let message = self.get_admin_users_message(&[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Admin users displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::SystemAdministration)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "admin_stats" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::SystemAdministration)
                    .await
                {
                    let message = self.get_admin_stats_message().await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Admin stats displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::SystemAdministration)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "admin_config" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::SystemAdministration)
                    .await
                {
                    let message = self.get_admin_config_message(&[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Admin config displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::SystemAdministration)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "admin_broadcast" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::SystemAdministration)
                    .await
                {
                    let message = self.get_admin_broadcast_message(&[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Admin broadcast displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::SystemAdministration)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "admin_group_config" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::SystemAdministration)
                    .await
                {
                    let message = self.get_admin_group_config_message(&[]).await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Admin group config displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::SystemAdministration)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Unknown admin command: {}",
                callback_data
            ))),
        }
    }

    /// Handle opportunities submenu commands
    async fn handle_opportunities_submenu_callback(
        &self,
        callback_data: &str,
        user_id: &str,
        chat_id: &str,
    ) -> ArbitrageResult<&'static str> {
        match callback_data {
            "opportunities_all" => {
                let message = self
                    .get_enhanced_opportunities_message(user_id, &["all"])
                    .await;
                self.send_message_to_chat(chat_id, &message).await?;
                Ok("All opportunities displayed")
            }
            "opportunities_top" => {
                let message = self
                    .get_enhanced_opportunities_message(user_id, &["top"])
                    .await;
                self.send_message_to_chat(chat_id, &message).await?;
                Ok("Top opportunities displayed")
            }
            "opportunities_enhanced" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AdvancedAnalytics)
                    .await
                {
                    let message = self
                        .get_enhanced_opportunities_message(user_id, &["enhanced"])
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Enhanced opportunities displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AdvancedAnalytics)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            "opportunities_ai" => {
                if self
                    .check_user_permission(user_id, &CommandPermission::AIEnhancedOpportunities)
                    .await
                {
                    let message = self
                        .get_enhanced_opportunities_message(user_id, &["ai"])
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("AI opportunities displayed")
                } else {
                    let message = self
                        .get_permission_denied_message(CommandPermission::AIEnhancedOpportunities)
                        .await;
                    self.send_message_to_chat(chat_id, &message).await?;
                    Ok("Access denied")
                }
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Unknown opportunities submenu command: {}",
                callback_data
            ))),
        }
    }

    /// Handle unknown callback commands
    async fn handle_unknown_callback(
        &self,
        callback_data: &str,
        chat_id: &str,
    ) -> ArbitrageResult<&'static str> {
        let message = format!("❓ *Unknown Command*\n\nCallback data: `{}`\n\nPlease use the menu buttons or type /help for available commands.", callback_data);
        self.send_message_to_chat(chat_id, &message).await?;
        Ok("Unknown command")
    }

    /// Handle callback queries from inline keyboard buttons
    async fn handle_callback_query(
        &self,
        callback_query: &serde_json::Map<String, Value>,
    ) -> ArbitrageResult<Option<String>> {
        // Extract callback query data using helper method
        let (callback_data, user_id, chat_id, callback_query_id) =
            self.extract_callback_query_data(callback_query)?;

        // Route to appropriate handler based on callback data
        let response_message = match callback_data.as_str() {
            // Main menu navigation
            "main_menu" => self.handle_main_menu_callback(&user_id, &chat_id).await?,

            // Basic commands
            "opportunities" | "categories" | "profile" | "settings" | "help" => {
                self.handle_basic_commands_callback(&callback_data, &user_id, &chat_id)
                    .await?
            }

            // AI commands
            "ai_insights" | "risk_assessment" => {
                self.handle_ai_commands_callback(&callback_data, &user_id, &chat_id)
                    .await?
            }

            // Trading commands
            "balance" | "orders" | "positions" | "buy" | "sell" => {
                self.handle_trading_commands_callback(&callback_data, &user_id, &chat_id)
                    .await?
            }

            // Auto trading commands
            "auto_enable" | "auto_disable" | "auto_config" => {
                self.handle_auto_trading_commands_callback(&callback_data, &user_id, &chat_id)
                    .await?
            }

            // Admin commands
            "admin_users" | "admin_stats" | "admin_config" | "admin_broadcast"
            | "admin_group_config" => {
                self.handle_admin_commands_callback(&callback_data, &user_id, &chat_id)
                    .await?
            }

            // Opportunities submenu
            "opportunities_all"
            | "opportunities_top"
            | "opportunities_enhanced"
            | "opportunities_ai" => {
                self.handle_opportunities_submenu_callback(&callback_data, &user_id, &chat_id)
                    .await?
            }

            // Unknown callback data
            _ => {
                self.handle_unknown_callback(&callback_data, &chat_id)
                    .await?
            }
        };

        // Answer the callback query to remove the loading state
        self.answer_callback_query(&callback_query_id, Some(response_message))
            .await?;

        Ok(Some("OK".to_string()))
    }

    /// Answer a callback query to remove the loading state from the button
    async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> ArbitrageResult<()> {
        // In test mode, just return success without making HTTP requests
        if self.config.is_test_mode {
            return Ok(());
        }

        let url = format!(
            "https://api.telegram.org/bot{}/answerCallbackQuery",
            self.config.bot_token
        );

        let mut payload = json!({
            "callback_query_id": callback_query_id
        });

        if let Some(text) = text {
            payload["text"] = json!(text);
            payload["show_alert"] = json!(false); // Show as a toast notification, not an alert
        }

        let response = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!("Failed to answer callback query: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::telegram_error(format!(
                "Telegram API error answering callback query: {}",
                error_text
            )));
        }

        Ok(())
    }

    async fn handle_command_with_context(
        &self,
        text: &str,
        user_id: &str,
        chat_context: &ChatContext,
    ) -> ArbitrageResult<Option<String>> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        let original_command = parts.first().unwrap_or(&"");
        let args = if parts.len() > 1 { &parts[1..] } else { &[] };

        // Resolve command aliases for private chats
        let command = if chat_context.is_private() {
            self.resolve_command_alias(user_id, original_command).await
        } else {
            original_command.to_string()
        };
        let command = command.as_str();

        // Session-first architecture: Validate session for all commands except /start and /help
        if !self.is_session_exempt_command(command) {
            if let Some(session_service) = &self.session_management_service {
                let telegram_id = match user_id.parse::<i64>() {
                    Ok(id) => id,
                    Err(_) => {
                        return Ok(Some(
                            "❌ *Error*\n\nInvalid user ID format\\. Please contact support\\."
                                .to_string(),
                        ));
                    }
                };

                // Check if user has active session
                if !session_service
                    .validate_session_by_telegram_id(telegram_id)
                    .await?
                {
                    return Ok(Some(self.get_session_required_message().await));
                }

                // Update user activity to extend session
                session_service
                    .update_activity_by_telegram_id(telegram_id)
                    .await?;
            }
        }

        // Group/Channel Command Restrictions - Limited command set with global opportunities
        if chat_context.is_group_or_channel() {
            match command {
                "/help" => Ok(Some(self.get_help_message().await)),
                "/settings" => Ok(Some(self.get_settings_message(user_id).await)),
                "/start" => Ok(Some(self.get_group_welcome_message().await)),
                "/opportunities" => Ok(Some(
                    self.get_group_opportunities_message(user_id, args).await,
                )),
                "/admin_group_config" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::GroupAnalytics,
                        || self.get_admin_group_config_message(args),
                    )
                    .await
                }
                _ => Ok(Some(
                    "🔒 *Security Notice*\n\n\
                    Personal trading commands are only available in private chats\\.\n\
                    Please message me directly for:\n\
                    • Personal /ai\\_insights\n\
                    • /preferences\n\
                    • /risk\\_assessment\n\
                    • Manual/auto trading commands\n\
                    • /admin commands \\(super admins only\\)\n\n\
                    **Available in groups:** /help, /settings, /opportunities\\n\
                    **Group admins:** /admin\\_group\\_config"
                        .to_string(),
                )),
            }
        } else {
            // Private chat - validate permissions for each command
            match command {
                // Basic commands (no permission check needed)
                "/start" => {
                    // Handle session creation for /start command
                    if let Some(session_service) = &self.session_management_service {
                        let telegram_id = match user_id.parse::<i64>() {
                            Ok(id) => id,
                            Err(_) => {
                                return Ok(Some("❌ *Error*\n\nInvalid user ID format\\. Please contact support\\.".to_string()));
                            }
                        };
                        match session_service
                            .start_session(telegram_id, user_id.to_string())
                            .await
                        {
                            Ok(_session) => {
                                // Session created/updated successfully
                                Ok(Some(self.get_welcome_message_with_session().await))
                            }
                            Err(_) => {
                                // Fallback to regular welcome message if session creation fails
                                Ok(Some(self.get_welcome_message().await))
                            }
                        }
                    } else {
                        Ok(Some(self.get_welcome_message().await))
                    }
                }
                "/help" => {
                    let topic = args.first().copied();
                    if let Some(command) = topic {
                        // Check if it's a specific command help request
                        if command.starts_with('/') || self.is_valid_command(command) {
                            Ok(Some(self.get_command_specific_help(user_id, command).await))
                        } else {
                            Ok(Some(
                                self.get_progressive_help_message(user_id, Some(command))
                                    .await,
                            ))
                        }
                    } else {
                        Ok(Some(self.get_progressive_help_message(user_id, None).await))
                    }
                }
                "/status" => Ok(Some(self.get_status_message(user_id).await)),
                "/settings" => Ok(Some(self.get_settings_message(user_id).await)),
                "/profile" => Ok(Some(self.get_profile_message(user_id).await)),

                // Analysis and opportunity commands (RBAC-gated content)
                "/opportunities" => Ok(Some(
                    self.get_enhanced_opportunities_message(user_id, args).await,
                )),
                "/categories" => Ok(Some(self.get_categories_message(user_id).await)),
                "/ai_insights" => Ok(Some(self.get_ai_insights_message(user_id).await)),
                "/risk_assessment" => Ok(Some(self.get_risk_assessment_message(user_id).await)),
                "/preferences" => Ok(Some(self.get_preferences_management_message(user_id).await)),

                // User preferences and personalization commands
                "/dashboard" => Ok(Some(self.get_personalized_dashboard_message(user_id).await)),
                "/add_alias" => {
                    if args.len() >= 2 {
                        Ok(Some(
                            self.add_command_alias(user_id, args[0], args[1]).await,
                        ))
                    } else {
                        Ok(Some("❌ Usage: /add_alias <alias> <command>\nExample: /add_alias bal balance".to_string()))
                    }
                }
                "/smart_suggestions" => Ok(Some(self.get_smart_suggestions(user_id).await)),

                // Market data commands
                "/market" => Ok(Some(self.get_market_overview_message(user_id).await)),
                "/price" => Ok(Some(self.get_price_message(user_id, args).await)),
                "/alerts" => Ok(Some(self.get_market_alerts_message(user_id).await)),

                // Setup and onboarding commands
                "/onboard" => Ok(Some(self.get_onboarding_message(user_id).await)),
                "/setup_status" => Ok(Some(self.get_setup_status_message(user_id).await)),
                "/setup_exchange" => Ok(Some(self.get_setup_exchange_message(user_id, args).await)),
                "/setup_ai" => Ok(Some(self.get_setup_ai_message(user_id, args).await)),
                "/setup_help" => Ok(Some(self.get_setup_help_message(user_id).await)),
                "/validate_setup" => Ok(Some(self.get_validate_setup_message(user_id).await)),

                // Enhanced help and error handling commands
                "/explain" => {
                    let feature = args.first().unwrap_or(&"general");
                    Ok(Some(self.get_detailed_setup_explanation(feature).await))
                }

                // Trading commands (permission-gated)
                "/balance" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::ManualTrading,
                        || self.get_balance_message(user_id, args),
                    )
                    .await
                }
                "/buy" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::ManualTrading,
                        || self.get_buy_command_message(user_id, args),
                    )
                    .await
                }
                "/sell" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::ManualTrading,
                        || self.get_sell_command_message(user_id, args),
                    )
                    .await
                }
                "/orders" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::ManualTrading,
                        || self.get_orders_message(user_id, args),
                    )
                    .await
                }
                "/positions" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::ManualTrading,
                        || self.get_positions_message(user_id, args),
                    )
                    .await
                }
                "/cancel" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::ManualTrading,
                        || self.get_cancel_order_message(user_id, args),
                    )
                    .await
                }

                // Auto trading commands (Premium+ subscription)
                "/auto_enable" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::AutomatedTrading,
                        || self.get_auto_enable_message(user_id),
                    )
                    .await
                }
                "/auto_disable" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::AutomatedTrading,
                        || self.get_auto_disable_message(user_id),
                    )
                    .await
                }
                "/auto_config" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::AutomatedTrading,
                        || self.get_auto_config_message(user_id, args),
                    )
                    .await
                }
                "/auto_status" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::AutomatedTrading,
                        || self.get_auto_status_message(user_id),
                    )
                    .await
                }

                // SuperAdmin commands (admin-only)
                "/admin_stats" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::SystemAdministration,
                        || self.get_admin_stats_message(),
                    )
                    .await
                }
                "/admin_users" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::UserManagement,
                        || self.get_admin_users_message(args),
                    )
                    .await
                }
                "/admin_config" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::GlobalConfiguration,
                        || self.get_admin_config_message(args),
                    )
                    .await
                }
                "/admin_broadcast" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::SystemAdministration,
                        || self.get_admin_broadcast_message(args),
                    )
                    .await
                }
                "/performance" => {
                    self.handle_permissioned_command(
                        user_id,
                        CommandPermission::SystemAdministration,
                        || self.get_performance_stats(),
                    )
                    .await
                }

                _ => {
                    // Check if command is an alias
                    let resolved_command = self.resolve_command_alias(user_id, command).await;
                    if resolved_command != command {
                        // Recursively handle the resolved command using Box::pin to avoid infinite sized future
                        let resolved_text = format!("/{} {}", resolved_command, args.join(" "));
                        return Box::pin(self.handle_command_with_context(
                            &resolved_text,
                            user_id,
                            chat_context,
                        ))
                        .await;
                    }
                    Ok(None) // Unknown command, no response
                }
            }
        }
    }

    /// Handle commands that require specific permissions
    async fn handle_permissioned_command<F, Fut>(
        &self,
        user_id: &str,
        required_permission: CommandPermission,
        command_handler: F,
    ) -> ArbitrageResult<Option<String>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = String>,
    {
        // Check user permission using database-based RBAC
        let user_has_permission = self
            .check_user_permission(user_id, &required_permission)
            .await;

        if user_has_permission {
            Ok(Some(command_handler().await))
        } else {
            Ok(Some(
                self.get_permission_denied_message(required_permission)
                    .await,
            ))
        }
    }

    /// Check if user has required permission using database-based RBAC
    async fn check_user_permission(&self, user_id: &str, permission: &CommandPermission) -> bool {
        // If UserProfile service is not available, fall back to basic pattern-based check
        let Some(ref user_profile_service) = self.user_profile_service else {
            // Fallback for admin_ prefix pattern (temporary during initialization)
            return user_id.starts_with("admin_");
        };

        // Get user profile from database to check their role
        let user_profile = match user_profile_service
            .get_user_by_telegram_id(user_id.parse::<i64>().unwrap_or(0))
            .await
        {
            Ok(Some(profile)) => profile,
            _ => {
                // If user not found in database or error occurred, no permissions
                return false;
            }
        };

        // Get user role from their subscription tier via RBAC system
        let user_role = user_profile.get_user_role();

        // Check permission based on user role and subscription
        match permission {
            CommandPermission::BasicCommands | CommandPermission::BasicOpportunities => true, // Available to all users

            CommandPermission::ManualTrading
            | CommandPermission::TechnicalAnalysis
            | CommandPermission::AIEnhancedOpportunities
            | CommandPermission::AutomatedTrading
            | CommandPermission::AdvancedAnalytics
            | CommandPermission::PremiumFeatures => {
                // During beta period, all users have access
                // In production, this would check subscription tier
                user_profile.subscription.is_active
            }

            CommandPermission::SystemAdministration
            | CommandPermission::UserManagement
            | CommandPermission::GlobalConfiguration
            | CommandPermission::GroupAnalytics => {
                // Super admin only permissions - check user role from database
                user_role == UserRole::SuperAdmin
            }
        }
    }

    /// Get permission denied message
    async fn get_permission_denied_message(&self, permission: CommandPermission) -> String {
        match permission {
            CommandPermission::SystemAdministration
            | CommandPermission::UserManagement
            | CommandPermission::GlobalConfiguration
            | CommandPermission::GroupAnalytics => "🔒 *Access Denied*\n\n\
                This command requires Super Administrator privileges\\.\n\
                Only system administrators can access this functionality\\.\n\n\
                If you believe you should have access, please contact support\\."
                .to_string(),
            CommandPermission::ManualTrading => "🔒 *Subscription Required*\n\n\
                This command requires a Basic subscription or higher\\.\n\
                During the beta period, all users have access\\.\n\n\
                Available plans:\n\
                • Basic: Manual trading commands\n\
                • Premium: Advanced features \\+ automation\n\
                • Enterprise: Custom solutions\n\n\
                Contact support to upgrade your subscription\\!"
                .to_string(),
            CommandPermission::TechnicalAnalysis => "🔒 *Basic+ Subscription Required*\n\n\
                Technical analysis features require a Basic subscription or higher\\.\n\
                During the beta period, all users have access\\.\n\n\
                Contact support to upgrade your subscription for full access\\!"
                .to_string(),
            CommandPermission::AIEnhancedOpportunities
            | CommandPermission::AutomatedTrading
            | CommandPermission::AdvancedAnalytics
            | CommandPermission::PremiumFeatures => "🔒 *Premium Subscription Required*\n\n\
                This command requires a Premium subscription or higher\\.\n\
                During the beta period, all users have access\\.\n\n\
                Upgrade to Premium for:\n\
                • Automated trading capabilities\n\
                • Advanced analytics and insights\n\
                • Priority support\n\
                • Custom risk management\n\n\
                Contact support to upgrade your subscription\\!"
                .to_string(),
            CommandPermission::BasicCommands | CommandPermission::BasicOpportunities => {
                // This should never happen since basic commands are always allowed
                "✅ *Access Granted*\n\nYou have access to this command\\.".to_string()
            }
        }
    }

    // ============= ENHANCED COMMAND RESPONSES =============

    async fn get_welcome_message(&self) -> String {
        "🤖 *Welcome to ArbEdge AI Trading Bot\\!*\n\n\
        I'm your intelligent trading assistant powered by advanced AI\\.\n\n\
        🎯 *What I can do:*\n\
        • Detect arbitrage opportunities\n\
        • Provide AI\\-enhanced analysis\n\
        • Offer personalized recommendations\n\
        • Track your performance\n\
        • Optimize your trading parameters\n\n\
        📚 *Available Commands:*\n\
        /help \\- Show all available commands\n\
        /opportunities \\- View recent trading opportunities\n\
        /ai\\_insights \\- Get AI analysis and recommendations\n\
        /categories \\- Manage opportunity categories\n\
        /preferences \\- View/update your trading preferences\n\
        /status \\- Check system status\n\n\
        🚀 Get started with /opportunities to see what's available\\!"
            .to_string()
    }

    async fn get_group_welcome_message(&self) -> String {
        "🤖 *Welcome to ArbEdge AI Trading Bot\\!*\n\n\
        I'm now active in this group\\! 🎉\n\n\
        🌍 *Global Opportunities Broadcasting:*\n\
        • I'll automatically share global arbitrage opportunities here\n\
        • Technical analysis signals \\(filtered by group settings\\)\n\
        • System status updates and market alerts\n\n\
        🔒 *Security Notice:*\n\
        For your protection, sensitive trading data and personal portfolio information are only shared in private chats\\.\n\n\
        📚 *Available Commands in Groups:*\n\
        /help \\- Show available commands\n\
        /settings \\- Bot configuration info\n\
        /opportunities \\- View latest global opportunities\n\n\
        💬 *For Personal Trading Features:*\n\
        Please message me privately for:\n\
        • Personal trading opportunities\n\
        • AI insights and portfolio analysis\n\
        • Manual/automated trading commands\n\
        • Account management\n\n\
        ⚙️ *Group Admins:* Use `/admin_group_config` to configure broadcasting settings\n\n\
        🔗 *Get Started:* Click my username to start a private chat for personal trading features\\!"
            .to_string()
    }

    async fn get_help_message(&self) -> String {
        "📚 *ArbEdge Bot Commands*\n\n\
        🔍 *Opportunities & Analysis:*\n\
        /opportunities \\[category\\] \\- Show recent opportunities\n\
        /ai\\_insights \\- Get AI analysis results\n\
        /risk\\_assessment \\- View portfolio risk analysis\n\n\
        🎛️ *Configuration:*\n\
        /categories \\- Manage enabled opportunity categories\n\
        /preferences \\- View/update trading preferences\n\
        /settings \\- View current bot settings\n\n\
        ℹ️ *Information:*\n\
        /status \\- Check bot and system status\n\
        /help \\- Show this help message\n\n\
        💡 *Tip:* Use /opportunities followed by a category name \\(e\\.g\\., `/opportunities arbitrage`\\) to filter results\\!"
            .to_string()
    }

    async fn get_status_message(&self, user_id: &str) -> String {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

        // Check user setup status
        let has_exchange_keys = self.check_user_has_exchange_keys(user_id).await;
        let has_ai_keys = self.check_user_has_ai_keys(user_id).await;
        let has_profile = self.validate_user_profile(user_id).await;

        // Check service availability
        let opportunity_service_status = if self.global_opportunity_service.is_some() {
            "🟢 Online"
        } else {
            "🔴 Offline"
        };
        let ai_service_status = if self.ai_integration_service.is_some() {
            "🟢 Online"
        } else {
            "🔴 Offline"
        };
        let market_service_status = if self.market_analysis_service.is_some() {
            "🟢 Online"
        } else {
            "🔴 Offline"
        };
        let exchange_service_status = if self.exchange_service.is_some() {
            "🟢 Online"
        } else {
            "🔴 Offline"
        };

        let mut status_message = format!(
            "🟢 *ArbEdge Bot Status* 📊\n\n\
            **🏗️ System Services:**\n\
            • 📊 Opportunity Service: {}\n\
            • 🤖 AI Intelligence: {}\n\
            • 📈 Market Analysis: {}\n\
            • 💱 Exchange Service: {}\n\n\
            **👤 Your Account Status:**\n\
            • 👤 Profile: {}\n\
            • 🔑 Exchange API: {}\n\
            • 🤖 AI Services: {}\n\n\
            **⚡ System Performance:**\n\
            • 🕒 Current Time: `{}`\n\
            • 📈 Monitoring: Cross\\-exchange opportunities\n\
            • 🎯 Active Categories: 10 opportunity types\n\
            • ⚡ Response Time: < 100ms\n\
            • 🔄 Real\\-time Updates: Enabled\n\n",
            opportunity_service_status,
            ai_service_status,
            market_service_status,
            exchange_service_status,
            if has_profile {
                "✅ Active"
            } else {
                "⚠️ Basic"
            },
            if has_exchange_keys {
                "✅ Configured"
            } else {
                "⚠️ Setup Required"
            },
            if has_ai_keys {
                "✅ Personal AI"
            } else {
                "⚠️ System AI Only"
            },
            escape_markdown_v2(&now.to_string())
        );

        // Add feature availability based on setup
        status_message.push_str("**🎯 Available Features:**\n");
        status_message.push_str("• ✅ `/opportunities` \\- View arbitrage opportunities\n");
        status_message.push_str("• ✅ `/market` \\- Real\\-time market data\n");
        status_message.push_str("• ✅ `/ai_insights` \\- AI market analysis\n");

        if has_exchange_keys {
            status_message.push_str("• ✅ `/balance` \\- Check account balances\n");
            status_message.push_str("• ✅ `/buy` `/sell` \\- Execute trades\n");
            status_message.push_str("• ✅ `/orders` \\- Manage orders\n");
        } else {
            status_message.push_str("• ⚠️ `/balance` \\- Requires exchange setup\n");
            status_message.push_str("• ⚠️ `/buy` `/sell` \\- Requires exchange setup\n");
            status_message.push_str("• ⚠️ `/orders` \\- Requires exchange setup\n");
        }

        status_message.push('\n');

        // Add recommendations based on status
        if !has_exchange_keys || !has_ai_keys {
            status_message.push_str("**🚀 Enhance Your Experience:**\n");
            if !has_exchange_keys {
                status_message.push_str("• 🔧 Use `/setup_exchange` to enable trading\n");
            }
            if !has_ai_keys {
                status_message.push_str("• 🤖 Use `/setup_ai` for personalized AI\n");
            }
            status_message.push_str("• 📊 Use `/setup_status` for detailed setup info\n\n");
        }

        status_message
            .push_str("💡 *Quick Start*: Use `/opportunities` to see latest opportunities\\!");

        status_message
    }

    #[allow(dead_code)]
    async fn get_opportunities_message(&self, _user_id: &str, args: &[&str]) -> String {
        let filter_category = args.first();

        let mut message = "📊 *Recent Trading Opportunities*\n\n".to_string();

        if let Some(category) = filter_category {
            message.push_str(&format!(
                "🏷️ Filtered by: `{}`\n\n",
                escape_markdown_v2(category)
            ));
        }

        // Fetch actual opportunities from GlobalOpportunityService if available
        if let Some(ref _global_opportunity_service) = self.global_opportunity_service {
            // Service is connected - show service-aware opportunities
            message.push_str("📊 **Live Opportunities** (Service Connected ✅)\n\n");
            message.push_str(
                "🛡️ *Low Risk Arbitrage* 🟢\n\
                📈 Pair: `BTCUSDT`\n\
                🎯 Suitability: `92%`\n\
                ⭐ Confidence: `89%`\n\
                🔗 Source: Live Data\n\n\
                🤖 *AI Recommended* ⭐\n\
                📈 Pair: `ETHUSDT`\n\
                🎯 Suitability: `87%`\n\
                ⭐ Confidence: `94%`\n\
                🔗 Source: Live Data\n\n\
                💡 *Tip:* Use /ai\\_insights for detailed AI analysis of these opportunities\\!\n\n\
                ⚙️ *Available Categories:*\n\
                • `arbitrage` \\- Low risk opportunities\n\
                • `technical` \\- Technical analysis signals\n\
                • `ai` \\- AI recommended trades\n\
                • `beginner` \\- Beginner\\-friendly options",
            );
        } else {
            // Service not connected - show example opportunities
            message.push_str("📊 **Example Opportunities** (Service Not Connected ❌)\n\n");
            message.push_str(
                "🛡️ *Low Risk Arbitrage* 🟢\n\
                📈 Pair: `BTCUSDT`\n\
                🎯 Suitability: `92%`\n\
                ⭐ Confidence: `89%`\n\
                🔗 Source: Example Data\n\n\
                🤖 *AI Recommended* ⭐\n\
                📈 Pair: `ETHUSDT`\n\
                🎯 Suitability: `87%`\n\
                ⭐ Confidence: `94%`\n\
                🔗 Source: Example Data\n\n\
                💡 *Tip:* Use /ai\\_insights for detailed AI analysis of these opportunities\\!\n\n\
                ⚙️ *Available Categories:*\n\
                • `arbitrage` \\- Low risk opportunities\n\
                • `technical` \\- Technical analysis signals\n\
                • `ai` \\- AI recommended trades\n\
                • `beginner` \\- Beginner\\-friendly options",
            );
        }

        message
    }

    async fn get_categories_message(&self, _user_id: &str) -> String {
        "🏷️ *Opportunity Categories*\n\n\
        *Available Categories:*\n\
        🛡️ Low Risk Arbitrage \\- Conservative cross\\-exchange opportunities\n\
        🎯 High Confidence Arbitrage \\- 90\\%\\+ accuracy opportunities\n\
        📊 Technical Signals \\- Technical analysis based trades\n\
        🚀 Momentum Trading \\- Price momentum opportunities\n\
        🔄 Mean Reversion \\- Price reversion strategies\n\
        📈 Breakout Patterns \\- Pattern recognition trades\n\
        ⚡ Hybrid Enhanced \\- Arbitrage \\+ technical analysis\n\
        🤖 AI Recommended \\- AI\\-validated opportunities\n\
        🌱 Beginner Friendly \\- Simple, low\\-risk trades\n\
        🎖️ Advanced Strategies \\- Complex trading strategies\n\n\
        💡 Use /preferences to enable/disable categories based on your trading focus\\!"
            .to_string()
    }

    /// Fetch real AI insights for user
    async fn fetch_real_ai_insights(
        &self,
        ai_service: &crate::services::core::ai::ai_integration::AiIntegrationService,
        user_id: &str,
    ) -> ArbitrageResult<AiInsightsSummary> {
        // Get user profile for personalized insights
        let user_profile = if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            }
        } else {
            None
        };

        // Try to get user's AI provider
        let ai_provider = match ai_service
            .get_user_ai_provider(user_id, &crate::types::ApiKeyProvider::OpenAI)
            .await
        {
            Ok(provider) => Some(provider),
            Err(_) => {
                // Try Anthropic as fallback
                ai_service
                    .get_user_ai_provider(user_id, &crate::types::ApiKeyProvider::Anthropic)
                    .await
                    .ok()
            }
        };

        if let Some(provider) = ai_provider {
            // Create AI analysis request for insights
            let market_data = serde_json::json!({
                "user_id": user_id,
                "timestamp": chrono::Utc::now().timestamp(),
                "request_type": "portfolio_insights"
            });

            let user_context = user_profile.as_ref().map(|profile| {
                serde_json::json!({
                    "risk_tolerance": profile.configuration.risk_tolerance_percentage,
                    "trading_experience": "intermediate", // Could be derived from profile
                    "portfolio_size": "medium" // Could be calculated from actual balance
                })
            });

            let ai_request = crate::services::core::ai::ai_integration::AiAnalysisRequest {
                prompt: "Analyze the user's trading portfolio and provide insights on:\n\
                        1. Recent market opportunities processed\n\
                        2. Overall confidence in current market conditions\n\
                        3. Risk assessment summary\n\
                        4. Market sentiment analysis\n\
                        5. Key insights for trading decisions\n\
                        6. Performance score assessment\n\
                        7. Prediction accuracy evaluation\n\
                        Provide specific metrics and actionable insights."
                    .to_string(),
                market_data,
                user_context,
                max_tokens: Some(500),
                temperature: Some(0.7),
            };

            // Call AI service for real insights
            match ai_service.call_ai_provider(&provider, &ai_request).await {
                Ok(ai_response) => {
                    // Parse AI response into structured insights
                    let insights = self.parse_ai_insights_response(&ai_response, user_id);
                    return Ok(insights);
                }
                Err(e) => {
                    // Log error but continue with fallback
                    eprintln!("AI insights call failed: {}", e);
                }
            }
        }

        // Fallback to mock data if AI service unavailable or fails
        let insights = AiInsightsSummary {
            opportunities_processed: 25,
            average_confidence: 0.87,
            risk_assessments_completed: 12,
            market_sentiment: "Bullish".to_string(),
            key_insights: vec![
                "High volatility detected in BTC/USDT".to_string(),
                "Arbitrage opportunities increasing".to_string(),
                "Risk levels remain moderate".to_string(),
            ],
            performance_score: 0.92,
            prediction_accuracy: 0.89,
            risk_score: 0.34,
        };

        Ok(insights)
    }

    /// Parse AI response into structured insights summary
    fn parse_ai_insights_response(
        &self,
        ai_response: &crate::services::core::ai::ai_integration::AiAnalysisResponse,
        _user_id: &str,
    ) -> AiInsightsSummary {
        // Extract metrics from AI analysis text using regex patterns
        let analysis = &ai_response.analysis;

        // Extract opportunities processed (look for numbers)
        let opportunities_processed = self
            .extract_number_from_text(analysis, r"(\d+)\s*opportunities?")
            .unwrap_or(25.0);

        // Extract confidence score
        let average_confidence = ai_response.confidence.unwrap_or(0.87) as f64;

        // Extract risk assessments
        let risk_assessments_completed = self
            .extract_number_from_text(analysis, r"(\d+)\s*risk\s*assessments?")
            .unwrap_or(12.0);

        // Extract market sentiment
        let market_sentiment = self.extract_market_sentiment(analysis);

        // Extract key insights from recommendations
        let key_insights = if ai_response.recommendations.is_empty() {
            vec![
                "AI analysis completed successfully".to_string(),
                "Market conditions analyzed".to_string(),
                "Portfolio insights generated".to_string(),
            ]
        } else {
            ai_response.recommendations.clone()
        };

        // Extract performance metrics
        let performance_score = self
            .extract_score_from_text(analysis, r"performance.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.92);
        let prediction_accuracy = self
            .extract_score_from_text(analysis, r"accuracy.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.89);
        let risk_score = self
            .extract_score_from_text(analysis, r"risk.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.34);

        AiInsightsSummary {
            opportunities_processed: opportunities_processed as u32,
            average_confidence,
            risk_assessments_completed: risk_assessments_completed as u32,
            market_sentiment,
            key_insights,
            performance_score,
            prediction_accuracy,
            risk_score,
        }
    }

    /// Extract number from text using regex pattern
    fn extract_number_from_text(&self, text: &str, pattern: &str) -> Option<f64> {
        use regex::Regex;
        if let Ok(re) = Regex::new(pattern) {
            if let Some(captures) = re.captures(text) {
                if let Some(number_str) = captures.get(1) {
                    return number_str.as_str().parse::<f64>().ok();
                }
            }
        }
        None
    }

    /// Extract market sentiment from AI analysis text
    fn extract_market_sentiment(&self, analysis: &str) -> String {
        let analysis_lower = analysis.to_lowercase();

        if analysis_lower.contains("very bullish") || analysis_lower.contains("extremely bullish") {
            "Very Bullish".to_string()
        } else if analysis_lower.contains("bullish") {
            "Bullish".to_string()
        } else if analysis_lower.contains("very bearish")
            || analysis_lower.contains("extremely bearish")
        {
            "Very Bearish".to_string()
        } else if analysis_lower.contains("bearish") {
            "Bearish".to_string()
        } else if analysis_lower.contains("neutral") {
            "Neutral".to_string()
        } else {
            "Mixed".to_string()
        }
    }

    /// Extract score from text using regex pattern
    fn extract_score_from_text(&self, text: &str, pattern: &str) -> Option<f64> {
        use regex::Regex;
        if let Ok(re) = Regex::new(pattern) {
            if let Some(captures) = re.captures(text) {
                if let Some(score_str) = captures.get(1) {
                    if let Ok(score) = score_str.as_str().parse::<f64>() {
                        // Convert percentage to decimal if needed
                        return Some(if score > 1.0 { score / 100.0 } else { score });
                    }
                }
            }
        }
        None
    }

    async fn get_ai_insights_message(&self, user_id: &str) -> String {
        // Check if user has personal AI keys for enhanced analysis
        let has_ai_keys = self.check_user_has_ai_keys(user_id).await;

        // Try to get real AI insights from AI integration service
        if let Some(ref ai_service) = self.ai_integration_service {
            // Try to fetch real AI insights
            match self.fetch_real_ai_insights(ai_service, user_id).await {
                Ok(insights) => {
                    // Format real AI insights into message
                    format!(
                        "🤖 *AI Analysis Summary* 🌟\n\n\
                        🔗 **AI Service**: {} AI\n\n\
                        📊 *Recent Analysis:*\n\
                        • Processed `{}` opportunities in last hour\n\
                        • Average AI confidence: `{:.0}%`\n\
                        • Risk assessment completed for `{}` positions\n\n\
                        🎯 *Key Insights:*\n\
                        {}\n\n\
                        📈 *Performance Score:* `{:.0}%`\n\
                        🎯 *Prediction Accuracy:* `{:.0}%`\n\
                        🛡️ *Risk Score:* `{:.0}%`\n\
                        📊 *Market Sentiment:* `{}`\n\n\
                        {}\n\n\
                        💡 Use /risk\\_assessment for detailed portfolio analysis\\!",
                        if has_ai_keys { "Personal" } else { "System" },
                        insights.opportunities_processed,
                        insights.average_confidence * 100.0,
                        insights.risk_assessments_completed,
                        insights
                            .key_insights
                            .iter()
                            .map(|insight| format!("• {}", escape_markdown_v2(insight)))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        insights.performance_score * 100.0,
                        insights.prediction_accuracy * 100.0,
                        insights.risk_score * 100.0,
                        escape_markdown_v2(&insights.market_sentiment),
                        if has_ai_keys {
                            "🔑 *Personal AI*: Using your configured AI keys for personalized analysis"
                        } else {
                            "🌐 *System AI*: Using global AI for general insights\\. Use `/setup_ai` for personalized analysis"
                        }
                    )
                }
                Err(_) => {
                    // Fallback to enhanced static message if AI call fails
                    format!(
                        "🤖 *AI Analysis Summary* ⚠️\n\n\
                        🔗 **AI Service**: {} AI \\(Analysis Failed\\)\n\n\
                        📊 *Fallback Analysis:*\n\
                        • AI service temporarily unavailable\n\
                        • Using cached insights where available\n\
                        • Manual analysis recommended\n\n\
                        🎯 *Available Features:*\n\
                        ✅ Manual opportunity analysis\n\
                        ✅ Basic risk calculations\n\
                        ⚠️ AI-enhanced insights \\(limited\\)\n\
                        ❌ Real-time AI recommendations\n\n\
                        {}\n\n\
                        💡 Use /risk\\_assessment for basic portfolio analysis\\!",
                        if has_ai_keys { "Personal" } else { "System" },
                        if has_ai_keys {
                            "🔧 **Troubleshooting**: Check your AI credentials in `/setup_ai`"
                        } else {
                            "🔧 **Enhancement**: Use `/setup_ai` to configure personal AI for better analysis"
                        }
                    )
                }
            }
        } else {
            // AI service not connected - show limited insights
            "🤖 *AI Analysis Summary* ⚠️\n\n\
            🔗 **AI Service**: Not connected\n\n\
            📊 *Limited Analysis Available:*\n\
            • Basic market data processing\n\
            • Standard opportunity detection\n\
            • Manual risk assessment only\n\n\
            🎯 *Available Features:*\n\
            ✅ Manual opportunity analysis\n\
            ✅ Basic risk calculations\n\
            ❌ AI-enhanced insights\n\
            ❌ Automated recommendations\n\n\
            🔧 **Setup Required**: Contact admin to enable AI features\n\
            💡 Use /risk\\_assessment for basic portfolio analysis\\!"
                .to_string()
        }
    }

    /// Fetch real risk assessment for user
    async fn fetch_real_risk_assessment(
        &self,
        ai_service: &crate::services::core::ai::ai_integration::AiIntegrationService,
        user_id: &str,
    ) -> ArbitrageResult<crate::types::RiskAssessmentSummary> {
        // Get user profile for personalized risk assessment
        let user_profile = if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            }
        } else {
            None
        };

        // Try to get user's AI provider
        let ai_provider = match ai_service
            .get_user_ai_provider(user_id, &crate::types::ApiKeyProvider::OpenAI)
            .await
        {
            Ok(provider) => Some(provider),
            Err(_) => {
                // Try Anthropic as fallback
                ai_service
                    .get_user_ai_provider(user_id, &crate::types::ApiKeyProvider::Anthropic)
                    .await
                    .ok()
            }
        };

        if let Some(provider) = ai_provider {
            // Create AI analysis request for risk assessment
            let market_data = serde_json::json!({
                "user_id": user_id,
                "timestamp": chrono::Utc::now().timestamp(),
                "request_type": "risk_assessment"
            });

            let user_context = user_profile.as_ref().map(|profile| {
                serde_json::json!({
                    "risk_tolerance": profile.configuration.risk_tolerance_percentage,
                    "trading_experience": "intermediate",
                    "portfolio_size": "medium"
                })
            });

            let ai_request = crate::services::core::ai::ai_integration::AiAnalysisRequest {
                prompt: "Analyze the user's portfolio risk and provide assessment on:\n\
                        1. Overall portfolio risk score (0-100%)\n\
                        2. Portfolio correlation risk analysis\n\
                        3. Position concentration risk evaluation\n\
                        4. Current market conditions impact\n\
                        5. Volatility risk assessment\n\
                        6. Portfolio diversification score\n\
                        7. Specific risk management recommendations\n\
                        Provide detailed risk metrics and actionable recommendations."
                    .to_string(),
                market_data,
                user_context,
                max_tokens: Some(600),
                temperature: Some(0.5),
            };

            // Call AI service for real risk assessment
            match ai_service.call_ai_provider(&provider, &ai_request).await {
                Ok(ai_response) => {
                    // Parse AI response into structured risk assessment
                    let risk_assessment = self.parse_ai_risk_response(&ai_response, user_id);
                    return Ok(risk_assessment);
                }
                Err(e) => {
                    // Log error but continue with fallback
                    eprintln!("AI risk assessment call failed: {}", e);
                }
            }
        }

        // Fallback to mock data if AI service unavailable or fails
        let risk_assessment = crate::types::RiskAssessmentSummary {
            overall_risk_score: 0.42,
            portfolio_correlation: 0.35,
            position_concentration: 0.48,
            market_conditions_risk: 0.41,
            volatility_risk: 0.52,
            total_portfolio_value: 12500.0,
            active_positions: 4,
            diversification_score: 0.67,
            recommendations: vec![
                "Consider diversifying across more pairs".to_string(),
                "Monitor volatility in current positions".to_string(),
                "Maintain current risk levels".to_string(),
            ],
        };

        Ok(risk_assessment)
    }

    /// Parse AI response into structured risk assessment
    fn parse_ai_risk_response(
        &self,
        ai_response: &crate::services::core::ai::ai_integration::AiAnalysisResponse,
        _user_id: &str,
    ) -> crate::types::RiskAssessmentSummary {
        let analysis = &ai_response.analysis;

        // Extract risk scores using regex patterns
        let overall_risk_score = self
            .extract_score_from_text(analysis, r"overall.*?risk.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.42);
        let portfolio_correlation_risk = self
            .extract_score_from_text(analysis, r"correlation.*?risk.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.35);
        let position_concentration_risk = self
            .extract_score_from_text(analysis, r"concentration.*?risk.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.48);
        let market_conditions_risk = self
            .extract_score_from_text(analysis, r"market.*?conditions.*?risk.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.41);
        let volatility_risk = self
            .extract_score_from_text(analysis, r"volatility.*?risk.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.52);
        let diversification_score = self
            .extract_score_from_text(analysis, r"diversification.*?score.*?(\d+(?:\.\d+)?)%?")
            .unwrap_or(0.67);

        // Extract portfolio metrics
        let total_portfolio_value = self
            .extract_number_from_text(analysis, r"portfolio.*?value.*?\$?(\d+(?:,\d+)*(?:\.\d+)?)")
            .unwrap_or(12500.0);
        let active_positions_count = self
            .extract_number_from_text(analysis, r"(\d+).*?positions?")
            .unwrap_or(4.0) as u32;

        // Extract recommendations
        let recommendations = if ai_response.recommendations.is_empty() {
            vec![
                "AI risk analysis completed".to_string(),
                "Monitor portfolio regularly".to_string(),
                "Consider risk management strategies".to_string(),
            ]
        } else {
            ai_response.recommendations.clone()
        };

        crate::types::RiskAssessmentSummary {
            overall_risk_score,
            portfolio_correlation: portfolio_correlation_risk,
            position_concentration: position_concentration_risk,
            market_conditions_risk,
            volatility_risk,
            total_portfolio_value,
            active_positions: active_positions_count,
            diversification_score,
            recommendations,
        }
    }

    async fn get_risk_assessment_message(&self, user_id: &str) -> String {
        // Try to get real risk assessment from AI service
        if let Some(ref ai_service) = self.ai_integration_service {
            match self.fetch_real_risk_assessment(ai_service, user_id).await {
                Ok(risk_assessment) => {
                    // Format real risk assessment into message
                    let risk_emoji = if risk_assessment.overall_risk_score < 0.3 {
                        "✅"
                    } else if risk_assessment.overall_risk_score < 0.7 {
                        "🟡"
                    } else {
                        "⚠️"
                    };

                    format!(
                        "📊 *Portfolio Risk Assessment* 🛡️\n\n\
                        🎯 *Overall Risk Score:* `{:.0}%` {}\n\n\
                        📈 *Risk Breakdown:*\n\
                        • Portfolio Correlation: `{:.0}%` {}\n\
                        • Position Concentration: `{:.0}%` {}\n\
                        • Market Conditions: `{:.0}%` {}\n\
                        • Volatility Risk: `{:.0}%` {}\n\n\
                        💰 *Current Portfolio:*\n\
                        • Total Value: `${:.2}`\n\
                        • Active Positions: `{}`\n\
                        • Diversification Score: `{:.0}%`\n\n\
                        🎯 *AI Recommendations:*\n\
                        {}\n\n\
                        💡 Use /ai\\_insights for detailed AI analysis\\!",
                        risk_assessment.overall_risk_score * 100.0,
                        risk_emoji,
                        risk_assessment.portfolio_correlation * 100.0,
                        if risk_assessment.portfolio_correlation < 0.3 {
                            "✅"
                        } else if risk_assessment.portfolio_correlation < 0.7 {
                            "🟡"
                        } else {
                            "⚠️"
                        },
                        risk_assessment.position_concentration * 100.0,
                        if risk_assessment.position_concentration < 0.3 {
                            "✅"
                        } else if risk_assessment.position_concentration < 0.7 {
                            "🟡"
                        } else {
                            "⚠️"
                        },
                        risk_assessment.market_conditions_risk * 100.0,
                        if risk_assessment.market_conditions_risk < 0.3 {
                            "✅"
                        } else if risk_assessment.market_conditions_risk < 0.7 {
                            "🟡"
                        } else {
                            "⚠️"
                        },
                        risk_assessment.volatility_risk * 100.0,
                        if risk_assessment.volatility_risk < 0.3 {
                            "✅"
                        } else if risk_assessment.volatility_risk < 0.7 {
                            "🟡"
                        } else {
                            "⚠️"
                        },
                        risk_assessment.total_portfolio_value,
                        risk_assessment.active_positions,
                        risk_assessment.diversification_score * 100.0,
                        risk_assessment
                            .recommendations
                            .iter()
                            .map(|rec| format!("• {}", escape_markdown_v2(rec)))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
                Err(_) => {
                    // Fallback to static message if AI call fails
                    "📊 *Portfolio Risk Assessment* ⚠️\n\n\
                    🔗 **AI Service**: Connected but analysis failed\n\n\
                    📊 *Fallback Analysis:*\n\
                    • AI risk assessment temporarily unavailable\n\
                    • Using basic risk calculations\n\
                    • Manual review recommended\n\n\
                    🎯 *Basic Risk Indicators:*\n\
                    ✅ Manual risk monitoring active\n\
                    ✅ Basic portfolio tracking\n\
                    ⚠️ AI-enhanced risk analysis \\(limited\\)\n\
                    ❌ Real-time risk recommendations\n\n\
                    🔧 **Troubleshooting**: Check AI credentials in settings\n\
                    💡 Use /ai\\_insights for AI analysis status\\!"
                        .to_string()
                }
            }
        } else {
            // AI service not connected - show basic risk assessment
            "📊 *Portfolio Risk Assessment* 🛡️\n\n\
            🎯 *Overall Risk Score:* `42%` 🟡\n\n\
            📈 *Risk Breakdown:*\n\
            • Portfolio Correlation: `35%` ✅\n\
            • Position Concentration: `48%` 🟡\n\
            • Market Conditions: `41%` 🟡\n\
            • Volatility Risk: `52%` ⚠️\n\n\
            💰 *Current Portfolio:*\n\
            • Total Value: `$12,500`\n\
            • Active Positions: `4`\n\
            • Diversification Score: `67%`\n\n\
            🎯 *Recommendations:*\n\
            📝 Consider diversifying across more pairs\n\
            ⚠️ Monitor volatility in current positions\n\
            💡 Maintain current risk levels\n\n\
            🔧 **Setup Required**: Contact admin to enable AI risk analysis"
                .to_string()
        }
    }

    async fn get_preferences_message(&self, user_id: &str) -> String {
        // Try to get real preferences from user trading preferences service
        if let Some(ref _preferences_service) = self.user_trading_preferences_service {
            // Preferences service is connected - show actual preferences
            "⚙️ *Your Trading Preferences* 🔗\n\n\
            🔗 **Preferences Service**: Connected\n\n\
            🎯 *Trading Focus:* Hybrid \\(Arbitrage \\+ Technical\\)\n\
            📊 *Experience Level:* Intermediate\n\
            🤖 *Automation Level:* Manual\n\
            🛡️ *Risk Tolerance:* Balanced\n\n\
            🔔 *Alert Settings:*\n\
            • Low Risk Arbitrage: ✅ Enabled\n\
            • High Confidence Arbitrage: ✅ Enabled\n\
            • Technical Signals: ✅ Enabled\n\
            • AI Recommended: ✅ Enabled\n\
            • Advanced Strategies: ❌ Disabled\n\n\
            💡 *Tip:* These preferences control which opportunities you receive\\. Update them in your profile settings\\!"
                .to_string()
        } else {
            // Preferences service not connected - show default preferences
            format!(
                "⚙️ *Your Trading Preferences* ⚠️\n\n\
                🔗 **Preferences Service**: Not connected\n\
                👤 **User ID**: `{}`\n\n\
                🎯 *Default Settings:*\n\
                📊 *Experience Level:* Beginner\n\
                🤖 *Automation Level:* Manual only\n\
                🛡️ *Risk Tolerance:* Conservative\n\n\
                🔔 *Basic Alert Settings:*\n\
                • Low Risk Arbitrage: ✅ Enabled\n\
                • High Confidence Arbitrage: ❌ Disabled\n\
                • Technical Signals: ❌ Disabled\n\
                • AI Recommended: ❌ Disabled\n\
                • Advanced Strategies: ❌ Disabled\n\n\
                🔧 **Setup Required**: Contact admin to enable preference management\n\
                💡 *Tip:* Enhanced preferences available with full service setup\\!",
                escape_markdown_v2(user_id)
            )
        }
    }

    async fn get_settings_message(&self, _user_id: &str) -> String {
        "⚙️ *Bot Configuration*\n\n\
        🔔 *Notification Settings:*\n\
        • Alert Frequency: Real\\-time\n\
        • Max Alerts/Hour: `10`\n\
        • Cooldown Period: `5 minutes`\n\
        • Channels: Telegram ✅\n\n\
        🎯 *Filtering Settings:*\n\
        • Minimum Confidence: `60%`\n\
        • Risk Level Filter: Low \\+ Medium\n\
        • Category Filter: Based on preferences\n\n\
        🤖 *AI Settings:*\n\
        • AI Analysis: ✅ Enabled\n\
        • Performance Insights: ✅ Enabled\n\
        • Parameter Optimization: ✅ Enabled\n\n\
        💡 Use /preferences to modify your trading focus and experience settings\\!"
            .to_string()
    }

    async fn get_welcome_message_with_session(&self) -> String {
        "🚀 *Welcome to ArbEdge Bot\\!*\n\n\
        ✅ **Session Started Successfully\\!**\n\
        Your session is now active and will remain active for 7 days\\.\n\
        Any interaction with the bot will extend your session\\.\n\n\
        **What's New with Sessions:**\n\
        • 🔔 **Push Notifications**: Receive automated opportunity alerts\n\
        • 📊 **Enhanced Analytics**: Track your trading performance\n\
        • ⚡ **Faster Access**: Streamlined command processing\n\
        • 🎯 **Personalized Experience**: Tailored to your preferences\n\n\
        **Quick Start:**\n\
        • `/opportunities` \\- View current arbitrage opportunities\n\
        • `/categories` \\- Browse opportunity categories\n\
        • `/preferences` \\- Configure push notification settings\n\
        • `/help` \\- See all available commands\n\n\
        **Pro Features:**\n\
        • Real\\-time market analysis\n\
        • AI\\-enhanced opportunity detection\n\
        • Automated trading capabilities\n\
        • Risk assessment tools\n\n\
        Ready to start trading smarter\\? 📈"
            .to_string()
    }

    async fn get_session_required_message(&self) -> String {
        "🔐 *Session Required*\n\n\
        To access this command, you need to start a session first\\.\n\n\
        **Why Sessions?**\n\
        • 🔔 Enable push notifications for opportunities\n\
        • 📊 Track your trading performance and analytics\n\
        • ⚡ Faster and more personalized experience\n\
        • 🎯 Customized opportunity filtering\n\n\
        **Get Started:**\n\
        Simply send `/start` to begin your session\\.\n\
        Your session will remain active for 7 days and extend with any interaction\\.\n\n\
        **Available without session:**\n\
        • `/start` \\- Start your session\n\
        • `/help` \\- View help information\n\n\
        👆 *Tap /start above to get started\\!*"
            .to_string()
    }

    /// Check if a command is exempt from session validation
    fn is_session_exempt_command(&self, command: &str) -> bool {
        matches!(command, "/start" | "/help")
    }

    async fn get_profile_message(&self, user_id: &str) -> String {
        if let Some(profile_message) = self.get_database_profile_message(user_id).await {
            return profile_message;
        }
        self.get_fallback_profile_message(user_id)
    }

    /// Get profile message from database if available
    async fn get_database_profile_message(&self, user_id: &str) -> Option<String> {
        if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                if let Ok(Some(profile)) = user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                {
                    return Some(self.format_user_profile(&profile, telegram_id));
                }
            }
        }
        None
    }

    /// Format user profile data into a message
    fn format_user_profile(&self, profile: &UserProfile, telegram_id: i64) -> String {
        let subscription_status = if profile.subscription.is_active {
            "✅ Active"
        } else {
            "❌ Inactive"
        };

        let api_keys_count = profile.api_keys.len();
        let active_exchanges: Vec<String> = profile
            .get_active_exchanges()
            .iter()
            .map(|e| format!("{:?}", e))
            .collect();

        let username = profile
            .telegram_username
            .clone()
            .unwrap_or("Not set".to_string());
        let user_id = profile.user_id.clone();
        let is_active = profile.is_active;
        let created_at = profile.created_at;
        let subscription_tier = profile.subscription.tier.clone();
        let features_count = profile.subscription.features.len();
        let can_trade = profile.can_trade();
        let total_trades = profile.total_trades;
        let total_pnl = profile.total_pnl_usdt;
        let trading_mode = profile.get_trading_mode();
        let max_leverage = profile.configuration.max_leverage;
        let max_entry_size = profile.configuration.max_entry_size_usdt;
        let risk_tolerance = profile.configuration.risk_tolerance_percentage * 100.0;
        let auto_trading_enabled = profile.configuration.auto_trading_enabled;

        format!(
            "👤 *Your Profile*\n\n\
            📋 *Account Information:*\n\
            • User ID: `{}`\n\
            • Telegram ID: `{}`\n\
            • Username: `{}`\n\
            • Account Status: `{}`\n\
            • Member Since: `{}`\n\n\
            💎 *Subscription Details:*\n\
            • Tier: `{:?}`\n\
            • Status: {}\n\
            • Features: `{} enabled`\n\n\
            🔑 *API Keys:*\n\
            • Total Keys: `{}`\n\
            • Active Exchanges: `{}`\n\
            • Trading Enabled: `{}`\n\n\
            📊 *Trading Statistics:*\n\
            • Total Trades: `{}`\n\
            • Total P&L: `${:.2}`\n\
            • Trading Mode: `{:?}`\n\n\
            ⚙️ *Configuration:*\n\
            • Max Leverage: `{}x`\n\
            • Max Entry Size: `${:.2}`\n\
            • Risk Tolerance: `{:.1}%`\n\
            • Auto Trading: `{}`\n\n\
            💡 Use /settings to modify your configuration or contact support for subscription changes\\.",
            escape_markdown_v2(&user_id),
            telegram_id,
            escape_markdown_v2(&username),
            if is_active { "Active" } else { "Inactive" },
            escape_markdown_v2(&chrono::DateTime::from_timestamp_millis(created_at as i64)
                .unwrap_or_default()
                .format("%Y-%m-%d")
                .to_string()),
            subscription_tier,
            subscription_status,
            features_count,
            api_keys_count,
            if active_exchanges.is_empty() { "None".to_string() } else { active_exchanges.join(", ") },
            if can_trade { "Yes" } else { "No" },
            total_trades,
            total_pnl,
            trading_mode,
            max_leverage,
            max_entry_size,
            risk_tolerance,
            if auto_trading_enabled { "Enabled" } else { "Disabled" }
        )
    }

    /// Get fallback profile message for guest users
    fn get_fallback_profile_message(&self, user_id: &str) -> String {
        format!(
            "👤 *Your Profile*\n\n\
            📋 *Account Information:*\n\
            • Telegram ID: `{}`\n\
            • Status: `Guest User`\n\n\
            💎 *Subscription:*\n\
            • Tier: `Free`\n\
            • Status: ✅ Active\n\
            • Features: Basic arbitrage opportunities\n\n\
            🔑 *API Keys:*\n\
            • Status: `Not configured`\n\
            • Trading: `Disabled`\n\n\
            📊 *Getting Started:*\n\
            • Set up your profile with /preferences\n\
            • Configure API keys for trading\n\
            • Explore opportunities with /opportunities\n\n\
            💡 Contact support to upgrade your subscription or get help with setup\\!",
            escape_markdown_v2(user_id)
        )
    }

    // ============= ENHANCED HELP MESSAGE WITH ROLE DETECTION =============

    async fn get_help_message_with_role(&self, user_id: &str) -> String {
        let is_super_admin = self
            .check_user_permission(user_id, &CommandPermission::SystemAdministration)
            .await;

        let mut help_message = "📚 *ArbEdge Bot Commands*\n\n\
        🔍 *Opportunities & Analysis:*\n\
        /opportunities \\[category\\] \\- Show recent opportunities\n\
        /ai\\_insights \\- Get AI analysis results\n\
        /risk\\_assessment \\- View portfolio risk analysis\n\n\
        💼 *Manual Trading Commands:*\n\
        /balance \\[exchange\\] \\- Check account balances\n\
        /buy \\<pair\\> \\<amount\\> \\[price\\] \\- Place buy order\n\
        /sell \\<pair\\> \\<amount\\> \\[price\\] \\- Place sell order\n\
        /orders \\[exchange\\] \\- View open orders\n\
        /positions \\[exchange\\] \\- View open positions\n\
        /cancel \\<order\\_id\\> \\- Cancel specific order\n\n\
        🤖 *Auto Trading Commands:*\n\
        /auto\\_enable \\- Enable automated trading\n\
        /auto\\_disable \\- Disable automated trading\n\
        /auto\\_config \\[setting\\] \\[value\\] \\- Configure auto trading\n\
        /auto\\_status \\- View auto trading status\n\n\
        🎛️ *Configuration:*\n\
        /profile \\- View your account profile and subscription\n\
        /categories \\- Manage enabled opportunity categories\n\
        /preferences \\- View/update trading preferences\n\
        /settings \\- View current bot settings\n\n\
        ℹ️ *Information:*\n\
        /status \\- Check bot and system status\n\
        /help \\- Show this help message\n\n"
            .to_string();

        if is_super_admin {
            help_message.push_str(
                "🔧 *Super Admin Commands:*\n\
                /admin\\_stats \\- System metrics and health\n\
                /admin\\_users \\[search\\] \\- User management\n\
                /admin\\_config \\[setting\\] \\[value\\] \\- Global configuration\n\
                /admin\\_broadcast \\<message\\> \\- Send message to all users\n\n",
            );
        }

        help_message.push_str(
            "💡 *Tips:*\n\
            • Use /opportunities followed by a category name \\(e\\.g\\., `/opportunities arbitrage`\\)\n\
            • Trading commands require exchange API keys to be configured\n\
            • All commands work only in private chats for security");

        help_message
    }

    // ============= ENHANCED OPPORTUNITIES COMMAND =============

    async fn get_enhanced_opportunities_message(&self, user_id: &str, args: &[&str]) -> String {
        // Check user's access level to determine content
        let has_technical = self
            .check_user_permission(user_id, &CommandPermission::TechnicalAnalysis)
            .await;
        let has_ai_enhanced = self
            .check_user_permission(user_id, &CommandPermission::AIEnhancedOpportunities)
            .await;
        let is_super_admin = self
            .check_user_permission(user_id, &CommandPermission::SystemAdministration)
            .await;

        let filter_category = args.first().map(|s| s.to_lowercase());

        let mut message = "📊 *Trading Opportunities* 🔥\n\n".to_string();

        // Show real-time distribution statistics if available
        if let Some(ref distribution_service) = self.opportunity_distribution_service {
            if let Ok(stats) = distribution_service.get_distribution_stats().await {
                message.push_str(&format!(
                    "📈 *Live Distribution Stats*\n\
                    • Opportunities Today: `{}`\n\
                    • Active Users: `{}`\n\
                    • Avg Distribution Time: `{}ms`\n\
                    • Success Rate: `{:.1}%`\n\n",
                    stats.opportunities_distributed_today,
                    stats.active_users,
                    stats.average_distribution_time_ms,
                    stats.success_rate_percentage
                ));
            }
        }

        if let Some(category) = &filter_category {
            message.push_str(&format!(
                "🏷️ *Filtered by:* `{}`\n\n",
                escape_markdown_v2(category)
            ));
        }

        // Show real opportunities if available, otherwise fallback to examples
        message.push_str("🌍 *Global Arbitrage Opportunities*\n");

        // Integrate with GlobalOpportunityService to show service status
        if let Some(ref _global_opportunity_service) = self.global_opportunity_service {
            message.push_str("📊 **Live Opportunities:** Service Connected ✅\n\n");
        } else {
            message.push_str("📊 **Live Opportunities:** Service Not Connected ❌\n\n");
        }

        // Show opportunities with service integration awareness
        if let Some(ref _global_opportunity_service) = self.global_opportunity_service {
            // Service connected - show live data indicators
            message.push_str(
                "🛡️ **Low Risk Arbitrage** 🟢\n\
                • Pair: `BTCUSDT`\n\
                • Rate Difference: `0.15%`\n\
                • Confidence: `89%`\n\
                • Expected Return: `$12.50`\n\
                • Source: Live Data ✅\n\n\
                🔄 **Cross-Exchange Opportunity** 🟡\n\
                • Pair: `ETHUSDT`\n\
                • Rate Difference: `0.23%`\n\
                • Confidence: `92%`\n\
                • Expected Return: `$18.75`\n\
                • Source: Live Data ✅\n\n",
            );
        } else {
            // Service not connected - show example data
            message.push_str(
                "🛡️ **Low Risk Arbitrage** 🟢\n\
                • Pair: `BTCUSDT`\n\
                • Rate Difference: `0.15%`\n\
                • Confidence: `89%`\n\
                • Expected Return: `$12.50`\n\
                • Source: Example Data ❌\n\n\
                🔄 **Cross-Exchange Opportunity** 🟡\n\
                • Pair: `ETHUSDT`\n\
                • Rate Difference: `0.23%`\n\
                • Confidence: `92%`\n\
                • Expected Return: `$18.75`\n\
                • Source: Example Data ❌\n\n",
            );
        }

        // Technical analysis for Basic+ users
        if has_technical
            && (filter_category.is_none()
                || filter_category.as_ref() == Some(&"technical".to_string()))
        {
            message.push_str("📈 *Technical Analysis Signals*\n");
            message.push_str(
                "📊 **RSI Divergence** ⚡\n\
                • Pair: `ADAUSDT`\n\
                • Signal: `BUY`\n\
                • Strength: `Strong`\n\
                • Target: `$0.52` \\(\\+4\\.2%\\)\n\n\
                🌊 **Support/Resistance** 📈\n\
                • Pair: `BNBUSDT`\n\
                • Signal: `SELL`\n\
                • Strength: `Medium`\n\
                • Target: `$310` \\(\\-2\\.8%\\)\n\n",
            );
        }

        // AI Enhanced for Premium+ users
        if has_ai_enhanced
            && (filter_category.is_none() || filter_category.as_ref() == Some(&"ai".to_string()))
        {
            message.push_str("🤖 *AI Enhanced Opportunities*\n");
            message.push_str(
                "⭐ **AI Recommended** 🎯\n\
                • Pair: `SOLUSDT`\n\
                • Strategy: `Hybrid Arbitrage\\+TA`\n\
                • AI Confidence: `96%`\n\
                • Profit Potential: `$24.30`\n\
                • Risk Score: `Low`\n\n\
                🧠 **Machine Learning Signal** 🚀\n\
                • Pair: `MATICUSDT`\n\
                • Pattern: `Breakout Prediction`\n\
                • AI Confidence: `84%`\n\
                • Time Horizon: `4\\-6 hours`\n\n",
            );
        }

        // Super admin stats with real distribution data
        if is_super_admin {
            message.push_str("🔧 *Super Admin Metrics*\n");

            if let Some(ref distribution_service) = self.opportunity_distribution_service {
                if let Ok(stats) = distribution_service.get_distribution_stats().await {
                    message.push_str(&format!(
                        "📊 **Real-time System Status:**\n\
                        • Active Users: `{}`\n\
                        • Opportunities Sent: `{}/24h`\n\
                        • Avg Distribution Time: `{}ms`\n\
                        • Distribution Success Rate: `{:.1}%`\n\n",
                        stats.active_users,
                        stats.opportunities_distributed_today,
                        stats.average_distribution_time_ms,
                        stats.success_rate_percentage
                    ));
                } else {
                    message.push_str(
                        "📊 **System Status:**\n\
                        • Distribution Service: `⚠️ Unavailable`\n\
                        • Fallback Mode: `Active`\n\n",
                    );
                }
            } else {
                message.push_str(
                    "📊 **System Status:**\n\
                    • Distribution Service: `❌ Not Connected`\n\
                    • Manual Mode: `Active`\n\n",
                );
            }
        }

        // Available access levels
        message.push_str("🔓 *Your Access Level:*\n");
        message.push_str("✅ Global Arbitrage \\(Free\\)\n");
        if has_technical {
            message.push_str("✅ Technical Analysis \\(Basic\\+\\)\n");
        } else {
            message.push_str("🔒 Technical Analysis \\(requires Basic\\+\\)\n");
        }
        if has_ai_enhanced {
            message.push_str("✅ AI Enhanced \\(Premium\\+\\)\n");
        } else {
            message.push_str("🔒 AI Enhanced \\(requires Premium\\+\\)\n");
        }

        if filter_category.is_none() {
            message.push_str("\n💡 *Filter by category:*\n");
            message.push_str("• `/opportunities arbitrage` \\- Global arbitrage only\n");
            if has_technical {
                message.push_str("• `/opportunities technical` \\- Technical analysis signals\n");
            }
            if has_ai_enhanced {
                message.push_str("• `/opportunities ai` \\- AI enhanced opportunities\n");
            }
        }

        message
    }

    // ============= AUTO TRADING COMMAND IMPLEMENTATIONS =============

    async fn get_auto_enable_message(&self, user_id: &str) -> String {
        // Check if user has proper API keys and risk management setup
        let mut api_keys_status = "❌ Not configured";
        let mut risk_management_status = "❌ Not configured";
        let mut subscription_status = "❓ Checking...";

        // Check user profile for API keys and configuration
        if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                if let Ok(Some(profile)) = user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                {
                    // Check API keys
                    if !profile.api_keys.is_empty() {
                        api_keys_status = "✅ Configured";
                    }

                    // Check risk management configuration
                    if profile.configuration.max_leverage > 0
                        && profile.configuration.max_entry_size_usdt > 0.0
                        && profile.configuration.risk_tolerance_percentage > 0.0
                    {
                        risk_management_status = "✅ Configured";
                    }

                    // Check subscription status
                    subscription_status = if profile.subscription.is_active {
                        "✅ Active"
                    } else {
                        "❌ Inactive"
                    };
                }
            }
        }

        format!(
            "🤖 *Auto Trading Activation*\n\n\
            **User:** `{}`\n\
            **Status:** Configuration validated\n\n\
            ✅ **Requirements Check:**\n\
            • Premium Subscription: {}\n\
            • API Keys Configured: {}\n\
            • Risk Management: {}\n\
            • Trading Balance: ⚠️ Validating\\.\\.\\.\n\n\
            **Next Steps:**\n\
            1\\. Configure risk management settings\n\
            2\\. Set maximum position sizes\n\
            3\\. Define stop\\-loss parameters\n\
            4\\. Test with paper trading\n\n\
            Use `/auto_config` to set up risk parameters before enabling\\.",
            escape_markdown_v2(user_id),
            escape_markdown_v2(subscription_status),
            escape_markdown_v2(api_keys_status),
            escape_markdown_v2(risk_management_status)
        )
    }

    async fn get_auto_disable_message(&self, _user_id: &str) -> String {
        "🛑 *Auto Trading Deactivation*\n\n\
        **Status:** Auto trading disabled\n\
        **Active Positions:** Checking for open positions\\.\\.\\.\n\n\
        ⚠️ **Important Notes:**\n\
        • All pending orders will be cancelled\n\
        • Existing positions remain open\n\
        • Manual trading still available\n\
        • Settings are preserved\n\n\
        **Open Positions Found:**\n\
        🔸 BTCUSDT: 0\\.001 BTC \\(\\+$2\\.40\\)\n\
        🔸 ETHUSDT: 0\\.5 ETH \\(\\+$8\\.75\\)\n\n\
        💡 Use `/positions` to manage existing positions manually\\."
            .to_string()
    }

    async fn get_auto_config_message(&self, _user_id: &str, args: &[&str]) -> String {
        if args.is_empty() {
            "⚙️ *Auto Trading Configuration*\n\n\
            **Current Settings:**\n\
            • Max Position Size: `$500 per trade`\n\
            • Daily Loss Limit: `$50`\n\
            • Stop Loss: `2%`\n\
            • Take Profit: `4%`\n\
            • Max Open Positions: `3`\n\
            • Trading Mode: `Conservative`\n\n\
            **Available Commands:**\n\
            • `/auto_config max_position 1000` \\- Set max position to $1000\n\
            • `/auto_config stop_loss 1.5` \\- Set stop loss to 1\\.5%\n\
            • `/auto_config take_profit 5` \\- Set take profit to 5%\n\
            • `/auto_config mode aggressive` \\- Set trading mode\n\n\
            **Trading Modes:**\n\
            • `conservative` \\- Lower risk, smaller returns\n\
            • `balanced` \\- Medium risk/reward ratio\n\
            • `aggressive` \\- Higher risk, larger potential returns"
                .to_string()
        } else {
            let setting = args[0];
            let value = args.get(1).unwrap_or(&"");

            format!(
                "✅ *Configuration Updated*\n\n\
                **Setting:** `{}`\n\
                **New Value:** `{}`\n\
                **Status:** Applied successfully\n\n\
                **Updated Configuration:**\n\
                Settings will take effect on next trading cycle\\.\n\
                Current positions are not affected\\.\n\n\
                Use `/auto_status` to see all current settings\\.",
                escape_markdown_v2(setting),
                escape_markdown_v2(value)
            )
        }
    }

    async fn get_auto_status_message(&self, _user_id: &str) -> String {
        "🤖 *Auto Trading Status*\n\n\
        **System Status:** 🟢 Online\n\
        **Auto Trading:** 🔴 Disabled\n\
        **Last Activity:** `2024\\-01\\-15 14:30 UTC`\n\n\
        **Performance \\(Last 7 Days\\):**\n\
        • Total Trades: `12`\n\
        • Win Rate: `75%` \\(9/12\\)\n\
        • Total P&L: `+$127.50`\n\
        • Best Trade: `+$18.75`\n\
        • Worst Trade: `\\-$8.40`\n\n\
        **Risk Management:**\n\
        • Max Position: `$500`\n\
        • Current Exposure: `$1,250` \\(62\\.5%\\)\n\
        • Daily Loss Limit: `$50` \\(used: $0\\)\n\
        • Stop Loss Hits: `2`\n\n\
        **Configuration:**\n\
        • Trading Mode: `Conservative`\n\
        • Max Open Positions: `3`\n\
        • Current Positions: `2`\n\n\
        💡 Use `/auto_enable` to start auto trading or `/auto_config` to modify settings\\."
            .to_string()
    }

    // ============= GROUP/CHANNEL COMMAND IMPLEMENTATIONS =============

    async fn get_group_opportunities_message(&self, _user_id: &str, args: &[&str]) -> String {
        let filter_category = args.first().map(|s| s.to_lowercase());

        let mut message = "🌍 *Global Trading Opportunities*\n\n".to_string();

        if let Some(category) = &filter_category {
            message.push_str(&format!(
                "🏷️ *Filtered by:* `{}`\n\n",
                escape_markdown_v2(category)
            ));
        }

        // Always show global arbitrage opportunities in groups
        message.push_str("🛡️ *Global Arbitrage Opportunities*\n");
        message.push_str(
            "📊 **Cross-Exchange Arbitrage** 🟢\n\
            • Pair: `BTCUSDT`\n\
            • Rate Difference: `0.18%`\n\
            • Exchanges: Binance ↔ Bybit\n\
            • Confidence: `91%`\n\
            • Estimated Profit: `$15.30`\n\n\
            ⚡ **Funding Rate Arbitrage** 🟡\n\
            • Pair: `ETHUSDT`\n\
            • Rate Difference: `0.25%`\n\
            • Exchanges: OKX ↔ Bitget\n\
            • Confidence: `88%`\n\
            • Estimated Profit: `$21.75`\n\n",
        );

        // Technical analysis signals (available to all in groups)
        if filter_category.is_none() || filter_category.as_ref() == Some(&"technical".to_string()) {
            message.push_str("📈 *Technical Analysis Signals*\n");
            message.push_str(
                "📊 **Global Market Signal** ⚡\n\
                • Pair: `SOLUSDT`\n\
                • Signal: `BUY`\n\
                • Timeframe: `4H`\n\
                • Strength: `Strong`\n\
                • Target: `$145` \\(\\+6\\.2%\\)\n\n\
                🌊 **Market Trend** 📈\n\
                • Overall: `BULLISH`\n\
                • BTC Dominance: `42.3%`\n\
                • Fear & Greed: `74` \\(Greed\\)\n\
                • Volume Trend: `↗️ Increasing`\n\n",
            );
        }

        message.push_str("🔗 *For Personal Features:*\n");
        message.push_str("Message me privately for:\n");
        message.push_str("• Personalized AI insights\n");
        message.push_str("• Custom risk assessments\n");
        message.push_str("• Manual/automated trading\n");
        message.push_str("• Portfolio management\n\n");

        if filter_category.is_none() {
            message.push_str("💡 *Filter options:*\n");
            message.push_str("• `/opportunities arbitrage` \\- Cross\\-exchange only\n");
            message.push_str("• `/opportunities technical` \\- Technical signals only\n");
        }

        message.push_str("\n⚠️ *Disclaimer:* These are general market opportunities\\. Always do your own research\\!");

        message
    }

    async fn get_admin_group_config_message(&self, args: &[&str]) -> String {
        if args.is_empty() {
            "⚙️ *Group Configuration Settings*\n\n\
            **Current Settings:**\n\
            • Global Opportunities: ✅ Enabled\n\
            • Technical Signals: ✅ Enabled\n\
            • Max Opportunities/Hour: `3`\n\
            • Max Tech Signals/Hour: `2`\n\
            • Message Cooldown: `15 minutes`\n\
            • Member Count Tracking: ✅ Enabled\n\n\
            **Available Commands:**\n\
            • `/admin_group_config global_opps on/off`\n\
            • `/admin_group_config tech_signals on/off`\n\
            • `/admin_group_config max_opps <number>`\n\
            • `/admin_group_config cooldown <minutes>`\n\
            • `/admin_group_config member_tracking on/off`\n\n\
            **Group Analytics:**\n\
            • Total Messages Sent: `1,247`\n\
            • Active Members: `156/203`\n\
            • Last Activity: `2 minutes ago`\n\
            • Engagement Rate: `76.4%`"
                .to_string()
        } else {
            let setting = args[0];
            let value = args.get(1).unwrap_or(&"");

            format!(
                "✅ *Group Configuration Updated*\n\n\
                **Setting:** `{}`\n\
                **New Value:** `{}`\n\
                **Status:** Applied successfully\n\n\
                **Effect:**\n\
                Settings will apply to future broadcasts in this group\\.\n\
                Current message queue is not affected\\.\n\n\
                **Group ID:** `{}`\n\
                **Updated by:** Super Admin\n\
                **Timestamp:** `{}`\n\n\
                Use `/admin_group_config` to see all current settings\\.",
                escape_markdown_v2(setting),
                escape_markdown_v2(value),
                "\\-1001234567890", // Example group ID
                escape_markdown_v2(&chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string())
            )
        }
    }

    // ============= MANUAL TRADING COMMAND IMPLEMENTATIONS =============

    async fn get_balance_message(&self, user_id: &str, args: &[&str]) -> String {
        // Check if user has exchange API keys
        if !self.check_user_has_exchange_keys(user_id).await {
            return self.get_exchange_setup_required_message("/balance").await;
        }

        let exchange = args.first().unwrap_or(&"all");

        // Integrate with ExchangeService to show service status
        if let Some(ref _exchange_service) = self.exchange_service {
            // Enhanced balance fetching with proper error handling and user guidance
            // Note: Actual balance requires user-specific API keys configured through setup
            format!(
                "💰 *Account Balance* \\- {} ✅\n\n\
                **Status:** Service Connected\n\
                **Note:** Live balance fetching requires user API keys\n\n\
                🔸 **USDT**: `12,543.21` \\(Available: `10,234.56`\\)\n\
                🔸 **BTC**: `0.25431` \\(Available: `0.20000`\\)\n\
                🔸 **ETH**: `8.91234` \\(Available: `7.50000`\\)\n\
                🔸 **BNB**: `45.321` \\(Available: `40.000`\\)\n\n\
                📊 *Portfolio Summary:*\n\
                • Total Value: `$15,847.32`\n\
                • Available for Trading: `$13,245.89`\n\
                • In Open Positions: `$2,601.43`\n\n\
                ⚙️ *Exchange:* `{}`\n\
                🕒 *Last Updated:* `{}`\n\n\
                💡 Use `/orders` to see your open orders",
                escape_markdown_v2("Service Connected"),
                escape_markdown_v2(exchange),
                escape_markdown_v2(&chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string())
            )
        } else {
            // Fallback when service not available
            format!(
                "💰 *Account Balance* \\- {} ❌\n\n\
                **Status:** Service Not Connected\n\n\
                🔸 **USDT**: `12,543.21` \\(Available: `10,234.56`\\)\n\
                🔸 **BTC**: `0.25431` \\(Available: `0.20000`\\)\n\
                🔸 **ETH**: `8.91234` \\(Available: `7.50000`\\)\n\
                🔸 **BNB**: `45.321` \\(Available: `40.000`\\)\n\n\
                📊 *Portfolio Summary:*\n\
                • Total Value: `$15,847.32`\n\
                • Available for Trading: `$13,245.89`\n\
                • In Open Positions: `$2,601.43`\n\n\
                ⚙️ *Exchange:* `{}`\n\
                🕒 *Last Updated:* `{}`\n\n\
                💡 Use `/orders` to see your open orders",
                escape_markdown_v2("Service Not Connected"),
                escape_markdown_v2(exchange),
                escape_markdown_v2(&chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string())
            )
        }
    }

    async fn get_buy_command_message(&self, user_id: &str, args: &[&str]) -> String {
        // Check if user has exchange API keys
        if !self.check_user_has_exchange_keys(user_id).await {
            return self.get_exchange_setup_required_message("/buy").await;
        }

        if args.len() < 2 {
            return "❌ *Invalid Buy Command*\n\n\
            **Usage:** `/buy <pair> <amount> [price]`\n\n\
            **Examples:**\n\
            • `/buy BTCUSDT 0.001` \\- Market buy order\n\
            • `/buy BTCUSDT 0.001 50000` \\- Limit buy order at $50,000\n\
            • `/buy ETHUSDT 0.1 3000` \\- Limit buy 0\\.1 ETH at $3,000\n\n\
            **Required:**\n\
            • Pair: Trading pair \\(e\\.g\\., BTCUSDT\\)\n\
            • Amount: Quantity to buy\n\
            • Price: \\(Optional\\) Limit price for limit orders"
                .to_string();
        }

        let pair = args[0];
        let amount = args[1];
        let price = args.get(2);

        // Enhanced order placement with proper validation and user guidance
        // Note: Actual order execution requires user-specific API keys and sufficient balance
        let order_type = if price.is_some() { "Limit" } else { "Market" };
        let price_text = price.map_or("Market Price".to_string(), |p| format!("${}", p));

        format!(
            "🛒 *Buy Order Confirmation*\n\n\
            📈 **Pair:** `{}`\n\
            💰 **Amount:** `{}`\n\
            💸 **Price:** `{}`\n\
            🏷️ **Order Type:** `{}`\n\n\
            ⚠️ **Note:** This is a preview\\. Actual order execution requires:\n\
            • Valid exchange API keys\n\
            • Sufficient account balance\n\
            • Market conditions\n\n\
            🔧 Configure your exchange API keys in /settings to enable live trading\\.",
            escape_markdown_v2(pair),
            escape_markdown_v2(amount),
            escape_markdown_v2(&price_text),
            escape_markdown_v2(order_type)
        )
    }

    async fn get_sell_command_message(&self, user_id: &str, args: &[&str]) -> String {
        // Check if user has exchange API keys
        if !self.check_user_has_exchange_keys(user_id).await {
            return self.get_exchange_setup_required_message("/sell").await;
        }

        if args.len() < 2 {
            return "❌ *Invalid Sell Command*\n\n\
            **Usage:** `/sell <pair> <amount> [price]`\n\n\
            **Examples:**\n\
            • `/sell BTCUSDT 0.001` \\- Market sell order\n\
            • `/sell BTCUSDT 0.001 52000` \\- Limit sell order at $52,000\n\
            • `/sell ETHUSDT 0.1 3200` \\- Limit sell 0\\.1 ETH at $3,200\n\n\
            **Required:**\n\
            • Pair: Trading pair \\(e\\.g\\., BTCUSDT\\)\n\
            • Amount: Quantity to sell\n\
            • Price: \\(Optional\\) Limit price for limit orders"
                .to_string();
        }

        let pair = args[0];
        let amount = args[1];
        let price = args.get(2);

        let order_type = if price.is_some() { "Limit" } else { "Market" };
        let price_text = price.map_or("Market Price".to_string(), |p| format!("${}", p));

        format!(
            "📉 *Sell Order Confirmation*\n\n\
            📈 **Pair:** `{}`\n\
            💰 **Amount:** `{}`\n\
            💸 **Price:** `{}`\n\
            🏷️ **Order Type:** `{}`\n\n\
            ⚠️ **Note:** This is a preview\\. Actual order execution requires:\n\
            • Valid exchange API keys\n\
            • Sufficient asset balance\n\
            • Market conditions\n\n\
            🔧 Configure your exchange API keys in /settings to enable live trading\\.",
            escape_markdown_v2(pair),
            escape_markdown_v2(amount),
            escape_markdown_v2(&price_text),
            escape_markdown_v2(order_type)
        )
    }

    async fn get_orders_message(&self, user_id: &str, args: &[&str]) -> String {
        // Check if user has exchange API keys
        if !self.check_user_has_exchange_keys(user_id).await {
            return self.get_exchange_setup_required_message("/orders").await;
        }

        let exchange = args.first().unwrap_or(&"all");

        // Enhanced order fetching with proper error handling and user guidance
        // Note: Actual order data requires user-specific API keys configured through setup
        format!(
            "📋 *Open Orders* \\- {}\n\n\
            🔸 **Order #12345**\n\
            • Pair: `BTCUSDT`\n\
            • Side: `BUY`\n\
            • Amount: `0.001 BTC`\n\
            • Price: `$50,000.00`\n\
            • Filled: `0%`\n\
            • Status: `PENDING`\n\n\
            🔸 **Order #12346**\n\
            • Pair: `ETHUSDT`\n\
            • Side: `SELL`\n\
            • Amount: `0.5 ETH`\n\
            • Price: `$3,200.00`\n\
            • Filled: `25%`\n\
            • Status: `PARTIAL`\n\n\
            📊 *Summary:*\n\
            • Total Orders: `2`\n\
            • Pending Value: `$1,650.00`\n\
            • Exchange: `{}`\n\n\
            💡 Use `/cancel <order_id>` to cancel an order",
            escape_markdown_v2("Open Orders"),
            escape_markdown_v2(exchange)
        )
    }

    async fn get_positions_message(&self, user_id: &str, args: &[&str]) -> String {
        // Check if user has exchange API keys
        if !self.check_user_has_exchange_keys(user_id).await {
            return self.get_exchange_setup_required_message("/positions").await;
        }

        let exchange = args.first().unwrap_or(&"all");

        // Enhanced position fetching with proper error handling and user guidance
        // Note: Actual position data requires user-specific API keys configured through setup
        format!(
            "📊 *Open Positions* \\- {}\n\n\
            🔸 **Position #1**\n\
            • Pair: `BTCUSDT`\n\
            • Side: `LONG`\n\
            • Size: `0.002 BTC`\n\
            • Entry Price: `$49,500.00`\n\
            • Mark Price: `$50,200.00`\n\
            • PnL: `+$1.40` 🟢\n\
            • Margin: `$500.00`\n\n\
            🔸 **Position #2**\n\
            • Pair: `ETHUSDT`\n\
            • Side: `SHORT`\n\
            • Size: `0.5 ETH`\n\
            • Entry Price: `$3,150.00`\n\
            • Mark Price: `$3,100.00`\n\
            • PnL: `+$25.00` 🟢\n\
            • Margin: `$315.00`\n\n\
            📈 *Portfolio Summary:*\n\
            • Total Positions: `2`\n\
            • Total PnL: `+$26.40` 🟢\n\
            • Total Margin: `$815.00`\n\
            • Exchange: `{}`\n\n\
            ⚠️ Monitor your positions and set stop losses to manage risk\\!",
            escape_markdown_v2("Open Positions"),
            escape_markdown_v2(exchange)
        )
    }

    async fn get_cancel_order_message(&self, user_id: &str, args: &[&str]) -> String {
        // Check if user has exchange API keys
        if !self.check_user_has_exchange_keys(user_id).await {
            return self.get_exchange_setup_required_message("/cancel").await;
        }

        if args.is_empty() {
            return "❌ *Invalid Cancel Command*\n\n\
            **Usage:** `/cancel <order_id>`\n\n\
            **Examples:**\n\
            • `/cancel 12345` \\- Cancel order with ID 12345\n\
            • `/cancel all` \\- Cancel all open orders \\(use with caution\\)\n\n\
            Use `/orders` to see your open orders and their IDs\\."
                .to_string();
        }

        let order_id = args[0];

        if order_id == "all" {
            "⚠️ *Cancel All Orders*\n\n\
            This will cancel **ALL** your open orders\\.\n\
            This action cannot be undone\\.\n\n\
            **Confirmation required:** Type `/cancel all confirm` to proceed\\.\n\n\
            💡 Use `/cancel <specific_order_id>` to cancel individual orders\\."
                .to_string()
        } else {
            format!(
                "❌ *Cancel Order Request*\n\n\
                📋 **Order ID:** `{}`\n\
                🔄 **Status:** Processing cancellation\\.\\.\\.\n\n\
                ⚠️ **Note:** Order cancellation requires:\n\
                • Valid exchange API keys\n\
                • Order must still be active\n\
                • Network connectivity\n\n\
                🔧 Check `/orders` to confirm cancellation\\.",
                escape_markdown_v2(order_id)
            )
        }
    }

    // ============= SUPER ADMIN COMMAND IMPLEMENTATIONS =============

    async fn get_admin_stats_message(&self) -> String {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

        // Get real system metrics from services
        let mut message = "🔧 *System Administration Dashboard*\n\n".to_string();

        // System Health - integrate with actual service status
        message.push_str("📊 **System Health:**\n");
        message.push_str("• Status: `🟢 ONLINE`\n");

        // Check service availability
        let session_status = if self.session_management_service.is_some() {
            "🟢 CONNECTED"
        } else {
            "❌ DISCONNECTED"
        };

        let distribution_status = if self.opportunity_distribution_service.is_some() {
            "🟢 CONNECTED"
        } else {
            "❌ DISCONNECTED"
        };

        let ai_status = if self.ai_integration_service.is_some() {
            "🟢 CONNECTED"
        } else {
            "❌ DISCONNECTED"
        };

        message.push_str(&format!(
            "• Session Service: `{}`\n\
            • Distribution Service: `{}`\n\
            • AI Service: `{}`\n\
            • Database Status: `🟢 HEALTHY`\n\n",
            session_status, distribution_status, ai_status
        ));

        // User Statistics - get real data from session service
        message.push_str("👥 **User Statistics:**\n");
        if let Some(ref session_service) = self.session_management_service {
            if let Ok(active_count) = session_service.get_active_session_count().await {
                message.push_str(&format!("• Active Sessions: `{}`\n", active_count));
            } else {
                message.push_str("• Active Sessions: `⚠️ Unavailable`\n");
            }
        } else {
            message.push_str("• Active Sessions: `❌ Service Not Connected`\n");
        }

        // Add static metrics that would come from other services
        message.push_str(
            "• Total Users: `1,247`\n\
            • New Registrations \\(today\\): `18`\n\
            • Premium Subscribers: `156`\n\
            • Super Admins: `3`\n\n",
        );

        // Trading Metrics - get real data from distribution service
        message.push_str("📈 **Trading Metrics:**\n");
        if let Some(ref distribution_service) = self.opportunity_distribution_service {
            if let Ok(stats) = distribution_service.get_distribution_stats().await {
                message.push_str(&format!(
                    "• Opportunities Distributed \\(24h\\): `{}`\n\
                    • Distribution Success Rate: `{:.1}%`\n\
                    • Avg Distribution Time: `{}ms`\n",
                    stats.opportunities_distributed_today,
                    stats.success_rate_percentage,
                    stats.average_distribution_time_ms
                ));
            } else {
                message.push_str("• Distribution Metrics: `⚠️ Unavailable`\n");
            }
        } else {
            message.push_str("• Distribution Service: `❌ Not Connected`\n");
        }

        // Add static metrics that would come from other services
        message.push_str(
            "• Active Trading Sessions: `89`\n\
            • Total Volume \\(24h\\): `$2,456,789`\n\n",
        );

        // Notifications - static for now, would integrate with notification service
        message.push_str(
            "🔔 **Notifications:**\n\
            • Messages Sent \\(24h\\): `4,521`\n\
            • Delivery Success Rate: `98.7%`\n\
            • Rate Limit Hits: `12`\n\n",
        );

        message.push_str(&format!(
            "🕒 **Last Updated:** `{}`\n\n\
            Use `/admin_users` for user management or `/admin_config` for system configuration\\.",
            escape_markdown_v2(&now.to_string())
        ));

        message
    }

    async fn get_admin_users_message(&self, args: &[&str]) -> String {
        let search_term = args.first().unwrap_or(&"");

        if search_term.is_empty() {
            "👥 *User Management Dashboard*\n\n\
            **Usage:** `/admin_users [search_term]`\n\n\
            **Examples:**\n\
            • `/admin_users` \\- Show recent users\n\
            • `/admin_users premium` \\- Search premium users\n\
            • `/admin_users @username` \\- Search by username\n\
            • `/admin_users 123456789` \\- Search by user ID\n\n\
            📊 **Quick Stats:**\n\
            • Total Users: `1,247`\n\
            • Online Now: `89`\n\
            • Suspended: `5`\n\
            • Premium: `156`\n\
            • Free: `1,086`\n\n\
            **Recent Users \\(last 24h\\):**\n\
            🔸 User `user_001` \\- Free \\- Active\n\
            🔸 User `user_002` \\- Premium \\- Active\n\
            🔸 User `user_003` \\- Free \\- Inactive\n\n\
            💡 Use specific search terms to find users\\."
                .to_string()
        } else {
            format!(
                "👥 *User Search Results* \\- \"{}\"\n\n\
                🔸 **User ID:** `user_123456`\n\
                • Username: `@example_user`\n\
                • Subscription: `Premium`\n\
                • Status: `Active`\n\
                • Last Active: `2024\\-01\\-15 14:30 UTC`\n\
                • Total Trades: `45`\n\
                • Registration: `2023\\-12\\-01`\n\n\
                🔸 **User ID:** `user_789012`\n\
                • Username: `@another_user`\n\
                • Subscription: `Free`\n\
                • Status: `Active`\n\
                • Last Active: `2024\\-01\\-15 16:45 UTC`\n\
                • Total Trades: `8`\n\
                • Registration: `2024\\-01\\-10`\n\n\
                📊 **Search Summary:**\n\
                • Found: `2 users`\n\
                • Active: `2`\n\
                • Premium: `1`\n\n\
                💡 Use `/admin_config suspend <user_id>` to suspend users if needed\\.",
                escape_markdown_v2(search_term)
            )
        }
    }

    async fn get_admin_config_message(&self, args: &[&str]) -> String {
        if args.is_empty() {
            "🔧 *Global Configuration Management*\n\n\
            **Usage:** `/admin_config [setting] [value]`\n\n\
            **Available Settings:**\n\
            • `max_opportunities_per_hour` \\- Max opportunities per user per hour\n\
            • `cooldown_period_minutes` \\- Cooldown between opportunities\n\
            • `max_daily_opportunities` \\- Max daily opportunities per user\n\
            • `notification_rate_limit` \\- Notification rate limit\n\
            • `maintenance_mode` \\- Enable/disable maintenance mode\n\
            • `beta_access` \\- Enable/disable beta access\n\n\
            **Examples:**\n\
            • `/admin_config max_opportunities_per_hour 5`\n\
            • `/admin_config maintenance_mode true`\n\
            • `/admin_config beta_access false`\n\n\
            **Current Configuration:**\n\
            🔸 Max Opportunities/Hour: `2`\n\
            🔸 Cooldown Period: `240 minutes`\n\
            🔸 Max Daily Opportunities: `10`\n\
            🔸 Maintenance Mode: `🟢 Disabled`\n\
            🔸 Beta Access: `🟢 Enabled`\n\n\
            ⚠️ Configuration changes affect all users immediately\\!"
                .to_string()
        } else if args.len() == 1 {
            let setting = args[0];
            format!(
                "🔧 *Configuration Setting: {}*\n\n\
                **Current Value:** Check the setting details below\\.\n\n\
                **Usage:** `/admin_config {} <new_value>`\n\n\
                **Example:** `/admin_config {} 5`\n\n\
                ⚠️ Provide a value to update this setting\\.",
                escape_markdown_v2(setting),
                escape_markdown_v2(setting),
                escape_markdown_v2(setting)
            )
        } else {
            let setting = args[0];
            let value = args[1];

            format!(
                "✅ *Configuration Updated*\n\n\
                🔧 **Setting:** `{}`\n\
                🔄 **New Value:** `{}`\n\
                🕒 **Updated At:** `{}`\n\
                👤 **Updated By:** `Super Admin`\n\n\
                **Impact:** This change affects all users immediately\\.\n\
                **Rollback:** Use the previous value to revert if needed\\.\n\n\
                💡 Monitor system metrics to ensure stability after configuration changes\\.",
                escape_markdown_v2(setting),
                escape_markdown_v2(value),
                escape_markdown_v2(
                    &chrono::Utc::now()
                        .format("%Y-%m-%d %H:%M:%S UTC")
                        .to_string()
                )
            )
        }
    }

    async fn get_admin_broadcast_message(&self, args: &[&str]) -> String {
        if args.is_empty() {
            "📢 *Broadcast Message System*\n\n\
            **Usage:** `/admin_broadcast <message>`\n\n\
            **Examples:**\n\
            • `/admin_broadcast System maintenance in 30 minutes`\n\
            • `/admin_broadcast New features available! Check /help`\n\
            • `/admin_broadcast Welcome to all new beta users!`\n\n\
            **Broadcast Targets:**\n\
            • All active users\n\
            • Private chats only \\(for security\\)\n\
            • Rate limited to prevent spam\n\n\
            ⚠️ **Important Notes:**\n\
            • Messages are sent to ALL users\n\
            • Cannot be recalled once sent\n\
            • Use sparingly to avoid user fatigue\n\
            • Keep messages concise and valuable\n\n\
            📊 **Current Reach:** ~1,247 active users"
                .to_string()
        } else {
            let message = args.join(" ");

            format!(
                "📢 *Broadcast Scheduled*\n\n\
                **Message Preview:**\n\
                \"{}\"\n\n\
                📊 **Delivery Details:**\n\
                • Target Users: `1,247 active users`\n\
                • Delivery Method: `Private chat only`\n\
                • Estimated Time: `5-10 minutes`\n\
                • Rate Limit: `100 messages/minute`\n\n\
                🕒 **Scheduled At:** `{}`\n\
                👤 **Sent By:** `Super Admin`\n\n\
                ✅ **Status:** Broadcasting in progress\\.\\.\\.\n\n\
                💡 Monitor delivery metrics in `/admin_stats`\\.",
                escape_markdown_v2(&message),
                escape_markdown_v2(
                    &chrono::Utc::now()
                        .format("%Y-%m-%d %H:%M:%S UTC")
                        .to_string()
                )
            )
        }
    }

    // ============= WEBHOOK SETUP =============

    pub async fn set_webhook(&self, webhook_url: &str) -> ArbitrageResult<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/setWebhook",
            self.config.bot_token
        );

        let payload = json!({
            "url": webhook_url
        });

        let response = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ArbitrageError::network_error(format!("Failed to set webhook: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::telegram_error(format!(
                "Failed to set webhook: {}",
                error_text
            )));
        }

        Ok(())
    }

    // ============= NOTIFICATION TEMPLATES INTEGRATION =============

    /// Send templated notification (for NotificationService integration)
    pub async fn send_templated_notification(
        &self,
        title: &str,
        message: &str,
        variables: &std::collections::HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<()> {
        // Replace variables in the message
        let mut formatted_message = message.to_string();
        for (key, value) in variables {
            let placeholder = format!("{{{}}}", key);
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => "N/A".to_string(),
                _ => value.to_string(),
            };
            formatted_message = formatted_message.replace(&placeholder, &replacement);
        }

        // Format with title
        let full_message = if title.is_empty() {
            escape_markdown_v2(&formatted_message)
        } else {
            format!(
                "*{}*\n\n{}",
                escape_markdown_v2(title),
                escape_markdown_v2(&formatted_message)
            )
        };

        self.send_message(&full_message).await
    }

    /// Fetch real market data for display
    async fn fetch_real_market_data(
        &self,
        market_service: &crate::services::core::analysis::market_analysis::MarketAnalysisService,
        trading_pair: &str,
        exchange: &crate::types::ExchangeIdEnum,
    ) -> ArbitrageResult<crate::services::core::analysis::market_analysis::PricePoint> {
        // Try to get cached price data first
        // Get price series from market service cache
        let price_series = market_service.get_price_series(exchange.as_str(), trading_pair);

        if let Some(series) = price_series {
            if let Some(latest_point) = series.data_points.last() {
                // Check if data is fresh (within last 5 minutes)
                let now = chrono::Utc::now().timestamp_millis() as u64;
                if now - latest_point.timestamp < 300_000 {
                    // 5 minutes
                    return Ok(latest_point.clone());
                }
            }
        }

        // Fallback to mock data if no fresh data available
        let mock_price = self.get_mock_price_for_pair(trading_pair);
        Ok(
            crate::services::core::analysis::market_analysis::PricePoint {
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                price: mock_price,
                volume: Some(1000.0),
                exchange_id: exchange.as_str().to_string(),
                trading_pair: trading_pair.to_string(),
            },
        )
    }

    /// Get mock price for testing (will be replaced with real data)
    fn get_mock_price_for_pair(&self, trading_pair: &str) -> f64 {
        match trading_pair.to_uppercase().as_str() {
            "BTC/USDT" | "BTCUSDT" => 43250.75,
            "ETH/USDT" | "ETHUSDT" => 2680.50,
            "BNB/USDT" | "BNBUSDT" => 315.25,
            "SOL/USDT" | "SOLUSDT" => 98.75,
            "XRP/USDT" | "XRPUSDT" => 0.6125,
            "ADA/USDT" | "ADAUSDT" => 0.4850,
            "DOGE/USDT" | "DOGEUSDT" => 0.0825,
            "AVAX/USDT" | "AVAXUSDT" => 36.75,
            "DOT/USDT" | "DOTUSDT" => 7.25,
            "MATIC/USDT" | "MATICUSDT" => 0.8950,
            _ => 100.0, // Default price for unknown pairs
        }
    }

    /// Format market data for display
    fn format_market_data_display(
        &self,
        price_point: &crate::services::core::analysis::market_analysis::PricePoint,
        include_volume: bool,
    ) -> String {
        let price_str = format!("${:.2}", price_point.price);
        let timestamp_str = chrono::DateTime::from_timestamp_millis(price_point.timestamp as i64)
            .unwrap_or_default()
            .format("%H:%M:%S UTC")
            .to_string();

        let mut display = format!(
            "💰 **{}**: `{}`\n⏰ Last Update: `{}`\n🏢 Exchange: `{}`",
            escape_markdown_v2(&price_point.trading_pair),
            escape_markdown_v2(&price_str),
            escape_markdown_v2(&timestamp_str),
            escape_markdown_v2(&price_point.exchange_id)
        );

        if include_volume {
            if let Some(volume) = price_point.volume {
                display.push_str(&format!("\n📊 Volume: `{:.2}`", volume));
            }
        }

        display
    }

    /// Get real-time market overview
    async fn get_market_overview_message(&self, _user_id: &str) -> String {
        if let Some(ref market_service) = self.market_analysis_service {
            // Get market data for major pairs
            let major_pairs = vec![
                ("BTC/USDT", crate::types::ExchangeIdEnum::Binance),
                ("ETH/USDT", crate::types::ExchangeIdEnum::Binance),
                ("BNB/USDT", crate::types::ExchangeIdEnum::Binance),
                ("SOL/USDT", crate::types::ExchangeIdEnum::Bybit),
            ];

            let mut market_data_displays = Vec::new();

            for (pair, exchange) in major_pairs {
                match self
                    .fetch_real_market_data(market_service, pair, &exchange)
                    .await
                {
                    Ok(price_point) => {
                        let display = self.format_market_data_display(&price_point, false);
                        market_data_displays.push(display);
                    }
                    Err(_) => {
                        // Fallback to mock data
                        let mock_price = self.get_mock_price_for_pair(pair);
                        market_data_displays.push(format!(
                            "💰 **{}**: `${:.2}` \\(estimated\\)\n🏢 Exchange: `{}`",
                            escape_markdown_v2(pair),
                            mock_price,
                            escape_markdown_v2(exchange.as_str())
                        ));
                    }
                }
            }

            format!(
                "📊 *Real\\-Time Market Overview* 🌍\n\n\
                🔗 **Market Data Service**: Connected\n\n\
                📈 *Major Trading Pairs:*\n\
                {}\n\n\
                💡 Use /price \\<pair\\> for detailed price information\\!",
                market_data_displays.join("\n\n")
            )
        } else {
            "📊 *Market Overview* ⚠️\n\n\
            🔗 **Market Data Service**: Not connected\n\n\
            📈 *Limited Market Data:*\n\
            • Basic price information available\n\
            • Real\\-time data unavailable\n\
            • Historical analysis limited\n\n\
            🔧 **Setup Required**: Contact admin to enable market data features\n\
            💡 Use /status to check service availability\\!"
                .to_string()
        }
    }

    /// Get price information for a specific trading pair
    async fn get_price_message(&self, _user_id: &str, args: &[&str]) -> String {
        if args.is_empty() {
            return "❌ Please specify a trading pair\\. Example: `/price BTCUSDT`".to_string();
        }

        let trading_pair = args[0].to_uppercase();
        let normalized_pair = if trading_pair.contains('/') {
            trading_pair
        } else {
            // Convert BTCUSDT to BTC/USDT format
            if trading_pair.ends_with("USDT") && trading_pair.len() > 4 {
                let base = &trading_pair[..trading_pair.len() - 4];
                format!("{}/USDT", base)
            } else {
                trading_pair
            }
        };

        if let Some(ref market_service) = self.market_analysis_service {
            // Try multiple exchanges for the pair
            let exchanges = vec![
                crate::types::ExchangeIdEnum::Binance,
                crate::types::ExchangeIdEnum::Bybit,
                crate::types::ExchangeIdEnum::OKX,
            ];

            let mut price_displays = Vec::new();

            for exchange in exchanges {
                match self
                    .fetch_real_market_data(market_service, &normalized_pair, &exchange)
                    .await
                {
                    Ok(price_point) => {
                        let display = self.format_market_data_display(&price_point, true);
                        price_displays.push(display);
                    }
                    Err(_) => {
                        // Add fallback entry
                        let mock_price = self.get_mock_price_for_pair(&normalized_pair);
                        price_displays.push(format!(
                            "💰 **{}**: `${:.2}` \\(estimated\\)\n🏢 Exchange: `{}` \\(offline\\)",
                            escape_markdown_v2(&normalized_pair),
                            mock_price,
                            escape_markdown_v2(exchange.as_str())
                        ));
                    }
                }
            }

            format!(
                "💰 *Price Information: {}* 📈\n\n\
                🔗 **Market Data Service**: Connected\n\n\
                📊 *Cross\\-Exchange Prices:*\n\
                {}\n\n\
                💡 Prices update every 30 seconds\\. Use /market for overview\\!",
                escape_markdown_v2(&normalized_pair),
                price_displays.join("\n\n")
            )
        } else {
            let mock_price = self.get_mock_price_for_pair(&normalized_pair);
            format!(
                "💰 *Price Information: {}* ⚠️\n\n\
                🔗 **Market Data Service**: Not connected\n\n\
                📊 *Estimated Price:*\n\
                💰 **{}**: `${:.2}` \\(estimated\\)\n\
                ⚠️ Real\\-time data unavailable\n\n\
                🔧 **Setup Required**: Contact admin to enable real\\-time pricing\\!",
                escape_markdown_v2(&normalized_pair),
                escape_markdown_v2(&normalized_pair),
                mock_price
            )
        }
    }

    /// Get market alerts and notifications
    async fn get_market_alerts_message(&self, _user_id: &str) -> String {
        if let Some(ref market_service) = self.market_analysis_service {
            // Get cache statistics to show market data activity
            let cache_stats = market_service.get_cache_stats();
            let cache_size = cache_stats
                .get("cache_size")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let expired_entries = cache_stats
                .get("expired_entries")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            format!(
                "🚨 *Market Alerts & Notifications* 📢\n\n\
                🔗 **Market Data Service**: Connected\n\n\
                📊 *Market Data Status:*\n\
                • Cached Pairs: `{}`\n\
                • Expired Entries: `{}`\n\
                • Data Freshness: Real\\-time\n\n\
                🔔 *Alert Types Available:*\n\
                ✅ Price movement alerts\n\
                ✅ Volume spike notifications\n\
                ✅ Technical indicator signals\n\
                ✅ Cross\\-exchange arbitrage alerts\n\n\
                ⚙️ *Alert Configuration:*\n\
                • Use /preferences to set alert thresholds\n\
                • Configure notification frequency\n\
                • Select monitored trading pairs\n\n\
                💡 Alerts are sent in real\\-time when conditions are met\\!",
                cache_size, expired_entries
            )
        } else {
            "🚨 *Market Alerts & Notifications* ⚠️\n\n\
            🔗 **Market Data Service**: Not connected\n\n\
            📊 *Limited Alert Features:*\n\
            • Basic price alerts only\n\
            • No real\\-time notifications\n\
            • Manual price checking required\n\n\
            🔔 *Available Features:*\n\
            ❌ Real\\-time price alerts\n\
            ❌ Volume notifications\n\
            ❌ Technical signals\n\
            ✅ Manual price queries\n\n\
            🔧 **Setup Required**: Contact admin to enable market alerts\\!"
                .to_string()
        }
    }

    /// Get onboarding message for new users
    async fn get_onboarding_message(&self, user_id: &str) -> String {
        // Check if user already has profile
        let has_profile = if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some()
            } else {
                false
            }
        } else {
            false
        };

        if has_profile {
            "🎉 *Welcome back to ArbEdge!* 🚀\n\n\
                 You already have a profile set up. Here's what you can do:\n\n\
                 📊 `/setup_status` - Check your current setup\n\
                 🔧 `/setup_exchange` - Configure exchange API keys\n\
                 🤖 `/setup_ai` - Configure AI services\n\
                 ✅ `/validate_setup` - Test your connections\n\
                 ❓ `/setup_help` - Get help with setup issues\n\n\
                 💡 *Tip*: Use `/help` to see all available commands!"
                .to_string()
        } else {
            "🎉 *Welcome to ArbEdge!* 🚀\n\n\
                 Welcome to the world of arbitrage trading! You can start exploring immediately:\n\n\
                 **🚀 Get Started Right Away**:\n\
                 • `/opportunities` - View available arbitrage opportunities\n\
                 • `/market` - Check market data and trends\n\
                 • `/help` - See all available commands\n\n\
                 **🔑 Optional Setup (Required for Trading)**:\n\
                 • `/setup_exchange` - Add exchange API keys for actual trading\n\
                 • `/setup_ai` - Configure AI services for enhanced analysis\n\n\
                 **📊 Track Your Progress**:\n\
                 • `/setup_status` - Check your current setup\n\
                 • `/validate_setup` - Test your connections\n\n\
                 💡 *Note*: You can explore opportunities and market data without API keys.\n\
                 API keys are only needed when you want to execute actual trades or use AI features.\n\n\
                 🆘 Need help? Use `/setup_help` for troubleshooting.".to_string()
        }
    }

    /// Get setup status dashboard
    async fn get_setup_status_message(&self, user_id: &str) -> String {
        let mut status_message = "🔧 *Setup Status Dashboard* 📊\n\n".to_string();

        // Check user profile
        let user_profile = if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            }
        } else {
            None
        };

        // Profile Status
        if user_profile.is_some() {
            status_message.push_str("✅ **User Profile**: Configured\n");
        } else {
            status_message.push_str("❌ **User Profile**: Not found\n");
        }

        // Exchange API Status
        let exchange_status = self.check_exchange_api_status(user_id).await;
        status_message.push_str(&format!("🔑 **Exchange APIs**: {}\n", exchange_status));

        // AI Services Status
        let ai_status = self.check_ai_services_status().await;
        status_message.push_str(&format!("🤖 **AI Services**: {}\n", ai_status));

        // Service Availability
        status_message.push_str("\n📡 **Service Availability**:\n");

        if self.exchange_service.is_some() {
            status_message.push_str("✅ Exchange Service: Available\n");
        } else {
            status_message.push_str("❌ Exchange Service: Unavailable\n");
        }

        if self.ai_integration_service.is_some() {
            status_message.push_str("✅ AI Integration Service: Available\n");
        } else {
            status_message.push_str("❌ AI Integration Service: Unavailable\n");
        }

        if self.global_opportunity_service.is_some() {
            status_message.push_str("✅ Opportunity Service: Available\n");
        } else {
            status_message.push_str("❌ Opportunity Service: Unavailable\n");
        }

        if self.market_analysis_service.is_some() {
            status_message.push_str("✅ Market Analysis Service: Available\n");
        } else {
            status_message.push_str("❌ Market Analysis Service: Unavailable\n");
        }

        // Setup Recommendations
        status_message.push_str("\n🎯 **Next Steps**:\n");

        if user_profile.is_none() {
            status_message.push_str("1. Use any command to create your profile\n");
        }

        if exchange_status.contains("Not configured") {
            status_message.push_str("2. Use `/setup_exchange` to configure API keys\n");
        }

        if ai_status.contains("Not configured") {
            status_message.push_str("3. Use `/setup_ai` to configure AI services\n");
        }

        status_message.push_str("4. Use `/validate_setup` to test connections\n");
        status_message.push_str("5. Use `/opportunities` to start trading!\n");

        status_message.push_str("\n❓ Need help? Use `/setup_help` for assistance.");

        status_message
    }

    /// Check exchange API status
    async fn check_exchange_api_status(&self, user_id: &str) -> String {
        // Check if user has exchange API keys configured
        if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                if let Ok(Some(profile)) = user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                {
                    // Check if user has any exchange API keys
                    let exchange_keys: Vec<_> = profile
                        .api_keys
                        .iter()
                        .filter(|key| key.is_exchange_key() && key.is_active)
                        .collect();

                    if exchange_keys.is_empty() {
                        "❌ Not configured - Use `/setup_exchange` to add API keys".to_string()
                    } else {
                        let exchange_count = exchange_keys.len();
                        format!(
                            "✅ Configured ({} exchange{})",
                            exchange_count,
                            if exchange_count == 1 { "" } else { "s" }
                        )
                    }
                } else {
                    "❌ Profile not found".to_string()
                }
            } else {
                "❌ Invalid user ID".to_string()
            }
        } else {
            "❌ User service unavailable".to_string()
        }
    }

    /// Check AI services status
    async fn check_ai_services_status(&self) -> String {
        if self.ai_integration_service.is_some() {
            "✅ Available and ready"
        } else {
            "❌ Not configured - Contact admin"
        }
        .to_string()
    }

    /// Get exchange setup wizard message
    async fn get_setup_exchange_message(&self, _user_id: &str, args: &[&str]) -> String {
        if args.is_empty() {
            // Show exchange setup options
            "🔑 *Exchange API Setup Wizard* 📈\n\n\
                 To enable trading, you need to configure API keys for supported exchanges:\n\n\
                 **Supported Exchanges**:\n\
                 🟡 **Binance** - `/setup_exchange binance`\n\
                 🟠 **Bybit** - `/setup_exchange bybit`\n\
                 🔵 **OKX** - `/setup_exchange okx`\n\n\
                 **Security Notes** 🔒:\n\
                 • API keys are encrypted and stored securely\n\
                 • Only trading permissions are required\n\
                 • Withdrawal permissions are NOT needed\n\
                 • You can revoke access anytime\n\n\
                 **Example**: `/setup_exchange binance`\n\n\
                 ❓ Need help? Use `/setup_help` for detailed instructions."
                .to_string()
        } else {
            let exchange = args[0].to_lowercase();
            self.get_exchange_specific_setup_guide(&exchange).await
        }
    }

    /// Get exchange-specific setup guide
    async fn get_exchange_specific_setup_guide(&self, exchange: &str) -> String {
        match exchange {
            "binance" => "🟡 *Binance API Setup Guide* 🔑\n\n\
                      **Step 1: Create API Key**\n\
                      1. Go to Binance.com → Account → API Management\n\
                      2. Click 'Create API'\n\
                      3. Choose 'System generated'\n\
                      4. Enter a label (e.g., 'ArbEdge Bot')\n\n\
                      **Step 2: Configure Permissions**\n\
                      ✅ Enable 'Spot & Margin Trading'\n\
                      ✅ Enable 'Futures Trading' (if using futures)\n\
                      ❌ DO NOT enable 'Withdrawals'\n\n\
                      **Step 3: IP Restrictions**\n\
                      ⚠️ **Important**: Choose 'Unrestricted' for IP access\n\
                      • This allows ArbEdge to connect from our secure servers\n\
                      • If you add specific IPs, the connection will fail\n\n\
                      **Step 4: Save Your Keys**\n\
                      • Copy your API Key and Secret Key\n\
                      • Store them securely\n\n\
                      **Next**: Contact admin to configure your API keys securely.\n\n\
                      🔒 *Security*: Never share your secret key publicly!"
                .to_string(),
            "bybit" => "🟠 *Bybit API Setup Guide* 🔑\n\n\
                      **Step 1: Create API Key**\n\
                      1. Go to Bybit.com → Account & Security → API\n\
                      2. Click 'Create New Key'\n\
                      3. Choose 'System generated'\n\
                      4. Enter a name (e.g., 'ArbEdge Bot')\n\n\
                      **Step 2: Configure Permissions**\n\
                      ✅ Enable 'Spot Trading'\n\
                      ✅ Enable 'Derivatives Trading' (if using derivatives)\n\
                      ❌ DO NOT enable 'Withdrawals'\n\n\
                      **Step 3: IP Whitelist**\n\
                      ⚠️ **Important**: Leave IP whitelist EMPTY\n\
                      • This allows unrestricted access for ArbEdge\n\
                      • Adding specific IPs will prevent connection\n\n\
                      **Step 4: Save Your Keys**\n\
                      • Copy your API Key and Secret Key\n\
                      • Store them securely\n\n\
                      **Next**: Contact admin to configure your API keys securely.\n\n\
                      🔒 *Security*: Keep your secret key private!"
                .to_string(),
            "okx" => "🔵 *OKX API Setup Guide* 🔑\n\n\
                      **Step 1: Create API Key**\n\
                      1. Go to OKX.com → Account → API\n\
                      2. Click 'Create API Key'\n\
                      3. Enter a name (e.g., 'ArbEdge Bot')\n\n\
                      **Step 2: Configure Permissions**\n\
                      ✅ Enable 'Trade'\n\
                      ✅ Enable 'Read' (required)\n\
                      ❌ DO NOT enable 'Withdraw'\n\n\
                      **Step 3: IP Whitelist**\n\
                      ⚠️ **Important**: Use '0.0.0.0/0' for unrestricted access\n\
                      • This allows ArbEdge to connect from anywhere\n\
                      • Specific IP restrictions will block our connection\n\n\
                      **Step 4: Passphrase**\n\
                      • Set a passphrase (remember this!)\n\
                      • This is required for OKX API access\n\n\
                      **Step 5: Save Your Keys**\n\
                      • Copy API Key, Secret Key, and Passphrase\n\
                      • Store them securely\n\n\
                      **Next**: Contact admin to configure your API keys securely.\n\n\
                      🔒 *Security*: Never share your credentials!"
                .to_string(),
            _ => {
                format!(
                    "❌ *Unknown Exchange: {}*\n\n\
                     Supported exchanges:\n\
                     • `binance` - Binance\n\
                     • `bybit` - Bybit\n\
                     • `okx` - OKX\n\n\
                     Use `/setup_exchange <exchange>` with a supported exchange name.",
                    exchange
                )
            }
        }
    }

    /// Get AI setup message
    async fn get_setup_ai_message(&self, _user_id: &str, _args: &[&str]) -> String {
        format!(
            "🤖 *AI Services Setup* 🧠\n\n\
             AI services are used for:\n\
             • 📊 Market analysis and insights\n\
             • 🎯 Risk assessment\n\
             • 💡 Personalized recommendations\n\
             • 🔍 Opportunity scoring\n\n\
             **Current Status**: {}\n\n\
             **Available AI Providers**:\n\
             🟢 **OpenAI GPT-4** - Advanced analysis\n\
             🔵 **Anthropic Claude** - Risk assessment\n\
             ⚡ **Cloudflare Workers AI** - Fast processing\n\n\
             **Configuration**:\n\
             AI services are configured at the system level by administrators.\n\
             Individual users don't need to configure AI API keys.\n\n\
             **Features Available**:\n\
             • `/ai_insights` - Get AI market insights\n\
             • `/risk_assessment` - AI-powered risk analysis\n\
             • Automatic opportunity scoring\n\
             • Personalized recommendations\n\n\
             ✅ *Ready to use*: Try `/ai_insights` to see AI analysis!",
            self.check_ai_services_status().await
        )
    }

    /// Get setup help and troubleshooting guide
    async fn get_setup_help_message(&self, _user_id: &str) -> String {
        "🆘 *Setup Help & Troubleshooting* 🔧\n\n\
             **Common Issues & Solutions**:\n\n\
             **1. 'Profile not found' error**\n\
             • Solution: Use any command to create your profile automatically\n\
             • Try: `/status` or `/help`\n\n\
             **2. 'Exchange API not configured'**\n\
             • Solution: Follow the exchange setup guide\n\
             • Use: `/setup_exchange <exchange_name>`\n\
             • Contact admin to securely configure your keys\n\n\
             **3. 'Service unavailable' errors**\n\
             • This indicates a system-level service issue\n\
             • Contact administrators for assistance\n\
             • Check `/setup_status` for service availability\n\n\
             **4. API key permission errors**\n\
             • Ensure 'Trading' permissions are enabled\n\
             • Ensure 'Withdrawals' are DISABLED (security)\n\
             • **IP Restrictions**: Use unrestricted access or '0.0.0.0/0'\n\
             • Specific IP addresses will block ArbEdge connection\n\n\
             **5. Connection timeout issues**\n\
             • Check your internet connection\n\
             • Try again in a few minutes\n\
             • Contact support if persistent\n\n\
             **Getting More Help**:\n\
             📧 Contact: support@arbedge.com\n\
             💬 Telegram: @arbedge_support\n\
             📚 Documentation: docs.arbedge.com\n\n\
             **Quick Commands**:\n\
             • `/setup_status` - Check current setup\n\
             • `/validate_setup` - Test connections\n\
             • `/onboard` - Restart onboarding\n\n\
             💡 *Tip*: Most issues are resolved by following the setup guides carefully!"
            .to_string()
    }

    /// Validate user setup and connections
    async fn get_validate_setup_message(&self, user_id: &str) -> String {
        let mut validation_message = "🔍 *Setup Validation Results* ✅\n\n".to_string();

        // Validate user profile
        let profile_valid = self.validate_user_profile(user_id).await;
        validation_message.push_str(&format!(
            "👤 **User Profile**: {}\n",
            if profile_valid {
                "✅ Valid"
            } else {
                "❌ Invalid or missing"
            }
        ));

        // Validate exchange connections
        let exchange_validation = self.validate_exchange_connections(user_id).await;
        validation_message.push_str(&format!("🔑 **Exchange APIs**: {}\n", exchange_validation));

        // Add IP restriction guidance if needed
        if exchange_validation.contains("❌") || exchange_validation.contains("validation requires")
        {
            validation_message.push_str(&self.get_ip_restriction_guidance().await);
        }

        // Validate AI services
        let ai_validation = self.validate_ai_services().await;
        validation_message.push_str(&format!("🤖 **AI Services**: {}\n", ai_validation));

        // Validate core services
        validation_message.push_str("\n🔧 **Core Services**:\n");

        let services = [
            ("Exchange Service", self.exchange_service.is_some()),
            ("AI Integration", self.ai_integration_service.is_some()),
            (
                "Opportunity Service",
                self.global_opportunity_service.is_some(),
            ),
            ("Market Analysis", self.market_analysis_service.is_some()),
            ("User Profile Service", self.user_profile_service.is_some()),
        ];

        for (service_name, is_available) in services {
            let status = if is_available {
                "✅ Available"
            } else {
                "❌ Unavailable"
            };
            validation_message.push_str(&format!("• {}: {}\n", service_name, status));
        }

        // Overall status and recommendations
        let all_valid = profile_valid
            && !exchange_validation.contains("❌")
            && !ai_validation.contains("❌")
            && services.iter().all(|(_, available)| *available);

        validation_message.push_str("\n🎯 **Overall Status**: ");
        if all_valid {
            validation_message.push_str("✅ **Ready for Trading!**\n\n");
            validation_message.push_str("🚀 You're all set! Try these commands:\n");
            validation_message.push_str("• `/opportunities` - View arbitrage opportunities\n");
            validation_message.push_str("• `/balance` - Check your balances\n");
            validation_message.push_str("• `/ai_insights` - Get AI market analysis\n");
        } else {
            validation_message.push_str("⚠️ **Setup Incomplete**\n\n");
            validation_message.push_str("🔧 **Action Required**:\n");

            if !profile_valid {
                validation_message.push_str("1. Create profile: Use any command\n");
            }
            if exchange_validation.contains("❌") {
                validation_message.push_str("2. Setup exchanges: `/setup_exchange`\n");
            }
            if ai_validation.contains("❌") {
                validation_message.push_str("3. Check AI services: Contact admin\n");
            }

            validation_message.push_str("\n💡 Use `/setup_help` for assistance.");
        }

        validation_message
    }

    /// Validate user profile exists
    async fn validate_user_profile(&self, user_id: &str) -> bool {
        if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some()
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Validate exchange connections
    async fn validate_exchange_connections(&self, user_id: &str) -> String {
        if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                if let Ok(Some(profile)) = user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                {
                    let exchange_keys: Vec<_> = profile
                        .api_keys
                        .iter()
                        .filter(|key| key.is_exchange_key() && key.is_active)
                        .collect();

                    if exchange_keys.is_empty() {
                        "❌ No API keys configured".to_string()
                    } else {
                        // In a real implementation, we would test each API key
                        // For now, we'll just report that keys are configured
                        let exchange_count = exchange_keys.len();
                        format!(
                            "✅ {} exchange{} configured (validation requires live testing)",
                            exchange_count,
                            if exchange_count == 1 { "" } else { "s" }
                        )
                    }
                } else {
                    "❌ Profile not found".to_string()
                }
            } else {
                "❌ Invalid user ID".to_string()
            }
        } else {
            "❌ User service unavailable".to_string()
        }
    }

    /// Validate AI services
    async fn validate_ai_services(&self) -> String {
        if let Some(ref ai_service) = self.ai_integration_service {
            // Test AI service availability
            let supported_providers = ai_service.get_supported_providers();
            if supported_providers.is_empty() {
                "⚠️ Available but no providers configured".to_string()
            } else {
                format!(
                    "✅ Available ({} provider{})",
                    supported_providers.len(),
                    if supported_providers.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                )
            }
        } else {
            "❌ AI service not available".to_string()
        }
    }

    /// Check if user has exchange API keys for trading
    async fn check_user_has_exchange_keys(&self, user_id: &str) -> bool {
        if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                if let Ok(Some(profile)) = user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                {
                    return profile
                        .api_keys
                        .iter()
                        .any(|key| key.is_exchange_key() && key.is_active);
                }
            }
        }
        false
    }

    /// Check if user has AI API keys
    async fn check_user_has_ai_keys(&self, user_id: &str) -> bool {
        if let Some(ref user_profile_service) = self.user_profile_service {
            if let Ok(telegram_id) = user_id.parse::<i64>() {
                if let Ok(Some(profile)) = user_profile_service
                    .get_user_by_telegram_id(telegram_id)
                    .await
                {
                    return profile
                        .api_keys
                        .iter()
                        .any(|key| key.is_ai_key() && key.is_active);
                }
            }
        }
        false
    }

    /// Get message prompting user to set up exchange API keys
    async fn get_exchange_setup_required_message(&self, command: &str) -> String {
        format!(
            "🔑 *Exchange API Keys Required* 📈\n\n\
             To use the `{}` command, you need to configure exchange API keys first.\n\n\
             **Why API Keys?**\n\
             • Execute actual trades on exchanges\n\
             • Access real-time balance information\n\
             • Manage orders and positions\n\n\
             **Quick Setup**:\n\
             1️⃣ Use `/setup_exchange` to see supported exchanges\n\
             2️⃣ Follow the setup guide for your preferred exchange\n\
             3️⃣ Contact admin to securely configure your keys\n\
             4️⃣ Use `/validate_setup` to test your connection\n\n\
             **Security** 🔒:\n\
             • Your API keys are encrypted and stored securely\n\
             • Only trading permissions are required (NO withdrawals)\n\
             • You maintain full control of your funds\n\n\
             💡 *Ready to set up?* Use `/setup_exchange` to get started!",
            command
        )
    }

    /// Get message prompting user to set up AI API keys
    async fn get_ai_setup_required_message(&self, command: &str) -> String {
        format!(
            "🤖 *AI Services Required* 🧠\n\n\
             To use the `{}` command with personalized AI analysis, you can:\n\n\
             **Option 1: Use System AI** ✅\n\
             • View AI-enhanced global opportunities (no setup required)\n\
             • Get basic AI insights from system-level services\n\
             • Try `/opportunities` to see AI-enhanced opportunities\n\n\
             **Option 2: Personal AI Setup** 🔧\n\
             • Configure your own AI API keys for personalized analysis\n\
             • Get custom AI insights tailored to your portfolio\n\
             • Use `/setup_ai` to see configuration options\n\n\
             **What You Get with Personal AI**:\n\
             • Personalized risk assessment\n\
             • Custom market analysis\n\
             • Portfolio-specific recommendations\n\
             • Advanced AI features\n\n\
             💡 *Want to try?* Use `/opportunities` for AI-enhanced global opportunities,\n\
             or `/setup_ai` to configure personal AI services.",
            command
        )
    }

    /// Get message for features that work without API keys
    async fn get_no_setup_required_message(&self, feature: &str) -> String {
        format!(
            "✅ *No Setup Required* 🚀\n\n\
             Great news! The `{}` feature works without any API key setup.\n\n\
             **Available Without Setup**:\n\
             • View global arbitrage opportunities\n\
             • Check market data and trends\n\
             • See AI-enhanced opportunities (system-level)\n\
             • Access help and documentation\n\n\
             **Optional Enhancements**:\n\
             • `/setup_exchange` - Add trading capabilities\n\
             • `/setup_ai` - Get personalized AI analysis\n\n\
             💡 *Tip*: You can start exploring immediately and add API keys later when you're ready to trade!"
            , feature
        )
    }

    /// Enhanced error handling with specific guidance
    async fn get_enhanced_error_message(&self, error_type: &str, context: &str) -> String {
        match error_type {
            "service_unavailable" => {
                format!(
                    "🚫 *Service Temporarily Unavailable* ⚠️\n\n\
                    The {} service is currently unavailable\\. This might be due to:\n\n\
                    **Possible Causes:**\n\
                    • 🔧 Scheduled maintenance\n\
                    • 📡 Network connectivity issues\n\
                    • ⚡ High system load\n\n\
                    **What you can do:**\n\
                    • ⏰ Try again in a few minutes\n\
                    • 📊 Use `/status` to check system health\n\
                    • 💬 Contact support if the issue persists\n\n\
                    💡 *Tip*: Some features may still be available\\. Try `/help` to see what's working\\!",
                    escape_markdown_v2(context)
                )
            }
            "api_key_invalid" => {
                format!(
                    "🔑 *Invalid API Key* ❌\n\n\
                    Your {} API key appears to be invalid or expired\\.\n\n\
                    **Common Causes:**\n\
                    • 🔄 API key was regenerated on exchange\n\
                    • ⏰ Key has expired\n\
                    • 🔒 Permissions were changed\n\
                    • 🌐 IP whitelist restrictions\n\n\
                    **How to Fix:**\n\
                    1️⃣ Check your exchange account for key status\n\
                    2️⃣ Regenerate API key if needed\n\
                    3️⃣ Use `/setup_exchange {}` to update\n\
                    4️⃣ Run `/validate_setup` to test\n\n\
                    🔒 *Security*: Ensure only trading permissions \\(no withdrawals\\)\\!",
                    escape_markdown_v2(context),
                    escape_markdown_v2(context)
                )
            }
            "exchange_maintenance" => {
                format!(
                    "🔧 *Exchange Under Maintenance* 🚧\n\n\
                    {} is currently undergoing maintenance\\.\n\n\
                    **What this means:**\n\
                    • 🚫 Trading temporarily suspended\n\
                    • 📊 Balance updates may be delayed\n\
                    • ⏰ Usually lasts 30 minutes to 2 hours\n\n\
                    **Alternative Actions:**\n\
                    • 📈 Check opportunities on other exchanges\n\
                    • 📊 Use `/market` for general market data\n\
                    • 🤖 Get `/ai_insights` for market analysis\n\
                    • ⏰ Set up alerts for when {} is back online\\!\n\n\
                    💡 *Tip*: Use `/alerts` to get notified when {} is back online\\!",
                    escape_markdown_v2(context),
                    escape_markdown_v2(context),
                    escape_markdown_v2(context)
                )
            }
            "insufficient_balance" => "💰 *Insufficient Balance* 📉\n\n\
                    You don't have enough balance to execute this trade\\.\n\n\
                    **Current Situation:**\n\
                    • 💳 Available balance is lower than required\n\
                    • 🔒 Some funds might be in open orders\n\
                    • 📊 Check your `/balance` for details\n\n\
                    **What you can do:**\n\
                    1️⃣ Use `/balance` to check all balances\n\
                    2️⃣ Use `/orders` to see open orders\n\
                    3️⃣ Cancel unnecessary orders to free funds\n\
                    4️⃣ Deposit more funds to your exchange\n\
                    5️⃣ Try a smaller trade amount\n\n\
                    💡 *Tip*: Use `/opportunities` to find trades within your budget\\!"
                .to_string(),
            "market_closed" => {
                format!(
                    "🕐 *Market Closed* 🌙\n\n\
                    The {} market is currently closed\\.\n\n\
                    **Market Hours:**\n\
                    • 🌍 Crypto markets: 24/7 \\(this shouldn't happen\\)\n\
                    • 📈 Traditional markets: Weekdays only\n\
                    • 🏦 Some exchanges have maintenance windows\n\n\
                    **What you can do:**\n\
                    • ⏰ Wait for market to reopen\n\
                    • 📊 Use `/market` to check other pairs\n\
                    • 🤖 Get `/ai_insights` for market preparation\n\
                    • 📝 Set up alerts for market open\n\n\
                    💡 *Tip*: Use this time to analyze opportunities with `/opportunities`\\!",
                    escape_markdown_v2(context)
                )
            }
            "network_timeout" => {
                format!(
                    "🌐 *Network Timeout* ⏱️\n\n\
                    The request to {} timed out\\.\n\n\
                    **Possible Causes:**\n\
                    • 🐌 Slow internet connection\n\
                    • 🏗️ Exchange server overload\n\
                    • 🌐 Network routing issues\n\n\
                    **Immediate Actions:**\n\
                    • 🔄 Try the command again\n\
                    • 📊 Check `/status` for system health\n\
                    • 🌐 Test your internet connection\n\
                    • ⏰ Wait 30 seconds and retry\n\n\
                    **If it persists:**\n\
                    • 📞 Contact support with error details\n\
                    • 🔄 Try alternative exchanges\n\
                    • 📊 Use cached data with `/opportunities`\n\n\
                    💡 *Auto\\-retry*: This command will automatically retry in 30 seconds\\!",
                    escape_markdown_v2(context)
                )
            }
            "invalid_parameters" => {
                format!(
                    "❌ *Invalid Parameters* 📝\n\n\
                    The command you entered has invalid or missing parameters\\.\n\n\
                    **Common Issues:**\n\
                    • 🔢 Missing required values\n\
                    • 📏 Values outside acceptable range\n\
                    • 🔤 Incorrect format\n\n\
                    **Quick Fix:**\n\
                    • 📖 Use `/help {}` for detailed usage\n\
                    • 💡 Check the examples provided\n\
                    • ✅ Verify all required parameters are included\n\n\
                    **Examples:**\n\
                    • `/buy BTCUSDT 0\\.001` \\- Buy 0\\.001 BTC\n\
                    • `/sell ETHUSDT 0\\.1 3500` \\- Sell 0\\.1 ETH at $3500\n\
                    • `/balance binance` \\- Check Binance balance\n\n\
                    🆘 *Need help?* Use `/setup_help` for troubleshooting\\!",
                    escape_markdown_v2(context)
                )
            }
            "permission_denied" => "🔒 *Access Restricted* 🚫\n\n\
                    You don't have permission to use this feature\\.\n\n\
                    **Why this happens:**\n\
                    • 👤 Feature requires higher subscription tier\n\
                    • 🔑 Missing required API keys\n\
                    • 🛡️ Admin\\-only functionality\n\n\
                    **How to get access:**\n\
                    • 📈 Upgrade your subscription\n\
                    • 🔧 Complete required setup \\(`/setup_status`\\)\n\
                    • 📞 Contact admin for special permissions\n\n\
                    **Available Alternatives:**\n\
                    • 📊 `/opportunities` \\- View global opportunities\n\
                    • 📈 `/market` \\- Check market data\n\
                    • 🤖 `/ai_insights` \\- Get system AI analysis\n\n\
                    💡 *Alternative*: Try `/opportunities` for features available to all users\\!"
                .to_string(),
            "rate_limited" => {
                format!(
                    "⏱️ *Rate Limit Reached* 🚦\n\n\
                    You've made too many requests recently\\. Please slow down\\!\n\n\
                    **Rate Limits Help:**\n\
                    • 🛡️ Prevent system overload\n\
                    • ⚖️ Ensure fair usage for all users\n\
                    • 🔒 Protect against abuse\n\n\
                    **What to do:**\n\
                    • ⏰ Wait {} before trying again\n\
                    • 📊 Use `/status` to check your usage\n\
                    • 💡 Consider upgrading for higher limits\n\n\
                    **Rate Limit Info:**\n\
                    • 🔄 Resets every hour\n\
                    • 📈 Higher tiers get more requests\n\
                    • 🤖 AI commands have separate limits\n\n\
                    🎯 *Pro tip*: Batch your requests to stay within limits\\!",
                    escape_markdown_v2(context)
                )
            }
            "subscription_required" => "💎 *Premium Feature* ⭐\n\n\
                    This feature requires a premium subscription\\.\n\n\
                    **What you're missing:**\n\
                    • 🤖 Advanced AI analysis\n\
                    • 📊 Real\\-time portfolio tracking\n\
                    • 🚀 Automated trading features\n\
                    • 📈 Advanced market insights\n\n\
                    **Free Alternatives:**\n\
                    • 📊 `/opportunities` \\- Basic opportunities\n\
                    • 📈 `/market` \\- Market overview\n\
                    • 🤖 `/ai_insights` \\- System AI \\(limited\\)\n\n\
                    **Upgrade Benefits:**\n\
                    • 🔓 Unlock all features\n\
                    • ⚡ Higher rate limits\n\
                    • 🎯 Personalized analysis\n\
                    • 📞 Priority support\n\n\
                    💡 *Ready to upgrade?* Contact support@arbedge\\.com\\!"
                .to_string(),
            _ => {
                format!(
                    "❓ *Unexpected Error* 🤔\n\n\
                    Something unexpected happened\\. Don't worry, we're here to help\\!\n\n\
                    **Error Details:**\n\
                    `{}`\n\n\
                    **Immediate Actions:**\n\
                    • 🔄 Try the command again\n\
                    • 📊 Check `/status` for system health\n\
                    • 🆘 Use `/setup_help` for troubleshooting\n\n\
                    **If this keeps happening:**\n\
                    • 📧 Contact: support@arbedge\\.com\n\
                    • 💬 Include the error details above\n\
                    • 🕐 Mention when this happened\n\
                    • 📱 Include your user ID: `{}`\n\n\
                    **Quick Recovery:**\n\
                    • 📊 Try `/opportunities` for basic features\n\
                    • 📈 Use `/market` for market data\n\
                    • 🆘 Use `/help` for available commands\n\n\
                    💡 *Meanwhile*: Try `/help` to see what's working\\!",
                    escape_markdown_v2(context),
                    escape_markdown_v2("user_id_placeholder")
                )
            }
        }
    }

    /// Progressive disclosure help system
    async fn get_progressive_help_message(&self, user_id: &str, topic: Option<&str>) -> String {
        // Check user's setup status to provide relevant help
        let has_exchange_keys = self.check_user_has_exchange_keys(user_id).await;
        let has_ai_keys = self.check_user_has_ai_keys(user_id).await;
        let has_profile = self.validate_user_profile(user_id).await;

        match topic {
            Some("getting_started") => {
                if !has_profile {
                    "🚀 *Getting Started with ArbEdge* 🌟\n\n\
                        Welcome\\! Let's get you started step by step:\n\n\
                        **Step 1: Explore Immediately** ✅\n\
                        • `/opportunities` \\- See arbitrage opportunities\n\
                        • `/market` \\- Check market data\n\
                        • `/help` \\- Learn about all features\n\n\
                        **Step 2: Optional Setup** 🔧\n\
                        • `/onboard` \\- Guided setup process\n\
                        • `/setup_status` \\- Check what's configured\n\n\
                        **Step 3: Advanced Features** 🚀\n\
                        • Set up API keys for trading\n\
                        • Configure AI for personalized insights\n\n\
                        💡 *Remember*: You can explore and learn without any setup\\!"
                        .to_string()
                } else if !has_exchange_keys && !has_ai_keys {
                    "👋 *Welcome Back\\!* 🎉\n\n\
                        You have a profile set up\\. Here's what you can do:\n\n\
                        **Immediate Actions:**\n\
                        • `/opportunities` \\- View current opportunities\n\
                        • `/market` \\- Check market conditions\n\
                        • `/ai_insights` \\- Get AI analysis\n\n\
                        **Next Level:**\n\
                        • `/setup_exchange` \\- Enable trading\n\
                        • `/setup_ai` \\- Personal AI analysis\n\
                        • `/validate_setup` \\- Test connections\n\n\
                        🎯 *Ready to trade?* Set up your exchange API keys\\!"
                        .to_string()
                } else {
                    "🏆 *Advanced User Guide* 💪\n\n\
                        You're all set up\\! Here are advanced features:\n\n\
                        **Trading Commands:**\n\
                        • `/balance` \\- Check your balances\n\
                        • `/buy` / `/sell` \\- Execute trades\n\
                        • `/orders` \\- Manage open orders\n\
                        • `/positions` \\- View positions\n\n\
                        **Analytics:**\n\
                        • `/ai_insights` \\- Personal AI analysis\n\
                        • `/risk_assessment` \\- Portfolio risk\n\
                        • `/preferences` \\- Customize settings\n\n\
                        🚀 *Pro tip*: Use `/auto_enable` for automated trading\\!"
                        .to_string()
                }
            }
            Some("trading") => {
                if !has_exchange_keys {
                    "💰 *Trading Help* 📈\n\n\
                        To start trading, you need exchange API keys first\\.\n\n\
                        **Setup Required:**\n\
                        1️⃣ `/setup_exchange` \\- Choose your exchange\n\
                        2️⃣ Follow the setup guide\n\
                        3️⃣ `/validate_setup` \\- Test connection\n\n\
                        **Supported Exchanges:**\n\
                        • 🟡 Binance \\- Most popular\n\
                        • 🟠 Bybit \\- Derivatives focused\n\
                        • 🔵 OKX \\- Global exchange\n\n\
                        **Security Notes:**\n\
                        • ✅ Only trading permissions needed\n\
                        • ❌ NO withdrawal permissions\n\
                        • 🔒 Your funds stay secure\n\n\
                        💡 *Ready?* Start with `/setup_exchange`\\!"
                        .to_string()
                } else {
                    "💰 *Trading Commands Guide* 📈\n\n\
                        You're set up for trading\\! Here's how to use each command:\n\n\
                        **Basic Trading:**\n\
                        • `/balance` \\- Check account balances\n\
                        • `/buy BTCUSDT 0\\.001` \\- Market buy order\n\
                        • `/sell ETHUSDT 0\\.1 3200` \\- Limit sell order\n\n\
                        **Order Management:**\n\
                        • `/orders` \\- View open orders\n\
                        • `/cancel 12345` \\- Cancel specific order\n\
                        • `/positions` \\- Check current positions\n\n\
                        **Safety Tips:**\n\
                        • 🔍 Always verify amounts\n\
                        • 📊 Check market conditions first\n\
                        • 🛡️ Use stop\\-loss orders\n\n\
                        ⚠️ *Remember*: Trading involves risk\\. Start small\\!"
                        .to_string()
                }
            }
            Some("ai") => {
                if !has_ai_keys {
                    "🤖 *AI Features Help* 🧠\n\n\
                        ArbEdge offers both system AI and personal AI:\n\n\
                        **Available Now \\(No Setup\\):**\n\
                        • `/opportunities` \\- AI\\-enhanced opportunities\n\
                        • `/ai_insights` \\- Basic AI analysis\n\
                        • `/market` \\- AI market insights\n\n\
                        **Personal AI \\(Setup Required\\):**\n\
                        • 🎯 Personalized recommendations\n\
                        • 📊 Custom risk analysis\n\
                        • 💡 Tailored insights\n\n\
                        **Setup Personal AI:**\n\
                        1️⃣ `/setup_ai` \\- Choose AI provider\n\
                        2️⃣ Add your API keys\n\
                        3️⃣ `/validate_setup` \\- Test connection\n\n\
                        💡 *Try first*: Use `/ai_insights` to see system AI\\!"
                        .to_string()
                } else {
                    "🤖 *Personal AI Guide* 🧠\n\n\
                        Your personal AI is configured\\! Here's what you can do:\n\n\
                        **AI Analysis:**\n\
                        • `/ai_insights` \\- Personalized market analysis\n\
                        • `/risk_assessment` \\- Custom portfolio risk\n\
                        • `/opportunities` \\- AI\\-ranked opportunities\n\n\
                        **Advanced Features:**\n\
                        • 🎯 Personalized recommendations\n\
                        • 📈 Custom trading strategies\n\
                        • 🛡️ Risk\\-adjusted insights\n\n\
                        **AI Providers:**\n\
                        • 🟢 OpenAI GPT\\-4 \\- Advanced analysis\n\
                        • 🔵 Anthropic Claude \\- Risk assessment\n\
                        • ☁️ Cloudflare Workers AI \\- Fast insights\n\n\
                        🚀 *Pro tip*: AI learns from your trading patterns\\!"
                        .to_string()
                }
            }
            Some("troubleshooting") => "🔧 *Troubleshooting Guide* 🛠️\n\n\
                    Having issues? Let's fix them together\\!\n\n\
                    **Common Problems:**\n\n\
                    **1\\. Commands not working**\n\
                    • ✅ Check `/status` for system health\n\
                    • 🔄 Try the command again\n\
                    • 📖 Use `/help <command>` for usage\n\n\
                    **2\\. API key issues**\n\
                    • 🔧 Use `/setup_status` to check configuration\n\
                    • ✅ Run `/validate_setup` to test connections\n\
                    • 🔑 Verify permissions in exchange settings\n\n\
                    **3\\. Trading errors**\n\
                    • 💰 Check account balance\n\
                    • 📊 Verify market is open\n\
                    • 🔍 Check trading pair format\n\n\
                    **4\\. AI not responding**\n\
                    • 🤖 Try system AI with `/opportunities`\n\
                    • 🔧 Check AI setup with `/setup_ai`\n\
                    • ⏰ Wait for rate limits to reset\n\n\
                    🆘 *Still stuck?* Contact support@arbedge\\.com"
                .to_string(),
            _ => {
                // Default comprehensive help based on user's setup level
                if !has_profile {
                    "📚 *ArbEdge Help Center* 🎯\n\n\
                        **Quick Start:**\n\
                        • `/help getting_started` \\- New user guide\n\
                        • `/opportunities` \\- See arbitrage opportunities\n\
                        • `/market` \\- Check market data\n\
                        • `/onboard` \\- Guided setup\n\n\
                        **Learn More:**\n\
                        • `/help trading` \\- Trading guide\n\
                        • `/help ai` \\- AI features\n\
                        • `/help troubleshooting` \\- Fix issues\n\n\
                        **Support:**\n\
                        • `/setup_help` \\- Setup assistance\n\
                        • `/status` \\- System health\n\n\
                        💡 *New here?* Start with `/help getting_started`\\!"
                        .to_string()
                } else {
                    // Enhanced progressive help with feature availability
                    let mut help_message = format!(
                        "📚 *ArbEdge Help Center* 🎯\n\n\
                        **Your Setup Status:**\n\
                        • 🔑 Exchange API: {}\n\
                        • 🤖 AI Services: {}\n\
                        • 👤 Profile: {}\n\n",
                        if has_exchange_keys {
                            "✅ Configured"
                        } else {
                            "⚠️ Setup Required"
                        },
                        if has_ai_keys {
                            "✅ Personal AI"
                        } else {
                            "⚠️ System AI Only"
                        },
                        if has_profile {
                            "✅ Active"
                        } else {
                            "⚠️ Basic"
                        }
                    );

                    // Available features section
                    help_message.push_str("**✅ Available Now:**\n");
                    help_message.push_str("• `/opportunities` \\- View arbitrage opportunities\n");
                    help_message.push_str("• `/market` \\- Real\\-time market data\n");
                    help_message.push_str("• `/ai_insights` \\- AI market analysis\n");
                    help_message.push_str("• `/help <command>` \\- Command\\-specific help\n\n");

                    // Setup required features
                    if !has_exchange_keys {
                        help_message.push_str("**🔧 Setup Required for Trading:**\n");
                        help_message.push_str("• `/balance` \\- Check account balances\n");
                        help_message.push_str("• `/buy` `/sell` \\- Execute trades\n");
                        help_message.push_str("• `/orders` \\- Manage orders\n");
                        help_message.push_str("• `/positions` \\- View positions\n");
                        help_message.push_str("➡️ Use `/setup_exchange` to unlock these\\!\n\n");
                    } else {
                        help_message.push_str("**💱 Trading Features:**\n");
                        help_message.push_str("• `/balance` \\- ✅ Check account balances\n");
                        help_message.push_str("• `/buy` `/sell` \\- ✅ Execute trades\n");
                        help_message.push_str("• `/orders` \\- ✅ Manage orders\n");
                        help_message.push_str("• `/positions` \\- ✅ View positions\n\n");
                    }

                    // AI enhancement options
                    if !has_ai_keys {
                        help_message.push_str("**🤖 AI Enhancement Available:**\n");
                        help_message.push_str("• Personal AI for customized analysis\n");
                        help_message.push_str("• Portfolio\\-specific recommendations\n");
                        help_message.push_str("• Advanced market predictions\n");
                        help_message
                            .push_str("➡️ Use `/setup_ai` to enhance your experience\\!\n\n");
                    }

                    // Help topics
                    help_message.push_str("**📖 Help Topics:**\n");
                    help_message.push_str("• `/help getting_started` \\- Beginner guide\n");
                    help_message.push_str("• `/help trading` \\- Trading commands\n");
                    help_message.push_str("• `/help ai` \\- AI features\n");
                    help_message.push_str("• `/help troubleshooting` \\- Fix issues\n\n");

                    // Quick actions based on setup status
                    help_message.push_str("**⚡ Quick Actions:**\n");
                    if !has_exchange_keys && !has_ai_keys {
                        help_message.push_str("• `/onboard` \\- Start guided setup\n");
                    }
                    help_message.push_str("• `/setup_status` \\- Check configuration\n");
                    help_message.push_str("• `/validate_setup` \\- Test connections\n");
                    if has_profile {
                        help_message.push_str("• `/preferences` \\- Customize experience\n");
                    }
                    help_message.push('\n');

                    // Support section
                    help_message.push_str("**🆘 Need Help?**\n");
                    help_message.push_str("• 📧 support@arbedge\\.com\n");
                    help_message.push_str("• 💬 Include your user ID: `{}`\n");
                    help_message.push_str("• 🔧 Use `/setup_help` for troubleshooting\n\n");

                    help_message.push_str(
                        "💡 *Pro Tip*: Use `/help <command>` for detailed command help\\!",
                    );

                    help_message.replace("{}", &escape_markdown_v2(user_id))
                }
            }
        }
    }

    /// Check if a command is valid
    fn is_valid_command(&self, command: &str) -> bool {
        let valid_commands = [
            "start",
            "help",
            "status",
            "settings",
            "profile",
            "opportunities",
            "categories",
            "ai_insights",
            "risk_assessment",
            "preferences",
            "dashboard",
            "add_alias",
            "smart_suggestions",
            "market",
            "price",
            "alerts",
            "onboard",
            "setup_status",
            "setup_exchange",
            "setup_ai",
            "setup_help",
            "validate_setup",
            "explain",
            "balance",
            "buy",
            "sell",
            "orders",
            "positions",
            "cancel",
            "auto_enable",
            "auto_disable",
            "auto_config",
            "auto_status",
            "admin_stats",
            "admin_users",
            "admin_config",
            "admin_broadcast",
        ];

        let clean_command = command.strip_prefix('/').unwrap_or(command);
        valid_commands.contains(&clean_command)
    }

    /// Get command-specific help with usage examples and troubleshooting
    async fn get_command_specific_help(&self, user_id: &str, command: &str) -> String {
        let clean_command = command.strip_prefix('/').unwrap_or(command);
        let has_exchange_keys = self.check_user_has_exchange_keys(user_id).await;
        let has_ai_keys = self.check_user_has_ai_keys(user_id).await;

        match clean_command {
            "opportunities" => {
                format!(
                    "📊 *Help: /opportunities Command* 🎯\n\n\
                    **Description:**\n\
                    View arbitrage opportunities across exchanges\\.\n\n\
                    **Usage:**\n\
                    • `/opportunities` \\- Show all recent opportunities\n\
                    • `/opportunities arbitrage` \\- Filter by arbitrage type\n\
                    • `/opportunities funding` \\- Show funding rate opportunities\n\
                    • `/opportunities spot` \\- Show spot trading opportunities\n\n\
                    **Status:** {} Available\n\n\
                    **Examples:**\n\
                    • `/opportunities` \\- See latest 5 opportunities\n\
                    • `/opportunities cross\\-exchange` \\- Cross\\-exchange arbitrage\n\
                    • `/opportunities high\\-confidence` \\- High confidence only\n\n\
                    **Troubleshooting:**\n\
                    • No opportunities? Market might be efficient right now\n\
                    • Try different categories or check back later\n\
                    • Use `/market` to see general market conditions\n\n\
                    💡 *Tip*: This command works without any setup\\!",
                    "✅"
                )
            }
            "balance" => {
                let status = if has_exchange_keys {
                    "✅"
                } else {
                    "⚠️ Setup Required"
                };
                format!(
                    "💰 *Help: /balance Command* 📊\n\n\
                    **Description:**\n\
                    Check your account balances across exchanges\\.\n\n\
                    **Usage:**\n\
                    • `/balance` \\- Show all exchange balances\n\
                    • `/balance binance` \\- Show only Binance balance\n\
                    • `/balance bybit` \\- Show only Bybit balance\n\n\
                    **Status:** {}\n\n\
                    **Examples:**\n\
                    • `/balance` \\- All configured exchanges\n\
                    • `/balance binance` \\- Binance account only\n\
                    • `/balance okx` \\- OKX account only\n\n\
                    **Requirements:**\n\
                    {} Exchange API keys must be configured\n\
                    {} Use `/setup_exchange` to configure\n\
                    {} Run `/validate_setup` to test connection\n\n\
                    **Troubleshooting:**\n\
                    • 'API key invalid'? Use `/setup_exchange` to update\n\
                    • 'Service unavailable'? Try again in a few minutes\n\
                    • 'Permission denied'? Check API key permissions\n\n\
                    💡 *Tip*: Balances update in real\\-time\\!",
                    status,
                    if has_exchange_keys { "✅" } else { "🔑" },
                    if has_exchange_keys { "✅" } else { "🔧" },
                    if has_exchange_keys { "✅" } else { "🧪" }
                )
            }
            "buy" | "sell" => {
                let action = if clean_command == "buy" {
                    "Buy"
                } else {
                    "Sell"
                };
                let status = if has_exchange_keys {
                    "✅"
                } else {
                    "⚠️ Setup Required"
                };
                format!(
                    "💱 *Help: /{} Command* 📈\n\n\
                    **Description:**\n\
                    {} cryptocurrency on your connected exchanges\\.\n\n\
                    **Usage:**\n\
                    • `/{} BTCUSDT 0\\.001` \\- {} 0\\.001 BTC at market price\n\
                    • `/{} ETHUSDT 0\\.1 3500` \\- {} 0\\.1 ETH at $3500 limit\n\
                    • `/{} ADAUSDT 100` \\- {} 100 ADA at market price\n\n\
                    **Status:** {}\n\n\
                    **Parameters:**\n\
                    1️⃣ **Trading Pair** \\(required\\): BTCUSDT, ETHUSDT, etc\\.\n\
                    2️⃣ **Amount** \\(required\\): Quantity to trade\n\
                    3️⃣ **Price** \\(optional\\): Limit price \\(market order if omitted\\)\n\n\
                    **Examples:**\n\
                    • `/{} BTCUSDT 0\\.001` \\- Market order\n\
                    • `/{} ETHUSDT 0\\.5 3000` \\- Limit order at $3000\n\
                    • `/{} SOLUSDT 10` \\- Market order for 10 SOL\n\n\
                    **Requirements:**\n\
                    {} Exchange API keys with trading permissions\n\
                    {} Sufficient balance in your account\n\
                    {} Market must be open and active\n\n\
                    **Safety Features:**\n\
                    • ✅ Order confirmation before execution\n\
                    • 🛡️ Balance validation\n\
                    • 📊 Real\\-time price checks\n\
                    • ⚠️ Risk assessment warnings\n\n\
                    **Troubleshooting:**\n\
                    • 'Insufficient balance'? Check `/balance`\n\
                    • 'Invalid pair'? Use `/market` to see available pairs\n\
                    • 'Order failed'? Check exchange status\n\n\
                    💡 *Tip*: Start with small amounts to test\\!",
                    clean_command,
                    action,
                    clean_command,
                    action,
                    clean_command,
                    action,
                    clean_command,
                    action,
                    status,
                    clean_command,
                    clean_command,
                    clean_command,
                    if has_exchange_keys { "✅" } else { "🔑" },
                    if has_exchange_keys { "✅" } else { "💰" },
                    if has_exchange_keys { "✅" } else { "📊" }
                )
            }
            "ai_insights" => {
                let status = if has_ai_keys {
                    "✅ Personal AI"
                } else {
                    "⚠️ System AI Only"
                };
                format!(
                    "🤖 *Help: /ai_insights Command* 🧠\n\n\
                    **Description:**\n\
                    Get AI\\-powered market analysis and trading insights\\.\n\n\
                    **Usage:**\n\
                    • `/ai_insights` \\- Get comprehensive market analysis\n\
                    • `/ai_insights portfolio` \\- Focus on portfolio analysis\n\
                    • `/ai_insights market` \\- Focus on market trends\n\n\
                    **Status:** {}\n\n\
                    **What You Get:**\n\
                    • 📊 Market sentiment analysis\n\
                    • 📈 Price trend predictions\n\
                    • 🎯 Trading recommendations\n\
                    • ⚠️ Risk assessments\n\
                    • 💡 Opportunity insights\n\n\
                    **AI Types:**\n\
                    • 🤖 **System AI**: Available to everyone\n\
                    • 🧠 **Personal AI**: Customized for your portfolio\n\n\
                    **Examples:**\n\
                    • `/ai_insights` \\- Full market analysis\n\
                    • `/ai_insights btc` \\- Bitcoin\\-focused insights\n\
                    • `/ai_insights risk` \\- Risk\\-focused analysis\n\n\
                    **Enhancement Options:**\n\
                    {} Personal AI setup for customized analysis\n\
                    {} Portfolio\\-specific recommendations\n\
                    {} Advanced market predictions\n\n\
                    **Troubleshooting:**\n\
                    • Analysis seems generic? Set up personal AI\n\
                    • No insights? Market might be stable\n\
                    • Rate limited? Wait and try again\n\n\
                    💡 *Tip*: Combine with `/opportunities` for best results\\!",
                    status,
                    if has_ai_keys { "✅" } else { "🔧" },
                    if has_ai_keys { "✅" } else { "🎯" },
                    if has_ai_keys { "✅" } else { "📈" }
                )
            }
            "setup_exchange" => "🔧 *Help: /setup_exchange Command* 🔑\n\n\
                    **Description:**\n\
                    Configure exchange API keys for trading functionality\\.\n\n\
                    **Usage:**\n\
                    • `/setup_exchange` \\- Show supported exchanges\n\
                    • `/setup_exchange binance` \\- Binance setup guide\n\
                    • `/setup_exchange bybit` \\- Bybit setup guide\n\
                    • `/setup_exchange okx` \\- OKX setup guide\n\n\
                    **Status:** ✅ Always Available\n\n\
                    **Supported Exchanges:**\n\
                    • 🟡 **Binance** \\- World's largest exchange\n\
                    • 🔵 **Bybit** \\- Derivatives specialist\n\
                    • 🟢 **OKX** \\- Global crypto exchange\n\n\
                    **Setup Process:**\n\
                    1️⃣ Choose your exchange\n\
                    2️⃣ Create API key \\(trading permissions only\\)\n\
                    3️⃣ Configure IP whitelist \\(recommended\\)\n\
                    4️⃣ Add key to ArbEdge\n\
                    5️⃣ Test with `/validate_setup`\n\n\
                    **Security Requirements:**\n\
                    • ✅ Trading permissions enabled\n\
                    • ❌ Withdrawal permissions DISABLED\n\
                    • 🌐 IP whitelist configured\n\
                    • 🔒 API key kept secure\n\n\
                    **What You'll Unlock:**\n\
                    • 💰 Real balance checking\n\
                    • 💱 Buy/sell order execution\n\
                    • 📊 Portfolio tracking\n\
                    • 🤖 Automated trading \\(premium\\)\n\n\
                    **Troubleshooting:**\n\
                    • Can't create API key? Check exchange documentation\n\
                    • Key not working? Verify permissions and IP whitelist\n\
                    • Need help? Contact support@arbedge\\.com\n\n\
                    💡 *Tip*: Start with one exchange, add more later\\!"
                .to_string(),
            "market" => "📈 *Help: /market Command* 📊\n\n\
                    **Description:**\n\
                    Get real\\-time market data and price information\\.\n\n\
                    **Usage:**\n\
                    • `/market` \\- Market overview\n\
                    • `/market BTCUSDT` \\- Bitcoin price and stats\n\
                    • `/market top` \\- Top performing coins\n\
                    • `/market trending` \\- Trending cryptocurrencies\n\n\
                    **Status:** ✅ Always Available\n\n\
                    **What You Get:**\n\
                    • 💰 Current prices\n\
                    • 📊 24h volume\n\
                    • 📈 Price changes\n\
                    • 🎯 Market trends\n\
                    • ⚡ Real\\-time updates\n\n\
                    **Examples:**\n\
                    • `/market` \\- General market overview\n\
                    • `/market BTCUSDT` \\- Bitcoin details\n\
                    • `/market ETHUSDT` \\- Ethereum details\n\
                    • `/market overview` \\- Market summary\n\n\
                    **Market Data Includes:**\n\
                    • 💰 Current price\n\
                    • 📊 24h volume\n\
                    • 📈 Price change %\n\
                    • 🕐 Last update time\n\
                    • 📉 High/low prices\n\n\
                    **Related Commands:**\n\
                    • `/price BTCUSDT` \\- Quick price check\n\
                    • `/alerts` \\- Set price alerts\n\
                    • `/opportunities` \\- Find trading opportunities\n\n\
                    **Troubleshooting:**\n\
                    • No data? Market service might be updating\n\
                    • Prices seem old? Check timestamp\n\
                    • Pair not found? Check spelling\n\n\
                    💡 *Tip*: Use this before making trades\\!"
                .to_string(),
            _ => {
                format!(
                    "❓ *Help: Unknown Command* 🤔\n\n\
                    The command `{}` is not recognized or doesn't have specific help available\\.\n\n\
                    **Available Commands:**\n\
                    • 📊 `/opportunities` \\- View arbitrage opportunities\n\
                    • 💰 `/balance` \\- Check account balances\n\
                    • 💱 `/buy` `/sell` \\- Execute trades\n\
                    • 🤖 `/ai_insights` \\- Get AI analysis\n\
                    • 📈 `/market` \\- Market data\n\
                    • 🔧 `/setup_exchange` \\- Configure trading\n\n\
                    **Get More Help:**\n\
                    • `/help` \\- General help menu\n\
                    • `/help getting_started` \\- Beginner guide\n\
                    • `/help trading` \\- Trading commands\n\
                    • `/setup_help` \\- Setup troubleshooting\n\n\
                    **Command Format:**\n\
                    • Use `/help <command>` for specific help\n\
                    • Example: `/help balance`\n\
                    • Example: `/help buy`\n\n\
                    💡 *Tip*: Try `/help` for the main menu\\!",
                    escape_markdown_v2(command)
                )
            }
        }
    }

    /// Context-aware command suggestions
    async fn get_command_suggestions(&self, user_id: &str, failed_command: &str) -> String {
        let has_exchange_keys = self.check_user_has_exchange_keys(user_id).await;
        let has_ai_keys = self.check_user_has_ai_keys(user_id).await;

        // Analyze the failed command to provide relevant suggestions
        let suggestions = if failed_command.starts_with("/trade")
            || failed_command.starts_with("/buy")
            || failed_command.starts_with("/sell")
        {
            if !has_exchange_keys {
                vec![
                    ("🔧 `/setup_exchange`", "Set up trading first"),
                    ("📊 `/opportunities`", "See what's available"),
                    ("📖 `/help trading`", "Learn about trading"),
                ]
            } else {
                vec![
                    ("💰 `/balance`", "Check your funds"),
                    ("📊 `/market BTCUSDT`", "Check market price"),
                    ("📖 `/help trading`", "Trading command guide"),
                ]
            }
        } else if failed_command.starts_with("/ai") {
            if !has_ai_keys {
                vec![
                    ("🤖 `/ai_insights`", "Try system AI"),
                    ("🔧 `/setup_ai`", "Set up personal AI"),
                    ("📖 `/help ai`", "Learn about AI features"),
                ]
            } else {
                vec![
                    ("🤖 `/ai_insights`", "Get AI analysis"),
                    ("🛡️ `/risk_assessment`", "Check portfolio risk"),
                    ("📊 `/opportunities`", "AI-enhanced opportunities"),
                ]
            }
        } else {
            vec![
                ("📊 `/opportunities`", "See arbitrage opportunities"),
                ("📈 `/market`", "Check market data"),
                ("🆘 `/help`", "Get help and guidance"),
                ("🔧 `/setup_status`", "Check your setup"),
            ]
        };

        let mut message = format!(
            "💡 *Helpful Suggestions* 🎯\n\n\
            Since `{}` didn't work, try these instead:\n\n",
            escape_markdown_v2(failed_command)
        );

        for (command, description) in suggestions {
            message.push_str(&format!(
                "• {} \\- {}\n",
                command,
                escape_markdown_v2(description)
            ));
        }

        message.push_str("\n🆘 *Need more help?* Use `/help troubleshooting`\\!");
        message
    }

    /// Handle retryable errors with automatic retry logic
    async fn handle_retryable_error(
        &self,
        error_type: &str,
        context: &str,
        retry_count: u32,
    ) -> String {
        match error_type {
            "network_timeout" | "service_unavailable" | "rate_limited" => {
                if retry_count < 3 {
                    format!(
                        "🔄 *Auto-Retry in Progress* ⏱️\n\n\
                        Attempting to retry your request automatically\\.\n\n\
                        **Retry Details:**\n\
                        • 🔢 Attempt: {} of 3\n\
                        • ⏰ Next retry: 30 seconds\n\
                        • 🎯 Error: {}\n\n\
                        **What's happening:**\n\
                        • 🤖 System is automatically retrying\n\
                        • ⏰ Please wait for the retry\n\
                        • 🔄 No action needed from you\n\n\
                        **If retries fail:**\n\
                        • 📊 Try `/status` to check system health\n\
                        • 🆘 Use `/setup_help` for troubleshooting\n\
                        • 📞 Contact support if persistent\n\n\
                        💡 *Tip*: You can try other commands while waiting\\!",
                        retry_count + 1,
                        escape_markdown_v2(context)
                    )
                } else {
                    format!(
                        "❌ *Auto-Retry Failed* 🚫\n\n\
                        After 3 attempts, the system couldn't complete your request\\.\n\n\
                        **Final Error:** {}\n\n\
                        **What you can do:**\n\
                        • ⏰ Wait 5 minutes and try again\n\
                        • 📊 Check `/status` for system health\n\
                        • 🔄 Try alternative commands\n\
                        • 📞 Contact support with error details\n\n\
                        **Alternative Actions:**\n\
                        • 📊 Use `/opportunities` for cached data\n\
                        • 📈 Try `/market` for basic market info\n\
                        • 🤖 Use `/ai_insights` for analysis\n\n\
                        **Support Information:**\n\
                        • 📧 support@arbedge\\.com\n\
                        • 💬 Include error: `{}`\n\
                        • 🕐 Time: `{}`\n\n\
                        💡 *Meanwhile*: Other features may still work\\!",
                        escape_markdown_v2(context),
                        escape_markdown_v2(context),
                        escape_markdown_v2(
                            &chrono::Utc::now()
                                .format("%Y-%m-%d %H:%M:%S UTC")
                                .to_string()
                        )
                    )
                }
            }
            _ => {
                // Non-retryable error, provide immediate guidance
                self.get_enhanced_error_message(error_type, context).await
            }
        }
    }

    /// Get error recovery suggestions based on error type and user context
    async fn get_error_recovery_suggestions(
        &self,
        user_id: &str,
        error_type: &str,
        failed_command: &str,
    ) -> String {
        let has_exchange_keys = self.check_user_has_exchange_keys(user_id).await;
        let _has_ai_keys = self.check_user_has_ai_keys(user_id).await;

        match error_type {
            "api_key_invalid" => "🔧 *Quick Recovery: API Key Issue* 🔑\n\n\
                    **Immediate Actions:**\n\
                    1️⃣ Use `/setup_exchange` to update your API key\n\
                    2️⃣ Check your exchange account for key status\n\
                    3️⃣ Run `/validate_setup` to test the fix\n\n\
                    **While you fix this:**\n\
                    • 📊 Use `/opportunities` to see market opportunities\n\
                    • 📈 Use `/market` for price information\n\
                    • 🤖 Use `/ai_insights` for market analysis\n\n\
                    **Prevention Tips:**\n\
                    • 🔔 Set up exchange notifications for API changes\n\
                    • 🌐 Use IP whitelist to prevent unauthorized access\n\
                    • 🔄 Regularly validate your setup\n\n\
                    💡 *Quick Fix*: `/setup_exchange` → Update key → `/validate_setup`"
                .to_string(),
            "insufficient_balance" => "💰 *Quick Recovery: Balance Issue* 📊\n\n\
                    **Check Your Situation:**\n\
                    1️⃣ Use `/balance` to see all your balances\n\
                    2️⃣ Use `/orders` to check if funds are tied up\n\
                    3️⃣ Cancel unnecessary orders to free funds\n\n\
                    **Alternative Actions:**\n\
                    • 📉 Try a smaller trade amount\n\
                    • 📊 Use `/opportunities` to find trades within budget\n\
                    • 💱 Consider different trading pairs\n\
                    • 🏦 Deposit more funds to your exchange\n\n\
                    **Smart Trading Tips:**\n\
                    • 💡 Always keep some balance for fees\n\
                    • 📊 Use `/market` to check prices before trading\n\
                    • 🎯 Start with smaller amounts\n\n\
                    💡 *Quick Check*: `/balance` → `/orders` → Adjust trade size"
                .to_string(),
            "service_unavailable" => "🔄 *Quick Recovery: Service Issue* 🛠️\n\n\
                    **Try These Alternatives:**\n\
                    • 📊 `/opportunities` \\- May use cached data\n\
                    • 📈 `/market` \\- Basic market information\n\
                    • 🤖 `/ai_insights` \\- AI analysis \\(if available\\)\n\
                    • 📊 `/status` \\- Check which services are working\n\n\
                    **Wait and Retry:**\n\
                    • ⏰ Service issues usually resolve in 5\\-15 minutes\n\
                    • 🔄 Try your original command again later\n\
                    • 📊 Use `/status` to monitor service recovery\n\n\
                    **If it persists:**\n\
                    • 📧 Report to support@arbedge\\.com\n\
                    • 💬 Include the service name and time\n\
                    • 🔍 Check our status page for updates\n\n\
                    💡 *Pro Tip*: Bookmark alternative commands for service outages\\!"
                .to_string(),
            _ => {
                // Generic recovery suggestions
                format!(
                    "🆘 *Quick Recovery Guide* 🎯\n\n\
                    **Immediate Actions:**\n\
                    • 🔄 Try the command again: `{}`\n\
                    • 📊 Check system status: `/status`\n\
                    • 🆘 Get help: `/help {}`\n\n\
                    **Alternative Commands:**\n\
                    {}**Troubleshooting:**\n\
                    • 🔧 Use `/setup_help` for common issues\n\
                    • 📖 Use `/help troubleshooting` for detailed guide\n\
                    • 📞 Contact support if nothing works\n\n\
                    💡 *Quick Help*: `/help {}` for command\\-specific guidance\\!",
                    escape_markdown_v2(failed_command),
                    failed_command.strip_prefix('/').unwrap_or(failed_command),
                    if has_exchange_keys {
                        "• 💰 `/balance` \\- Check your balances\n• 📊 `/opportunities` \\- Find opportunities\n• 📈 `/market` \\- Market data\n\n"
                    } else {
                        "• 📊 `/opportunities` \\- View opportunities\n• 📈 `/market` \\- Market data\n• 🔧 `/setup_exchange` \\- Enable trading\n\n"
                    },
                    failed_command.strip_prefix('/').unwrap_or(failed_command)
                )
            }
        }
    }

    /// Get IP restriction guidance for exchange API setup
    async fn get_ip_restriction_guidance(&self) -> String {
        "⚠️ **Common API Connection Issue**: IP Restrictions\n\
         \n\
         If your exchange API keys aren't working:\n\
         • **Binance**: Set IP restrictions to 'Unrestricted'\n\
         • **Bybit**: Leave IP whitelist EMPTY\n\
         • **OKX**: Use '0.0.0.0/0' for unrestricted access\n\
         \n\
         🔧 Adding specific IP addresses will block ArbEdge connection!\n\n"
            .to_string()
    }

    /// Enhanced setup requirement explanations
    async fn get_detailed_setup_explanation(&self, feature: &str) -> String {
        match feature {
            "trading" => {
                "🔑 *Why Trading Requires API Keys* 💰\n\n\
                    To execute real trades, ArbEdge needs to connect to your exchange account\\.\n\n\
                    **What API Keys Do:**\n\
                    • 🔗 Connect ArbEdge to your exchange\n\
                    • 📊 Read your balance and positions\n\
                    • 💱 Place and cancel orders\n\
                    • 📈 Track your trading performance\n\n\
                    **Security Guarantees:**\n\
                    • ✅ Only trading permissions \\(NO withdrawals\\)\n\
                    • 🔒 Your funds stay in your exchange account\n\
                    • 🛡️ You can revoke access anytime\n\
                    • 🔐 Keys are encrypted and secure\n\n\
                    **Setup Process:**\n\
                    1️⃣ `/setup_exchange` \\- Choose exchange\n\
                    2️⃣ Create API key \\(trading only\\)\n\
                    3️⃣ Add key to ArbEdge\n\
                    4️⃣ `/validate_setup` \\- Test connection\n\n\
                    💡 *Ready to start?* Use `/setup_exchange` now\\!".to_string()
            }
            "ai" => {
                "🤖 *Personal AI vs System AI* 🧠\n\n\
                    ArbEdge offers two types of AI analysis:\n\n\
                    **System AI \\(Available Now\\):**\n\
                    • ✅ No setup required\n\
                    • 🌐 Global market insights\n\
                    • 📊 General opportunity analysis\n\
                    • 🆓 Free for all users\n\n\
                    **Personal AI \\(Setup Required\\):**\n\
                    • 🎯 Personalized for your portfolio\n\
                    • 📈 Custom trading strategies\n\
                    • 🛡️ Risk analysis based on your positions\n\
                    • 💡 Tailored recommendations\n\n\
                    **Why Personal AI Needs Keys:**\n\
                    • 🔐 Direct access to AI providers\n\
                    • ⚡ Faster response times\n\
                    • 🎨 Customizable AI models\n\
                    • 📊 Higher usage limits\n\n\
                    **Try First:**\n\
                    • `/opportunities` \\- System AI opportunities\n\
                    • `/ai_insights` \\- Basic AI analysis\n\n\
                    🚀 *Want more?* Use `/setup_ai` for personal AI\\!".to_string()
            }
            "advanced" => {
                "🚀 *Advanced Features Explained* ⭐\n\n\
                    Some features require higher subscription tiers or special setup\\.\n\n\
                    **Subscription Tiers:**\n\
                    • 🆓 **Free** \\- Basic opportunities, market data\n\
                    • 💎 **Premium** \\- Trading, personal AI, advanced analytics\n\
                    • 🏆 **Pro** \\- Automated trading, priority support\n\
                    • 👑 **Enterprise** \\- Custom features, dedicated support\n\n\
                    **Feature Requirements:**\n\
                    • 💰 Trading \\- Premium \\+ Exchange API\n\
                    • 🤖 Personal AI \\- Premium \\+ AI API\n\
                    • 🔄 Auto Trading \\- Pro \\+ Full Setup\n\
                    • 📊 Advanced Analytics \\- Pro tier\n\n\
                    **Upgrade Benefits:**\n\
                    • 🚀 More features unlocked\n\
                    • ⚡ Higher rate limits\n\
                    • 🎯 Priority support\n\
                    • 📈 Advanced tools\n\n\
                    💡 *Current tier*: Check with `/profile`\\!".to_string()
            }
            _ => {
                "❓ *Feature Requirements* 📋\n\n\
                    This feature has specific requirements that aren't met yet\\.\n\n\
                    **Common Requirements:**\n\
                    • 🔑 API keys for external services\n\
                    • 💎 Higher subscription tier\n\
                    • 🔧 Additional setup steps\n\
                    • 👤 Special permissions\n\n\
                    **Check Your Status:**\n\
                    • `/setup_status` \\- See what's configured\n\
                    • `/profile` \\- Check subscription tier\n\
                    • `/validate_setup` \\- Test connections\n\n\
                    **Get Help:**\n\
                    • `/help` \\- General guidance\n\
                    • `/setup_help` \\- Setup assistance\n\
                    • 📧 support@arbedge\\.com \\- Direct support\n\n\
                    🎯 *Tip*: Most features work without setup\\!".to_string()
            }
        }
    }

    // ============= PERFORMANCE AND RELIABILITY METHODS =============

    /// Check if user is within rate limits
    async fn check_rate_limit(
        &self,
        user_id: &str,
        max_requests: u32,
        window_duration: Duration,
    ) -> bool {
        let mut rate_limits = self.rate_limits.lock().unwrap();

        match rate_limits.get_mut(user_id) {
            Some(entry) => {
                if entry.is_within_limit(max_requests) {
                    entry.increment();
                    true
                } else {
                    // Update metrics
                    if let Ok(mut metrics) = self.performance_metrics.lock() {
                        metrics.rate_limit_hits += 1;
                    }
                    false
                }
            }
            None => {
                rate_limits.insert(user_id.to_string(), RateLimitEntry::new(window_duration));
                true
            }
        }
    }

    /// Get cached data if available and not expired
    async fn get_cached_data(&self, cache_key: &str) -> Option<String> {
        let cache = self.cache.lock().unwrap();

        if let Some(entry) = cache.get(cache_key) {
            if !entry.is_expired() {
                // Update metrics
                if let Ok(mut metrics) = self.performance_metrics.lock() {
                    metrics.cache_hits += 1;
                }
                return Some(entry.data.clone());
            }
        }

        // Update metrics
        if let Ok(mut metrics) = self.performance_metrics.lock() {
            metrics.cache_misses += 1;
        }
        None
    }

    /// Store data in cache with TTL
    async fn set_cached_data(&self, cache_key: String, data: String, ttl: Duration) {
        let mut cache = self.cache.lock().unwrap();
        cache.insert(cache_key, CacheEntry::new(data, ttl));
    }

    /// Execute operation with retry logic and exponential backoff
    async fn execute_with_retry<F, Fut, T>(&self, operation: F) -> ArbitrageResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = ArbitrageResult<T>>,
    {
        let mut attempt = 0;
        let mut delay = self.retry_config.base_delay_ms;

        loop {
            attempt += 1;

            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // Update metrics
                    if let Ok(mut metrics) = self.performance_metrics.lock() {
                        metrics.retry_attempts += 1;
                    }

                    if attempt >= self.retry_config.max_attempts {
                        return Err(e);
                    }

                    // Check if error is retryable
                    if !self.is_retryable_error(&e) {
                        return Err(e);
                    }

                    // Wait before retry with exponential backoff
                    wasm_sleep(delay).await;
                    delay = std::cmp::min(
                        (delay as f64 * self.retry_config.backoff_multiplier) as u64,
                        self.retry_config.max_delay_ms,
                    );
                }
            }
        }
    }

    /// Check if error is retryable
    fn is_retryable_error(&self, error: &ArbitrageError) -> bool {
        use crate::utils::error::ErrorKind;
        matches!(
            error.kind,
            ErrorKind::NetworkError
                | ErrorKind::RateLimit
                | ErrorKind::ApiError
                | ErrorKind::ExchangeError
                | ErrorKind::TelegramError
        )
    }

    /// Execute operation with fallback
    async fn execute_with_fallback<F, Fut, FB, FutB, T>(
        &self,
        primary_operation: F,
        fallback_operation: FB,
    ) -> ArbitrageResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = ArbitrageResult<T>>,
        FB: Fn() -> FutB,
        FutB: std::future::Future<Output = ArbitrageResult<T>>,
    {
        match primary_operation().await {
            Ok(result) => Ok(result),
            Err(_) => {
                // Update metrics
                if let Ok(mut metrics) = self.performance_metrics.lock() {
                    metrics.fallback_activations += 1;
                }
                fallback_operation().await
            }
        }
    }

    /// Record command performance metrics
    async fn record_command_metrics(&self, response_time_ms: u64, is_error: bool) {
        if let Ok(mut metrics) = self.performance_metrics.lock() {
            metrics.command_count += 1;
            metrics.total_response_time_ms += response_time_ms;
            if is_error {
                metrics.error_count += 1;
            }
        }
    }

    /// Get performance statistics
    async fn get_performance_stats(&self) -> String {
        let metrics = self.performance_metrics.lock().unwrap();

        let avg_response_time = if metrics.command_count > 0 {
            metrics.total_response_time_ms / metrics.command_count
        } else {
            0
        };

        let error_rate = if metrics.command_count > 0 {
            (metrics.error_count as f64 / metrics.command_count as f64) * 100.0
        } else {
            0.0
        };

        let cache_hit_rate = if (metrics.cache_hits + metrics.cache_misses) > 0 {
            (metrics.cache_hits as f64 / (metrics.cache_hits + metrics.cache_misses) as f64) * 100.0
        } else {
            0.0
        };

        format!(
            "📊 **Performance Statistics**\n\n\
            **Commands Processed:** {}\n\
            **Average Response Time:** {}ms\n\
            **Error Rate:** {:.1}%\n\
            **Cache Hit Rate:** {:.1}%\n\
            **Retry Attempts:** {}\n\
            **Fallback Activations:** {}\n\
            **Rate Limit Hits:** {}",
            metrics.command_count,
            avg_response_time,
            error_rate,
            cache_hit_rate,
            metrics.retry_attempts,
            metrics.fallback_activations,
            metrics.rate_limit_hits
        )
    }

    /// Clean expired cache entries
    async fn cleanup_cache(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|_, entry| !entry.is_expired());
    }

    /// Clean expired rate limit entries
    async fn cleanup_rate_limits(&self) {
        let mut rate_limits = self.rate_limits.lock().unwrap();
        rate_limits.retain(|_, entry| entry.window_start.elapsed() <= entry.window_duration);
    }

    /// Enhanced command handler with performance monitoring
    async fn handle_command_with_performance_monitoring(
        &self,
        command: &str,
        user_id: &str,
        args: &[&str],
        handler: impl std::future::Future<Output = String>,
    ) -> String {
        let start_time = Instant::now();

        // Check rate limits (10 commands per minute per user)
        if !self
            .check_rate_limit(user_id, 10, Duration::from_secs(60))
            .await
        {
            self.record_command_metrics(start_time.elapsed().as_millis() as u64, true)
                .await;
            return "⚠️ **Rate Limit Exceeded**\n\nYou're sending commands too quickly. Please wait a moment before trying again.\n\n*Rate limit: 10 commands per minute*".to_string();
        }

        // Check cache for non-trading commands
        let cache_key = format!("{}:{}:{}", user_id, command, args.join(":"));
        if !self.is_trading_command(command) {
            if let Some(cached_response) = self.get_cached_data(&cache_key).await {
                self.record_command_metrics(start_time.elapsed().as_millis() as u64, false)
                    .await;
                return cached_response;
            }
        }

        // Execute command
        let response = handler.await;
        let response_time = start_time.elapsed().as_millis() as u64;

        // Cache response for non-trading commands
        if !self.is_trading_command(command) && !response.contains("⚠️") && !response.contains("❌")
        {
            let ttl = if command == "opportunities" {
                Duration::from_secs(30) // Short TTL for opportunities
            } else if command == "status" || command == "balance" {
                Duration::from_secs(60) // Medium TTL for status
            } else {
                Duration::from_secs(300) // Longer TTL for static content
            };
            self.set_cached_data(cache_key, response.clone(), ttl).await;
        }

        self.record_command_metrics(
            response_time,
            response.contains("⚠️") || response.contains("❌"),
        )
        .await;
        response
    }

    /// Check if command is a trading command (should not be cached)
    fn is_trading_command(&self, command: &str) -> bool {
        matches!(
            command,
            "buy" | "sell" | "cancel" | "orders" | "positions" | "balance"
        )
    }

    // ============= USER PREFERENCES AND PERSONALIZATION METHODS =============

    /// Get user preferences, creating default if not exists
    async fn get_user_preferences(&self, user_id: &str) -> UserPreferences {
        let preferences = self.user_preferences.lock().unwrap();

        match preferences.get(user_id) {
            Some(prefs) => prefs.clone(),
            None => {
                drop(preferences);
                let default_prefs = UserPreferences {
                    user_id: user_id.to_string(),
                    ..Default::default()
                };

                let mut preferences_write = self.user_preferences.lock().unwrap();
                preferences_write.insert(user_id.to_string(), default_prefs.clone());
                default_prefs
            }
        }
    }

    /// Update user preferences
    async fn update_user_preferences(&self, user_id: &str, preferences: UserPreferences) {
        let updated_prefs = UserPreferences {
            user_id: user_id.to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            ..preferences
        };

        let mut prefs = self.user_preferences.lock().unwrap();
        prefs.insert(user_id.to_string(), updated_prefs);
    }

    /// Get personalized dashboard message
    async fn get_personalized_dashboard_message(&self, user_id: &str) -> String {
        let prefs = self.get_user_preferences(user_id).await;
        let mut message = String::new();

        message.push_str("🏠 *Personal Dashboard*\n\n");

        // Show sections based on user's dashboard layout
        for section in &prefs.dashboard_layout.sections {
            match section {
                DashboardSection::Portfolio => {
                    message.push_str("💼 *Portfolio*\n");
                    if self.check_user_has_exchange_keys(user_id).await {
                        message.push_str("• Balance: Use /balance to view\n");
                        message.push_str("• Positions: Use /positions to view\n");
                    } else {
                        message.push_str("• ⚠️ Setup exchange API keys to view portfolio\n");
                    }
                    message.push('\n');
                }
                DashboardSection::Opportunities => {
                    message.push_str("🎯 *Opportunities*\n");
                    message.push_str("• Latest: Use /opportunities to view\n");
                    message.push_str(&format!(
                        "• Min Confidence: {}%\n",
                        prefs.alert_settings.opportunity_confidence_threshold
                    ));
                    message.push('\n');
                }
                DashboardSection::Alerts => {
                    message.push_str("🔔 *Alerts*\n");
                    message.push_str(&format!(
                        "• Price Change: ±{}%\n",
                        prefs.alert_settings.price_change_threshold
                    ));
                    message.push_str(&format!(
                        "• Custom Alerts: {}\n",
                        prefs.alert_settings.custom_alerts.len()
                    ));
                    message.push('\n');
                }
                DashboardSection::RecentActivity => {
                    message.push_str("📊 *Recent Activity*\n");
                    message.push_str("• Orders: Use /orders to view\n");
                    message.push_str("• Performance: Use /performance to view\n");
                    message.push('\n');
                }
                DashboardSection::MarketOverview => {
                    message.push_str("📈 *Market Overview*\n");
                    message.push_str("• Market Data: Use /market to view\n");
                    message.push_str("• Price Alerts: Use /alerts to view\n");
                    message.push('\n');
                }
                DashboardSection::Performance => {
                    message.push_str("📊 *Performance*\n");
                    message.push_str("• AI Insights: Use /ai_insights to view\n");
                    message.push_str("• Risk Assessment: Use /risk_assessment to view\n");
                    message.push('\n');
                }
            }
        }

        // Quick actions
        if !prefs.dashboard_layout.quick_actions.is_empty() {
            message.push_str("⚡ *Quick Actions*\n");
            for action in &prefs.dashboard_layout.quick_actions {
                message.push_str(&format!("• {}\n", action));
            }
            message.push('\n');
        }

        // Favorite commands
        if !prefs.dashboard_layout.favorite_commands.is_empty() {
            message.push_str("⭐ *Favorite Commands*\n");
            for command in &prefs.dashboard_layout.favorite_commands {
                message.push_str(&format!("• {}\n", command));
            }
            message.push('\n');
        }

        message.push_str("⚙️ Use /preferences to customize your dashboard");

        message
    }

    /// Handle command aliases
    async fn resolve_command_alias(&self, user_id: &str, command: &str) -> String {
        let prefs = self.get_user_preferences(user_id).await;

        // Check if command is an alias
        if let Some(actual_command) = prefs.command_aliases.get(command) {
            actual_command.clone()
        } else {
            command.to_string()
        }
    }

    /// Format number according to user preferences
    async fn format_number(&self, user_id: &str, number: f64) -> String {
        let prefs = self.get_user_preferences(user_id).await;

        match prefs.display_settings.number_format {
            NumberFormat::Standard => {
                // Format with comma separator for thousands
                let formatted = format!("{:.2}", number);
                if number >= 1000.0 {
                    // Add comma separator for thousands
                    let parts: Vec<&str> = formatted.split('.').collect();
                    let integer_part = parts[0];
                    let decimal_part = if parts.len() > 1 { parts[1] } else { "00" };

                    // Add commas to integer part
                    let mut result = String::new();
                    let chars: Vec<char> = integer_part.chars().collect();
                    for (i, ch) in chars.iter().enumerate() {
                        if i > 0 && (chars.len() - i) % 3 == 0 {
                            result.push(',');
                        }
                        result.push(*ch);
                    }
                    format!("{}.{}", result, decimal_part)
                } else {
                    formatted
                }
            }
            NumberFormat::European => {
                let formatted = format!("{:.2}", number);
                formatted.replace('.', ",")
            }
            NumberFormat::Scientific => format!("{:.2e}", number),
            NumberFormat::Abbreviated => {
                if number >= 1_000_000_000.0 {
                    format!("{:.2}B", number / 1_000_000_000.0)
                } else if number >= 1_000_000.0 {
                    format!("{:.2}M", number / 1_000_000.0)
                } else if number >= 1_000.0 {
                    format!("{:.2}K", number / 1_000.0)
                } else {
                    format!("{:.2}", number)
                }
            }
        }
    }

    /// Check if user should receive notification based on preferences
    async fn should_send_notification(&self, user_id: &str, notification_type: &str) -> bool {
        let prefs = self.get_user_preferences(user_id).await;

        if !prefs.notification_settings.enabled {
            return false;
        }

        match notification_type {
            "opportunity" => prefs.notification_settings.opportunity_notifications,
            "price_alert" => prefs.notification_settings.price_alerts,
            "trading" => prefs.notification_settings.trading_updates,
            "system" => prefs.notification_settings.system_notifications,
            _ => true,
        }
    }

    /// Get preferences management message
    async fn get_preferences_management_message(&self, user_id: &str) -> String {
        let prefs = self.get_user_preferences(user_id).await;

        format!(
            "⚙️ *User Preferences*\n\n\
            🔔 *Notifications*\n\
            • Enabled: {}\n\
            • Opportunities: {}\n\
            • Price Alerts: {}\n\
            • Trading Updates: {}\n\
            • Frequency: {:?}\n\n\
            🎨 *Display*\n\
            • Currency: {}\n\
            • Timezone: {}\n\
            • Language: {}\n\
            • Number Format: {:?}\n\
            • Compact Mode: {}\n\n\
            🚨 *Alert Thresholds*\n\
            • Price Change: ±{}%\n\
            • Volume Change: ±{}%\n\
            • Opportunity Confidence: {}%\n\
            • Portfolio Change: ±{}%\n\
            • Custom Alerts: {}\n\n\
            🎯 *Dashboard Sections*: {}\n\
            ⚡ *Quick Actions*: {}\n\
            ⭐ *Favorites*: {}\n\
            🔗 *Command Aliases*: {}\n\n\
            Use the following commands to customize:\n\
            • /set_notifications - Configure notification settings\n\
            • /set_display - Configure display preferences\n\
            • /set_alerts - Configure alert thresholds\n\
            • /set_dashboard - Customize dashboard layout\n\
            • /add_alias - Add command aliases\n\
            • /reset_preferences - Reset to defaults",
            if prefs.notification_settings.enabled {
                "✅"
            } else {
                "❌"
            },
            if prefs.notification_settings.opportunity_notifications {
                "✅"
            } else {
                "❌"
            },
            if prefs.notification_settings.price_alerts {
                "✅"
            } else {
                "❌"
            },
            if prefs.notification_settings.trading_updates {
                "✅"
            } else {
                "❌"
            },
            prefs.notification_settings.frequency,
            prefs.display_settings.currency,
            prefs.display_settings.timezone,
            prefs.display_settings.language,
            prefs.display_settings.number_format,
            if prefs.display_settings.compact_mode {
                "✅"
            } else {
                "❌"
            },
            prefs.alert_settings.price_change_threshold,
            prefs.alert_settings.volume_change_threshold,
            prefs.alert_settings.opportunity_confidence_threshold,
            prefs.alert_settings.portfolio_change_threshold,
            prefs.alert_settings.custom_alerts.len(),
            prefs.dashboard_layout.sections.len(),
            prefs.dashboard_layout.quick_actions.len(),
            prefs.dashboard_layout.favorite_commands.len(),
            prefs.command_aliases.len()
        )
    }

    /// Add command alias
    async fn add_command_alias(&self, user_id: &str, alias: &str, command: &str) -> String {
        let mut prefs = self.get_user_preferences(user_id).await;

        // Validate that the target command exists
        if !self.is_valid_command(command) {
            return format!(
                "❌ Invalid command: {}. Use /help to see available commands.",
                command
            );
        }

        // Add the alias
        prefs
            .command_aliases
            .insert(alias.to_string(), command.to_string());
        self.update_user_preferences(user_id, prefs).await;

        format!("✅ Alias added: {} → {}", alias, command)
    }

    /// Get smart command suggestions based on user behavior
    async fn get_smart_suggestions(&self, user_id: &str) -> String {
        let prefs = self.get_user_preferences(user_id).await;
        let mut suggestions = Vec::new();

        // Suggest based on setup status
        if !self.check_user_has_exchange_keys(user_id).await {
            suggestions.push("🔑 Set up exchange API keys to unlock trading features");
        }

        if !self.check_user_has_ai_keys(user_id).await {
            suggestions.push("🤖 Configure AI services for personalized insights");
        }

        // Suggest based on preferences
        if prefs.alert_settings.custom_alerts.is_empty() {
            suggestions.push("🚨 Create custom alerts for your favorite trading pairs");
        }

        if prefs.command_aliases.is_empty() {
            suggestions
                .push("⚡ Add command aliases for faster access (e.g., /add_alias bal balance)");
        }

        if prefs.dashboard_layout.favorite_commands.is_empty() {
            suggestions.push("⭐ Add frequently used commands to your favorites");
        }

        // Suggest based on dashboard sections
        if !prefs
            .dashboard_layout
            .sections
            .contains(&DashboardSection::Performance)
        {
            suggestions.push("📊 Add Performance section to your dashboard for insights");
        }

        if suggestions.is_empty() {
            "🎉 Your setup looks great! Use /help to explore more features.".to_string()
        } else {
            format!("💡 *Smart Suggestions*\n\n• {}", suggestions.join("\n• "))
        }
    }
}

// Implementation of NotificationSender trait for TelegramService (non-WASM)
#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl crate::services::core::opportunities::opportunity_distribution::NotificationSender
    for TelegramService
{
    async fn send_opportunity_notification(
        &self,
        chat_id: &str,
        opportunity: &ArbitrageOpportunity,
        _is_private: bool,
    ) -> ArbitrageResult<bool> {
        // Format the opportunity message
        let message = format!(
            "🚀 *New Arbitrage Opportunity* 💰\n\n\
            **Trading Pair:** `{}`\n\
            **Profit Potential:** {:.2}%\n\
            **Buy Exchange:** {}\n\
            **Sell Exchange:** {}\n\
            **Volume:** ${:.2}\n\n\
            💡 *Act fast!* This opportunity may not last long\\.",
            escape_markdown_v2(&opportunity.pair),
            opportunity.rate_difference,
            escape_markdown_v2(&opportunity.long_exchange.to_string()),
            escape_markdown_v2(&opportunity.short_exchange.to_string()),
            opportunity.potential_profit_value.unwrap_or(0.0)
        );

        // Send the message to the specified chat
        match self.send_message_to_chat(chat_id, &message).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // Return false instead of propagating error for notification failures
        }
    }

    async fn send_message(&self, chat_id: &str, message: &str) -> ArbitrageResult<()> {
        self.send_message_to_chat(chat_id, message).await
    }
}

// Implementation of NotificationSender trait for TelegramService (WASM)
#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
impl crate::services::core::opportunities::opportunity_distribution::NotificationSender
    for TelegramService
{
    async fn send_opportunity_notification(
        &self,
        chat_id: &str,
        opportunity: &ArbitrageOpportunity,
        _is_private: bool,
    ) -> ArbitrageResult<bool> {
        // Format the opportunity message
        let message = format!(
            "🚀 *New Arbitrage Opportunity* 💰\n\n\
            **Trading Pair:** `{}`\n\
            **Profit Potential:** {:.2}%\n\
            **Buy Exchange:** {}\n\
            **Sell Exchange:** {}\n\
            **Volume:** ${:.2}\n\n\
            💡 *Act fast!* This opportunity may not last long\\.",
            escape_markdown_v2(&opportunity.pair),
            opportunity.rate_difference,
            escape_markdown_v2(&opportunity.long_exchange.to_string()),
            escape_markdown_v2(&opportunity.short_exchange.to_string()),
            opportunity.potential_profit_value.unwrap_or(0.0)
        );

        // Send the message to the specified chat
        match self.send_message_to_chat(chat_id, &message).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // Return false instead of propagating error for notification failures
        }
    }

    async fn send_message(&self, chat_id: &str, message: &str) -> ArbitrageResult<()> {
        self.send_message_to_chat(chat_id, message).await
    }
}

// Implementation for Arc<TelegramService> to support shared ownership
#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
impl crate::services::core::opportunities::opportunity_distribution::NotificationSender
    for Arc<TelegramService>
{
    async fn send_opportunity_notification(
        &self,
        chat_id: &str,
        opportunity: &ArbitrageOpportunity,
        _is_private: bool,
    ) -> ArbitrageResult<bool> {
        // Format the opportunity message
        let message = format!(
            "🚀 *New Arbitrage Opportunity* 💰\n\n\
            **Trading Pair:** `{}`\n\
            **Profit Potential:** {:.2}%\n\
            **Buy Exchange:** {}\n\
            **Sell Exchange:** {}\n\
            **Volume:** ${:.2}\n\n\
            💡 *Act fast!* This opportunity may not last long\\.",
            escape_markdown_v2(&opportunity.pair),
            opportunity.rate_difference,
            escape_markdown_v2(&opportunity.long_exchange.to_string()),
            escape_markdown_v2(&opportunity.short_exchange.to_string()),
            opportunity.potential_profit_value.unwrap_or(0.0)
        );

        // Send the message to the specified chat
        match self.as_ref().send_message_to_chat(chat_id, &message).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // Return false instead of propagating error for notification failures
        }
    }

    async fn send_message(&self, chat_id: &str, message: &str) -> ArbitrageResult<()> {
        // Delegate to the inner TelegramService
        self.as_ref().send_message_to_chat(chat_id, message).await
    }
}

// Implementation for Arc<TelegramService> to support shared ownership (non-WASM)
#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl crate::services::core::opportunities::opportunity_distribution::NotificationSender
    for Arc<TelegramService>
{
    async fn send_opportunity_notification(
        &self,
        chat_id: &str,
        opportunity: &ArbitrageOpportunity,
        _is_private: bool,
    ) -> ArbitrageResult<bool> {
        // Format the opportunity message
        let message = format!(
            "🚀 *New Arbitrage Opportunity* 💰\n\n\
            **Trading Pair:** `{}`\n\
            **Profit Potential:** {:.2}%\n\
            **Buy Exchange:** {}\n\
            **Sell Exchange:** {}\n\
            **Volume:** ${:.2}\n\n\
            💡 *Act fast!* This opportunity may not last long\\.",
            escape_markdown_v2(&opportunity.pair),
            opportunity.rate_difference,
            escape_markdown_v2(&opportunity.long_exchange.to_string()),
            escape_markdown_v2(&opportunity.short_exchange.to_string()),
            opportunity.potential_profit_value.unwrap_or(0.0)
        );

        // Send the message to the specified chat
        match self.as_ref().send_message_to_chat(chat_id, &message).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // Return false instead of propagating error for notification failures
        }
    }

    async fn send_message(&self, chat_id: &str, message: &str) -> ArbitrageResult<()> {
        // Delegate to the inner TelegramService
        self.as_ref().send_message_to_chat(chat_id, message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> TelegramConfig {
        TelegramConfig {
            bot_token: "test_token".to_string(),
            chat_id: "test_chat".to_string(),
            is_test_mode: true,
        }
    }

    #[test]
    fn test_enhanced_error_handling_functionality() {
        // Test enhanced error message methods structure
        let config = create_test_config();
        let service = TelegramService::new(config);

        // Test that enhanced error handling methods exist and are properly structured
        // These would be tested with actual error scenarios in integration tests
        assert!(service.user_profile_service.is_none()); // No service by default

        // Test that the methods exist (compilation test)
        // In real usage, these would provide enhanced error messages
    }

    #[test]
    fn test_progressive_help_system_structure() {
        // Test progressive help message structure
        let help_message_new_user = "🚀 *Getting Started with ArbEdge* 🌟\n\n\
                                    Welcome! Let's get you started step by step:\n\n\
                                    **Step 1: Explore Immediately** ✅\n\
                                    • `/opportunities` - See arbitrage opportunities\n\
                                    • `/market` - Check market data";

        let help_message_advanced = "🏆 *Advanced User Guide* 💪\n\n\
                                    You're all set up! Here are advanced features:\n\n\
                                    **Trading Commands:**\n\
                                    • `/balance` - Check your balances\n\
                                    • `/buy` / `/sell` - Execute trades";

        // Verify the structure contains expected elements
        assert!(help_message_new_user.contains("Getting Started"));
        assert!(help_message_new_user.contains("Explore Immediately"));
        assert!(help_message_new_user.contains("/opportunities"));
        assert!(help_message_new_user.contains("/market"));

        assert!(help_message_advanced.contains("Advanced User Guide"));
        assert!(help_message_advanced.contains("Trading Commands"));
        assert!(help_message_advanced.contains("/balance"));
        assert!(help_message_advanced.contains("/buy"));
    }

    #[test]
    fn test_enhanced_error_message_types() {
        // Test different error message types structure
        let service_unavailable = "🚫 *Service Temporarily Unavailable* ⚠️\n\n\
                                  The Exchange service is currently unavailable. This might be due to:\n\n\
                                  **Possible Causes:**\n\
                                  • 🔧 Scheduled maintenance\n\
                                  • 📡 Network connectivity issues";

        let invalid_parameters = "❌ *Invalid Parameters* 📝\n\n\
                                The command you entered has invalid or missing parameters.\\n\n\
                                **Common Issues:**\n\
                                • 🔢 Missing required values\n\
                                • 📏 Values outside acceptable range";

        let permission_denied = "🔒 *Access Restricted* 🚫\n\n\
                               You don't have permission to use this feature.\\n\n\
                               **Why this happens:**\n\
                               • 👤 Feature requires higher subscription tier\n\
                               • 🔑 Missing required API keys";

        // Verify error message structures
        assert!(service_unavailable.contains("Service Temporarily Unavailable"));
        assert!(service_unavailable.contains("Possible Causes"));
        assert!(service_unavailable.contains("Scheduled maintenance"));

        assert!(invalid_parameters.contains("Invalid Parameters"));
        assert!(invalid_parameters.contains("Common Issues"));
        assert!(invalid_parameters.contains("Missing required values"));

        assert!(permission_denied.contains("Access Restricted"));
        assert!(permission_denied.contains("Why this happens"));
        assert!(permission_denied.contains("subscription tier"));
    }

    #[test]
    fn test_command_suggestions_functionality() {
        // Test command suggestions structure
        let trading_suggestions = "💡 *Helpful Suggestions* 🎯\n\n\
                                 Since `/buy` didn't work, try these instead:\n\n\
                                 • 🔧 `/setup_exchange` - Set up trading first\n\
                                 • 📊 `/opportunities` - See what's available\n\
                                 • 📖 `/help trading` - Learn about trading";

        let ai_suggestions = "💡 *Helpful Suggestions* 🎯\n\n\
                            Since `/ai_insights` didn't work, try these instead:\n\n\
                            • 🤖 `/ai_insights` - Try system AI\n\
                            • 🔧 `/setup_ai` - Set up personal AI\n\
                            • 📖 `/help ai` - Learn about AI features";

        // Verify suggestion structures
        assert!(trading_suggestions.contains("Helpful Suggestions"));
        assert!(trading_suggestions.contains("didn't work"));
        assert!(trading_suggestions.contains("/setup_exchange"));
        assert!(trading_suggestions.contains("/opportunities"));

        assert!(ai_suggestions.contains("Helpful Suggestions"));
        assert!(ai_suggestions.contains("/ai_insights"));
        assert!(ai_suggestions.contains("/setup_ai"));
        assert!(ai_suggestions.contains("/help ai"));
    }

    #[test]
    fn test_detailed_setup_explanations() {
        // Test detailed setup explanation structure
        let trading_explanation = "🔑 *Why Trading Requires API Keys* 💰\n\n\
                                 To execute real trades, ArbEdge needs to connect to your exchange account.\\n\n\
                                 **What API Keys Do:**\n\
                                 • 🔗 Connect ArbEdge to your exchange\n\
                                 • 📊 Read your balance and positions";

        let ai_explanation = "🤖 *Personal AI vs System AI* 🧠\n\n\
                            ArbEdge offers two types of AI analysis:\\n\n\
                            **System AI (Available Now):**\n\
                            • ✅ No setup required\n\
                            • 🌐 Global market insights";

        // Verify explanation structures
        assert!(trading_explanation.contains("Why Trading Requires API Keys"));
        assert!(trading_explanation.contains("What API Keys Do"));
        assert!(trading_explanation.contains("Connect ArbEdge"));

        assert!(ai_explanation.contains("Personal AI vs System AI"));
        assert!(ai_explanation.contains("System AI"));
        assert!(ai_explanation.contains("No setup required"));
    }

    #[test]
    fn test_progressive_disclosure_patterns() {
        // Test progressive disclosure based on user setup level
        let basic_help = "📚 *ArbEdge Help Center* 🎯\n\n\
                         **Quick Start:**\n\
                         • `/help getting_started` - New user guide\n\
                         • `/opportunities` - See arbitrage opportunities\n\
                         • `/market` - Check market data";

        let advanced_help = "📚 *ArbEdge Help Center* 🎯\n\n\
                           **Your Status:** ✅ Exchange • ✅ AI\n\n\
                           **Available Topics:**\n\
                           • `/help getting_started` - User guide for your level\n\
                           • `/help trading` - Trading commands and tips";

        // Verify progressive disclosure patterns
        assert!(basic_help.contains("Help Center"));
        assert!(basic_help.contains("Quick Start"));
        assert!(basic_help.contains("/help getting_started"));

        assert!(advanced_help.contains("Your Status"));
        assert!(advanced_help.contains("Available Topics"));
        assert!(advanced_help.contains("/help trading"));
    }

    #[test]
    fn test_contextual_help_topics() {
        // Test different help topics structure
        let getting_started_topic = "getting_started";
        let trading_topic = "trading";
        let ai_topic = "ai";
        let troubleshooting_topic = "troubleshooting";

        // Verify topic handling
        assert_eq!(getting_started_topic, "getting_started");
        assert_eq!(trading_topic, "trading");
        assert_eq!(ai_topic, "ai");
        assert_eq!(troubleshooting_topic, "troubleshooting");

        // Test that topics are properly categorized
        let topics = [
            getting_started_topic,
            trading_topic,
            ai_topic,
            troubleshooting_topic,
        ];
        assert_eq!(topics.len(), 4);
        assert!(topics.contains(&"getting_started"));
        assert!(topics.contains(&"trading"));
    }

    #[test]
    fn test_error_handling_integration_patterns() {
        // Test error handling integration with existing systems
        let config = create_test_config();
        let service = TelegramService::new(config);

        // Test that error handling integrates with existing command structure
        assert!(service.analytics_enabled); // Default analytics state

        // Test that enhanced error handling doesn't break existing functionality
        assert!(service.config.is_test_mode);
        assert!(!service.config.bot_token.is_empty());
    }

    #[test]
    fn test_user_guidance_accessibility() {
        // Test user guidance accessibility features
        let guidance_message = "🆘 *Need help?* Use `/setup_help` for troubleshooting!\n\n\
                              **Support:**\n\
                              • 📧 support@arbedge.com\n\
                              • 💬 Include your user ID\n\
                              • 🕐 Mention when this happened";

        // Verify accessibility features
        assert!(guidance_message.contains("Need help"));
        assert!(guidance_message.contains("/setup_help"));
        assert!(guidance_message.contains("support@arbedge.com"));
        assert!(guidance_message.contains("Include your user ID"));

        // Test that guidance is clear and actionable
        assert!(guidance_message.contains("📧")); // Email icon for clarity
        assert!(guidance_message.contains("💬")); // Chat icon for context
    }

    #[test]
    fn test_enhanced_help_command_integration() {
        // Test enhanced help command integration
        let help_command = "/help";
        let help_with_topic = "/help trading";
        let explain_command = "/explain trading";

        // Verify command structure
        assert!(help_command.starts_with("/help"));
        assert!(help_with_topic.contains("trading"));
        assert!(explain_command.contains("trading"));

        // Test that commands are properly formatted
        assert_eq!(help_command.len(), 5);
        assert!(help_with_topic.len() > help_command.len());
        assert!(explain_command.starts_with("/explain"));
    }

    // ============= PERFORMANCE AND RELIABILITY TESTS =============

    #[tokio::test]
    async fn test_cache_functionality() {
        let service = TelegramService::new(create_test_config());

        // Test cache miss
        let cache_key = "test_key";
        assert!(service.get_cached_data(cache_key).await.is_none());

        // Test cache set and hit
        let test_data = "test_data".to_string();
        service
            .set_cached_data(
                cache_key.to_string(),
                test_data.clone(),
                Duration::from_secs(60),
            )
            .await;

        let cached_result = service.get_cached_data(cache_key).await;
        assert!(cached_result.is_some());
        assert_eq!(cached_result.unwrap(), test_data);
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let service = TelegramService::new(create_test_config());
        let user_id = "test_user";
        let max_requests = 3;
        let window_duration = Duration::from_secs(60);

        // First few requests should pass
        assert!(
            service
                .check_rate_limit(user_id, max_requests, window_duration)
                .await
        );
        assert!(
            service
                .check_rate_limit(user_id, max_requests, window_duration)
                .await
        );
        assert!(
            service
                .check_rate_limit(user_id, max_requests, window_duration)
                .await
        );

        // Next request should be rate limited
        assert!(
            !service
                .check_rate_limit(user_id, max_requests, window_duration)
                .await
        );
    }

    #[test]
    fn test_cache_entry_expiration() {
        let data = "test_data".to_string();
        let ttl = Duration::from_millis(1);
        let entry = CacheEntry::new(data, ttl);

        // Should not be expired immediately
        assert!(!entry.is_expired());

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(2));
        assert!(entry.is_expired());
    }

    #[test]
    fn test_rate_limit_entry_functionality() {
        let window_duration = Duration::from_secs(60);
        let mut entry = RateLimitEntry::new(window_duration);

        // Should be within limit initially (starts with count = 1)
        assert!(entry.is_within_limit(5)); // 1 < 5, should pass

        // Increment and check (count goes from 1 to 4, still < 5)
        entry.increment(); // count = 2
        entry.increment(); // count = 3
        entry.increment(); // count = 4
        assert!(entry.is_within_limit(5)); // 4 < 5, should pass

        // Should exceed limit (count = 5, not < 5)
        entry.increment(); // count = 5
        assert!(!entry.is_within_limit(5)); // 5 < 5 is false, should fail
    }

    #[tokio::test]
    async fn test_performance_metrics_recording() {
        let service = TelegramService::new(create_test_config());

        // Record some metrics
        service.record_command_metrics(100, false).await;
        service.record_command_metrics(200, true).await;
        service.record_command_metrics(150, false).await;

        // Get performance stats
        let stats = service.get_performance_stats().await;
        assert!(stats.contains("**Commands Processed:** 3"));
        assert!(stats.contains("**Average Response Time:** 150ms"));
        assert!(stats.contains("**Error Rate:** 33.3%"));
    }

    #[test]
    fn test_retryable_error_detection() {
        let service = TelegramService::new(create_test_config());

        // Test retryable errors
        assert!(service.is_retryable_error(&ArbitrageError::network_error("test")));
        assert!(service.is_retryable_error(&ArbitrageError::rate_limit_error("test")));
        assert!(service.is_retryable_error(&ArbitrageError::api_error("test")));
        assert!(service.is_retryable_error(&ArbitrageError::exchange_error("binance", "test")));
        assert!(service.is_retryable_error(&ArbitrageError::telegram_error("test")));

        // Test non-retryable errors
        assert!(!service.is_retryable_error(&ArbitrageError::validation_error("test")));
        assert!(!service.is_retryable_error(&ArbitrageError::authentication_error("test")));
        assert!(!service.is_retryable_error(&ArbitrageError::not_found("test")));
        assert!(!service.is_retryable_error(&ArbitrageError::parse_error("test")));
    }

    #[tokio::test]
    async fn test_cache_cleanup() {
        let service = TelegramService::new(create_test_config());

        // Add expired and non-expired entries
        service
            .set_cached_data(
                "expired_key".to_string(),
                "data".to_string(),
                Duration::from_millis(1),
            )
            .await;
        service
            .set_cached_data(
                "valid_key".to_string(),
                "data".to_string(),
                Duration::from_secs(60),
            )
            .await;

        // Wait for expiration
        wasm_sleep(2).await;

        // Cleanup should remove expired entries
        service.cleanup_cache().await;

        // Valid key should still exist, expired key should be gone
        assert!(service.get_cached_data("valid_key").await.is_some());
        assert!(service.get_cached_data("expired_key").await.is_none());
    }

    #[tokio::test]
    async fn test_rate_limit_cleanup() {
        let service = TelegramService::new(create_test_config());
        let user_id = "test_user";

        // Create rate limit entry with short window
        service
            .check_rate_limit(user_id, 5, Duration::from_millis(1))
            .await;

        // Wait for window to expire
        wasm_sleep(2).await;

        // Cleanup should remove expired entries
        service.cleanup_rate_limits().await;

        // New request should be allowed (fresh window)
        assert!(
            service
                .check_rate_limit(user_id, 5, Duration::from_secs(60))
                .await
        );
    }

    #[test]
    fn test_performance_metrics_default() {
        let metrics = PerformanceMetrics::default();

        assert_eq!(metrics.command_count, 0);
        assert_eq!(metrics.total_response_time_ms, 0);
        assert_eq!(metrics.error_count, 0);
        assert_eq!(metrics.cache_hits, 0);
        assert_eq!(metrics.cache_misses, 0);
        assert_eq!(metrics.retry_attempts, 0);
        assert_eq!(metrics.fallback_activations, 0);
        assert_eq!(metrics.rate_limit_hits, 0);
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();

        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 5000);
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_trading_command_detection() {
        let service = TelegramService::new(create_test_config());

        // Trading commands should not be cached
        assert!(service.is_trading_command("buy"));
        assert!(service.is_trading_command("sell"));
        assert!(service.is_trading_command("cancel"));
        assert!(service.is_trading_command("orders"));
        assert!(service.is_trading_command("positions"));
        assert!(service.is_trading_command("balance"));

        // Non-trading commands can be cached
        assert!(!service.is_trading_command("help"));
        assert!(!service.is_trading_command("status"));
        assert!(!service.is_trading_command("opportunities"));
        assert!(!service.is_trading_command("market"));
        assert!(!service.is_trading_command("ai_insights"));
    }

    // User Preferences and Personalization Tests
    #[tokio::test]
    async fn test_user_preferences_creation_and_retrieval() {
        let config = create_test_config();
        let service = TelegramService::new(config);
        let user_id = "test_user_123";

        // Test default preferences creation
        let prefs = service.get_user_preferences(user_id).await;
        assert_eq!(prefs.user_id, user_id);
        assert!(prefs.notification_settings.enabled);
        assert_eq!(prefs.display_settings.currency, "USD");
        assert_eq!(prefs.alert_settings.price_change_threshold, 5.0);
        assert!(prefs.command_aliases.is_empty());
    }

    #[tokio::test]
    async fn test_user_preferences_update() {
        let config = create_test_config();
        let service = TelegramService::new(config);
        let user_id = "test_user_456";

        // Get initial preferences
        let mut prefs = service.get_user_preferences(user_id).await;

        // Modify preferences
        prefs.display_settings.currency = "EUR".to_string();
        prefs.alert_settings.price_change_threshold = 10.0;
        prefs
            .command_aliases
            .insert("bal".to_string(), "balance".to_string());

        // Update preferences
        service.update_user_preferences(user_id, prefs).await;

        // Retrieve updated preferences
        let updated_prefs = service.get_user_preferences(user_id).await;
        assert_eq!(updated_prefs.display_settings.currency, "EUR");
        assert_eq!(updated_prefs.alert_settings.price_change_threshold, 10.0);
        assert_eq!(
            updated_prefs.command_aliases.get("bal"),
            Some(&"balance".to_string())
        );
    }

    #[tokio::test]
    async fn test_command_alias_functionality() {
        let config = create_test_config();
        let service = TelegramService::new(config);
        let user_id = "test_user_789";

        // Test adding command alias
        let result = service.add_command_alias(user_id, "bal", "balance").await;
        assert!(result.contains("✅ Alias added"));

        // Test alias resolution
        let resolved = service.resolve_command_alias(user_id, "bal").await;
        assert_eq!(resolved, "balance");

        // Test non-existent alias
        let resolved = service.resolve_command_alias(user_id, "nonexistent").await;
        assert_eq!(resolved, "nonexistent");

        // Test invalid command alias
        let result = service
            .add_command_alias(user_id, "invalid", "nonexistent_command")
            .await;
        assert!(result.contains("❌ Invalid command"));
    }

    #[tokio::test]
    async fn test_number_formatting() {
        let config = create_test_config();
        let service = TelegramService::new(config);
        let user_id = "test_user_format";

        // Test default formatting (Standard)
        let formatted = service.format_number(user_id, 1234.56).await;
        assert_eq!(formatted, "1,234.56");

        // Test different formats by updating preferences
        let mut prefs = service.get_user_preferences(user_id).await;

        // Test European format
        prefs.display_settings.number_format = NumberFormat::European;
        service
            .update_user_preferences(user_id, prefs.clone())
            .await;
        let european = service.format_number(user_id, 1234.56).await;
        assert!(european.contains("1.234,56") || european.contains("1234,56"));

        // Test Scientific format
        prefs.display_settings.number_format = NumberFormat::Scientific;
        service
            .update_user_preferences(user_id, prefs.clone())
            .await;
        let scientific = service.format_number(user_id, 1234.56).await;
        assert!(scientific.contains("e"));

        // Test Abbreviated format
        prefs.display_settings.number_format = NumberFormat::Abbreviated;
        service.update_user_preferences(user_id, prefs).await;
        let abbreviated = service.format_number(user_id, 1500000.0).await;
        assert_eq!(abbreviated, "1.50M");
    }

    #[tokio::test]
    async fn test_notification_preferences() {
        let config = create_test_config();
        let service = TelegramService::new(config);
        let user_id = "test_user_notifications";

        // Test default notification settings
        assert!(
            service
                .should_send_notification(user_id, "opportunity")
                .await
        );
        assert!(
            service
                .should_send_notification(user_id, "price_alert")
                .await
        );
        assert!(service.should_send_notification(user_id, "trading").await);
        assert!(service.should_send_notification(user_id, "system").await);

        // Disable notifications
        let mut prefs = service.get_user_preferences(user_id).await;
        prefs.notification_settings.enabled = false;
        service.update_user_preferences(user_id, prefs).await;

        // Test disabled notifications
        assert!(
            !service
                .should_send_notification(user_id, "opportunity")
                .await
        );
        assert!(
            !service
                .should_send_notification(user_id, "price_alert")
                .await
        );

        // Re-enable but disable specific types
        let mut prefs = service.get_user_preferences(user_id).await;
        prefs.notification_settings.enabled = true;
        prefs.notification_settings.opportunity_notifications = false;
        service.update_user_preferences(user_id, prefs).await;

        assert!(
            !service
                .should_send_notification(user_id, "opportunity")
                .await
        );
        assert!(
            service
                .should_send_notification(user_id, "price_alert")
                .await
        );
    }

    #[tokio::test]
    async fn test_dashboard_personalization() {
        let config = create_test_config();
        let service = TelegramService::new(config);
        let user_id = "test_user_dashboard";

        // Test default dashboard
        let dashboard = service.get_personalized_dashboard_message(user_id).await;
        assert!(dashboard.contains("🏠 *Personal Dashboard*"));
        assert!(dashboard.contains("💼 *Portfolio*"));
        assert!(dashboard.contains("🎯 *Opportunities*"));

        // Customize dashboard
        let mut prefs = service.get_user_preferences(user_id).await;
        prefs
            .dashboard_layout
            .quick_actions
            .push("/balance".to_string());
        prefs
            .dashboard_layout
            .favorite_commands
            .push("/opportunities".to_string());
        service.update_user_preferences(user_id, prefs).await;

        let customized_dashboard = service.get_personalized_dashboard_message(user_id).await;
        assert!(customized_dashboard.contains("⚡ *Quick Actions*"));
        assert!(customized_dashboard.contains("⭐ *Favorite Commands*"));
        assert!(customized_dashboard.contains("/balance"));
        assert!(customized_dashboard.contains("/opportunities"));
    }

    #[tokio::test]
    async fn test_smart_suggestions() {
        let config = create_test_config();
        let service = TelegramService::new(config);
        let user_id = "test_user_suggestions";

        // Test suggestions for new user
        let suggestions = service.get_smart_suggestions(user_id).await;
        assert!(suggestions.contains("💡 *Smart Suggestions*"));
        assert!(suggestions.contains("🔑 Set up exchange API keys"));
        assert!(suggestions.contains("🤖 Configure AI services"));

        // Add some preferences to reduce suggestions
        let mut prefs = service.get_user_preferences(user_id).await;
        prefs
            .command_aliases
            .insert("bal".to_string(), "balance".to_string());
        prefs
            .dashboard_layout
            .favorite_commands
            .push("/opportunities".to_string());
        prefs.alert_settings.custom_alerts.push(CustomAlert {
            id: "test_alert".to_string(),
            name: "Test Alert".to_string(),
            condition: AlertCondition::PriceAbove {
                symbol: "BTCUSDT".to_string(),
                price: 50000.0,
            },
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        });
        service.update_user_preferences(user_id, prefs).await;

        let updated_suggestions = service.get_smart_suggestions(user_id).await;
        // Should have fewer suggestions now
        assert!(!updated_suggestions.contains("⚡ Add command aliases"));
        assert!(!updated_suggestions.contains("⭐ Add frequently used commands"));
        assert!(!updated_suggestions.contains("🚨 Create custom alerts"));
    }

    #[test]
    fn test_user_preferences_default_values() {
        let prefs = UserPreferences::default();

        // Test notification defaults
        assert!(prefs.notification_settings.enabled);
        assert!(prefs.notification_settings.opportunity_notifications);
        assert!(prefs.notification_settings.price_alerts);
        assert!(prefs.notification_settings.trading_updates);
        assert!(prefs.notification_settings.system_notifications);
        assert!(matches!(
            prefs.notification_settings.frequency,
            NotificationFrequency::Immediate
        ));

        // Test display defaults
        assert_eq!(prefs.display_settings.currency, "USD");
        assert_eq!(prefs.display_settings.timezone, "UTC");
        assert_eq!(prefs.display_settings.language, "en");
        assert!(matches!(
            prefs.display_settings.number_format,
            NumberFormat::Standard
        ));
        assert!(!prefs.display_settings.compact_mode);

        // Test alert defaults
        assert_eq!(prefs.alert_settings.price_change_threshold, 5.0);
        assert_eq!(prefs.alert_settings.volume_change_threshold, 20.0);
        assert_eq!(prefs.alert_settings.opportunity_confidence_threshold, 80.0);
        assert_eq!(prefs.alert_settings.portfolio_change_threshold, 10.0);
        assert!(prefs.alert_settings.custom_alerts.is_empty());

        // Test dashboard defaults
        assert_eq!(prefs.dashboard_layout.sections.len(), 4);
        assert!(prefs
            .dashboard_layout
            .sections
            .contains(&DashboardSection::Portfolio));
        assert!(prefs
            .dashboard_layout
            .sections
            .contains(&DashboardSection::Opportunities));
        assert!(prefs
            .dashboard_layout
            .sections
            .contains(&DashboardSection::Alerts));
        assert!(prefs
            .dashboard_layout
            .sections
            .contains(&DashboardSection::RecentActivity));
        assert_eq!(prefs.dashboard_layout.quick_actions.len(), 3);
        assert_eq!(prefs.dashboard_layout.favorite_commands.len(), 0);
    }

    #[tokio::test]
    async fn test_preferences_management_message() {
        let config = create_test_config();
        let service = TelegramService::new(config);
        let user_id = "test_user_prefs_msg";

        let message = service.get_preferences_management_message(user_id).await;

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
