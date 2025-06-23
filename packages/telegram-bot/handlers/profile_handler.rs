//! Profile Command Handler
//!
//! Handles the /profile command to show user's trading profile.

use crate::core::bot_client::TelegramResult;
use crate::core::command_router::{CommandContext, CommandHandler};
use crate::integrations::get_user_profile_data;
use async_trait::async_trait;
use worker::{console_log, Env};

pub struct ProfileHandler;

impl ProfileHandler {
    pub fn new() -> Self {
        Self
    }

    /// Fetches user profile and formats it into a response string.
    async fn get_profile_text(&self, env: &Env, user_id: &str) -> String {
        match get_user_profile_data(env, user_id).await {
            Ok(profile_data) => format!(
                "👤 *Your Profile*:\n\n
                - *User ID:* {}\n
                - *Telegram:* @{}\n
                - *Subscription:* {}\n
                - *Total Trades:* {}\n
                - *Total P&L:* ${:.2}\n
                - *Balance:* ${:.2}\n
                - *Status:* {}\n\n
                Use `/settings` to modify your preferences.",
                profile_data.user_id,
                profile_data
                    .telegram_username
                    .unwrap_or_else(|| "Not set".to_string()),
                profile_data.subscription_tier,
                profile_data.total_trades,
                profile_data.total_pnl_usdt,
                profile_data.account_balance_usdt,
                if profile_data.is_active {
                    "Active"
                } else {
                    "Inactive"
                }
            ),
            Err(e) => {
                console_log!("❌ Failed to get user profile for {}: {:?}", user_id, e);
                "❌ Unable to retrieve your profile. Please try again later.".to_string()
            }
        }
    }
}

#[async_trait]
impl CommandHandler for ProfileHandler {
    async fn handle(
        &self,
        _chat_id: i64,
        user_id: i64,
        _args: &[&str],
        context: &CommandContext,
    ) -> TelegramResult<String> {
        console_log!("👤 Processing /profile command for user {}", user_id);

        let env = context.env();
        let user_id_str = user_id.to_string();

        let response_text = self.get_profile_text(env, &user_id_str).await;

        Ok(response_text)
    }
}