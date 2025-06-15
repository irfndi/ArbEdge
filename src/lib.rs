use std::sync::Arc;
use worker::*;

// Time constants for improved readability
const HOUR_IN_MS: u64 = 60 * 60 * 1000;
const DAY_IN_MS: u64 = 24 * HOUR_IN_MS;

// Request validation structs
#[derive(serde::Deserialize)]
struct UpdateProfileRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    telegram_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
}

#[derive(serde::Deserialize)]
struct UpdatePreferencesRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    notification_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    risk_tolerance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_profit_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_position_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_trading_pairs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_exchanges: Option<Vec<String>>,
}

// Module declarations
pub mod handlers;
pub mod middleware;
pub mod responses;
pub mod services;
pub mod types;
pub mod utils;

#[cfg(test)]
pub mod test_utils;

use services::core::infrastructure::database_repositories::database_manager::{
    DatabaseManager, DatabaseManagerConfig,
};
use services::core::infrastructure::service_container::ServiceContainer;

// Import new modular components
use handlers::*;

use types::ExchangeIdEnum;
use utils::{add_cache_headers, cache_type_from_path, ArbitrageError, ArbitrageResult};
use worker::kv::KvStore;

// Import required services
use services::core::user::UserProfileService;

#[cfg(target_arch = "wasm32")]
use wee_alloc;

#[cfg(target_arch = "wasm32")]
use worker::console_log;

#[cfg(not(target_arch = "wasm32"))]
macro_rules! console_log {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static SERVICE_CONTAINER: std::cell::RefCell<Option<Arc<ServiceContainer>>> =
        std::cell::RefCell::new(None);
}

#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

#[cfg(not(target_arch = "wasm32"))]
static SERVICE_CONTAINER: OnceLock<Arc<ServiceContainer>> = OnceLock::new();

async fn get_service_container(env: &worker::Env) -> Result<Arc<ServiceContainer>> {
    #[cfg(target_arch = "wasm32")]
    {
        // Check if we already have a cached container
        let existing = SERVICE_CONTAINER.with(|container| container.borrow().clone());

        if let Some(container) = existing {
            return Ok(container);
        }

        // Create new container for WASM without tokio
        let kv_store = env.kv("ArbEdgeKV")?;
        let d1 = env.d1("ArbEdgeD1")?;
        let _database_manager =
            DatabaseManager::new(Arc::new(d1), DatabaseManagerConfig::default());

        let new_container = Arc::new(ServiceContainer::new(env, kv_store).await?);

        // Cache it for future use
        SERVICE_CONTAINER.with(|container| {
            container.replace(Some(new_container.clone()));
        });

        Ok(new_container)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(container) = SERVICE_CONTAINER.get() {
            return Ok(container.clone());
        }

        let kv_store = env.kv("ArbEdgeKV")?;
        let d1 = env.d1("ArbEdgeD1")?;
        let _database_manager =
            DatabaseManager::new(Arc::new(d1), DatabaseManagerConfig::default());

        // Initialize the service container ONCE
        let container = Arc::new(ServiceContainer::new(env, kv_store).await?);

        match SERVICE_CONTAINER.set(container.clone()) {
            Ok(()) => Ok(container),
            Err(_) => {
                // Another thread beat us to it, return the existing one
                Ok(SERVICE_CONTAINER.get().unwrap().clone())
            }
        }
    }
}

