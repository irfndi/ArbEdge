// src/services/interfaces/telegram/core/bot_client.rs

//! Telegram Bot API Client
//!
//! Handles all communication with the Telegram Bot API including:
//! - Sending messages
//! - API requests
//! - Error handling and retries
//! - Rate limiting

use crate::services::core::infrastructure::UnifiedRetryConfig;
use crate::utils::time::WasmInstant;
use crate::utils::{ArbitrageError, ArbitrageResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use worker::console_log;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
    pub is_test_mode: bool,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            chat_id: String::new(),
            is_test_mode: true,
        }
    }
}

// Use unified retry configuration
pub type RetryConfig = UnifiedRetryConfig;

#[derive(Debug, Clone)]
pub struct RateLimitEntry {
    pub last_request: WasmInstant,
    pub request_count: u32,
    pub window_start: WasmInstant,
}

#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    pub value: T,
    pub expires_at: WasmInstant,
}

/// Telegram Bot API Client
pub struct TelegramBotClient {
    config: TelegramConfig,
    http_client: Client,
    retry_config: RetryConfig,
}

impl TelegramBotClient {
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
            retry_config: UnifiedRetryConfig::delivery_optimized(),
        }
    }

    /// Send a message to Telegram
    pub async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<&str>,
        reply_markup: Option<Value>,
    ) -> ArbitrageResult<Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );

        let mut payload = json!({
            "chat_id": chat_id,
            "text": text,
        });

        if let Some(mode) = parse_mode {
            payload["parse_mode"] = json!(mode);
        }

        if let Some(markup) = reply_markup {
            payload["reply_markup"] = markup;
        }

        self.make_api_request(&url, payload).await
    }

    /// Send a photo to Telegram
    pub async fn send_photo(
        &self,
        chat_id: &str,
        photo_url: &str,
        caption: Option<&str>,
    ) -> ArbitrageResult<Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendPhoto",
            self.config.bot_token
        );

        let mut payload = json!({
            "chat_id": chat_id,
            "photo": photo_url,
        });

        if let Some(cap) = caption {
            payload["caption"] = json!(cap);
        }

        self.make_api_request(&url, payload).await
    }

    /// Edit an existing message
    pub async fn edit_message(
        &self,
        chat_id: &str,
        message_id: i64,
        text: &str,
        parse_mode: Option<&str>,
        reply_markup: Option<Value>,
    ) -> ArbitrageResult<Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/editMessageText",
            self.config.bot_token
        );

        let mut payload = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text,
        });

        if let Some(mode) = parse_mode {
            payload["parse_mode"] = json!(mode);
        }

        if let Some(markup) = reply_markup {
            payload["reply_markup"] = markup;
        }

        self.make_api_request(&url, payload).await
    }

    /// Answer callback query
    pub async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
        show_alert: bool,
    ) -> ArbitrageResult<Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/answerCallbackQuery",
            self.config.bot_token
        );

        let mut payload = json!({
            "callback_query_id": callback_query_id,
            "show_alert": show_alert,
        });

        if let Some(text) = text {
            payload["text"] = json!(text);
        }

        self.make_api_request(&url, payload).await
    }

    /// Make API request with retry logic
    async fn make_api_request(&self, url: &str, payload: Value) -> ArbitrageResult<Value> {
        let mut last_error = None;

        for attempt in 0..=self.retry_config.max_attempts {
            match self.send_request(url, &payload).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = Some(e);

                    if attempt < self.retry_config.max_attempts {
                        let delay = self.calculate_retry_delay(attempt);
                        console_log!(
                            "🔄 Telegram API request failed, retrying in {}ms (attempt {}/{})",
                            delay,
                            attempt + 1,
                            self.retry_config.max_attempts
                        );

                        // WASM-compatible sleep
                        #[cfg(target_arch = "wasm32")]
                        {
                            use worker::Delay;
                            Delay::from(Duration::from_millis(delay)).await;
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                        }
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ArbitrageError::api_error("Failed to make Telegram API request after retries")
        }))
    }

    /// Send HTTP request to Telegram API
    async fn send_request(&self, url: &str, payload: &Value) -> ArbitrageResult<Value> {
        if self.config.is_test_mode {
            console_log!("🧪 Test mode: Would send to Telegram: {}", payload);
            return Ok(json!({"ok": true, "result": {"message_id": 12345}}));
        }

        let response = self
            .http_client
            .post(url)
            .json(payload)
            .send()
            .await
            .map_err(|e| ArbitrageError::api_error(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .map_err(|e| ArbitrageError::api_error(format!("Failed to read response: {}", e)))?;

        if !status.is_success() {
            return Err(ArbitrageError::api_error(format!(
                "Telegram API error {}: {}",
                status, response_text
            )));
        }

        serde_json::from_str(&response_text)
            .map_err(|e| ArbitrageError::api_error(format!("Failed to parse response: {}", e)))
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, attempt: u32) -> u64 {
        // Use the unified retry config's calculate_delay method
        self.retry_config.calculate_delay(attempt + 1).as_millis() as u64
    }

    /// Get bot info
    pub async fn get_me(&self) -> ArbitrageResult<Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/getMe",
            self.config.bot_token
        );

        self.make_api_request(&url, json!({})).await
    }

    /// Set webhook URL
    pub async fn set_webhook(&self, webhook_url: &str) -> ArbitrageResult<Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/setWebhook",
            self.config.bot_token
        );

        let payload = json!({
            "url": webhook_url,
            "allowed_updates": ["message", "callback_query"]
        });

        self.make_api_request(&url, payload).await
    }

    /// Delete webhook
    pub async fn delete_webhook(&self) -> ArbitrageResult<Value> {
        let url = format!(
            "https://api.telegram.org/bot{}/deleteWebhook",
            self.config.bot_token
        );

        self.make_api_request(&url, json!({})).await
    }
}
