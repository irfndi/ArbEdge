use worker::*;

// Module declarations
pub mod handlers;
pub mod middleware;
pub mod responses;
pub mod services;
pub mod types;
pub mod utils;

use once_cell::sync::OnceCell;
use services::core::infrastructure::d1_database::D1Service;
use services::core::infrastructure::service_container::ServiceContainer;
use services::core::opportunities::opportunity::OpportunityServiceConfig;
use services::core::opportunities::OpportunityService;
use services::core::trading::exchange::{ExchangeInterface, ExchangeService};
use services::core::trading::positions::{
    CreatePositionData, ProductionPositionsService, UpdatePositionData,
};
use services::core::user::user_profile::UserProfileService;
use services::interfaces::telegram::telegram::{TelegramConfig, TelegramService};

// Import new modular components
use handlers::*;

use std::sync::Arc;
use types::ExchangeIdEnum;
use utils::{ArbitrageError, ArbitrageResult};

#[cfg(target_arch = "wasm32")]
use worker::console_log;

#[cfg(not(target_arch = "wasm32"))]
macro_rules! console_log {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}

static SERVICE_CONTAINER: OnceCell<Arc<ServiceContainer>> = OnceCell::new();

async fn get_service_container(env: &Env) -> Result<Arc<ServiceContainer>> {
    if let Some(container) = SERVICE_CONTAINER.get() {
        return Ok(container.clone());
    }

    let d1_service = D1Service::new(env)?;
    let kv_store = env.kv("ArbEdgeKV")?;
    let encryption_key = env
        .var("ENCRYPTION_KEY")
        .map_err(|_| worker::Error::RustError("Missing ENCRYPTION_KEY".to_string()))?
        .to_string();

    let _user_profile_service =
        UserProfileService::new(kv_store.clone(), d1_service, encryption_key);

    let _telegram_service = TelegramService::new(TelegramConfig {
        bot_token: env
            .var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| worker::Error::RustError("Missing TELEGRAM_BOT_TOKEN".to_string()))?
            .to_string(),
        chat_id: env
            .var("TELEGRAM_CHAT_ID")
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "".to_string()),
        is_test_mode: env
            .var("TELEGRAM_TEST_MODE")
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "false".to_string())
            == "true",
    });

    let _exchange_service = ExchangeService::new(&types::Env::new(env.clone()))?;
    let _positions_service = ProductionPositionsService::new(kv_store.clone());

    let container = Arc::new(ServiceContainer::new(env, kv_store.clone())?);

    SERVICE_CONTAINER
        .set(container.clone())
        .map_err(|_| worker::Error::RustError("Failed to set service container".to_string()))?;

    Ok(container)
}

#[durable_object]
pub struct PositionsManager {
    _state: State,
}

#[durable_object]
impl DurableObject for PositionsManager {
    fn new(state: State, _env: Env) -> Self {
        Self { _state: state }
    }

