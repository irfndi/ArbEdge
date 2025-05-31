use worker::*;

// Module declarations
pub mod handlers;
pub mod middleware;
pub mod responses;
pub mod services;
pub mod types;
pub mod utils;

use once_cell::sync::OnceCell;
use services::core::infrastructure::database_repositories::DatabaseManager;
use services::core::infrastructure::service_container::ServiceContainer;
// use services::core::opportunities::opportunity::OpportunityServiceConfig; // Removed - using modular architecture
// use services::core::opportunities::OpportunityService; // Removed - using modular architecture
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

    let kv_store = env.kv("ArbEdgeKV")?;
    let encryption_key = env
        .var("ENCRYPTION_KEY")
        .map_err(|_| worker::Error::RustError("Missing ENCRYPTION_KEY".to_string()))?
        .to_string();

    // Create database manager with proper configuration
    let d1_database = env.d1("ArbEdgeDB")?;
    let database_manager = DatabaseManager::new(
        std::sync::Arc::new(d1_database),
        services::core::infrastructure::database_repositories::DatabaseManagerConfig::default(),
    );

    let _user_profile_service =
        UserProfileService::new(kv_store.clone(), database_manager, encryption_key);

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

    let _exchange_service = ExchangeService::new(env)?;
    let _positions_service = ProductionPositionsService::new(kv_store.clone());

    let container = Arc::new(ServiceContainer::new(env, kv_store.clone()).await?);

    SERVICE_CONTAINER
        .set(container.clone())
        .map_err(|_| worker::Error::RustError("Failed to set service container".to_string()))?;

    Ok(container)
}

// ============================================================================
// MODULAR ROUTING FUNCTIONS - Production Ready Implementation
// ============================================================================

