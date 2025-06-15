// Cloudflare Queue Message Handlers
// Production implementation for consuming queue messages using #[event(queue)] macro

use worker::*;
use crate::services::core::infrastructure::cloudflare_queues::{
    OpportunityDistributionMessage, UserNotificationMessage, AnalyticsEventMessage, QueueEvent
};
use crate::utils::{ArbitrageResult, ArbitrageError};
use std::time::Duration;
use core::future::Future;
use reqwest;
use serde::Serialize;

/// Handle opportunity distribution queue messages
#[event(queue)]
pub async fn opportunity_queue_handler(
    message_batch: MessageBatch<OpportunityDistributionMessage>,
    env: Env,
    _ctx: Context,
) -> Result<()> {
    console_log!("Processing opportunity distribution batch: {} messages", message_batch.messages().len());

    // Initialize telegram service once for the entire batch
    let telegram_service = match initialize_telegram_service(&env).await {
        Ok(service) => Some(service),
        Err(e) => {
            console_error!("Failed to initialize telegram service: {}", e);
            None
        }
    };

    for message in message_batch.messages() {
        match process_opportunity_message(message.body(), &env, telegram_service.as_ref()).await {
            Ok(_) => {
                console_log!("Successfully processed opportunity message: {}", message.body().message_id);
                message.ack();
            }
            Err(e) => {
                console_error!("Failed to process opportunity message: {}", e);
                message.retry();
            }
        }
    }

    Ok(())
}

/// Handle user notification queue messages
#[event(queue)]
pub async fn notification_queue_handler(
    message_batch: MessageBatch<UserNotificationMessage>,
    env: Env,
    _ctx: Context,
) -> Result<()> {
    console_log!("Processing notification batch: {} messages", message_batch.messages().len());

    // Initialize telegram service once for the entire batch
    let telegram_service = match initialize_telegram_service(&env).await {
        Ok(service) => Some(service),
        Err(e) => {
            console_error!("Failed to initialize telegram service: {}", e);
            None
        }
    };

    for message in message_batch.messages() {
        match process_notification_message(message.body(), &env, telegram_service.as_ref()).await {
            Ok(_) => {
                console_log!("Successfully processed notification message: {}", message.body().message_id);
                message.ack();
            }
            Err(e) => {
                console_error!("Failed to process notification message: {}", e);
                message.retry();
            }
        }
    }

    Ok(())
}

/// Handle analytics event queue messages
#[event(queue)]
pub async fn analytics_queue_handler(
    message_batch: MessageBatch<AnalyticsEventMessage>,
    env: Env,
    _ctx: Context,
) -> Result<()> {
    console_log!("Processing analytics batch: {} messages", message_batch.messages().len());

    // Initialize analytics service once for the entire batch
    let analytics_service = match initialize_analytics_service(&env).await {
        Ok(service) => Some(service),
        Err(e) => {
            console_error!("Failed to initialize analytics service: {}", e);
            None
        }
    };

    for message in message_batch.messages() {
        match process_analytics_message(message.body(), &env, analytics_service.as_ref()).await {
            Ok(_) => {
                console_log!("Successfully processed analytics message: {}", message.body().event_id);
                message.ack();
            }
            Err(e) => {
                console_error!("Failed to process analytics message: {}", e);
                message.retry();
            }
        }
    }

    Ok(())
}

