// src/services/telegram.rs

use crate::types::{ArbitrageOpportunity, ArbitrageType, ExchangeIdEnum};
use crate::utils::{ArbitrageError, ArbitrageResult};

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::{json, Value};

pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

pub struct TelegramService {
    config: TelegramConfig,
    http_client: Client,
}

impl TelegramService {
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
        }
    }

    pub async fn send_message(&self, text: &str) -> ArbitrageResult<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.config.bot_token);
        
        let payload = json!({
            "chat_id": self.config.chat_id,
            "text": text,
            "parse_mode": "MarkdownV2"
        });

        let response = self.http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ArbitrageError::network_error(format!("Failed to send Telegram message: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::telegram_error(format!("Telegram API error: {}", error_text)));
        }

        let result: Value = response.json().await
            .map_err(|e| ArbitrageError::parse_error(format!("Failed to parse Telegram response: {}", e)))?;

        if !result["ok"].as_bool().unwrap_or(false) {
            let error_description = result["description"].as_str().unwrap_or("Unknown error");
            return Err(ArbitrageError::telegram_error(format!("Telegram API error: {}", error_description)));
        }

        Ok(())
    }

    pub async fn send_opportunity_notification(&self, opportunity: &ArbitrageOpportunity) -> ArbitrageResult<()> {
        let message = self.format_opportunity_message(opportunity);
        self.send_message(&message).await
    }

    fn format_opportunity_message(&self, opportunity: &ArbitrageOpportunity) -> String {
        // Extract and format values
        let pair_escaped = self.escape_markdown_v2(&opportunity.pair);
        let long_exchange_escaped = self.format_exchange(&opportunity.long_exchange);
        let short_exchange_escaped = self.format_exchange(&opportunity.short_exchange);
        let long_rate_escaped = self.format_optional_percentage(&opportunity.long_rate);
        let short_rate_escaped = self.format_optional_percentage(&opportunity.short_rate);
        let diff_escaped = self.escape_markdown_v2(&self.format_percentage(opportunity.rate_difference));
        let net_diff_escaped = self.format_optional_percentage(&opportunity.net_rate_difference);
        let potential_profit_escaped = self.format_money(&opportunity.potential_profit_value);
        let date_escaped = self.escape_markdown_v2(&self.format_timestamp(opportunity.timestamp));
        let details_escaped = opportunity.details.as_ref()
            .map(|d| self.escape_markdown_v2(d))
            .unwrap_or_default();

        // Build the message using MarkdownV2 syntax
        let mut message = format!(
            "🚨 *Arbitrage Opportunity Detected* 🚨\n\n📈 *Pair:* `{}`",
            pair_escaped
        );

        // Format based on opportunity type
        match opportunity.r#type {
            ArbitrageType::FundingRate if opportunity.long_exchange.is_some() && opportunity.short_exchange.is_some() => {
                message.push_str(&format!(
                    "\n↔️ *Action:* LONG `{}` / SHORT `{}`\n\n*Rates \\(Funding\\):*\n   \\- Long \\({}\\): `{}%`\n   \\- Short \\({}\\): `{}%`\n💰 *Gross Difference:* `{}%`",
                    long_exchange_escaped,
                    short_exchange_escaped,
                    long_exchange_escaped,
                    long_rate_escaped,
                    short_exchange_escaped,
                    short_rate_escaped,
                    diff_escaped
                ));
            }
            _ => {
                // Generic message for other types or if specific fields are missing
                let type_str = match opportunity.r#type {
                    ArbitrageType::FundingRate => "Funding Rate",
                    ArbitrageType::SpotFutures => "Spot Futures",
                    ArbitrageType::CrossExchange => "Cross Exchange",
                };
                message.push_str(&format!(
                    "\nℹ️ *Type:* {}\n💰 *Gross Metric:* `{}%`",
                    self.escape_markdown_v2(type_str),
                    diff_escaped
                ));
                
                if opportunity.long_exchange.is_some() {
                    message.push_str(&format!("\n➡️ *Exchange 1:* `{}`", long_exchange_escaped));
                }
                if opportunity.short_exchange.is_some() {
                    message.push_str(&format!("\n⬅️ *Exchange 2:* `{}`", short_exchange_escaped));
                }
            }
        }

        // Add net difference if available
        if opportunity.net_rate_difference.is_some() && net_diff_escaped != self.escape_markdown_v2("N/A") {
            message.push_str(&format!("\n💹 *Net Difference:* `{}%`", net_diff_escaped));
        }

        // Add potential profit if available
        if opportunity.potential_profit_value.is_some() && potential_profit_escaped != self.escape_markdown_v2("N/A") {
            message.push_str(&format!("\n💸 *Potential Profit:* \\~${}", potential_profit_escaped));
        }

        // Add details if available
        if !details_escaped.is_empty() {
            message.push_str(&format!("\n📝 *Details:* {}", details_escaped));
        }

        // Add timestamp
        message.push_str(&format!("\n🕒 *Timestamp:* {}", date_escaped));

        message
    }

    // Helper formatting methods
    fn escape_markdown_v2(&self, text: &str) -> String {
        // Characters to escape: _ * [ ] ( ) ~ ` > # + - = | { } . !
        let chars_to_escape = ['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!'];
        
        text.chars()
            .map(|c| {
                if chars_to_escape.contains(&c) {
                    format!("\\{}", c)
                } else {
                    c.to_string()
                }
            })
            .collect()
    }

    fn format_exchange(&self, exchange: &Option<ExchangeIdEnum>) -> String {
        match exchange {
            Some(exchange) => exchange.to_string().to_uppercase(),
            None => "N/A".to_string(),
        }
    }

    fn format_percentage(&self, value: f64) -> String {
        format!("{:.4}", value * 100.0)
    }

    fn format_optional_percentage(&self, value: &Option<f64>) -> String {
        match value {
            Some(v) => self.escape_markdown_v2(&self.format_percentage(*v)),
            None => self.escape_markdown_v2("N/A"),
        }
    }

    fn format_money(&self, value: &Option<f64>) -> String {
        match value {
            Some(v) => self.escape_markdown_v2(&format!("{:.2}", v)),
            None => self.escape_markdown_v2("N/A"),
        }
    }

    fn format_timestamp(&self, timestamp: u64) -> String {
        let datetime = DateTime::from_timestamp_millis(timestamp as i64)
            .unwrap_or_else(Utc::now);
        datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    }

    // Bot command handlers (for webhook mode)
    pub async fn handle_webhook(&self, update: Value) -> ArbitrageResult<Option<String>> {
        if let Some(message) = update["message"].as_object() {
            if let Some(text) = message["text"].as_str() {
                return self.handle_command(text).await;
            }
        }
        Ok(None)
    }

    async fn handle_command(&self, text: &str) -> ArbitrageResult<Option<String>> {
        match text {
            "/start" => Ok(Some(
                "Welcome to the Arbitrage Bot!\n\
                I can help you detect funding rate arbitrage opportunities and notify you about them.\n\n\
                Here are the available commands:\n\
                /help - Show this help message and list all commands.\n\
                /status - Check the bot's current operational status.\n\
                /opportunities - Show recent arbitrage opportunities (currently placeholder).\n\
                /settings - View current bot settings (currently placeholder).\n\n\
                Use /help to see this list again.".to_string()
            )),
            "/help" => Ok(Some(
                "Available commands:\n\
                /help - Show this help message\n\
                /status - Check bot status\n\
                /opportunities - Show recent opportunities\n\
                /settings - View current settings".to_string()
            )),
            "/status" => {
                let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
                Ok(Some(format!(
                    "Bot is active and monitoring for arbitrage opportunities.\nCurrent time: {}",
                    now
                )))
            }
            "/opportunities" => Ok(Some(
                "No recent opportunities found. Will notify you when new ones are detected.".to_string()
            )),
            "/settings" => Ok(Some(
                "Current settings:\n\
                Threshold: 0.001 (0.1%)\n\
                Pairs monitored: BTC/USDT, ETH/USDT\n\
                Exchanges: Binance, Bybit, OKX".to_string()
            )),
            _ => Ok(None), // Unknown command, no response
        }
    }

    pub async fn set_webhook(&self, webhook_url: &str) -> ArbitrageResult<()> {
        let url = format!("https://api.telegram.org/bot{}/setWebhook", self.config.bot_token);
        
        let payload = json!({
            "url": webhook_url
        });

        let response = self.http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ArbitrageError::network_error(format!("Failed to set webhook: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::telegram_error(format!("Failed to set webhook: {}", error_text)));
        }

        Ok(())
    }
} 