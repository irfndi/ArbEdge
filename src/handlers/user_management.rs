use crate::middleware::extract_user_id_from_headers;
use crate::responses::ApiResponse;
use crate::services;
use crate::types;
use worker::{Env, Request, Response, Result};

/// Get user profile endpoint
pub async fn handle_api_get_user_profile(req: Request, env: Env) -> Result<Response> {
    let user_id = match extract_user_id_from_headers(&req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Get encryption key from environment
    let encryption_key = env
        .var("ENCRYPTION_KEY")
        .map(|secret| secret.to_string())
        .map_err(|_| {
            worker::Error::RustError(
                "ENCRYPTION_KEY environment variable is required for security".to_string(),
            )
        })?;

    // Initialize services
    let kv_store = env.kv("ArbEdgeKV")?;
    let d1_service = services::core::infrastructure::D1Service::new(&env)?;
    let user_profile_service = services::core::user::user_profile::UserProfileService::new(
        kv_store,
        d1_service,
        encryption_key,
    );

    // Fetch real user profile from database
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            let profile_data = serde_json::json!({
                "user_id": profile.user_id,
                "telegram_user_id": profile.telegram_user_id,
                "telegram_username": profile.telegram_username,
                "subscription": {
                    "tier": profile.subscription.tier,
                    "is_active": profile.subscription.is_active,
                    "expires_at": profile.subscription.expires_at,
                    "features": profile.subscription.features
                },
                "configuration": profile.configuration,
                "created_at": profile.created_at,
                "updated_at": profile.updated_at,
                "last_active": profile.last_active,
                "is_active": profile.is_active,
                "total_trades": profile.total_trades,
                "total_pnl_usdt": profile.total_pnl_usdt,
                "account_balance_usdt": profile.account_balance_usdt
            });

            let response = ApiResponse::success(profile_data);
            Response::from_json(&response)
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

/// Update user profile endpoint
pub async fn handle_api_update_user_profile(mut req: Request, env: Env) -> Result<Response> {
    let user_id = match extract_user_id_from_headers(&req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Parse and validate the update request
    let update_request: types::UpdateUserProfileRequest = match req.json().await {
        Ok(data) => data,
        Err(e) => {
            let response = ApiResponse::<()>::error(format!("Invalid JSON format: {}", e));
            return Ok(Response::from_json(&response)?.with_status(400));
        }
    };

    // Validate the request
    if let Err(validation_error) = update_request.validate() {
        let response = ApiResponse::<()>::error(format!("Validation error: {}", validation_error));
        return Ok(Response::from_json(&response)?.with_status(400));
    }

    // Get encryption key from environment
    let encryption_key = env
        .var("ENCRYPTION_KEY")
        .map(|secret| secret.to_string())
        .map_err(|_| {
            worker::Error::RustError("ENCRYPTION_KEY environment variable is required for security. Application cannot start without proper encryption key.".to_string())
        })?;

    // Initialize services
    let kv_store = env.kv("ArbEdgeKV")?;
    let d1_service = services::core::infrastructure::D1Service::new(&env)?;
    let user_profile_service = services::core::user::user_profile::UserProfileService::new(
        kv_store,
        d1_service,
        encryption_key,
    );

    // Update user profile in database
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(mut profile)) => {
            // Apply validated updates to the profile
            match update_request.apply_to_profile(&mut profile) {
                Ok(_) => {
                    // Update the profile in database
                    match user_profile_service.update_user_profile(&profile).await {
                        Ok(_) => {
                            let response = ApiResponse::success(serde_json::json!({
                                "user_id": user_id,
                                "updated": true,
                                "profile": {
                                    "user_id": profile.user_id,
                                    "telegram_username": profile.telegram_username,
                                    "configuration": profile.configuration,
                                    "updated_at": profile.updated_at
                                },
                                "timestamp": chrono::Utc::now().timestamp()
                            }));
                            Response::from_json(&response)
                        }
                        Err(e) => {
                            let response = ApiResponse::<()>::error(format!(
                                "Failed to update user profile: {}",
                                e
                            ));
                            Ok(Response::from_json(&response)?.with_status(500))
                        }
                    }
                }
                Err(e) => {
                    let response =
                        ApiResponse::<()>::error(format!("Failed to apply updates: {}", e));
                    Ok(Response::from_json(&response)?.with_status(400))
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

/// Get user preferences endpoint
pub async fn handle_api_get_user_preferences(req: Request, env: Env) -> Result<Response> {
    let user_id = match extract_user_id_from_headers(&req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Get encryption key from environment
    let encryption_key = env
        .var("ENCRYPTION_KEY")
        .map(|secret| secret.to_string())
        .map_err(|_| {
            worker::Error::RustError("ENCRYPTION_KEY environment variable is required for security. Application cannot start without proper encryption key.".to_string())
        })?;

    // Initialize services
    let kv_store = env.kv("ArbEdgeKV")?;
    let d1_service = services::core::infrastructure::D1Service::new(&env)?;
    let user_profile_service = services::core::user::user_profile::UserProfileService::new(
        kv_store,
        d1_service,
        encryption_key,
    );

    // Fetch user profile from database
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            let preferences = serde_json::json!({
                "user_id": user_id,
                "risk_tolerance_percentage": profile.configuration.risk_tolerance_percentage,
                "trading_pairs": profile.configuration.trading_pairs,
                "auto_trading_enabled": profile.configuration.auto_trading_enabled,
                "max_leverage": profile.configuration.max_leverage,
                "max_entry_size_usdt": profile.configuration.max_entry_size_usdt,
                "min_entry_size_usdt": profile.configuration.min_entry_size_usdt,
                "opportunity_threshold": profile.configuration.opportunity_threshold,
                "notification_preferences": profile.configuration.notification_preferences,
                "excluded_pairs": profile.configuration.excluded_pairs,
                "timestamp": chrono::Utc::now().timestamp()
            });

            let response = ApiResponse::success(preferences);
            Response::from_json(&response)
        }
        Ok(None) => {
            let response = ApiResponse::<()>::error("User profile not found".to_string());
            Ok(Response::from_json(&response)?.with_status(404))
        }
        Err(e) => {
            let response =
                ApiResponse::<()>::error(format!("Failed to fetch user preferences: {}", e));
            Ok(Response::from_json(&response)?.with_status(500))
        }
    }
}

/// Update user preferences endpoint
pub async fn handle_api_update_user_preferences(mut req: Request, env: Env) -> Result<Response> {
    let user_id = match extract_user_id_from_headers(&req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Parse and validate the update request
    let update_request: types::UpdateUserPreferencesRequest = match req.json().await {
        Ok(data) => data,
        Err(e) => {
            let response = ApiResponse::<()>::error(format!("Invalid JSON format: {}", e));
            return Ok(Response::from_json(&response)?.with_status(400));
        }
    };

    // Validate the request
    if let Err(validation_error) = update_request.validate() {
        let response = ApiResponse::<()>::error(format!("Validation error: {}", validation_error));
        return Ok(Response::from_json(&response)?.with_status(400));
    }

    // Get encryption key from environment
    let encryption_key = env
        .var("ENCRYPTION_KEY")
        .map(|secret| secret.to_string())
        .map_err(|_| {
            worker::Error::RustError("ENCRYPTION_KEY environment variable is required for security. Application cannot start without proper encryption key.".to_string())
        })?;

    // Initialize services
    let kv_store = env.kv("ArbEdgeKV")?;
    let d1_service = services::core::infrastructure::D1Service::new(&env)?;
    let user_profile_service = services::core::user::user_profile::UserProfileService::new(
        kv_store,
        d1_service,
        encryption_key,
    );

    // Update user preferences in database
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(mut profile)) => {
            // Apply validated updates to the profile
            match update_request.apply_to_profile(&mut profile) {
                Ok(_) => {
                    // Update the profile in database
                    match user_profile_service.update_user_profile(&profile).await {
                        Ok(_) => {
                            let response = ApiResponse::success(serde_json::json!({
                                "user_id": user_id,
                                "updated": true,
                                "preferences": {
                                    "risk_tolerance_percentage": profile.configuration.risk_tolerance_percentage,
                                    "trading_pairs": profile.configuration.trading_pairs,
                                    "auto_trading_enabled": profile.configuration.auto_trading_enabled,
                                    "max_leverage": profile.configuration.max_leverage,
                                    "max_entry_size_usdt": profile.configuration.max_entry_size_usdt,
                                    "min_entry_size_usdt": profile.configuration.min_entry_size_usdt,
                                    "opportunity_threshold": profile.configuration.opportunity_threshold,
                                    "notification_preferences": profile.configuration.notification_preferences,
                                    "excluded_pairs": profile.configuration.excluded_pairs
                                },
                                "timestamp": chrono::Utc::now().timestamp()
                            }));
                            Response::from_json(&response)
                        }
                        Err(e) => {
                            let response = ApiResponse::<()>::error(format!(
                                "Failed to update user preferences: {}",
                                e
                            ));
                            Ok(Response::from_json(&response)?.with_status(500))
                        }
                    }
                }
                Err(e) => {
                    let response =
                        ApiResponse::<()>::error(format!("Failed to apply updates: {}", e));
                    Ok(Response::from_json(&response)?.with_status(400))
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