    async fn fetch(&mut self, _req: Request) -> Result<Response> {
        Response::ok("Positions Manager")
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    utils::logger::set_panic_hook();

    let url = req.url()?;
    let path = url.path();
    let method = req.method();

    console_log!("🌐 Request: {} {}", method, path);

    // CORS headers for all responses
    let mut cors_headers = worker::Headers::new();
    cors_headers.set("Access-Control-Allow-Origin", "*")?;
    cors_headers.set(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, DELETE, OPTIONS",
    )?;
    cors_headers.set(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization, X-User-ID",
    )?;

    // Handle preflight requests
    if method == Method::Options {
        return Ok(Response::empty()?.with_headers(cors_headers));
    }

    let mut response = match (method.clone(), path) {
        // Health endpoints
        (Method::Get, "/health") => handle_api_health_check(req, env).await,
        (Method::Get, "/health/detailed") => handle_api_detailed_health_check(req, env).await,

        // User management endpoints
        (Method::Get, "/api/v1/user/profile") => handle_api_get_user_profile(req, env).await,
        (Method::Put, "/api/v1/user/profile") => handle_api_update_user_profile(req, env).await,
        (Method::Get, "/api/v1/user/preferences") => {
            handle_api_get_user_preferences(req, env).await
        }
        (Method::Put, "/api/v1/user/preferences") => {
            handle_api_update_user_preferences(req, env).await
        }

        // Analytics endpoints
        (Method::Get, "/api/v1/analytics/dashboard") => {
            handle_api_get_dashboard_analytics(req, env).await
        }

        // Admin endpoints
        (Method::Get, "/api/v1/admin/users") => handle_api_admin_get_users(req, env).await,

        // Trading endpoints
        (Method::Get, "/api/v1/trading/balance") => handle_api_get_trading_balance(req, env).await,

        // AI endpoints
        (Method::Post, "/api/v1/ai/analyze") => handle_api_ai_analyze(req, env).await,

        // Legacy endpoints (keep for backward compatibility)
        (Method::Get, "/markets") => handle_get_markets(req, env).await,
        (Method::Get, "/ticker") => handle_get_ticker(req, env).await,
        (Method::Get, "/funding-rate") => handle_funding_rate(req, env).await,
        (Method::Get, "/orderbook") => handle_get_orderbook(req, env).await,
        (Method::Post, "/find-opportunities") => handle_find_opportunities(req, env).await,
        (Method::Post, "/telegram/webhook") => handle_telegram_webhook(req, env).await,
        (Method::Post, "/positions") => handle_create_position(req, env).await,
        (Method::Get, "/positions") => handle_get_all_positions(req, env).await,
        (Method::Get, path) if path.starts_with("/positions/") => {
            let id = path.strip_prefix("/positions/").unwrap_or("");
            handle_get_position(req, env, id).await
        }
        (Method::Put, path) if path.starts_with("/positions/") => {
            let id = path.strip_prefix("/positions/").unwrap_or("");
            handle_update_position(req, env, id).await
        }
        (Method::Delete, path) if path.starts_with("/positions/") => {
            let id = path.strip_prefix("/positions/").unwrap_or("");
            handle_close_position(req, env, id).await
        }

        _ => {
            console_log!("❌ Route not found: {} {}", method, path);
            Response::error("Not Found", 404)
        }
    };

    // Add CORS headers to response
    if let Ok(ref mut resp) = response {
        let headers = resp.headers_mut();
        headers.set("Access-Control-Allow-Origin", "*")?;
        headers.set(
            "Access-Control-Allow-Methods",
            "GET, POST, PUT, DELETE, OPTIONS",
        )?;
        headers.set(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization, X-User-ID",
        )?;
    }

    response
}

#[event(scheduled)]
pub async fn scheduled(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    console_log!("🕐 Scheduled event triggered: {:?}", event.cron());

    if let Err(e) = monitor_opportunities_scheduled(env).await {
        console_log!("❌ Scheduled monitoring failed: {:?}", e);
    }
}

fn parse_exchanges_from_env(
    exchanges_str: &str,
) -> std::result::Result<Vec<ExchangeIdEnum>, ArbitrageError> {
    exchanges_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| match s.to_lowercase().as_str() {
            "binance" => Ok(ExchangeIdEnum::Binance),
            "bybit" => Ok(ExchangeIdEnum::Bybit),
            "okx" => Ok(ExchangeIdEnum::OKX),
            "bitget" => Ok(ExchangeIdEnum::Bitget),
            _ => Err(ArbitrageError::validation_error(format!(
                "Unsupported exchange: {}",
                s
            ))),
        })
        .collect()
}

async fn create_opportunity_service(
    custom_env: &types::Env,
) -> ArbitrageResult<OpportunityService> {
    let config = OpportunityServiceConfig {
        exchanges: parse_exchanges_from_env("binance,bybit")?,
        monitored_pairs: vec![], // Empty for now, will be populated as needed
        threshold: 0.01,
    };

    let exchange_service = Arc::new(ExchangeService::new(custom_env)?);

    Ok(OpportunityService::new(
        config,
        exchange_service,
        None, // No telegram service for now
    ))
}

