// Template Engine - Dynamic Template Management with Variable Substitution and Localization
// Part of Notification Module replacing notifications.rs

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use worker::kv::KvStore;

/// Template categories for organization
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCategory {
    OpportunityAlert,
    BalanceChange,
    PriceAlert,
    RiskWarning,
    SystemMaintenance,
    SecurityAlert,
    TradingSignal,
    Welcome,
    Custom(String),
}

impl TemplateCategory {
    pub fn as_str(&self) -> &str {
        match self {
            TemplateCategory::OpportunityAlert => "opportunity_alert",
            TemplateCategory::BalanceChange => "balance_change",
            TemplateCategory::PriceAlert => "price_alert",
            TemplateCategory::RiskWarning => "risk_warning",
            TemplateCategory::SystemMaintenance => "system_maintenance",
            TemplateCategory::SecurityAlert => "security_alert",
            TemplateCategory::TradingSignal => "trading_signal",
            TemplateCategory::Welcome => "welcome",
            TemplateCategory::Custom(name) => name,
        }
    }

    pub fn default_priority(&self) -> u8 {
        match self {
            TemplateCategory::SecurityAlert => 10,
            TemplateCategory::RiskWarning => 9,
            TemplateCategory::OpportunityAlert => 8,
            TemplateCategory::TradingSignal => 7,
            TemplateCategory::BalanceChange => 6,
            TemplateCategory::PriceAlert => 5,
            TemplateCategory::SystemMaintenance => 4,
            TemplateCategory::Welcome => 3,
            TemplateCategory::Custom(_) => 5,
        }
    }
}

/// Template variable definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    pub name: String,
    pub variable_type: VariableType,
    pub description: String,
    pub required: bool,
    pub default_value: Option<String>,
    pub validation_pattern: Option<String>,
    pub format_options: HashMap<String, String>,
}

impl TemplateVariable {
    pub fn new(name: String, variable_type: VariableType, description: String) -> Self {
        Self {
            name,
            variable_type,
            description,
            required: false,
            default_value: None,
            validation_pattern: None,
            format_options: HashMap::new(),
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_default(mut self, default_value: String) -> Self {
        self.default_value = Some(default_value);
        self
    }

    pub fn with_validation(mut self, pattern: String) -> Self {
        self.validation_pattern = Some(pattern);
        self
    }

    pub fn with_format_option(mut self, key: String, value: String) -> Self {
        self.format_options.insert(key, value);
        self
    }

    pub fn validate_value(&self, value: &str) -> ArbitrageResult<()> {
        if let Some(pattern) = &self.validation_pattern {
            // Simple pattern validation (in real implementation, use regex)
            if !value.contains(&pattern.replace("*", "")) {
                return Err(ArbitrageError::validation_error(&format!(
                    "Variable '{}' value '{}' does not match pattern '{}'",
                    self.name, value, pattern
                )));
            }
        }

        match self.variable_type {
            VariableType::Number => {
                if value.parse::<f64>().is_err() {
                    return Err(ArbitrageError::validation_error(&format!(
                        "Variable '{}' must be a number, got '{}'",
                        self.name, value
                    )));
                }
            }
            VariableType::Email => {
                if !value.contains('@') {
                    return Err(ArbitrageError::validation_error(&format!(
                        "Variable '{}' must be a valid email, got '{}'",
                        self.name, value
                    )));
                }
            }
            VariableType::Url => {
                if !value.starts_with("http") {
                    return Err(ArbitrageError::validation_error(&format!(
                        "Variable '{}' must be a valid URL, got '{}'",
                        self.name, value
                    )));
                }
            }
            _ => {} // No validation for other types
        }

        Ok(())
    }
}

/// Variable types for template substitution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableType {
    Text,
    Number,
    Currency,
    Percentage,
    DateTime,
    Email,
    Url,
    Boolean,
    Array,
    Object,
}

impl VariableType {
    pub fn as_str(&self) -> &str {
        match self {
            VariableType::Text => "text",
            VariableType::Number => "number",
            VariableType::Currency => "currency",
            VariableType::Percentage => "percentage",
            VariableType::DateTime => "datetime",
            VariableType::Email => "email",
            VariableType::Url => "url",
            VariableType::Boolean => "boolean",
            VariableType::Array => "array",
            VariableType::Object => "object",
        }
    }
}

/// Notification template with multi-channel support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationTemplate {
    pub template_id: String,
    pub name: String,
    pub category: TemplateCategory,
    pub description: String,
    pub version: String,
    pub language: String,
    pub channel_templates: HashMap<String, ChannelTemplate>, // channel_name -> template
    pub variables: Vec<TemplateVariable>,
    pub metadata: HashMap<String, String>,
    pub is_active: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub usage_count: u64,
    pub last_used_at: Option<u64>,
}

