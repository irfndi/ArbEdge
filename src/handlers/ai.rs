use crate::middleware::extract_user_id_from_headers;
use crate::responses::ApiResponse;
use crate::services;
use crate::utils::now_system_time;
use std::sync::Arc;
use worker::{Env, Request, Response, Result};

/// AI market analysis endpoint
pub async fn handle_api_ai_analyze(req: Request, env: Env) -> Result<Response> {
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

    // Check user permissions for AI features
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            // Check if user has AI features enabled
            if !profile
                .subscription
                .features
                .contains(&"ai_analysis".to_string())
            {
                let response = ApiResponse::<()>::error(
                    "AI analysis requires premium subscription".to_string(),
                );
                return Ok(Response::from_json(&response)?.with_status(403));
            }

            // Initialize AI service
            let ai_service = services::core::ai::AiAnalysisService::new(kv_store, d1_database);

            // Perform AI market analysis
            match ai_service.analyze_market(&user_id).await {
                Ok(analysis) => {
                    let analysis_data = serde_json::json!({
                        "user_id": user_id,
                        "analysis": analysis,
                        "timestamp": now_system_time()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    });

                    let response = ApiResponse::success(analysis_data);
                    Response::from_json(&response)
                }
                Err(e) => {
                    let response = ApiResponse::<()>::error(format!("AI analysis failed: {}", e));
                    Ok(Response::from_json(&response)?.with_status(500))
                }
            }
        }
        Ok(None) => {
            let response = ApiResponse::<()>::error("User profile not found".to_string());
            Ok(Response::from_json(&response)?.with_status(404))
        }
        Err(e) => {
            let response =
                ApiResponse::<()>::error(format!("Failed to verify user permissions: {}", e));
            Ok(Response::from_json(&response)?.with_status(500))
        }
    }
}