// Legacy handlers (keep for backward compatibility)
async fn handle_get_markets(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let exchange = url
        .query_pairs()
        .find(|(key, _)| key == "exchange")
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| "binance".to_string());

    let exchange_enum = match exchange.to_lowercase().as_str() {
        "binance" => ExchangeIdEnum::Binance,
        "bybit" => ExchangeIdEnum::Bybit,
        "okx" => ExchangeIdEnum::OKX,
        "bitget" => ExchangeIdEnum::Bitget,
        _ => return Response::error("Unsupported exchange", 400),
    };

    let exchange_service = ExchangeService::new(&types::Env::new(env.clone()))?;
    match exchange_service
        .get_markets(&exchange_enum.to_string())
        .await
    {
        Ok(markets) => Response::from_json(&markets),
        Err(e) => Response::error(format!("Failed to get markets: {:?}", e), 500),
    }
}

async fn handle_get_ticker(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let query_pairs: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let exchange = query_pairs
        .get("exchange")
        .cloned()
        .unwrap_or_else(|| "binance".to_string());
    let symbol = query_pairs
        .get("symbol")
        .cloned()
        .unwrap_or_else(|| "BTCUSDT".to_string());

    let exchange_enum = match exchange.to_lowercase().as_str() {
        "binance" => ExchangeIdEnum::Binance,
        "bybit" => ExchangeIdEnum::Bybit,
        "okx" => ExchangeIdEnum::OKX,
        "bitget" => ExchangeIdEnum::Bitget,
        _ => return Response::error("Unsupported exchange", 400),
    };

    let exchange_service = ExchangeService::new(&types::Env::new(env.clone()))?;
    match exchange_service
        .get_ticker(&exchange_enum.to_string(), &symbol)
        .await
    {
        Ok(ticker) => Response::from_json(&ticker),
        Err(e) => Response::error(format!("Failed to get ticker: {:?}", e), 500),
    }
}

async fn handle_funding_rate(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let query_pairs: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let exchange = query_pairs
        .get("exchange")
        .cloned()
        .unwrap_or_else(|| "binance".to_string());
    let symbol = query_pairs
        .get("symbol")
        .cloned()
        .unwrap_or_else(|| "BTCUSDT".to_string());

    let exchange_enum = match exchange.to_lowercase().as_str() {
        "binance" => ExchangeIdEnum::Binance,
        "bybit" => ExchangeIdEnum::Bybit,
        "okx" => ExchangeIdEnum::OKX,
        "bitget" => ExchangeIdEnum::Bitget,
        _ => return Response::error("Unsupported exchange", 400),
    };

    let exchange_service = ExchangeService::new(&types::Env::new(env.clone()))?;
    match exchange_service
        .fetch_funding_rates(&exchange_enum.to_string(), Some(&symbol))
        .await
    {
        Ok(funding_rates) => Response::from_json(&funding_rates),
        Err(e) => Response::error(format!("Failed to get funding rate: {:?}", e), 500),
    }
}

async fn handle_get_orderbook(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let query_pairs: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let exchange = query_pairs
        .get("exchange")
        .cloned()
        .unwrap_or_else(|| "binance".to_string());
    let symbol = query_pairs
        .get("symbol")
        .cloned()
        .unwrap_or_else(|| "BTCUSDT".to_string());

    let exchange_enum = match exchange.to_lowercase().as_str() {
        "binance" => ExchangeIdEnum::Binance,
        "bybit" => ExchangeIdEnum::Bybit,
        "okx" => ExchangeIdEnum::OKX,
        "bitget" => ExchangeIdEnum::Bitget,
        _ => return Response::error("Unsupported exchange", 400),
    };

    let exchange_service = ExchangeService::new(&types::Env::new(env.clone()))?;
    match exchange_service
        .get_orderbook(&exchange_enum.to_string(), &symbol, None)
        .await
    {
        Ok(orderbook) => Response::from_json(&orderbook),
        Err(e) => Response::error(format!("Failed to get orderbook: {:?}", e), 500),
    }
}

