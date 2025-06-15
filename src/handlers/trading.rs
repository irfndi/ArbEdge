use crate::middleware::extract_user_id_from_headers;
use crate::responses::ApiResponse;
use crate::services;
use crate::utils::now_system_time;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use worker::{Env, Request, Response, Result};

/// Get trading balance for authenticated user
pub async fn handle_api_get_trading_balance(req: Request, env: Env) -> Result<Response> {
    let user_id = match extract_user_id_from_headers(&req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Initialize services
    let kv_store = env.kv("ArbEdgeKV")?;
    let d1_database = env.d1("ArbEdgeD1")?;
    let encryption_key = env
        .var("ENCRYPTION_KEY")
        .map(|secret| secret.to_string())
        .map_err(|_| {
            worker::Error::RustError("ENCRYPTION_KEY environment variable is required".to_string())
        })?;

    let d1_service = services::core::infrastructure::DatabaseManager::new(
        Arc::new(d1_database),
        services::core::infrastructure::database_repositories::DatabaseManagerConfig::default(),
    );

    let user_profile_service = services::core::user::user_profile::UserProfileService::new(
        kv_store,
        d1_service,
        encryption_key,
    );

    // Get user profile for trading balance
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            let balance_data = serde_json::json!({
                "user_id": user_id,
                "account_balance_usdt": profile.account_balance_usdt,
                "total_pnl_usdt": profile.total_pnl_usdt,
                "total_trades": profile.total_trades,
                "risk_profile": {
                    "max_position_size_usd": profile.risk_profile.max_position_size_usd,
                    "daily_loss_limit_usd": profile.risk_profile.daily_loss_limit_usd,
                    "risk_tolerance": profile.configuration.trading_settings.risk_tolerance
                },
                "trading_settings": {
                    "auto_trading_enabled": profile.configuration.trading_settings.auto_trading_enabled,
                    "max_leverage": profile.configuration.trading_settings.max_leverage,
                    "min_profit_threshold": profile.configuration.trading_settings.min_profit_threshold
                },
                "api_keys_configured": profile.api_keys.iter().any(|key| !key.is_read_only && key.is_active),
                "timestamp": now_system_time()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            });

            let response = ApiResponse::success(balance_data);
            Response::from_json(&response)
        }
        Ok(None) => {
            let response = ApiResponse::<()>::error("User profile not found".to_string());
            Ok(Response::from_json(&response)?.with_status(404))
        }
        Err(e) => {
            let response =
                ApiResponse::<()>::error(format!("Failed to fetch trading balance: {}", e));
            Ok(Response::from_json(&response)?.with_status(500))
        }
    }
}