/// Generic helper function for retrying an asynchronous operation with exponential backoff.
async fn send_with_retry<F, Fut, T>(
    operation: F,
    max_retries: u32,
) -> ArbitrageResult<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = ArbitrageResult<T>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                // Simple retry for any error, consider more specific error checking for retry eligibility
                if attempt < max_retries {
                    attempt += 1;
                    // Safe exponential backoff with overflow protection and maximum cap
                    let base_delay = 100_u64;
                    let max_delay = 30000_u64; // Cap at 30 seconds
                    let delay_ms = base_delay
                        .checked_mul(2_u64.saturating_pow(attempt.saturating_sub(1)))
                        .unwrap_or(max_delay)
                        .min(max_delay);
                    console_log!("Operation failed. Retrying attempt {}/{} in {}ms. Error: {}", attempt, max_retries, delay_ms, e.to_string());
                    // In a worker environment, tokio::time::sleep might not be available.
                    // Using a promise-based delay for CF Workers.
                    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                        let win = web_sys::window().expect("should have a window in this context");
                        win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, delay_ms as i32)
                            .expect("should be able to set timeout");
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                } else {
                    console_error!("Operation failed after {} retries. Error: {}", max_retries, e.to_string());
                    return Err(e);
                }
            }
        }
    }
}