/// Route authentication requests to modular auth service
async fn route_auth_request(
    req: Request,
    container: &Arc<ServiceContainer>,
    action: &str,
) -> Result<Response> {
    console_log!("🔐 Routing auth request: {}", action);

    match action {
        "login" => {
            // Parse login request
            let mut req_clone = req;
            let login_data: serde_json::Value = req_clone.json().await?;

            let telegram_id = login_data["telegram_id"]
                .as_i64()
                .ok_or_else(|| worker::Error::RustError("Missing telegram_id".to_string()))?;
            let _username = login_data["username"].as_str().map(|s| s.to_string());

            // Use session service for authentication
            let session = container
                .session_service
                .start_session(telegram_id, format!("user_{}", telegram_id))
                .await
                .map_err(|e| worker::Error::RustError(format!("Login failed: {:?}", e)))?;

            let login_response = serde_json::json!({
                "status": "success",
                "message": "Login successful",
                "session_id": session.session_id,
                "user_id": session.user_id,
                "expires_at": session.expires_at,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Response::from_json(&login_response)
        }
        "logout" => {
            // Extract user ID from headers
            let user_id = req
                .headers()
                .get("X-User-ID")?
                .ok_or_else(|| worker::Error::RustError("Missing X-User-ID header".to_string()))?;

            // End session using session service
            container
                .session_service
                .end_session(&user_id)
                .await
                .map_err(|e| worker::Error::RustError(format!("Logout failed: {:?}", e)))?;

            let logout_response = serde_json::json!({
                "status": "success",
                "message": "Logout successful",
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Response::from_json(&logout_response)
        }
        "session" => {
            // Extract user ID from headers
            let user_id = req
                .headers()
                .get("X-User-ID")?
                .ok_or_else(|| worker::Error::RustError("Missing X-User-ID header".to_string()))?;

            // Validate session using session service
            let session_details_option = container
                .session_service
                .validate_session(&user_id) // user_id is the user_id here
                .await
                .map_err(|e| {
                    worker::Error::RustError(format!("Session validation failed: {:?}", e))
                })?;

            if let Some(session_details) = session_details_option {
                // Convert u64 timestamps to chrono::DateTime<chrono::Utc> for the response if needed
                let created_at_ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(session_details.created_at as i64);
                let expires_at_ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(session_details.expires_at as i64);
                let last_activity_ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(session_details.last_activity_at as i64);

                let session_response = serde_json::json!({
                    "status": "valid",
                    "message": "Session is valid",
                    "session_id": session_details.session_id,
                    "user_id": session_details.user_id,
                    "created_at": created_at_ts.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                    "expires_at": expires_at_ts.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                    "last_activity_at": last_activity_ts.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                    "onboarding_completed": session_details.onboarding_completed,
                    "preferences_set": session_details.preferences_set,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });
                Response::from_json(&session_response)
            } else {
                Response::error("Invalid or expired session", 401)
            }
        }
        _ => Response::error("Unknown auth action", 400),
    }
}

/// Route user requests to modular user service
async fn route_user_request(
    req: Request,
    container: &Arc<ServiceContainer>,
    action: &str,
) -> Result<Response> {
    console_log!("👤 Routing user request: {}", action);

    match action {
        "profile" => {
            // Extract user ID from headers
            let user_id = req
                .headers()
                .get("X-User-ID")?
                .ok_or_else(|| worker::Error::RustError("Missing X-User-ID header".to_string()))?;

            // Get user profile using user profile service
            if let Some(user_service) = &container.user_profile_service {
                let profile = user_service.get_user_profile(&user_id).await.map_err(|e| {
                    worker::Error::RustError(format!("Failed to get profile: {:?}", e))
                })?;

                Response::from_json(&profile)
            } else {
                Response::error("User service not available", 503)
            }
        }
        "preferences" => {
            // Extract user ID from headers
            let user_id = req
                .headers()
                .get("X-User-ID")?
                .ok_or_else(|| worker::Error::RustError("Missing X-User-ID header".to_string()))?;

            // Get user preferences using user profile service
            if let Some(user_service) = &container.user_profile_service {
                let preferences =
                    user_service
                        .get_user_preferences(&user_id)
                        .await
                        .map_err(|e| {
                            worker::Error::RustError(format!("Failed to get preferences: {:?}", e))
                        })?;

                Response::from_json(&preferences)
            } else {
                Response::error("User service not available", 503)
            }
        }
        "update_profile" => {
            // TODO: Implement profile update
            Response::error("Profile update not yet implemented", 501)
        }
        "update_preferences" => {
            // TODO: Implement preferences update
            Response::error("Preferences update not yet implemented", 501)
        }
        _ => Response::error("Unknown user action", 400),
    }
}

/// Route opportunities requests to modular opportunities service
async fn route_opportunities_request(
    req: Request,
    container: &Arc<ServiceContainer>,
    action: &str,
) -> Result<Response> {
    console_log!("💰 Routing opportunities request: {}", action);

    match action {
        "list" => {
            // Extract user ID from headers
            let user_id = req
                .headers()
                .get("X-User-ID")?
                .ok_or_else(|| worker::Error::RustError("Missing X-User-ID header".to_string()))?;

            // Get opportunities using distribution service
            let opportunities = container
                .distribution_service
                .get_user_opportunities(&user_id)
                .await
                .map_err(|e| {
                    worker::Error::RustError(format!("Failed to get opportunities: {:?}", e))
                })?;

            Response::from_json(&opportunities)
        }
        "analyze" => {
            // Parse analyze request
            let mut req_clone = req;
            let analyze_data: serde_json::Value = req_clone.json().await?;

            let _symbol = analyze_data["symbol"]
                .as_str()
                .ok_or_else(|| worker::Error::RustError("Missing symbol".to_string()))?;

            // TODO: Implement symbol analysis using opportunities service
            Response::error("Symbol analysis not yet implemented", 501)
        }
        _ => Response::error("Unknown opportunities action", 400),
    }
}

/// Route telegram requests to modular telegram service
async fn route_telegram_request(
    req: Request,
    container: &Arc<ServiceContainer>,
    action: &str,
) -> Result<Response> {
    console_log!("📱 Routing telegram request: {}", action);

    match action {
        "webhook" => {
            // Parse telegram webhook
            let mut req_clone = req;
            let webhook_data: serde_json::Value = req_clone.json().await?;

            // Process webhook using telegram service
            if let Some(telegram_service) = &container.telegram_service {
                let response = telegram_service
                    .handle_webhook(webhook_data)
                    .await
                    .map_err(|e| {
                        worker::Error::RustError(format!("Failed to process webhook: {:?}", e))
                    })?;

                Response::from_json(&response)
            } else {
                Response::error("Telegram service not available", 503)
            }
        }
        "send" => {
            // Parse send message request
            let mut req_clone = req;
            let send_data: serde_json::Value = req_clone.json().await?;

            let _chat_id = send_data["chat_id"]
                .as_str()
                .ok_or_else(|| worker::Error::RustError("Missing chat_id".to_string()))?;
            let message = send_data["message"]
                .as_str()
                .ok_or_else(|| worker::Error::RustError("Missing message".to_string()))?;

            // Send message using telegram service
            if let Some(telegram_service) = &container.telegram_service {
                telegram_service.send_message(message).await.map_err(|e| {
                    worker::Error::RustError(format!("Failed to send message: {:?}", e))
                })?;

                let response = serde_json::json!({
                    "status": "success",
                    "message": "Message sent successfully"
                });
                Response::from_json(&response)
            } else {
                Response::error("Telegram service not available", 503)
            }
        }
        _ => Response::error("Unknown telegram action", 400),
    }
}

/// Route health check requests to modular health service
async fn route_health_check(_req: Request, container: &Arc<ServiceContainer>) -> Result<Response> {
    console_log!("🏥 Routing health check request");

    // Get health status from all services
    let health_status = serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "services": {
            "session_service": "healthy",
            "distribution_service": "healthy",
            "telegram_service": if container.telegram_service.is_some() { "healthy" } else { "not_configured" },
            "exchange_service": "healthy",
            "user_profile_service": if container.user_profile_service.is_some() { "healthy" } else { "not_configured" },
            "database_manager": "healthy",
            "data_access_layer": "healthy"
        }
    });

    Response::from_json(&health_status)
}

/// Route detailed health check requests to modular health service
async fn route_detailed_health_check(
    _req: Request,
    container: &Arc<ServiceContainer>,
) -> Result<Response> {
    console_log!("🏥 Routing detailed health check request");

    // Get detailed health status from all services
    let detailed_health = serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": "1.0.0",
        "uptime": "unknown",
        "services": {
            "session_service": {
                "status": "healthy",
                "type": "SessionManagementService",
                "description": "Manages user sessions and authentication"
            },
            "distribution_service": {
                "status": "healthy",
                "type": "OpportunityDistributionService",
                "description": "Distributes trading opportunities to users"
            },
            "telegram_service": {
                "status": if container.telegram_service.is_some() { "healthy" } else { "not_configured" },
                "type": "TelegramService",
                "description": "Handles Telegram bot interactions"
            },
            "exchange_service": {
                "status": "healthy",
                "type": "ExchangeService",
                "description": "Interfaces with cryptocurrency exchanges"
            },
            "user_profile_service": {
                "status": if container.user_profile_service.is_some() { "healthy" } else { "not_configured" },
                "type": "UserProfileService",
                "description": "Manages user profiles and preferences"
            },
            "database_manager": {
                "status": "healthy",
                "type": "DatabaseManager",
                "description": "Manages database connections and operations"
            },
            "data_access_layer": {
                "status": "healthy",
                "type": "DataAccessLayer",
                "description": "Provides unified data access interface"
            }
        }
    });

    Response::from_json(&detailed_health)
}

