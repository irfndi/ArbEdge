use worker::{Request, Result};

/// Extract user ID from request headers
pub fn extract_user_id_from_headers(req: &Request) -> Result<String> {
    req.headers()
        .get("X-User-ID")?
        .ok_or_else(|| worker::Error::RustError("Missing X-User-ID header".to_string()))
}