impl NotificationTemplate {
    pub fn new(
        name: String,
        category: TemplateCategory,
        description: String,
        language: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            template_id: uuid::Uuid::new_v4().to_string(),
            name,
            category,
            description,
            version: "1.0.0".to_string(),
            language,
            channel_templates: HashMap::new(),
            variables: Vec::new(),
            metadata: HashMap::new(),
            is_active: true,
            created_at: now,
            updated_at: now,
            usage_count: 0,
            last_used_at: None,
        }
    }

    pub fn with_channel_template(mut self, channel: String, template: ChannelTemplate) -> Self {
        self.channel_templates.insert(channel, template);
        self
    }

    pub fn with_variable(mut self, variable: TemplateVariable) -> Self {
        self.variables.push(variable);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn get_channel_template(&self, channel: &str) -> Option<&ChannelTemplate> {
        self.channel_templates.get(channel)
    }

    pub fn validate_variables(&self, variables: &HashMap<String, String>) -> ArbitrageResult<()> {
        // Check required variables
        for template_var in &self.variables {
            if template_var.required && !variables.contains_key(&template_var.name) {
                return Err(ArbitrageError::validation_error(&format!(
                    "Required variable '{}' is missing",
                    template_var.name
                )));
            }

            if let Some(value) = variables.get(&template_var.name) {
                template_var.validate_value(value)?;
            }
        }

        Ok(())
    }

    pub fn increment_usage(&mut self) {
        self.usage_count += 1;
        self.last_used_at = Some(chrono::Utc::now().timestamp_millis() as u64);
        self.updated_at = chrono::Utc::now().timestamp_millis() as u64;
    }
}

/// Channel-specific template content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTemplate {
    pub channel: String,
    pub subject: Option<String>, // For email, push notifications
    pub title: Option<String>,   // For rich notifications
    pub body: String,
    pub footer: Option<String>,
    pub format: TemplateFormat,
    pub attachments: Vec<TemplateAttachment>,
    pub styling: HashMap<String, String>,
}

impl ChannelTemplate {
    pub fn new(channel: String, body: String, format: TemplateFormat) -> Self {
        Self {
            channel,
            subject: None,
            title: None,
            body,
            footer: None,
            format,
            attachments: Vec::new(),
            styling: HashMap::new(),
        }
    }

    pub fn with_subject(mut self, subject: String) -> Self {
        self.subject = Some(subject);
        self
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_footer(mut self, footer: String) -> Self {
        self.footer = Some(footer);
        self
    }

    pub fn with_attachment(mut self, attachment: TemplateAttachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    pub fn with_styling(mut self, key: String, value: String) -> Self {
        self.styling.insert(key, value);
        self
    }
}

/// Template format types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateFormat {
    PlainText,
    Html,
    Markdown,
    Json,
    Custom(String),
}

impl TemplateFormat {
    pub fn as_str(&self) -> &str {
        match self {
            TemplateFormat::PlainText => "plain_text",
            TemplateFormat::Html => "html",
            TemplateFormat::Markdown => "markdown",
            TemplateFormat::Json => "json",
            TemplateFormat::Custom(format) => format,
        }
    }
}

/// Template attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateAttachment {
    pub attachment_type: AttachmentType,
    pub url: Option<String>,
    pub content: Option<String>,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<u64>,
}

/// Attachment types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentType {
    Image,
    Document,
    Audio,
    Video,
    Data,
    Custom(String),
}

/// Template engine health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateEngineHealth {
    pub is_healthy: bool,
    pub total_templates: u64,
    pub active_templates: u64,
    pub template_cache_hit_rate: f32,
    pub avg_render_time_ms: f64,
    pub successful_renders: u64,
    pub failed_renders: u64,
    pub kv_store_available: bool,
    pub last_health_check: u64,
    pub last_error: Option<String>,
}

