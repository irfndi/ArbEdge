// src/services/interfaces/telegram/core/message_handler.rs

//! Telegram Message Handler
//! 
//! Handles message processing, formatting, and sending including:
//! - Message formatting and escaping
//! - Rich message composition
//! - Message queuing and rate limiting
//! - Template rendering

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use worker::console_log;

/// Message formatting and processing
pub struct MessageHandler {
    // Future: Add template engine, formatting options, etc.
}

impl MessageHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// Format a simple text message
    pub fn format_text_message(&self, text: &str, parse_mode: Option<&str>) -> Value {
        let mut message = json!({
            "text": text
        });

        if let Some(mode) = parse_mode {
            message["parse_mode"] = json!(mode);
        }

        message
    }

    /// Format a message with inline keyboard
    pub fn format_message_with_keyboard(
        &self,
        text: &str,
        keyboard: Value,
        parse_mode: Option<&str>,
    ) -> Value {
        let mut message = json!({
            "text": text,
            "reply_markup": {
                "inline_keyboard": keyboard
            }
        });

        if let Some(mode) = parse_mode {
            message["parse_mode"] = json!(mode);
        }

        message
    }

    /// Format an opportunity message
    pub fn format_opportunity_message(&self, opportunity: &Value) -> ArbitrageResult<String> {
        let symbol = opportunity
            .get("symbol")
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown");

        let price_diff = opportunity
            .get("price_difference")
            .and_then(|p| p.as_f64())
            .unwrap_or(0.0);

        let percentage = opportunity
            .get("percentage")
            .and_then(|p| p.as_f64())
            .unwrap_or(0.0);

        let exchange_a = opportunity
            .get("exchange_a")
            .and_then(|e| e.as_str())
            .unwrap_or("Unknown");

        let exchange_b = opportunity
            .get("exchange_b")
            .and_then(|e| e.as_str())
            .unwrap_or("Unknown");

        let message = format!(
            "🚀 *Arbitrage Opportunity*\n\n\
            💰 *Symbol*: `{}`\n\
            📊 *Price Difference*: `${:.4}`\n\
            📈 *Percentage*: `{:.2}%`\n\
            🏪 *Exchanges*: {} ↔️ {}\n\n\
            ⏰ *Detected*: Just now",
            symbol, price_diff, percentage, exchange_a, exchange_b
        );

        Ok(message)
    }

    /// Format a balance message
    pub fn format_balance_message(&self, balances: &HashMap<String, f64>) -> ArbitrageResult<String> {
        let mut message = String::from("💳 *Account Balances*\n\n");

        if balances.is_empty() {
            message.push_str("No balances available");
            return Ok(message);
        }

        for (currency, amount) in balances {
            message.push_str(&format!("💰 *{}*: `{:.8}`\n", currency, amount));
        }

        message.push_str("\n⏰ *Last Updated*: Just now");

        Ok(message)
    }

    /// Format a user profile message
    pub fn format_user_profile_message(&self, profile: &Value) -> ArbitrageResult<String> {
        let username = profile
            .get("username")
            .and_then(|u| u.as_str())
            .unwrap_or("Unknown");

        let user_id = profile
            .get("user_id")
            .and_then(|u| u.as_str())
            .unwrap_or("Unknown");

        let role = profile
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("User");

        let created_at = profile
            .get("created_at")
            .and_then(|c| c.as_str())
            .unwrap_or("Unknown");

        let message = format!(
            "👤 *User Profile*\n\n\
            🏷️ *Username*: `{}`\n\
            🆔 *User ID*: `{}`\n\
            👑 *Role*: `{}`\n\
            📅 *Member Since*: `{}`\n\n\
            Use /settings to modify your preferences",
            username, user_id, role, created_at
        );

        Ok(message)
    }

    /// Format an error message
    pub fn format_error_message(&self, error: &str) -> String {
        format!("❌ *Error*\n\n{}\n\nPlease try again or contact support.", error)
    }

    /// Format a success message
    pub fn format_success_message(&self, message: &str) -> String {
        format!("✅ *Success*\n\n{}", message)
    }

    /// Format a warning message
    pub fn format_warning_message(&self, message: &str) -> String {
        format!("⚠️ *Warning*\n\n{}", message)
    }

    /// Format an info message
    pub fn format_info_message(&self, message: &str) -> String {
        format!("ℹ️ *Info*\n\n{}", message)
    }

    /// Format a help message
    pub fn format_help_message(&self) -> String {
        String::from(
            "🤖 *ArbEdge Bot Help*\n\n\
            *Available Commands:*\n\n\
            🚀 `/start` - Start the bot\n\
            ❓ `/help` - Show this help message\n\
            💰 `/opportunities` - View arbitrage opportunities\n\
            💳 `/balance` - Check account balances\n\
            ⚙️ `/settings` - User settings and preferences\n\
            👤 `/profile` - View your profile\n\
            📊 `/stats` - View trading statistics\n\
            👑 `/admin` - Admin commands (admin only)\n\n\
            *Quick Actions:*\n\
            • Send any message to get started\n\
            • Use inline buttons for easy navigation\n\
            • Set up notifications in settings\n\n\
            Need help? Contact support at @arbedge_support"
        )
    }

    /// Format admin help message
    pub fn format_admin_help_message(&self) -> String {
        String::from(
            "👑 *Admin Commands*\n\n\
            🔧 `/admin system` - System status\n\
            👥 `/admin users` - User management\n\
            📊 `/admin stats` - System statistics\n\
            ⚙️ `/admin config` - Configuration\n\
            🔄 `/admin restart` - Restart services\n\
            📝 `/admin logs` - View logs\n\
            🚨 `/admin alerts` - System alerts\n\
            💾 `/admin backup` - Backup data\n\n\
            *Monitoring:*\n\
            • Real-time system metrics\n\
            • User activity tracking\n\
            • Performance monitoring\n\
            • Error tracking and alerts"
        )
    }

    /// Escape markdown v2 special characters
    pub fn escape_markdown_v2(&self, text: &str) -> String {
        text.chars()
            .map(|c| match c {
                '_' | '*' | '[' | ']' | '(' | ')' | '~' | '`' | '>' | '#' | '+' | '-' | '=' | '|' | '{' | '}' | '.' | '!' => {
                    format!("\\{}", c)
                }
                _ => c.to_string(),
            })
            .collect()
    }

    /// Create inline keyboard button
    pub fn create_inline_button(&self, text: &str, callback_data: &str) -> Value {
        json!({
            "text": text,
            "callback_data": callback_data
        })
    }

    /// Create inline keyboard row
    pub fn create_inline_row(&self, buttons: Vec<Value>) -> Value {
        json!(buttons)
    }

    /// Create inline keyboard
    pub fn create_inline_keyboard(&self, rows: Vec<Value>) -> Value {
        json!(rows)
    }

    /// Create quick action keyboard for opportunities
    pub fn create_opportunities_keyboard(&self) -> Value {
        let row1 = self.create_inline_row(vec![
            self.create_inline_button("🔄 Refresh", "refresh_opportunities"),
            self.create_inline_button("⚙️ Settings", "opportunity_settings"),
        ]);

        let row2 = self.create_inline_row(vec![
            self.create_inline_button("📊 Analytics", "view_analytics"),
            self.create_inline_button("📈 History", "view_history"),
        ]);

        self.create_inline_keyboard(vec![row1, row2])
    }

    /// Create settings keyboard
    pub fn create_settings_keyboard(&self) -> Value {
        let row1 = self.create_inline_row(vec![
            self.create_inline_button("🔔 Notifications", "settings_notifications"),
            self.create_inline_button("🎨 Display", "settings_display"),
        ]);

        let row2 = self.create_inline_row(vec![
            self.create_inline_button("🚨 Alerts", "settings_alerts"),
            self.create_inline_button("📊 Dashboard", "settings_dashboard"),
        ]);

        let row3 = self.create_inline_row(vec![
            self.create_inline_button("🔙 Back", "main_menu"),
        ]);

        self.create_inline_keyboard(vec![row1, row2, row3])
    }

    /// Create admin keyboard
    pub fn create_admin_keyboard(&self) -> Value {
        let row1 = self.create_inline_row(vec![
            self.create_inline_button("🔧 System", "admin_system"),
            self.create_inline_button("👥 Users", "admin_users"),
        ]);

        let row2 = self.create_inline_row(vec![
            self.create_inline_button("📊 Stats", "admin_stats"),
            self.create_inline_button("⚙️ Config", "admin_config"),
        ]);

        let row3 = self.create_inline_row(vec![
            self.create_inline_button("📝 Logs", "admin_logs"),
            self.create_inline_button("🚨 Alerts", "admin_alerts"),
        ]);

        self.create_inline_keyboard(vec![row1, row2, row3])
    }

    /// Validate message length
    pub fn validate_message_length(&self, text: &str) -> ArbitrageResult<()> {
        const MAX_MESSAGE_LENGTH: usize = 4096;
        
        if text.len() > MAX_MESSAGE_LENGTH {
            return Err(ArbitrageError::validation_error(&format!(
                "Message too long: {} characters (max: {})",
                text.len(),
                MAX_MESSAGE_LENGTH
            )));
        }

        Ok(())
    }

    /// Truncate message if too long
    pub fn truncate_message(&self, text: &str, max_length: usize) -> String {
        if text.len() <= max_length {
            return text.to_string();
        }

        let truncated = &text[..max_length.saturating_sub(3)];
        format!("{}...", truncated)
    }

    /// Split long message into chunks
    pub fn split_message(&self, text: &str, max_length: usize) -> Vec<String> {
        if text.len() <= max_length {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for line in text.lines() {
            if current_chunk.len() + line.len() + 1 > max_length {
                if !current_chunk.is_empty() {
                    chunks.push(current_chunk);
                    current_chunk = String::new();
                }
                
                if line.len() > max_length {
                    // Split very long lines
                    let line_chunks = self.split_long_line(line, max_length);
                    chunks.extend(line_chunks);
                } else {
                    current_chunk = line.to_string();
                }
            } else {
                if !current_chunk.is_empty() {
                    current_chunk.push('\n');
                }
                current_chunk.push_str(line);
            }
        }

        if !current_chunk.is_empty() {
            chunks.push(current_chunk);
        }

        chunks
    }

    /// Split a very long line into chunks
    fn split_long_line(&self, line: &str, max_length: usize) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < line.len() {
            let end = (start + max_length).min(line.len());
            chunks.push(line[start..end].to_string());
            start = end;
        }

        chunks
    }
}

impl Default for MessageHandler {
    fn default() -> Self {
        Self::new()
    }
} 