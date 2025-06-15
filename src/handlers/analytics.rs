use crate::middleware::extract_user_id_from_headers;
use crate::responses::ApiResponse;
use crate::services;
use crate::utils::now_system_time;
use std::sync::Arc;
use worker::{Env, Request, Response, Result};

/// Get dashboard analytics for authenticated user
pub async fn handle_api_get_dashboard_analytics(req: Request, env: Env) -> Result<Response> {
    let user_id = match extract_user_id_from_headers(&req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Initialize services
    let kv_store = env.kv("ArbEdgeKV")?;
    let d1_database = Arc::new(env.d1("ArbEdgeD1")?);
    let encryption_key = env
        .var("ENCRYPTION_KEY")
        .map(|secret| secret.to_string())
        .map_err(|_| {
            worker::Error::RustError("ENCRYPTION_KEY environment variable is required".to_string())
        })?;

    let d1_service = services::core::infrastructure::DatabaseManager::new(
        d1_database.clone(),
        services::core::infrastructure::database_repositories::DatabaseManagerConfig::default(),
    );

    let user_profile_service = services::core::user::user_profile::UserProfileService::new(
        kv_store.clone(),
        d1_service.clone(),
        encryption_key,
    );

    // Initialize analytics service
    let analytics_service =
        services::core::analysis::AnalyticsService::new(kv_store.clone(), d1_database.clone());

    // Get user profile and analytics data
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            // Get user-specific analytics
            match analytics_service.get_dashboard_analytics(&user_id).await {
                Ok(analytics) => {
                    let dashboard_data = serde_json::json!({
                        "user_id": user_id,
                        "performance": {
                            "total_pnl_usdt": profile.total_pnl_usdt,
                            "total_trades": profile.total_trades,
                        },
                        "analytics": analytics,
                        "subscription": {
                            "tier": profile.subscription.tier,
                            "expires_at": profile.subscription.expires_at
                        },
                        "timestamp": now_system_time()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    });

                    let response = ApiResponse::success(dashboard_data);
                    Response::from_json(&response)
                }
                Err(e) => {
                    let response =
                        ApiResponse::<()>::error(format!("Failed to fetch analytics: {}", e));
                    Ok(Response::from_json(&response)?.with_status(500))
                }
            }
        }
        Ok(None) => {
            let response = ApiResponse::<()>::error("User profile not found".to_string());
            Ok(Response::from_json(&response)?.with_status(404))
        }
        Err(e) => {
            let response = ApiResponse::<()>::error(format!("Failed to fetch user profile: {}", e));
            Ok(Response::from_json(&response)?.with_status(500))
        }
    }
}