impl Default for TemplateEngineHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            total_templates: 0,
            active_templates: 0,
            template_cache_hit_rate: 0.0,
            avg_render_time_ms: 0.0,
            successful_renders: 0,
            failed_renders: 0,
            kv_store_available: false,
            last_health_check: 0,
            last_error: None,
        }
    }
}

/// Template engine performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateEngineMetrics {
    pub total_renders: u64,
    pub renders_per_second: f64,
    pub successful_renders: u64,
    pub failed_renders: u64,
    pub avg_render_time_ms: f64,
    pub min_render_time_ms: f64,
    pub max_render_time_ms: f64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f32,
    pub templates_by_category: HashMap<TemplateCategory, u64>,
    pub renders_by_channel: HashMap<String, u64>,
    pub renders_by_language: HashMap<String, u64>,
    pub template_usage: HashMap<String, u64>,
    pub variable_usage: HashMap<String, u64>,
    pub last_updated: u64,
}

impl Default for TemplateEngineMetrics {
    fn default() -> Self {
        Self {
            total_renders: 0,
            renders_per_second: 0.0,
            successful_renders: 0,
            failed_renders: 0,
            avg_render_time_ms: 0.0,
            min_render_time_ms: f64::MAX,
            max_render_time_ms: 0.0,
            cache_hits: 0,
            cache_misses: 0,
            cache_hit_rate: 0.0,
            templates_by_category: HashMap::new(),
            renders_by_channel: HashMap::new(),
            renders_by_language: HashMap::new(),
            template_usage: HashMap::new(),
            variable_usage: HashMap::new(),
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for TemplateEngine
#[derive(Debug, Clone)]
pub struct TemplateEngineConfig {
    pub enable_template_engine: bool,
    pub enable_caching: bool,
    pub enable_validation: bool,
    pub enable_localization: bool,
    pub cache_ttl_seconds: u64,
    pub max_templates_in_cache: usize,
    pub max_template_size_bytes: usize,
    pub max_variables_per_template: usize,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
    pub enable_a_b_testing: bool,
    pub default_language: String,
    pub supported_languages: Vec<String>,
    pub enable_template_inheritance: bool,
    pub enable_performance_monitoring: bool,
}

impl Default for TemplateEngineConfig {
    fn default() -> Self {
        Self {
            enable_template_engine: true,
            enable_caching: true,
            enable_validation: true,
            enable_localization: true,
            cache_ttl_seconds: 3600, // 1 hour
            max_templates_in_cache: 1000,
            max_template_size_bytes: 1048576, // 1MB
            max_variables_per_template: 100,
            enable_kv_storage: true,
            kv_key_prefix: "template:".to_string(),
            enable_compression: true,
            compression_threshold_bytes: 1024,
            enable_a_b_testing: false,
            default_language: "en".to_string(),
            supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string()],
            enable_template_inheritance: true,
            enable_performance_monitoring: true,
        }
    }
}

impl TemplateEngineConfig {
    pub fn high_performance() -> Self {
        Self {
            cache_ttl_seconds: 7200, // 2 hours
            max_templates_in_cache: 2000,
            enable_compression: true,
            enable_a_b_testing: true,
            enable_performance_monitoring: true,
            ..Default::default()
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            cache_ttl_seconds: 1800, // 30 minutes
            max_templates_in_cache: 500,
            enable_validation: true,
            enable_template_inheritance: true,
            enable_performance_monitoring: true,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.cache_ttl_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "cache_ttl_seconds must be greater than 0",
            ));
        }
        if self.max_templates_in_cache == 0 {
            return Err(ArbitrageError::validation_error(
                "max_templates_in_cache must be greater than 0",
            ));
        }
        if self.max_template_size_bytes == 0 {
            return Err(ArbitrageError::validation_error(
                "max_template_size_bytes must be greater than 0",
            ));
        }
        if self.supported_languages.is_empty() {
            return Err(ArbitrageError::validation_error(
                "supported_languages cannot be empty",
            ));
        }
        Ok(())
    }
}

/// Template Engine for dynamic template management
pub struct TemplateEngine {
    config: TemplateEngineConfig,
    logger: crate::utils::logger::Logger,
    kv_store: KvStore,

    // Template storage
    templates: Arc<Mutex<HashMap<String, NotificationTemplate>>>,
    template_cache: Arc<Mutex<HashMap<String, (String, u64)>>>, // template_id -> (rendered_content, timestamp)

    // Health and performance tracking
    health: Arc<Mutex<TemplateEngineHealth>>,
    metrics: Arc<Mutex<TemplateEngineMetrics>>,