/// Process opportunity distribution message
async fn process_opportunity_message(
    message: &OpportunityDistributionMessage,
    _env: &Env,
    telegram_service: Option<&crate::services::interfaces::telegram::telegram::TelegramService>,
) -> ArbitrageResult<()> {
    // Use the pre-initialized telegram service
    let telegram_service = telegram_service.ok_or_else(|| {
        crate::utils::ArbitrageError::configuration_error(
            format!("Telegram service not available for message: {}", message.message_id)
        )
    })?;
    
    // Distribute opportunity to target users
    for user_id in &message.target_users {
        match message.distribution_strategy {
            crate::services::core::infrastructure::cloudflare_queues::DistributionStrategy::Broadcast => {
                // Send to all users
                let notification_text = format!(
                    "🚀 New Arbitrage Opportunity!\n\n\
                    Pair: {}\n\
                    Exchanges: {} ↔ {}\n\
                    Profit: {:.2}%\n\
                    Confidence: {:.1}%",
                    message.opportunity.pair,
                    message.opportunity.long_exchange,
                    message.opportunity.short_exchange,
                    message.opportunity.rate_difference * 100.0,
                    message.opportunity.confidence * 100.0
                );
                
                telegram_service.send_private_message(&notification_text, user_id).await?;
            }
            crate::services::core::infrastructure::cloudflare_queues::DistributionStrategy::RoundRobin => {
                let kv = _env.kv("ArbEdgeKV").map_err(|e| {
                    crate::utils::ArbitrageError::configuration_error(
                        format!("Failed to access KV store for round-robin distribution: {}", e)
                    )
                })?;
                
                let last_index_key = "distribution:roundrobin:last_user_index";
                let last_index: usize = match kv.get(last_index_key).text().await {
                    Ok(Some(index_str)) => index_str.parse().unwrap_or(0),
                    _ => 0,
                };
                
                let next_index = (last_index + 1) % message.target_users.len();
                let selected_user = &message.target_users[next_index];
                
                let notification_text = format!(
                    "🚀 New Arbitrage Opportunity!\n\n\
                    Pair: {}\n\
                    Exchanges: {} ↔ {}\n\
                    Profit: {:.2}%\n\
                    Confidence: {:.1}%",
                    message.opportunity.pair,
                    message.opportunity.long_exchange,
                    message.opportunity.short_exchange,
                    message.opportunity.rate_difference * 100.0,
                    message.opportunity.confidence * 100.0
                );
                
                telegram_service.send_private_message(&notification_text, selected_user).await?;
                
                kv.put(last_index_key, &next_index.to_string()).map_err(|e| {
                    crate::utils::ArbitrageError::storage_error(
                        format!("Failed to update round-robin index: {}", e)
                    )
                })?.execute().await.map_err(|e| {
                    crate::utils::ArbitrageError::storage_error(
                        format!("Failed to save round-robin index: {}", e)
                    )
                })?;
            }
            crate::services::core::infrastructure::cloudflare_queues::DistributionStrategy::PriorityBased => {
                let d1 = _env.d1("ARBITRAGE_DB").map_err(|e| {
                    crate::utils::ArbitrageError::configuration_error(
                        format!("Failed to access D1 database for priority-based distribution: {}", e)
                    )
                })?;
                
                let mut user_priorities: Vec<(String, i32)> = Vec::new();
                
                for user_id in &message.target_users {
                    let query = "SELECT subscription_tier, activity_score FROM user_profiles WHERE user_id = ?";
                    let stmt = d1.prepare(query);
                    let result = stmt.bind(&[user_id.into()]).map_err(|e| {
                        crate::utils::ArbitrageError::database_error(
                            format!("Failed to bind user query: {}", e)
                        )
                    })?.first::<serde_json::Value>(None).await.map_err(|e| {
                        crate::utils::ArbitrageError::database_error(
                            format!("Failed to query user priority data: {}", e)
                        )
                    })?;
                    
                    let priority = if let Some(data) = result {
                        let tier_priority = match data.get("subscription_tier").and_then(|v| v.as_str()) {
                            Some("premium") => 100,
                            Some("pro") => 50,
                            Some("basic") => 20,
                            _ => 10,
                        };
                        let activity_score = data.get("activity_score").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        tier_priority + (activity_score / 10)
                    } else {
                        10
                    };
                    
                    user_priorities.push((user_id.clone(), priority));
                }
                
                user_priorities.sort_by(|a, b| b.1.cmp(&a.1));
                
                let top_users = user_priorities.into_iter()
                    .take(std::cmp::min(5, message.target_users.len()))
                    .map(|(user_id, _)| user_id)
                    .collect::<Vec<_>>();
                
                let notification_text = format!(
                    "🚀 Priority Arbitrage Opportunity!\n\n\
                    Pair: {}\n\
                    Exchanges: {} ↔ {}\n\
                    Profit: {:.2}%\n\
                    Confidence: {:.1}%",
                    message.opportunity.pair,
                    message.opportunity.long_exchange,
                    message.opportunity.short_exchange,
                    message.opportunity.rate_difference * 100.0,
                    message.opportunity.confidence * 100.0
                );
                
                for user_id in &top_users {
                    telegram_service.send_private_message(&notification_text, user_id).await?;
                }
            }
            crate::services::core::infrastructure::cloudflare_queues::DistributionStrategy::GeographicBased => {
                let d1 = _env.d1("ARBITRAGE_DB").map_err(|e| {
                    crate::utils::ArbitrageError::configuration_error(
                        format!("Failed to access D1 database for geographic distribution: {}", e)
                    )
                })?;
                
                let current_hour = chrono::Utc::now().hour();
                let mut eligible_users = Vec::new();
                
                for user_id in &message.target_users {
                    let query = "SELECT timezone_offset, trading_hours_start, trading_hours_end FROM user_profiles WHERE user_id = ?";
                    let stmt = d1.prepare(query);
                    let result = stmt.bind(&[user_id.into()]).map_err(|e| {
                        crate::utils::ArbitrageError::database_error(
                            format!("Failed to bind geographic query: {}", e)
                        )
                    })?.first::<serde_json::Value>(None).await.map_err(|e| {
                        crate::utils::ArbitrageError::database_error(
                            format!("Failed to query user geographic data: {}", e)
                        )
                    })?;
                    
                    if let Some(data) = result {
                        let timezone_offset = data.get("timezone_offset").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let trading_start = data.get("trading_hours_start").and_then(|v| v.as_u64()).unwrap_or(9) as u32;
                        let trading_end = data.get("trading_hours_end").and_then(|v| v.as_u64()).unwrap_or(17) as u32;
                        
                        let user_local_hour = ((current_hour as i32 + timezone_offset).rem_euclid(24)) as u32;
                        
                        if user_local_hour >= trading_start && user_local_hour <= trading_end {
                            eligible_users.push(user_id.clone());
                        }
                    } else {
                        eligible_users.push(user_id.clone());
                    }
                }
                
                if eligible_users.is_empty() {
                    console_log!("No users in active trading hours for geographic distribution");
                    return Ok(());
                }
                
                let notification_text = format!(
                    "🌍 Regional Arbitrage Opportunity!\n\n\
                    Pair: {}\n\
                    Exchanges: {} ↔ {}\n\
                    Profit: {:.2}%\n\
                    Confidence: {:.1}%",
                    message.opportunity.pair,
                    message.opportunity.long_exchange,
                    message.opportunity.short_exchange,
                    message.opportunity.rate_difference * 100.0,
                    message.opportunity.confidence * 100.0
                );
                
                for user_id in &eligible_users {
                    telegram_service.send_private_message(&notification_text, user_id).await?;
                }
            }
        }
    }

    Ok(())
}

