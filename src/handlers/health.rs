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
    // Test KV store with dedicated health check key
    let kv_operational = match env.kv("ArbEdgeKV") {
        Ok(kv) => {
            // Check against dedicated health check key that should be updated by background process
            match kv.get("system_health_check").text().await {
                Ok(Some(value)) => {
                    // Validate that the health check key contains recent timestamp
                    if let Ok(timestamp) = value.parse::<u64>() {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        // Health check key should be updated within last 5 minutes (300 seconds)
                        now.saturating_sub(timestamp) < 300
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }
        Err(_) => false,
    };

    // Test D1 database with a simple query
    let d1_operational = match env.d1("ArbEdgeDB") {
        Ok(d1_database) => {
            let database_manager = services::core::infrastructure::database_repositories::DatabaseManager::new(
                std::sync::Arc::new(d1_database),
                services::core::infrastructure::database_repositories::DatabaseManagerConfig::default()
            );
            // Try a simple query to test connectivity
            database_manager.health_check().await
        }
        Err(_) => false,
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

    // Use consistent SystemTime for timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let response = ApiResponse::success(serde_json::json!({
        "status": if overall_healthy { "healthy" } else { "degraded" },
        "services": {
            "kv_store": if kv_operational { "online" } else { "offline" },
            "d1_database": if d1_operational { "online" } else { "offline" },
            "telegram_service": if telegram_healthy { "online" } else { "offline" },
            "exchange_service": if exchange_healthy { "online" } else { "offline" },
            "ai_service": if ai_healthy { "online" } else { "offline" }
        },
        "timestamp": timestamp
    }));
    Response::from_json(&response)
}

/// Update the health check key in KV store (should be called by background process)
pub async fn update_health_check_key(env: &Env) -> Result<()> {
    if let Ok(kv) = env.kv("ArbEdgeKV") {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Update the health check key with current timestamp
        kv.put("system_health_check", timestamp.to_string())?
            .execute()
            .await?;
    }
    Ok(())
}