    // Performance tracking
    startup_time: u64,
}

impl TemplateEngine {
    /// Create new TemplateEngine instance
    pub async fn new(
        config: TemplateEngineConfig,
        kv_store: KvStore,
        _env: &worker::Env,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        logger.info(&format!(
            "TemplateEngine initialized: caching={}, validation={}, localization={}",
            config.enable_caching, config.enable_validation, config.enable_localization
        ));

        let engine = Self {
            config,
            logger,
            kv_store,
            templates: Arc::new(Mutex::new(HashMap::new())),
            template_cache: Arc::new(Mutex::new(HashMap::new())),
            health: Arc::new(Mutex::new(TemplateEngineHealth::default())),
            metrics: Arc::new(Mutex::new(TemplateEngineMetrics::default())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        };

        // Load default templates
        engine.load_default_templates().await?;

        Ok(engine)
    }

    /// Register a new template
    pub async fn register_template(&self, template: NotificationTemplate) -> ArbitrageResult<()> {
        if !self.config.enable_template_engine {
            return Ok(());
        }

        // Validate template
        if self.config.enable_validation {
            self.validate_template(&template)?;
        }

        let template_id = template.template_id.clone();

        // Store in memory
        if let Ok(mut templates) = self.templates.lock() {
            templates.insert(template_id.clone(), template.clone());
        }

        // Store in KV if enabled
        if self.config.enable_kv_storage {
            self.store_template_in_kv(&template).await?;
        }

        self.logger.info(&format!(
            "Registered template: {} ({})",
            template.name, template_id
        ));
        Ok(())
    }

    /// Render template with variables
    pub async fn render_template(
        &self,
        template_id: &str,
        channel: &str,
        variables: HashMap<String, String>,
        language: Option<String>,
    ) -> ArbitrageResult<String> {
        if !self.config.enable_template_engine {
            return Ok("Template engine disabled".to_string());
        }

        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check cache first
        if self.config.enable_caching {
            let cache_key = format!(
                "{}:{}:{}",
                template_id,
                channel,
                language.as_deref().unwrap_or(&self.config.default_language)
            );
            if let Some(cached_content) = self.get_from_cache(&cache_key).await {
                self.record_cache_hit().await;
                return Ok(cached_content);
            }
            self.record_cache_miss().await;
        }

        // Get template
        let template = self.get_template(template_id, language.as_deref()).await?;

        // Validate variables
        if self.config.enable_validation {
            template.validate_variables(&variables)?;
        }

        // Get channel template
        let channel_template = template.get_channel_template(channel).ok_or_else(|| {
            ArbitrageError::validation_error(&format!(
                "Channel '{}' not found in template",
                channel
            ))
        })?;

        // Render content
        let rendered_content = self.substitute_variables(&channel_template.body, &variables)?;

        // Cache result
        if self.config.enable_caching {
            let cache_key = format!(
                "{}:{}:{}",
                template_id,
                channel,
                language.as_deref().unwrap_or(&self.config.default_language)
            );
            self.store_in_cache(cache_key, rendered_content.clone())
                .await;
        }

        // Update metrics
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let render_time = (end_time - start_time) as f64;
        self.record_render_metrics(template_id, channel, render_time, true)
            .await;

        // Update template usage
        self.update_template_usage(template_id).await;

        Ok(rendered_content)
    }

    /// Get template by ID and language
    async fn get_template(
        &self,
        template_id: &str,
        language: Option<&str>,
    ) -> ArbitrageResult<NotificationTemplate> {
        let target_language = language.unwrap_or(&self.config.default_language);

        // Try to get from memory first
        if let Ok(templates) = self.templates.lock() {
            if let Some(template) = templates.get(template_id) {
                if template.language == target_language {
                    return Ok(template.clone());
                }
            }
        }

        // Try to load from KV
        if self.config.enable_kv_storage {
            if let Ok(template) = self
                .load_template_from_kv(template_id, target_language)
                .await
            {
                return Ok(template);
            }
        }

        Err(ArbitrageError::not_found(&format!(
            "Template '{}' not found for language '{}'",
            template_id, target_language
        )))
    }

    /// Substitute variables in template content
    fn substitute_variables(
        &self,
        content: &str,
        variables: &HashMap<String, String>,
    ) -> ArbitrageResult<String> {
        let mut result = content.to_string();

        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key); // {{variable_name}}
            result = result.replace(&placeholder, value);
        }