/// Process user notification message
async fn process_notification_message(
    message: &UserNotificationMessage,
    _env: &Env,
    telegram_service: Option<&crate::services::interfaces::telegram::telegram::TelegramService>,
) -> ArbitrageResult<()> {
    match message.delivery_method {
        crate::services::core::infrastructure::cloudflare_queues::DeliveryMethod::Telegram => {
            let telegram_service = telegram_service.ok_or_else(|| {
                crate::utils::ArbitrageError::configuration_error(
                    format!("Telegram service not available for message: {}", message.message_id)
                )
            })?;
            telegram_service.send_private_message(&message.content, &message.user_id).await?;
        }
        crate::services::core::infrastructure::cloudflare_queues::DeliveryMethod::Email => {
            let email_service = initialize_email_service(_env).await?;
            
            let d1 = _env.d1("ARBITRAGE_DB").map_err(|e| {
                crate::utils::ArbitrageError::configuration_error(
                    format!("Failed to access D1 database for email delivery: {}", e)
                )
            })?;
            
            let query = "SELECT email FROM user_profiles WHERE user_id = ?";
            let stmt = d1.prepare(query);
            let result = stmt.bind(&[message.user_id.clone().into()]).map_err(|e| {
                crate::utils::ArbitrageError::database_error(
                    format!("Failed to bind email query: {}", e)
                )
            })?.first::<serde_json::Value>(None).await.map_err(|e| {
                crate::utils::ArbitrageError::database_error(
                    format!("Failed to query user email: {}", e)
                )
            })?;
            
            if let Some(data) = result {
                if let Some(email) = data.get("email").and_then(|v| v.as_str()) {
                    email_service.send_email(email, "ArbEdge Notification", &message.content).await?;
                } else {
                    return Err(crate::utils::ArbitrageError::configuration_error(
                        format!("No email address found for user: {}", message.user_id)
                    ));
                }
            } else {
                return Err(crate::utils::ArbitrageError::configuration_error(
                    format!("User not found: {}", message.user_id)
                ));
            }
        }
        crate::services::core::infrastructure::cloudflare_queues::DeliveryMethod::WebPush => {
            let d1 = _env.d1("ARBITRAGE_DB").map_err(|e| {
                crate::utils::ArbitrageError::configuration_error(
                    format!("Failed to access D1 database for WebPush delivery: {}", e)
                )
            })?;
            
            let query = "SELECT webpush_endpoint, webpush_p256dh, webpush_auth FROM user_profiles WHERE user_id = ?";
            let stmt = d1.prepare(query);
            let result = stmt.bind(&[message.user_id.clone().into()]).map_err(|e| {
                crate::utils::ArbitrageError::database_error(
                    format!("Failed to bind WebPush query: {}", e)
                )
            })?.first::<serde_json::Value>(None).await.map_err(|e| {
                crate::utils::ArbitrageError::database_error(
                    format!("Failed to query user WebPush data: {}", e)
                )
            })?;
            
            if let Some(data) = result {
                let endpoint = data.get("webpush_endpoint").and_then(|v| v.as_str());
                let p256dh = data.get("webpush_p256dh").and_then(|v| v.as_str());
                let auth = data.get("webpush_auth").and_then(|v| v.as_str());
                
                if let (Some(endpoint), Some(p256dh), Some(auth)) = (endpoint, p256dh, auth) {
                    let webpush_service = initialize_webpush_service(_env).await?;
                    webpush_service.send_notification(endpoint, p256dh, auth, &message.content).await?;
                } else {
                    return Err(crate::utils::ArbitrageError::configuration_error(
                        format!("Incomplete WebPush subscription data for user: {}", message.user_id)
                    ));
                }
            } else {
                return Err(crate::utils::ArbitrageError::configuration_error(
                    format!("User not found: {}", message.user_id)
                ));
            }
        }
        crate::services::core::infrastructure::cloudflare_queues::DeliveryMethod::SMS => {
            let sms_service = initialize_sms_service(_env).await?;
            
            let d1 = _env.d1("ARBITRAGE_DB").map_err(|e| {
                crate::utils::ArbitrageError::configuration_error(
                    format!("Failed to access D1 database for SMS delivery: {}", e)
                )
            })?;
            
            let query = "SELECT phone_number FROM user_profiles WHERE user_id = ?";
            let stmt = d1.prepare(query);
            let result = stmt.bind(&[message.user_id.clone().into()]).map_err(|e| {
                crate::utils::ArbitrageError::database_error(
                    format!("Failed to bind SMS query: {}", e)
                )
            })?.first::<serde_json::Value>(None).await.map_err(|e| {
                crate::utils::ArbitrageError::database_error(
                    format!("Failed to query user phone number: {}", e)
                )
            })?;
            
            if let Some(data) = result {
                if let Some(phone) = data.get("phone_number").and_then(|v| v.as_str()) {
                    sms_service.send_sms(phone, &message.content).await?;
                } else {
                    return Err(crate::utils::ArbitrageError::configuration_error(
                        format!("No phone number found for user: {}", message.user_id)
                    ));
                }
            } else {
                return Err(crate::utils::ArbitrageError::configuration_error(
                    format!("User not found: {}", message.user_id)
                ));
            }
        }
    }

    Ok(())
}

