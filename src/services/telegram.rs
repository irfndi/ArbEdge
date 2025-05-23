// src/services/telegram.rs

use crate::types::ArbitrageOpportunity;
use crate::utils::{ArbitrageError, ArbitrageResult};
use crate::utils::formatter::format_opportunity_message;
use chrono::Utc;
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
        let message = format_opportunity_message(opportunity);
        self.send_message(&message).await
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