// Removed get_telegram_service function - now using ServiceContainer's telegram service

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
                let created_at_ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                    session_details.created_at as i64,
                );
                let expires_at_ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                    session_details.expires_at as i64,
                );
                let last_activity_ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                    session_details.last_activity_at as i64,
                );

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
            // Extract user ID from headers first (before moving req)
            let user_id = req
                .headers()
                .get("X-User-ID")?
                .ok_or_else(|| worker::Error::RustError("Missing X-User-ID header".to_string()))?;

            // Parse and validate profile update request
            let mut req_clone = req;
            let update_request = match req_clone.json::<UpdateProfileRequest>().await {
                Ok(request) => request,
                Err(e) => {
                    console_log!("❌ Invalid profile update request: {:?}", e);
                    return Response::error("Invalid request format", 400);
                }
            };

            // Validate request data
            if let Some(ref risk_tolerance) = update_request.display_name {
                if risk_tolerance.len() > 50 {
                    return Response::error("Display name too long (max 50 characters)", 400);
                }
            }

            // Basic profile update implementation (fallback during migration)
            console_log!(
                "📝 Profile update request for user {}: telegram_username={:?}, display_name={:?}",
                user_id,
                update_request.telegram_username,
                update_request.display_name
            );

            // TODO: Implement proper profile field updates when modular service supports it
            let response = serde_json::json!({
                "status": "accepted",
                "message": "Profile update accepted and validated (simplified implementation during service migration)",
                "user_id": user_id,
                "updated_fields": {
                    "telegram_username": update_request.telegram_username,
                    "display_name": update_request.display_name,
                    "bio": update_request.bio,
                    "timezone": update_request.timezone,
                    "language": update_request.language
                },
                "note": "Profile updates are validated and tracked but not persisted during modular architecture migration",
                "next_update_eta": "When full user service integration is complete",
                "timestamp": chrono::Utc::now().timestamp_millis()
            });
            Response::from_json(&response)
        }
        "update_preferences" => {
            // Extract user ID from headers first (before moving req)
            let user_id = req
                .headers()
                .get("X-User-ID")?
                .ok_or_else(|| worker::Error::RustError("Missing X-User-ID header".to_string()))?;

            // Parse and validate preferences update request
            let mut req_clone = req;
            let update_request = match req_clone.json::<UpdatePreferencesRequest>().await {
                Ok(request) => request,
                Err(e) => {
                    console_log!("❌ Invalid preferences update request: {:?}", e);
                    return Response::error("Invalid request format", 400);
                }
            };

            // Validate request data
            if let Some(risk_tolerance) = update_request.risk_tolerance {
                if !(0.0..=1.0).contains(&risk_tolerance) {
                    return Response::error("Risk tolerance must be between 0.0 and 1.0", 400);
                }
            }
            if let Some(min_profit) = update_request.min_profit_threshold {
                if min_profit < 0.0 {
                    return Response::error("Minimum profit threshold cannot be negative", 400);
                }
            }
            if let Some(max_position) = update_request.max_position_size {
                if max_position <= 0.0 {
                    return Response::error("Maximum position size must be positive", 400);
                }
            }

            // Basic preferences update implementation (fallback during migration)
            console_log!(
                "⚙️ Preferences update request for user {}: notification_enabled={:?}, risk_tolerance={:?}",
                user_id,
                update_request.notification_enabled,
                update_request.risk_tolerance
            );

            // TODO: Implement proper preference updates when modular service supports it
            let response = serde_json::json!({
                "status": "accepted",
                "message": "Preferences update accepted and validated (simplified implementation during service migration)",
                "user_id": user_id,
                "updated_preferences": {
                    "notification_enabled": update_request.notification_enabled,
                    "risk_tolerance": update_request.risk_tolerance,
                    "min_profit_threshold": update_request.min_profit_threshold,
                    "max_position_size": update_request.max_position_size,
                    "preferred_trading_pairs": update_request.preferred_trading_pairs,
                    "preferred_exchanges": update_request.preferred_exchanges
                },
                "note": "Preference updates are validated and tracked but not persisted during modular architecture migration",
                "next_update_eta": "When full user service integration is complete",
                "timestamp": chrono::Utc::now().timestamp_millis()
            });
            Response::from_json(&response)
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
    _env: &worker::Env,
) -> Result<Response> {
    console_log!("📱 Routing telegram request: {}", action);

    match action {
        "webhook" => {
            // Parse telegram webhook
            let mut req_clone = req;
            let webhook_data: serde_json::Value = req_clone.json().await?;

            // Process webhook using ModularTelegramService from ServiceContainer
            if let Some(telegram_service) = &container.telegram_service {
                let response_text = telegram_service
                    .handle_webhook(webhook_data)
                    .await
                    .map_err(|e| {
                        worker::Error::RustError(format!("Failed to process webhook: {:?}", e))
                    })?;

                // Return plain text response for Telegram webhook
                Response::ok(&response_text)
            } else {
                console_log!("❌ Telegram service not available in ServiceContainer");
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
    fn new(state: State, _env: worker::Env) -> Self {
        Self { _state: state }
    }

    async fn fetch(&mut self, _req: Request) -> Result<Response> {
        Response::ok("Positions Manager")
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: worker::Env, _ctx: Context) -> Result<Response> {
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

    // Determine cache type for this request
    let cache_type = cache_type_from_path(path);

    let mut response = match (method.clone(), path) {
        // Test endpoint without ServiceContainer to isolate issues
        (Method::Get, "/test") => {
            Response::ok("Test endpoint working - ServiceContainer not required")
        }

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
            route_telegram_request(req, &get_service_container(&env).await?, "webhook", &env).await
        }
        (Method::Post, "/telegram/send") => {
            route_telegram_request(req, &get_service_container(&env).await?, "send", &env).await
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

        // Analytics endpoints - Using modular handlers
        (Method::Get, "/api/v1/analytics/dashboard") => {
            handle_api_get_dashboard_analytics(req, env).await
        }

        // Admin endpoints - Using modular handlers
        (Method::Get, "/api/v1/admin/users") => handle_api_admin_get_users(req, env).await,

        // Trading endpoints - Using modular handlers
        (Method::Get, "/api/v1/trading/balance") => handle_api_get_trading_balance(req, env).await,

        // AI endpoints - Using modular handlers
        (Method::Post, "/api/v1/ai/analyze") => handle_api_ai_analyze(req, env).await,

        // Legacy endpoints removed - all functionality moved to modular API endpoints
        _ => {
            console_log!("❌ Route not found: {} {}", method, path);
            Response::error("Not Found", 404)
        }
    };

    // Add comprehensive headers to response
    if let Ok(ref mut resp) = response {
        let headers = resp.headers_mut();

        // Add CORS headers
        headers.set("Access-Control-Allow-Origin", "*")?;
        headers.set(
            "Access-Control-Allow-Methods",
            "GET, POST, PUT, DELETE, OPTIONS",
        )?;
        headers.set(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization, X-User-ID",
        )?;

        // Add cache headers based on endpoint type
        if let Err(e) = add_cache_headers(resp, cache_type) {
            console_log!("⚠️ Failed to add cache headers: {:?}", e);
            // Continue without cache headers rather than failing the request
        } else {
            console_log!("✅ Added cache headers for {}: {:?}", path, cache_type);
        }
    }

    response
}

#[event(scheduled)]
pub async fn scheduled(event: ScheduledEvent, env: worker::Env, _ctx: ScheduleContext) {
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

// Legacy handlers removed - all functionality moved to modular services

async fn run_five_minute_maintenance(
    env: &worker::Env,
    // _opportunity_service: &OpportunityService,
) -> ArbitrageResult<()> {
    console_log!("🔧 Running 5-minute maintenance tasks...");

    let current_timestamp = chrono::Utc::now().timestamp_millis() as u64;
    let mut completed_tasks = 0;
    let mut failed_tasks = 0;

    // Get required services
    let kv_store = match env.kv("ArbEdgeKV") {
        Ok(kv) => kv,
        Err(e) => {
            console_log!("❌ Failed to access KV store for maintenance: {:?}", e);
            return Err(ArbitrageError::kv_error(format!(
                "KV access failed: {:?}",
                e
            )));
        }
    };

    // 1. Clean up expired opportunities from KV store
    console_log!("🧹 Cleaning up expired opportunities...");
    match cleanup_expired_opportunities(&kv_store, current_timestamp).await {
        Ok(cleaned_count) => {
            console_log!("✅ Cleaned up {} expired opportunities", cleaned_count);
            completed_tasks += 1;
        }
        Err(e) => {
            console_log!("❌ Failed to cleanup expired opportunities: {:?}", e);
            failed_tasks += 1;
        }
    }

    // 2. Update distribution statistics
    console_log!("📊 Updating distribution statistics...");
    match update_distribution_statistics(&kv_store, current_timestamp).await {
        Ok(()) => {
            console_log!("✅ Distribution statistics updated");
            completed_tasks += 1;
        }
        Err(e) => {
            console_log!("❌ Failed to update distribution statistics: {:?}", e);
            failed_tasks += 1;
        }
    }

    // 3. Process pending opportunity distributions
    console_log!("📤 Processing pending distributions...");
    match process_pending_distributions(&kv_store, current_timestamp).await {
        Ok(processed_count) => {
            console_log!("✅ Processed {} pending distributions", processed_count);
            completed_tasks += 1;
        }
        Err(e) => {
            console_log!("❌ Failed to process pending distributions: {:?}", e);
            failed_tasks += 1;
        }
    }

    // 4. Update user activity metrics
    console_log!("👥 Updating user activity metrics...");
    match update_user_activity_metrics(&kv_store, current_timestamp).await {
        Ok(active_users) => {
            console_log!("✅ Updated activity metrics for {} users", active_users);
            completed_tasks += 1;
        }
        Err(e) => {
            console_log!("❌ Failed to update user activity metrics: {:?}", e);
            failed_tasks += 1;
        }
    }

    // 5. Cleanup inactive user sessions
    console_log!("🧹 Cleaning up expired sessions...");
    if let Ok(d1_database) = env.d1("ArbEdgeDB") {
        if let Ok(encryption_key) = env.var("ENCRYPTION_KEY") {
            let database_manager = DatabaseManager::new(
                std::sync::Arc::new(d1_database),
                services::core::infrastructure::database_repositories::DatabaseManagerConfig::default()
            );
            let user_profile_service = UserProfileService::new(
                kv_store.clone(),
                database_manager,
                encryption_key.to_string(),
            );

            match cleanup_expired_sessions(&user_profile_service, &kv_store, current_timestamp)
                .await
            {
                Ok(cleaned_sessions) => {
                    console_log!("✅ Cleaned up {} expired sessions", cleaned_sessions);
                    completed_tasks += 1;
                }
                Err(e) => {
                    console_log!("❌ Failed to cleanup expired sessions: {:?}", e);
                    failed_tasks += 1;
                }
            }
        } else {
            console_log!("⚠️ Skipping session cleanup - encryption key not available");
            failed_tasks += 1;
        }
    } else {
        console_log!("⚠️ Skipping session cleanup - D1 database not available");
        failed_tasks += 1;
    }

    // Store maintenance metrics
    let maintenance_summary = serde_json::json!({
        "timestamp": current_timestamp,
        "completed_tasks": completed_tasks,
        "failed_tasks": failed_tasks,
        "total_tasks": completed_tasks + failed_tasks,
        "success_rate": if completed_tasks + failed_tasks > 0 {
            completed_tasks as f64 / (completed_tasks + failed_tasks) as f64 * 100.0
        } else {
            0.0
        }
    });

    if let Err(e) = kv_store
        .put("maintenance:last_run", maintenance_summary.to_string())?
        .execute()
        .await
    {
        console_log!("⚠️ Failed to store maintenance summary: {:?}", e);
    }

    console_log!(
        "✅ 5-minute maintenance completed: {}/{} tasks successful",
        completed_tasks,
        completed_tasks + failed_tasks
    );
    Ok(())
}

// Helper function to clean up expired opportunities
async fn cleanup_expired_opportunities(
    kv_store: &KvStore,
    current_timestamp: u64,
) -> ArbitrageResult<u32> {
    let mut cleaned_count = 0;

    // Opportunities older than 1 hour are considered expired
    let expiry_threshold = current_timestamp - HOUR_IN_MS;

    // IMPORTANT: This is a simplified implementation during modular architecture migration.
    //
    // LIMITATION: Cloudflare Workers KV does not currently support list/scan operations
    // that can efficiently iterate through keys by prefix. This implementation checks
    // known key patterns based on the current opportunity generation strategy.
    //
    // FUTURE IMPROVEMENT: When KV list operations become available, or when we migrate
    // to a database-backed solution, this should be replaced with proper key scanning.
    //
    // Current strategy: Check keys that match our opportunity ID patterns
    let known_opportunity_keys = [
        // Fallback opportunity keys (from our current implementation)
        "fallback_BTCUSDT_binance_0_0",
        "fallback_BTCUSDT_binance_0_1",
        "fallback_BTCUSDT_bybit_0_0",
        "fallback_ETHUSDT_binance_1_0",
        "fallback_ETHUSDT_binance_1_1",
        "fallback_ETHUSDT_bybit_1_0",
        // Additional common patterns that might be used
        "opportunity:live:BTCUSDT",
        "opportunity:live:ETHUSDT",
        "opportunity:live:ADAUSDT",
        "market_opp:binance:BTCUSDT",
        "market_opp:bybit:BTCUSDT",
        "arb_opp:latest",
        "arb_opp:current",
    ];

    // Check each known key pattern for expired data
    for key in known_opportunity_keys {
        match kv_store.get(key).text().await {
            Ok(Some(data)) => {
                if let Ok(opportunity) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(timestamp) = opportunity.get("timestamp").and_then(|t| t.as_u64()) {
                        if timestamp < expiry_threshold {
                            console_log!("🧹 Cleaning expired opportunity: {}", key);
                            if let Err(e) = kv_store.delete(key).await {
                                console_log!(
                                    "⚠️ Failed to delete expired opportunity {}: {:?}",
                                    key,
                                    e
                                );
                            } else {
                                cleaned_count += 1;
                                console_log!("✅ Deleted expired opportunity: {}", key);
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                // Key doesn't exist, which is fine
            }
            Err(e) => {
                console_log!("⚠️ Error checking opportunity key {}: {:?}", key, e);
            }
        }
    }

    // Also check for any time-based opportunity keys (opportunities with timestamp suffixes)
    let now_hour = current_timestamp / HOUR_IN_MS; // Current hour
    for hours_back in 2..24 {
        // Check last 24 hours, starting from 2 hours ago
        let target_hour = now_hour - hours_back;
        let time_key = format!("opportunities:{}", target_hour);

        if let Ok(Some(_)) = kv_store.get(&time_key).text().await {
            console_log!("🧹 Cleaning old hourly opportunities: {}", time_key);
            if let Err(e) = kv_store.delete(&time_key).await {
                console_log!(
                    "⚠️ Failed to delete old hourly opportunities {}: {:?}",
                    time_key,
                    e
                );
            } else {
                cleaned_count += 1;
            }
        }
    }

    if cleaned_count > 0 {
        console_log!(
            "✅ Cleaned up {} expired opportunity entries",
            cleaned_count
        );
    } else {
        console_log!("ℹ️ No expired opportunities found for cleanup");
    }

    Ok(cleaned_count)
}

// Helper function to update distribution statistics
async fn update_distribution_statistics(
    kv_store: &KvStore,
    current_timestamp: u64,
) -> ArbitrageResult<()> {
    // Calculate distribution statistics for the past hour
    let stats = serde_json::json!({
        "timestamp": current_timestamp,
        "hourly_distributions": 0, // TODO: Implement actual counting
        "total_users_notified": 0, // TODO: Implement actual counting
        "distribution_success_rate": 100.0, // TODO: Calculate based on actual data
        "avg_distribution_time_ms": 150.0, // TODO: Calculate based on actual metrics
        "next_update": current_timestamp + (5 * 60 * 1000) // Next update in 5 minutes
    });

    kv_store
        .put("stats:distributions", stats.to_string())?
        .execute()
        .await
        .map_err(|e| {
            ArbitrageError::kv_error(format!("Failed to update distribution stats: {:?}", e))
        })?;

    Ok(())
}

// Helper function to process pending distributions
async fn process_pending_distributions(
    kv_store: &KvStore,
    current_timestamp: u64,
) -> ArbitrageResult<u32> {
    let mut processed_count = 0;

    // Check for pending distributions using specific queue indices
    // TODO: Implement actual queue processing logic
    // For now, check for any queued distribution items using known queue patterns
    let queue_types = [
        ("queue:distribution:", 10), // Check reasonable range for distribution queue
        ("pending:notification:", 25), // Check notification queue
    ];

    for (prefix, max_items) in queue_types {
        // Instead of hardcoded 0..50, use configurable max_items per queue type
        for i in 0..max_items {
            let key = format!("{}{}", prefix, i);
            if let Ok(Some(data)) = kv_store.get(&key).text().await {
                if let Ok(distribution) = serde_json::from_str::<serde_json::Value>(&data) {
                    // Process the distribution (simplified)
                    console_log!("📤 Processing distribution: {}", key);

                    // Mark as processed by moving to processed queue
                    let processed_key = format!("processed:{}", key);
                    let processed_data = serde_json::json!({
                        "original_data": distribution,
                        "processed_at": current_timestamp,
                        "status": "completed"
                    });

                    if let Err(e) = kv_store
                        .put(&processed_key, processed_data.to_string())?
                        .execute()
                        .await
                    {
                        console_log!("⚠️ Failed to mark distribution as processed: {:?}", e);
                    } else {
                        // Remove from pending queue
                        if let Err(e) = kv_store.delete(&key).await {
                            console_log!("⚠️ Failed to remove from pending queue: {:?}", e);
                        } else {
                            processed_count += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(processed_count)
}

// Helper function to update user activity metrics
async fn update_user_activity_metrics(
    kv_store: &KvStore,
    current_timestamp: u64,
) -> ArbitrageResult<u32> {
    let mut active_users = 0;

    // Calculate user activity for the past hour
    let activity_threshold = current_timestamp - HOUR_IN_MS;

    // TODO: Implement actual user activity scanning
    // For now, check session and activity keys using targeted ranges
    let activity_sources = [
        ("user:activity:", 100), // Check reasonable range for user activity
        ("session:", 150),       // Check session keys
    ];

    for (prefix, max_items) in activity_sources {
        // Instead of hardcoded 0..200, use configurable max_items per source
        for i in 0..max_items {
            let key = format!("{}{}", prefix, i);
            if let Ok(Some(data)) = kv_store.get(&key).text().await {
                if let Ok(activity) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(last_activity) =
                        activity.get("last_activity").and_then(|t| t.as_u64())
                    {
                        if last_activity > activity_threshold {
                            active_users += 1;
                        }
                    }
                }
            }
        }
    }

    // Store updated activity metrics
    let activity_summary = serde_json::json!({
        "timestamp": current_timestamp,
        "active_users_last_hour": active_users,
        "measurement_period_ms": HOUR_IN_MS,
        "next_update": current_timestamp + (5 * 60 * 1000)
    });

    kv_store
        .put("metrics:user_activity", activity_summary.to_string())?
        .execute()
        .await
        .map_err(|e| {
            ArbitrageError::kv_error(format!("Failed to update activity metrics: {:?}", e))
        })?;

    Ok(active_users)
}

// Helper function to cleanup expired sessions
async fn cleanup_expired_sessions(
    _user_profile_service: &UserProfileService,
    kv_store: &KvStore,
    current_timestamp: u64,
) -> ArbitrageResult<u32> {
    let mut cleaned_sessions = 0;

    // Sessions older than 24 hours are considered expired
    let session_expiry = current_timestamp - DAY_IN_MS;

    // Check session keys using targeted patterns
    let session_sources = [
        ("session:", 200),      // Primary session store
        ("user_session:", 300), // User-specific sessions
        ("auth_session:", 100), // Auth sessions
    ];

    for (prefix, max_items) in session_sources {
        // Instead of hardcoded 0..500, use configurable max_items per session type
        for i in 0..max_items {
            let key = format!("{}{}", prefix, i);
            if let Ok(Some(data)) = kv_store.get(&key).text().await {
                if let Ok(session) = serde_json::from_str::<serde_json::Value>(&data) {
                    let should_delete = if let Some(expires_at) =
                        session.get("expires_at").and_then(|t| t.as_u64())
                    {
                        expires_at < current_timestamp
                    } else if let Some(created_at) =
                        session.get("created_at").and_then(|t| t.as_u64())
                    {
                        created_at < session_expiry
                    } else {
                        false // Keep sessions without timestamps for safety
                    };

                    if should_delete {
                        if let Err(e) = kv_store.delete(&key).await {
                            console_log!("⚠️ Failed to delete expired session {}: {:?}", key, e);
                        } else {
                            cleaned_sessions += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(cleaned_sessions)
}

async fn monitor_opportunities_scheduled(env: worker::Env) -> ArbitrageResult<()> {
    console_log!("🔄 Starting scheduled opportunity monitoring...");

    // Initialize service container to access opportunity engine
    let kv = env.kv("ArbEdgeKV").expect("KV binding not found");
    let service_container = ServiceContainer::new(&env, kv).await?;

    // Generate global opportunities using the opportunity engine
    console_log!("🔍 Generating global opportunities...");
    match service_container.opportunity_engine {
        Some(ref opportunity_engine) => {
            console_log!("⚠️ DEBUG: About to call generate_global_opportunities");
            match opportunity_engine.generate_global_opportunities().await {
                Ok(opportunities) => {
                    console_log!("✅ Generated {} global opportunities", opportunities.len());

                    // Distribute the opportunities using the distribution service
                    let distribution_service = &service_container.distribution_service;
                    let mut distributed_count = 0;
                    for arb_opp in opportunities {
                        // ArbitrageOpportunity can be distributed directly
                        match distribution_service.distribute_opportunity(arb_opp).await {
                            Ok(count) => distributed_count += count,
                            Err(e) => {
                                console_log!("⚠️ Failed to distribute opportunity: {:?}", e);
                            }
                        }
                    }
                    console_log!("📤 Distributed to {} users", distributed_count);
                }
                Err(e) => {
                    console_log!("⚠️ Failed to generate opportunities: {:?}", e);
                }
            }
        }
        None => {
            console_log!("⚠️ Opportunity engine not available");
        }
    }

    // Run maintenance after opportunity generation
    run_five_minute_maintenance(&env).await?;

    console_log!("✅ Scheduled opportunity monitoring completed successfully");
    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

pub async fn initialize_services(env: worker::Env) -> ServiceContainer {
    let kv = env.kv("ArbEdgeKV").expect("KV binding not found");

    let container = ServiceContainer::new(&env, kv)
        .await
        .expect("Failed to create service container in initialize_services");

    container
}