/// Process analytics event message
async fn process_analytics_message(
    message: &AnalyticsEventMessage,
    _env: &Env,
    analytics_service: Option<&crate::services::core::infrastructure::analytics_engine::AnalyticsEngineClient>,
) -> ArbitrageResult<()> {
    // Use the pre-initialized analytics service
    let analytics_service = analytics_service.ok_or_else(|| {
        crate::utils::ArbitrageError::configuration_error(
            format!("Analytics service not available for message: {}", message.event_id)
        )
    })?;
    
    // Send event to Analytics Engine
    let event_data = serde_json::json!({
        "event_id": message.event_id,
        "event_type": message.event_type,
        "user_id": message.user_id,
        "timestamp": message.timestamp,
        "data": message.data
    });

    analytics_service.write_data_point(&[event_data]).await
        .map_err(|e| crate::utils::ArbitrageError::storage_error(format!("Analytics write failed: {}", e)))?;

    Ok(())
}

/// Initialize Telegram service from environment
async fn initialize_telegram_service(env: &Env) -> ArbitrageResult<crate::services::interfaces::telegram::telegram::TelegramService> {
    let bot_token = env.secret("TELEGRAM_BOT_TOKEN")
        .map_err(|e| crate::utils::ArbitrageError::configuration_error(
            format!("TELEGRAM_BOT_TOKEN secret not found or accessible: {}", e)
        ))?
        .to_string();
    
    let chat_id = env.var("TELEGRAM_CHAT_ID")
        .map_err(|e| crate::utils::ArbitrageError::configuration_error(
            format!("TELEGRAM_CHAT_ID environment variable not found: {}", e)
        ))?
        .to_string();

    let config = crate::services::interfaces::telegram::telegram::TelegramConfig {
        bot_token,
        chat_id,
        is_test_mode: env.var("TELEGRAM_TEST_MODE")
            .map(|v| match v.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => true,
                _ => false,
            })
            .unwrap_or(false),
    };

    Ok(crate::services::interfaces::telegram::telegram::TelegramService::new(config))
}