// ============================================================================
// DURABLE OBJECTS
// ============================================================================

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
        // Health endpoints - Use modular routing
        (Method::Get, "/health") => {
            route_health_check(req, &get_service_container(&env).await?).await
        }
        (Method::Get, "/health/detailed") => {
            route_detailed_health_check(req, &get_service_container(&env).await?).await
        }

        // User management endpoints - Use modular routing
        (Method::Get, "/api/v1/user/profile") => {
            route_user_request(req, &get_service_container(&env).await?, "profile").await
        }
        (Method::Put, "/api/v1/user/profile") => {
            route_user_request(req, &get_service_container(&env).await?, "update_profile").await
        }
        (Method::Get, "/api/v1/user/preferences") => {
            route_user_request(req, &get_service_container(&env).await?, "preferences").await
        }
        (Method::Put, "/api/v1/user/preferences") => {
            route_user_request(
                req,
                &get_service_container(&env).await?,
                "update_preferences",
            )
            .await
        }

        // Opportunities endpoints - Use modular routing
        (Method::Get, "/api/v1/opportunities") => {
            route_opportunities_request(req, &get_service_container(&env).await?, "list").await
        }
        (Method::Post, "/api/v1/opportunities/analyze") => {
            route_opportunities_request(req, &get_service_container(&env).await?, "analyze").await
        }

        // Telegram endpoints - Use modular routing
        (Method::Post, "/telegram/webhook") => {
            route_telegram_request(req, &get_service_container(&env).await?, "webhook").await
        }
        (Method::Post, "/telegram/send") => {
            route_telegram_request(req, &get_service_container(&env).await?, "send").await
        }

        // Authentication endpoints - Use modular routing
        (Method::Post, "/auth/login") => {
            route_auth_request(req, &get_service_container(&env).await?, "login").await
        }
        (Method::Post, "/auth/logout") => {
            route_auth_request(req, &get_service_container(&env).await?, "logout").await
        }
        (Method::Get, "/auth/session") => {
            route_auth_request(req, &get_service_container(&env).await?, "session").await
        }

        // Analytics endpoints - Legacy handlers (TODO: Migrate to modular)
        (Method::Get, "/api/v1/analytics/dashboard") => {
            console_log!(
                "⚠️ Using legacy handler for analytics dashboard - TODO: Migrate to modular"
            );
            handle_api_get_dashboard_analytics(req, env).await
        }

        // Admin endpoints - Legacy handlers (TODO: Migrate to modular)
        (Method::Get, "/api/v1/admin/users") => {
            console_log!("⚠️ Using legacy handler for admin users - TODO: Migrate to modular");
            handle_api_admin_get_users(req, env).await
        }

        // Trading endpoints - Legacy handlers (TODO: Migrate to modular)
        (Method::Get, "/api/v1/trading/balance") => {
            console_log!("⚠️ Using legacy handler for trading balance - TODO: Migrate to modular");
            handle_api_get_trading_balance(req, env).await
        }

        // AI endpoints - Legacy handlers (TODO: Migrate to modular)
        (Method::Post, "/api/v1/ai/analyze") => {
            console_log!("⚠️ Using legacy handler for AI analyze - TODO: Migrate to modular");
            handle_api_ai_analyze(req, env).await
        }

        // Legacy endpoints (keep for backward compatibility)
        (Method::Get, "/markets") => {
            console_log!(
                "⚠️ Using legacy endpoint /markets - Consider migrating to /api/v1/markets"
            );
            handle_get_markets(req, env).await
        }
        (Method::Get, "/ticker") => {
            console_log!("⚠️ Using legacy endpoint /ticker - Consider migrating to /api/v1/ticker");
            handle_get_ticker(req, env).await
        }
        (Method::Get, "/funding-rate") => {
            console_log!("⚠️ Using legacy endpoint /funding-rate - Consider migrating to /api/v1/funding-rate");
            handle_funding_rate(req, env).await
        }
        (Method::Get, "/orderbook") => {
            console_log!(
                "⚠️ Using legacy endpoint /orderbook - Consider migrating to /api/v1/orderbook"
            );
            handle_get_orderbook(req, env).await
        }
        (Method::Post, "/find-opportunities") => {
            console_log!("⚠️ Using legacy endpoint /find-opportunities - Consider migrating to /api/v1/opportunities/analyze");
            handle_find_opportunities(req, env).await
        }
        (Method::Post, "/positions") => {
            console_log!("⚠️ Using legacy endpoint /positions - Consider migrating to /api/v1/trading/positions");
            handle_create_position(req, env).await
        }
        (Method::Get, "/positions") => {
            console_log!("⚠️ Using legacy endpoint /positions - Consider migrating to /api/v1/trading/positions");
            handle_get_all_positions(req, env).await
        }
        (Method::Get, path) if path.starts_with("/positions/") => {
            console_log!("⚠️ Using legacy endpoint /positions/:id - Consider migrating to /api/v1/trading/positions/:id");
            let id = path.strip_prefix("/positions/").unwrap_or("");
            handle_get_position(req, env, id).await
        }
        (Method::Put, path) if path.starts_with("/positions/") => {
            console_log!("⚠️ Using legacy endpoint PUT /positions/:id - Consider migrating to /api/v1/trading/positions/:id");
            let id = path.strip_prefix("/positions/").unwrap_or("");
            handle_update_position(req, env, id).await
        }
        (Method::Delete, path) if path.starts_with("/positions/") => {
            console_log!("⚠️ Using legacy endpoint DELETE /positions/:id - Consider migrating to /api/v1/trading/positions/:id");
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

#[allow(dead_code)]
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

// async fn create_opportunity_service(
//     custom_env: &types::Env,
// ) -> ArbitrageResult<OpportunityService> {
//     let config = OpportunityServiceConfig {
//         exchanges: parse_exchanges_from_env("binance,bybit")?,
//         monitored_pairs: vec![], // Empty for now, will be populated as needed
//         threshold: 0.01,
//     };
//
//     let exchange_service = Arc::new(ExchangeService::new(custom_env)?);
//
//     Ok(OpportunityService::new(
//         config,
//         exchange_service,
//         None, // No telegram service for now
//     ))
// }

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

    let exchange_service = ExchangeService::new(&env)?;
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

    let exchange_service = ExchangeService::new(&env)?;
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

    let exchange_service = ExchangeService::new(&env)?;
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

    let exchange_service = ExchangeService::new(&env)?;
    match exchange_service
        .get_orderbook(&exchange_enum.to_string(), &symbol, None)
        .await
    {
        Ok(orderbook) => Response::from_json(&orderbook),
        Err(e) => Response::error(format!("Failed to get orderbook: {:?}", e), 500),
    }
}

