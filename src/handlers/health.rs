use crate::responses::ApiResponse;
use crate::services;
use worker::{Env, Request, Response, Result};

/// Basic health check endpoint
pub async fn handle_api_health_check(_req: Request, _env: Env) -> Result<Response> {
    let response = ApiResponse::success(serde_json::json!({
        "status": "healthy",
        "service": "ArbEdge API",
        "version": "1.0.0"
    }));
    Response::from_json(&response)
}

/// Detailed health check endpoint that tests all services
pub async fn handle_api_detailed_health_check(_req: Request, env: Env) -> Result<Response> {
    // Check service health
    let kv_healthy = env.kv("ArbEdgeKV").is_ok();
    let d1_healthy = env.d1("ArbEdgeD1").is_ok();
    // add R2 health check

    // Test KV store with a simple operation
    let kv_operational = if kv_healthy {
        match env.kv("ArbEdgeKV") {
            Ok(kv) => {
                // Try a simple get operation to test connectivity
                (kv.get("health_check_test").text().await).is_ok()
            }
            Err(_) => false,
        }
    } else {
        false
    };

    // Test D1 database with a simple query
    let d1_operational = if d1_healthy {
        match services::core::infrastructure::D1Service::new(&env) {
            Ok(d1_service) => {
                // Try a simple query to test connectivity
                (d1_service.health_check().await).unwrap_or(false)
            }
            Err(_) => false,
        }
    } else {
        false
    };

    // Test Telegram service by checking if bot token is configured
    let telegram_healthy = env.var("TELEGRAM_BOT_TOKEN").is_ok();

    // Test Exchange service by checking if API keys are configured
    let exchange_healthy =
        env.var("BINANCE_API_KEY").is_ok() && env.var("BINANCE_SECRET_KEY").is_ok();

    // Test AI service by checking if OpenAI API key is configured
    let ai_healthy = env.var("OPENAI_API_KEY").is_ok();

    // Determine overall health status
    let overall_healthy =
        kv_operational && d1_operational && telegram_healthy && exchange_healthy && ai_healthy;

    let response = ApiResponse::success(serde_json::json!({
        "status": if overall_healthy { "healthy" } else { "degraded" },
        "services": {
            "kv_store": if kv_operational { "online" } else { "offline" },
            "d1_database": if d1_operational { "online" } else { "offline" },
            "telegram_service": if telegram_healthy { "online" } else { "offline" },
            "exchange_service": if exchange_healthy { "online" } else { "offline" },
            "ai_service": if ai_healthy { "online" } else { "offline" }
        },
        "timestamp": chrono::Utc::now().timestamp()
    }));
    Response::from_json(&response)
}