/// Initialize Analytics Engine service from environment
async fn initialize_analytics_service(env: &Env) -> ArbitrageResult<crate::services::core::infrastructure::analytics_engine::AnalyticsEngineClient> {
    let account_id = env.var("CLOUDFLARE_ACCOUNT_ID")
        .map_err(|e| crate::utils::ArbitrageError::configuration_error(
            format!("CLOUDFLARE_ACCOUNT_ID not found: {}", e)
        ))?
        .to_string();
    
    let api_token = env.secret("CLOUDFLARE_API_TOKEN")
        .map_err(|e| crate::utils::ArbitrageError::configuration_error(
            format!("CLOUDFLARE_API_TOKEN not found: {}", e)
        ))?
        .to_string();

    let dataset_name = env.var("ANALYTICS_DATASET_NAME")
        .unwrap_or_else(|_| "arbitrage_analytics".to_string())
        .to_string();

    Ok(crate::services::core::infrastructure::analytics_engine::AnalyticsEngineClient::new(
        account_id,
        api_token,
        dataset_name,
    ))
}

/// Initialize Email service from environment
async fn initialize_email_service(env: &Env) -> ArbitrageResult<EmailService> {
    let api_key = env.secret("EMAIL_API_KEY")?.to_string();
    let from_email = env.var("FROM_EMAIL")?.to_string();
    let api_url = env.var("EMAIL_API_URL")?.to_string();
    Ok(EmailService::new(api_key, from_email, api_url))
}

/// Initialize SMS service from environment
async fn initialize_sms_service(env: &Env) -> ArbitrageResult<SmsService> {
    let account_sid = env.secret("TWILIO_ACCOUNT_SID")?.to_string();
    let auth_token = env.secret("TWILIO_AUTH_TOKEN")?.to_string();
    let from_number = env.var("TWILIO_FROM_NUMBER")?.to_string();
    Ok(SmsService::new(account_sid, auth_token, from_number))
}

/// Initialize WebPush service from environment
async fn initialize_webpush_service(env: &Env) -> ArbitrageResult<WebPushService> {
    let vapid_private_key = env.secret("VAPID_PRIVATE_KEY")?.to_string();
    let vapid_public_key = env.var("VAPID_PUBLIC_KEY")?.to_string();
    Ok(WebPushService::new(vapid_private_key, vapid_public_key))
}

pub struct EmailService {
    api_key: String,
    from_email: String,
    api_url: String,
    client: reqwest::Client,
}

impl EmailService {
    pub fn new(api_key: String, from_email: String, api_url: String) -> Self {
        Self { api_key, from_email, api_url, client: reqwest::Client::new() }
    }
    
    pub async fn send_email(&self, to: &str, subject: &str, content: &str) -> ArbitrageResult<()> {
        let to_owned = to.to_string();
        let subject_owned = subject.to_string();
        let content_owned = content.to_string();

        send_with_retry(
            || async {
                console_log!("Attempting to send email to: {}", to_owned);
                let response = self.client.post(&self.api_url)
                    .header("X-Api-Key", &self.api_key)
                    .json(&serde_json::json!({
                        "to": to_owned,
                        "from": self.from_email,
                        "subject": subject_owned,
                        "body": content_owned
                    }))
                    .send()
                    .await
                    .map_err(|e| ArbitrageError::network_error(format!("Email request failed: {}", e)))?;

                if response.status().is_success() {
                    console_log!("Email sent successfully to: {}", to_owned);
                    Ok(())
                } else {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_else(|_| "Unknown error body".to_string());
                    console_error!("Failed to send email to {}: Status {}, Body: {}", to_owned, status, error_body);
                    Err(ArbitrageError::service_error(format!(
                        "Email service failed: {} - {}",
                        status,
                        error_body
                    )))
                }
            },
            3,
        )
        .await
    }
}

pub struct SmsService {
    account_sid: String,
    auth_token: String,
    from_number: String,
    client: reqwest::Client,
}