        // Check for unsubstituted variables
        if self.config.enable_validation && result.contains("{{") {
            return Err(ArbitrageError::validation_error(
                "Template contains unsubstituted variables",
            ));
        }

        Ok(result)
    }

    /// Validate template structure and content
    fn validate_template(&self, template: &NotificationTemplate) -> ArbitrageResult<()> {
        if template.name.is_empty() {
            return Err(ArbitrageError::validation_error(
                "Template name cannot be empty",
            ));
        }

        if template.channel_templates.is_empty() {
            return Err(ArbitrageError::validation_error(
                "Template must have at least one channel template",
            ));
        }

        if template.variables.len() > self.config.max_variables_per_template {
            return Err(ArbitrageError::validation_error(&format!(
                "Template has too many variables: {} > {}",
                template.variables.len(),
                self.config.max_variables_per_template
            )));
        }

        // Validate each channel template
        for (channel, channel_template) in &template.channel_templates {
            if channel_template.body.is_empty() {
                return Err(ArbitrageError::validation_error(&format!(
                    "Channel '{}' template body cannot be empty",
                    channel
                )));
            }

            let body_size = channel_template.body.len();
            if body_size > self.config.max_template_size_bytes {
                return Err(ArbitrageError::validation_error(&format!(
                    "Channel '{}' template body too large: {} > {} bytes",
                    channel, body_size, self.config.max_template_size_bytes
                )));
            }
        }

        Ok(())
    }

    /// Load default templates
    async fn load_default_templates(&self) -> ArbitrageResult<()> {
        // Opportunity Alert Template
        let opportunity_template = NotificationTemplate::new(
            "Opportunity Alert".to_string(),
            TemplateCategory::OpportunityAlert,
            "Alert for new arbitrage opportunities".to_string(),
            "en".to_string(),
        )
        .with_channel_template(
            "telegram".to_string(),
            ChannelTemplate::new(
                "telegram".to_string(),
                "🚀 *New Opportunity Alert*\n\nExchange: {{exchange}}\nPair: {{pair}}\nProfit: {{profit}}%\nExpires: {{expires_at}}\n\n[View Details]({{details_url}})".to_string(),
                TemplateFormat::Markdown,
            )
        )
        .with_channel_template(
            "email".to_string(),
            ChannelTemplate::new(
                "email".to_string(),
                "<h2>New Arbitrage Opportunity</h2><p>Exchange: {{exchange}}</p><p>Pair: {{pair}}</p><p>Profit: {{profit}}%</p><p>Expires: {{expires_at}}</p><a href=\"{{details_url}}\">View Details</a>".to_string(),
                TemplateFormat::Html,
            ).with_subject("New Arbitrage Opportunity - {{profit}}% profit".to_string())
        )
        .with_variable(
            TemplateVariable::new("exchange".to_string(), VariableType::Text, "Exchange name".to_string()).required()
        )
        .with_variable(
            TemplateVariable::new("pair".to_string(), VariableType::Text, "Trading pair".to_string()).required()
        )
        .with_variable(
            TemplateVariable::new("profit".to_string(), VariableType::Percentage, "Profit percentage".to_string()).required()
        )
        .with_variable(
            TemplateVariable::new("expires_at".to_string(), VariableType::DateTime, "Expiration time".to_string()).required()
        )
        .with_variable(
            TemplateVariable::new("details_url".to_string(), VariableType::Url, "Details URL".to_string()).required()
        );

        self.register_template(opportunity_template).await?;

        // Balance Change Template
        let balance_template = NotificationTemplate::new(
            "Balance Change".to_string(),
            TemplateCategory::BalanceChange,
            "Notification for balance changes".to_string(),
            "en".to_string(),
        )
        .with_channel_template(
            "telegram".to_string(),
            ChannelTemplate::new(
                "telegram".to_string(),
                "💰 *Balance Update*\n\nExchange: {{exchange}}\nCurrency: {{currency}}\nChange: {{change_amount}} {{currency}}\nNew Balance: {{new_balance}} {{currency}}".to_string(),
                TemplateFormat::Markdown,
            )
        )
        .with_variable(
            TemplateVariable::new("exchange".to_string(), VariableType::Text, "Exchange name".to_string()).required()
        )
        .with_variable(
            TemplateVariable::new("currency".to_string(), VariableType::Text, "Currency symbol".to_string()).required()
        )
        .with_variable(
            TemplateVariable::new("change_amount".to_string(), VariableType::Currency, "Change amount".to_string()).required()
        )
        .with_variable(
            TemplateVariable::new("new_balance".to_string(), VariableType::Currency, "New balance".to_string()).required()
        );

        self.register_template(balance_template).await?;

        self.logger.info("Default templates loaded successfully");
        Ok(())
    }

    /// Store template in KV store
    async fn store_template_in_kv(&self, template: &NotificationTemplate) -> ArbitrageResult<()> {
        let key = format!(
            "{}{}:{}",
            self.config.kv_key_prefix, template.template_id, template.language
        );
        let value = serde_json::to_string(template)?;

        self.kv_store
            .put(&key, value)?
            .expiration_ttl(self.config.cache_ttl_seconds)
            .execute()
            .await
            .map_err(|e| ArbitrageError::kv_error(&format!("Failed to store template: {}", e)))?;

        Ok(())
    }

    /// Load template from KV store
    async fn load_template_from_kv(
        &self,
        template_id: &str,
        language: &str,
    ) -> ArbitrageResult<NotificationTemplate> {
        let key = format!("{}{}:{}", self.config.kv_key_prefix, template_id, language);

        let value = self
            .kv_store
            .get(&key)
            .text()
            .await
            .map_err(|e| ArbitrageError::kv_error(&format!("Failed to load template: {}", e)))?
            .ok_or_else(|| {
                ArbitrageError::not_found(&format!("Template not found in KV: {}", key))
            })?;

        let template: NotificationTemplate = serde_json::from_str(&value)?;

        // Store in memory cache
        if let Ok(mut templates) = self.templates.lock() {
            templates.insert(template.template_id.clone(), template.clone());
        }

        Ok(template)
    }

    /// Get content from cache
    async fn get_from_cache(&self, cache_key: &str) -> Option<String> {
        if let Ok(cache) = self.template_cache.lock() {
            if let Some((content, timestamp)) = cache.get(cache_key) {
                let now = chrono::Utc::now().timestamp_millis() as u64;
                if now - timestamp < self.config.cache_ttl_seconds * 1000 {
                    return Some(content.clone());
                }
            }
        }
        None
    }

    /// Store content in cache
    async fn store_in_cache(&self, cache_key: String, content: String) {
        if let Ok(mut cache) = self.template_cache.lock() {
            let timestamp = chrono::Utc::now().timestamp_millis() as u64;
            cache.insert(cache_key, (content, timestamp));

            // Clean up old entries if cache is too large
            if cache.len() > self.config.max_templates_in_cache {
                let oldest_key = cache
                    .iter()
                    .min_by_key(|(_, (_, timestamp))| timestamp)
                    .map(|(key, _)| key.clone());

                if let Some(key) = oldest_key {
                    cache.remove(&key);
                }
            }
        }
    }

    /// Record cache hit
    async fn record_cache_hit(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.cache_hits += 1;
            metrics.cache_hit_rate = (metrics.cache_hits as f32)
                / (metrics.cache_hits + metrics.cache_misses) as f32
                * 100.0;
        }
    }

    /// Record cache miss
    async fn record_cache_miss(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.cache_misses += 1;
            metrics.cache_hit_rate = (metrics.cache_hits as f32)
                / (metrics.cache_hits + metrics.cache_misses) as f32
                * 100.0;
        }
    }

    /// Record render metrics
    async fn record_render_metrics(
        &self,
        template_id: &str,
        channel: &str,
        render_time: f64,
        success: bool,
    ) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_renders += 1;

            if success {
                metrics.successful_renders += 1;
            } else {
                metrics.failed_renders += 1;
            }

            // Update render time metrics
            metrics.avg_render_time_ms =
                (metrics.avg_render_time_ms * (metrics.total_renders - 1) as f64 + render_time)
                    / metrics.total_renders as f64;
            metrics.min_render_time_ms = metrics.min_render_time_ms.min(render_time);
            metrics.max_render_time_ms = metrics.max_render_time_ms.max(render_time);

            // Update usage counters
            *metrics
                .renders_by_channel
                .entry(channel.to_string())
                .or_insert(0) += 1;
            *metrics
                .template_usage
                .entry(template_id.to_string())
                .or_insert(0) += 1;

            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Update template usage
    async fn update_template_usage(&self, template_id: &str) {
        if let Ok(mut templates) = self.templates.lock() {
            if let Some(template) = templates.get_mut(template_id) {
                template.increment_usage();
            }
        }
    }

    /// Get template engine health
    pub async fn get_health(&self) -> TemplateEngineHealth {
        if let Ok(health) = self.health.lock() {
            health.clone()
        } else {
            TemplateEngineHealth::default()
        }
    }

    /// Get template engine metrics
    pub async fn get_metrics(&self) -> TemplateEngineMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            TemplateEngineMetrics::default()
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Test template rendering
        let test_variables = HashMap::from([("test_var".to_string(), "test_value".to_string())]);

        let test_content = "Hello {{test_var}}!";
        let rendered = self.substitute_variables(test_content, &test_variables)?;

        if rendered != "Hello test_value!" {
            return Ok(false);
        }

        // Update health status
        if let Ok(mut health) = self.health.lock() {
            health.is_healthy = true;
            health.kv_store_available = self.config.enable_kv_storage;
            health.last_health_check = start_time;
            health.last_error = None;

            // Update template counts
            if let Ok(templates) = self.templates.lock() {
                health.total_templates = templates.len() as u64;
                health.active_templates = templates.values().filter(|t| t.is_active).count() as u64;
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_category_properties() {
        assert_eq!(
            TemplateCategory::OpportunityAlert.as_str(),
            "opportunity_alert"
        );
        assert_eq!(TemplateCategory::SecurityAlert.default_priority(), 10);
        assert_eq!(TemplateCategory::Welcome.default_priority(), 3);
    }

    #[test]
    fn test_template_variable_validation() {
        let var = TemplateVariable::new(
            "email".to_string(),
            VariableType::Email,
            "User email".to_string(),
        )
        .required();

        assert!(var.validate_value("test@example.com").is_ok());
        assert!(var.validate_value("invalid-email").is_err());
    }

    #[test]
    fn test_notification_template_creation() {
        let template = NotificationTemplate::new(
            "Test Template".to_string(),
            TemplateCategory::OpportunityAlert,
            "Test description".to_string(),
            "en".to_string(),
        )
        .with_channel_template(
            "telegram".to_string(),
            ChannelTemplate::new(
                "telegram".to_string(),
                "Hello {{name}}!".to_string(),
                TemplateFormat::PlainText,
            ),
        )
        .with_variable(
            TemplateVariable::new(
                "name".to_string(),
                VariableType::Text,
                "User name".to_string(),
            )
            .required(),
        );

        assert_eq!(template.name, "Test Template");
        assert_eq!(template.category, TemplateCategory::OpportunityAlert);
        assert_eq!(template.language, "en");
        assert!(template.get_channel_template("telegram").is_some());
        assert_eq!(template.variables.len(), 1);
    }

    #[test]
    fn test_template_variable_validation_with_variables() {
        let template = NotificationTemplate::new(
            "Test Template".to_string(),
            TemplateCategory::OpportunityAlert,
            "Test description".to_string(),
            "en".to_string(),
        )
        .with_variable(
            TemplateVariable::new(
                "required_var".to_string(),
                VariableType::Text,
                "Required variable".to_string(),
            )
            .required(),
        )
        .with_variable(TemplateVariable::new(
            "optional_var".to_string(),
            VariableType::Text,
            "Optional variable".to_string(),
        ));

        let mut variables = HashMap::new();
        variables.insert("required_var".to_string(), "value".to_string());

        assert!(template.validate_variables(&variables).is_ok());

        let empty_variables = HashMap::new();
        assert!(template.validate_variables(&empty_variables).is_err());
    }

    #[test]
    fn test_template_engine_config_validation() {
        let mut config = TemplateEngineConfig::default();
        assert!(config.validate().is_ok());

        config.cache_ttl_seconds = 0;
        assert!(config.validate().is_err());

        config.cache_ttl_seconds = 3600;
        config.supported_languages.clear();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = TemplateEngineConfig::high_performance();
        assert_eq!(config.cache_ttl_seconds, 7200);
        assert_eq!(config.max_templates_in_cache, 2000);
        assert!(config.enable_a_b_testing);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = TemplateEngineConfig::high_reliability();
        assert_eq!(config.cache_ttl_seconds, 1800);
        assert_eq!(config.max_templates_in_cache, 500);
        assert!(config.enable_validation);
    }
}
