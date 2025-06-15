use crate::middleware::extract_user_id_from_headers;
use crate::responses::ApiResponse;
use crate::services;
use crate::services::core::admin::SimpleAdminService;
use crate::types::UserAccessLevel;
use crate::utils::now_system_time;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use worker::{console_log, Env, Request, Response, Result};

/// Get admin users statistics
pub async fn handle_api_admin_get_users(req: Request, env: Env) -> Result<Response> {
    console_log!("👑 Admin get users request");

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

    // Verify admin permissions
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            if profile.access_level != UserAccessLevel::Admin
                && profile.access_level != UserAccessLevel::SuperAdmin
            {
                let response = ApiResponse::<()>::error("Admin access required".to_string());
                return Ok(Response::from_json(&response)?.with_status(403));
            }

            // Initialize admin service
            let admin_service = SimpleAdminService::new(kv_store, d1_database);

            // Get user statistics
            match admin_service.get_user_statistics().await {
                Ok(stats) => {
                    let user_stats = serde_json::json!({
                        "total_users": stats.total_users,
                        "active_users": stats.active_users,
                        "premium_users": stats.premium_users,
                        "users_by_tier": {
                            "free": stats.free_tier_users,
                            "basic": stats.basic_tier_users,
                            "premium": stats.premium_tier_users,
                            "enterprise": stats.enterprise_tier_users
                        },
                        "activity_metrics": {
                            "daily_active_users": stats.daily_active_users,
                            "weekly_active_users": stats.weekly_active_users,
                            "monthly_active_users": stats.monthly_active_users
                        },
                        "registration_trends": {
                            "registrations_today": stats.registrations_today,
                            "registrations_this_week": stats.registrations_this_week,
                            "registrations_this_month": stats.registrations_this_month
                        },
                        "timestamp": now_system_time()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    });

                    let response = ApiResponse::success(user_stats);
                    Response::from_json(&response)
                }
                Err(e) => {
                    let response =
                        ApiResponse::<()>::error(format!("Failed to fetch user statistics: {}", e));
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
                ApiResponse::<()>::error(format!("Failed to verify admin permissions: {}", e));
            Ok(Response::from_json(&response)?.with_status(500))
        }
    }
}

/// Handle super admin system info request
pub async fn handle_api_admin_system_info(_req: Request, _env: Env) -> Result<Response> {
    console_log!("👑 Super admin system info request");

    let user_id = match extract_user_id_from_headers(&_req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Initialize services for admin verification
    let kv_store = _env.kv("ArbEdgeKV")?;
    let d1_database = Arc::new(_env.d1("ArbEdgeD1")?);
    let encryption_key = _env
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

    // Verify super admin permissions
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            if profile.access_level != UserAccessLevel::SuperAdmin {
                let response = ApiResponse::<()>::error("Super admin access required".to_string());
                return Ok(Response::from_json(&response)?.with_status(403));
            }

            // Initialize admin service
            let admin_service = SimpleAdminService::new(kv_store, d1_database);

            // Get comprehensive system info
            match admin_service.get_system_info().await {
                Ok(system_info) => {
                    let response = ApiResponse::success(system_info);
                    Response::from_json(&response)
                }
                Err(e) => {
                    let response =
                        ApiResponse::<()>::error(format!("Failed to fetch system info: {}", e));
                    Ok(Response::from_json(&response)?.with_status(500))
                }
            }
        }
        Ok(None) => {
            let response = ApiResponse::<()>::error("User profile not found".to_string());
            Ok(Response::from_json(&response)?.with_status(404))
        }
        Err(e) => {
            let response = ApiResponse::<()>::error(format!(
                "Failed to verify super admin permissions: {}",
                e
            ));
            Ok(Response::from_json(&response)?.with_status(500))
        }
    }
}

/// Handle super admin get config request
pub async fn handle_api_admin_get_config(_req: Request, _env: Env) -> Result<Response> {
    console_log!("👑 Super admin get config request");

    let user_id = match extract_user_id_from_headers(&_req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Initialize services for admin verification
    let kv_store = _env.kv("ArbEdgeKV")?;
    let d1_database = Arc::new(_env.d1("ArbEdgeD1")?);
    let encryption_key = _env
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

    // Verify super admin permissions
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            if profile.access_level != UserAccessLevel::SuperAdmin {
                let response = ApiResponse::<()>::error("Super admin access required".to_string());
                return Ok(Response::from_json(&response)?.with_status(403));
            }

            // Initialize admin service
            let admin_service = SimpleAdminService::new(kv_store, d1_database);

            // Get system configuration
            match admin_service.get_system_config().await {
                Ok(config) => {
                    let response = ApiResponse::success(config);
                    Response::from_json(&response)
                }
                Err(e) => {
                    let response =
                        ApiResponse::<()>::error(format!("Failed to fetch system config: {}", e));
                    Ok(Response::from_json(&response)?.with_status(500))
                }
            }
        }
        Ok(None) => {
            let response = ApiResponse::<()>::error("User profile not found".to_string());
            Ok(Response::from_json(&response)?.with_status(404))
        }
        Err(e) => {
            let response = ApiResponse::<()>::error(format!(
                "Failed to verify super admin permissions: {}",
                e
            ));
            Ok(Response::from_json(&response)?.with_status(500))
        }
    }
}

/// Handle super admin update config request
pub async fn handle_api_admin_update_config(_req: Request, _env: Env) -> Result<Response> {
    console_log!("👑 Super admin update config request");

    let user_id = match extract_user_id_from_headers(&_req) {
        Ok(id) => id,
        Err(_) => {
            let response = ApiResponse::<()>::error("Authentication required".to_string());
            return Ok(Response::from_json(&response)?.with_status(401));
        }
    };

    // Initialize services for admin verification
    let kv_store = _env.kv("ArbEdgeKV")?;
    let d1_database = Arc::new(_env.d1("ArbEdgeD1")?);
    let encryption_key = _env
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

    // Verify super admin permissions
    match user_profile_service.get_user_profile(&user_id).await {
        Ok(Some(profile)) => {
            if profile.access_level != UserAccessLevel::SuperAdmin {
                let response = ApiResponse::<()>::error("Super admin access required".to_string());
                return Ok(Response::from_json(&response)?.with_status(403));
            }

            // Parse update request
            let mut req = _req;
            let update_request: serde_json::Value = match req.json().await {
                Ok(data) => data,
                Err(e) => {
                    let response = ApiResponse::<()>::error(format!("Invalid JSON format: {}", e));
                    return Ok(Response::from_json(&response)?.with_status(400));
                }
            };

            // Initialize admin service
            let admin_service = SimpleAdminService::new(kv_store, d1_database);

            // Update system configuration
            match admin_service.update_system_config(update_request).await {
                Ok(updated_config) => {
                    let response = ApiResponse::success(serde_json::json!({
                        "updated": true,
                        "config": updated_config,
                        "timestamp": now_system_time()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    }));
                    Response::from_json(&response)
                }
                Err(e) => {
                    let response =
                        ApiResponse::<()>::error(format!("Failed to update system config: {}", e));
                    Ok(Response::from_json(&response)?.with_status(500))
                }
            }
        }
        Ok(None) => {
            let response = ApiResponse::<()>::error("User profile not found".to_string());
            Ok(Response::from_json(&response)?.with_status(404))
        }
        Err(e) => {
            let response = ApiResponse::<()>::error(format!(
                "Failed to verify super admin permissions: {}",
                e
            ));
            Ok(Response::from_json(&response)?.with_status(500))
        }
    }
}