impl SmsService {
    pub fn new(account_sid: String, auth_token: String, from_number: String) -> Self {
        Self { account_sid, auth_token, from_number, client: reqwest::Client::new() }
    }
    
    pub async fn send_sms(&self, to: &str, content: &str) -> ArbitrageResult<()> {
        let to_owned = to.to_string();
        let content_owned = content.to_string();
        
        send_with_retry(
            || async {
                console_log!("Attempting to send SMS to: {}", to_owned);
                let twilio_url = format!("https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json", self.account_sid);
                let response = self.client.post(&twilio_url)
                    .basic_auth(&self.account_sid, Some(&self.auth_token))
                    .form(&[
                        ("To", to_owned.as_str()),
                        ("From", self.from_number.as_str()),
                        ("Body", content_owned.as_str()),
                    ])
                    .send()
                    .await
                    .map_err(|e| ArbitrageError::network_error(format!("SMS request failed: {}", e)))?;

                if response.status().is_success() {
                    console_log!("SMS sent successfully to: {}", to_owned);
                    Ok(())
                } else {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_else(|_| "Unknown error body".to_string());
                    console_error!("Failed to send SMS to {}: Status {}, Body: {}", to_owned, status, error_body);
                    Err(ArbitrageError::service_error(format!(
                        "SMS service failed: {} - {}",
                        status,
                        error_body
                    )))
                }
            },
            3,
        )
        .await
    }
}

pub struct WebPushService {
    _vapid_private_key: String,
    _vapid_public_key: String,
    client: reqwest::Client,
}

// CRITICAL: WebPush VAPID authentication not implemented
// This is a production-blocking issue that must be resolved before deployment.
// Missing implementations:
// 1. JWT token generation using VAPID private key (ES256 algorithm)
// 2. Authorization header: "vapid t=<jwt_token>, k=<public_key>"
// 3. Crypto-Key header for payload encryption
// 4. Payload encryption using p256dh and auth keys from subscription
// 5. Full RFC 8292 VAPID protocol compliance
// 
// Current implementation will be rejected by all major push services
// (FCM, Mozilla, Microsoft) due to missing authentication.
//
// REQUIRED DEPENDENCIES: jsonwebtoken, p256, aes-gcm (already available)
// ESTIMATED EFFORT: 4-6 hours for full implementation
// PRIORITY: HIGH - Required for production web push notifications

impl WebPushService {
    pub fn new(vapid_private_key: String, vapid_public_key: String) -> Self {
        Self { 
            _vapid_private_key: vapid_private_key, 
            _vapid_public_key: vapid_public_key, 
            client: reqwest::Client::new() 
        }
    }
    
    pub async fn send_notification(&self, endpoint: &str, _p256dh: &str, _auth: &str, content: &str) -> ArbitrageResult<()> {
        let endpoint_owned = endpoint.to_string();
        let content_owned = content.to_string();

        send_with_retry(
            || async {
                let short_endpoint = &endpoint_owned[..endpoint_owned.len().min(20)];
                console_log!("Attempting to send web push notification to endpoint starting with: {}", short_endpoint);
                
                let payload = serde_json::json!({
                    "title": "ArbEdge Notification",
                    "body": content_owned.clone(),
                    "icon": "/icon-192x192.png",
                    "badge": "/badge-72x72.png"
                });

                let response = self.client.post(&endpoint_owned)
                    .header("Content-Type", "application/json")
                    .header("TTL", "86400")
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| ArbitrageError::network_error(format!("Web push request failed: {}", e)))?;

                if response.status().is_success() {
                    console_log!("Web push notification sent successfully to endpoint starting with: {}", short_endpoint);
                    Ok(())
                } else {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_else(|_| "Unknown error body".to_string());
                    console_error!("Failed to send web push to {}: Status {}, Body: {}", short_endpoint, status, error_body);
                    Err(ArbitrageError::service_error(format!(
                        "Web push service failed: {} - {}",
                        status,
                        error_body
                    )))
                }
            },
            3,
        )
        .await
    }
}