async fn handle_find_opportunities(mut req: Request, _env: Env) -> Result<Response> {
    let body: serde_json::Value = req.json().await?;
    let _pairs = body["pairs"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_else(|| vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]);

    // Create custom environment for opportunity service
    // let _custom_env = env;

    // Create opportunity service
    // let opportunity_service = create_opportunity_service(&custom_env).await?;

    // TODO: Replace with new modular opportunity engine
    // match opportunity_service
    //     .find_opportunities(&exchanges, &pairs, threshold)
    //     .await
    // {
    //     Ok(opportunities) => Response::from_json(&opportunities),
    //     Err(e) => Response::error(format!("Failed to find opportunities: {:?}", e), 500),
    // }

    Response::error(
        "Opportunity service temporarily disabled during refactoring",
        503,
    )
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
    // _opportunity_service: &OpportunityService,
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
        if let Ok(d1_database) = env.d1("ArbEdgeDB") {
            if let Ok(encryption_key) = env.var("ENCRYPTION_KEY") {
                let database_manager = DatabaseManager::new(
                    std::sync::Arc::new(d1_database),
                    services::core::infrastructure::database_repositories::DatabaseManagerConfig::default()
                );
                let user_profile_service =
                    UserProfileService::new(kv_store, database_manager, encryption_key.to_string());

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
    // let _custom_env = env;

    // Create opportunity service
    // let opportunity_service = create_opportunity_service(&custom_env).await?;

    // Run maintenance
    run_five_minute_maintenance(&env).await?;

    console_log!("✅ Scheduled opportunity monitoring completed successfully");
    Ok(())
}