async fn handle_find_opportunities(mut req: Request, env: Env) -> Result<Response> {
    let body: serde_json::Value = req.json().await?;
    let pairs = body["pairs"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_else(|| vec!["BTCUSDT".to_string()]);

    let custom_env = types::Env::new(env);
    let opportunity_service = create_opportunity_service(&custom_env).await?;

    // Default exchanges and threshold
    let exchanges = vec![ExchangeIdEnum::Binance, ExchangeIdEnum::Bybit];
    let threshold = 0.01;

    match opportunity_service
        .find_opportunities(&exchanges, &pairs, threshold)
        .await
    {
        Ok(opportunities) => Response::from_json(&opportunities),
        Err(e) => Response::error(format!("Failed to find opportunities: {:?}", e), 500),
    }
}

async fn handle_telegram_webhook(mut req: Request, env: Env) -> Result<Response> {
    let update: serde_json::Value = req.json().await?;
    console_log!("📱 Telegram webhook received: {}", update);

    let container = get_service_container(&env).await?;

    match container.telegram_service() {
        Some(telegram_service) => match telegram_service.handle_webhook(update).await {
            Ok(response) => {
                console_log!("✅ Telegram update processed successfully");
                match response {
                    Some(msg) => Response::from_json(&msg),
                    None => Response::ok("OK"),
                }
            }
            Err(e) => {
                console_log!("❌ Failed to process Telegram update: {:?}", e);
                Response::error(format!("Failed to process update: {:?}", e), 500)
            }
        },
        None => {
            console_log!("⚠️ Telegram service not configured");
            Response::error("Telegram service not available", 503)
        }
    }
}

async fn handle_create_position(mut req: Request, _env: Env) -> Result<Response> {
    let _position_data: CreatePositionData = req.json().await?;
    console_log!("📊 Position creation not implemented yet");
    Response::error("Position management not implemented", 501)
}

async fn handle_get_all_positions(_req: Request, _env: Env) -> Result<Response> {
    console_log!("📊 Position listing not implemented yet");
    Response::error("Position management not implemented", 501)
}

async fn handle_get_position(_req: Request, _env: Env, _id: &str) -> Result<Response> {
    console_log!("📊 Position retrieval not implemented yet");
    Response::error("Position management not implemented", 501)
}

async fn handle_update_position(mut req: Request, _env: Env, _id: &str) -> Result<Response> {
    let _update_data: UpdatePositionData = req.json().await?;
    console_log!("📊 Position update not implemented yet");
    Response::error("Position management not implemented", 501)
}

async fn handle_close_position(_req: Request, _env: Env, _id: &str) -> Result<Response> {
    console_log!("📊 Position closure not implemented yet");
    Response::error("Position management not implemented", 501)
}

async fn run_five_minute_maintenance(
    env: &Env,
    _opportunity_service: &OpportunityService,
) -> ArbitrageResult<()> {
    console_log!("🔧 Running 5-minute maintenance tasks...");

    // 1. Clean up expired opportunities - placeholder
    console_log!("🧹 Cleanup expired opportunities - not implemented yet");

    // 2. Update distribution statistics - placeholder
    console_log!("📊 Update distribution statistics - not implemented yet");

    // 3. Process pending opportunity distributions - placeholder
    console_log!("📤 Process pending distributions - not implemented yet");

    // 4. Update user activity metrics - placeholder
    console_log!("👥 Update user activity metrics - not implemented yet");

    // 5. Cleanup inactive user sessions
    if let Ok(kv_store) = env.kv("ArbEdgeKV") {
        if let Ok(d1_service) = D1Service::new(env) {
            if let Ok(encryption_key) = env.var("ENCRYPTION_KEY") {
                let user_profile_service =
                    UserProfileService::new(kv_store, d1_service, encryption_key.to_string());

                console_log!("🧹 Cleanup expired sessions - not implemented yet");
                // Note: cleanup_expired_sessions method doesn't exist yet
                let _ = user_profile_service;
            }
        }
    }

    console_log!("✅ 5-minute maintenance completed");
    Ok(())
}

async fn monitor_opportunities_scheduled(env: Env) -> ArbitrageResult<()> {
    console_log!("🔄 Starting scheduled opportunity monitoring...");

    // Create custom environment for opportunity service
    let custom_env = types::Env::new(env.clone());

    // Create opportunity service
    let opportunity_service = create_opportunity_service(&custom_env).await?;

    // Run maintenance
    run_five_minute_maintenance(&env, &opportunity_service).await?;

    console_log!("✅ Scheduled opportunity monitoring completed successfully");
    Ok(())
}