async fn store_failed_notification_in_kv<T: Serialize>(
    env: &Env,
    message: &T,
    error: &str,
) -> ArbitrageResult<()> {
    let kv = env.kv("ArbEdgeKV").map_err(|e| {
        crate::utils::ArbitrageError::storage_error(format!("Failed to access KV store: {}", e))
    })?;

    // Generate unique key for failed notification
    let timestamp = chrono::Utc::now().timestamp_millis();
    let unique_id = format!("{}_{}", timestamp, uuid::Uuid::new_v4().to_string()[..8].to_string());
    let failed_key = format!("failed_notification:{}", unique_id);

    // Serialize the message
    let serialized_message = serde_json::to_string(message).map_err(|e| {
        crate::utils::ArbitrageError::serialization_error(format!("Failed to serialize message: {}", e))
    })?;

    // Create failed notification entry
    let failed_entry = serde_json::json!({
        "id": unique_id,
        "timestamp": timestamp,
        "error": error,
        "message": serde_json::from_str::<serde_json::Value>(&serialized_message).unwrap_or(serde_json::json!({})),
        "retry_count": 0,
        "max_retries": 3,
        "next_retry_at": timestamp + (5 * 60 * 1000), // Retry in 5 minutes
        "status": "failed",
        "created_at": chrono::Utc::now().to_rfc3339()
    });

    // Store in KV
    kv.put(&failed_key, &failed_entry.to_string())?
        .execute()
        .await
        .map_err(|e| {
            crate::utils::ArbitrageError::storage_error(format!("Failed to store failed notification: {}", e))
        })?;

    // Update failed notification index for easier retrieval
    let index_key = "failed_notifications:index";
    let mut notification_ids = Vec::new();
    
    // Get existing index
    if let Ok(Some(existing_index)) = kv.get(index_key).text().await {
        if let Ok(existing_ids) = serde_json::from_str::<Vec<String>>(&existing_index) {
            notification_ids = existing_ids;
        }
    }
    
    // Add new failed notification ID
    notification_ids.push(unique_id.clone());
    
    // Keep only last 100 failed notifications in index
    if notification_ids.len() > 100 {
        // Remove oldest entries from KV and index
        let to_remove = notification_ids.len() - 100;
        for old_id in notification_ids.drain(0..to_remove) {
            let old_key = format!("failed_notification:{}", old_id);
            if let Err(e) = kv.delete(&old_key).await {
                console_log!("⚠️ Failed to cleanup old failed notification {}: {:?}", old_key, e);
            }
        }
    }
    
    // Update index
    if let Err(e) = kv.put(index_key, &serde_json::to_string(&notification_ids)?)?
        .execute()
        .await {
        console_log!("⚠️ Failed to update failed notifications index: {:?}", e);
    }

    // Update metrics
    let metrics_key = "failed_notifications:metrics";
    let mut metrics = if let Ok(Some(existing_metrics)) = kv.get(metrics_key).text().await {
        serde_json::from_str::<serde_json::Value>(&existing_metrics).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Update failure counts
    let current_count = metrics.get("total_failures").and_then(|v| v.as_u64()).unwrap_or(0);
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let daily_count = metrics.get("daily_failures").and_then(|d| d.get(&today)).and_then(|v| v.as_u64()).unwrap_or(0);
    
    metrics["total_failures"] = serde_json::json!(current_count + 1);
    metrics["last_failure"] = serde_json::json!(timestamp);
    
    if let Some(daily_failures) = metrics.get_mut("daily_failures") {
        daily_failures[&today] = serde_json::json!(daily_count + 1);
    } else {
        metrics["daily_failures"] = serde_json::json!({ today: daily_count + 1 });
    }

    if let Err(e) = kv.put(metrics_key, &metrics.to_string())?.execute().await {
        console_log!("⚠️ Failed to update failed notification metrics: {:?}", e);
    }

    console_log!(
        "📋 Stored failed notification {} in KV: {}",
        unique_id,
        error
    );

    Ok(())
}