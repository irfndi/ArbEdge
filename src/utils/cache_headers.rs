// Cache Headers Module - Production-Ready Cloudflare Edge Caching Support
// Provides comprehensive cache header management for optimal Cloudflare performance

use worker::Response;

/// Cache types with specific TTL and header configurations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CacheType {
    /// Market data: Short TTL, frequently updated
    MarketData,
    /// Opportunities: Short TTL, real-time updates needed
    Opportunities,
    /// User profiles: Medium TTL, personalized content
    UserProfile,
    /// Analytics: Medium TTL, aggregated data
    Analytics,
    /// Static content: Long TTL, immutable
    Static,
    /// Health checks: Very short TTL, monitoring endpoints
    Health,
    /// Trading data: Short TTL, critical for trading decisions
    Trading,
    /// AI responses: Medium TTL, computationally expensive
    AiResponse,
    /// Admin endpoints: No cache, always fresh
    Admin,
    /// Auth endpoints: No cache, security sensitive
    Auth,
}

/// Cache configuration for each type
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub max_age: u64,
    pub s_maxage: Option<u64>,
    pub stale_while_revalidate: Option<u64>,
    pub stale_if_error: Option<u64>,
    pub must_revalidate: bool,
    pub no_cache: bool,
    pub no_store: bool,
    pub public: bool,
    pub immutable: bool,
    pub vary: Vec<String>,
    pub etag_generation: bool,
}

impl CacheType {
    /// Get cache configuration for the cache type
    pub fn config(&self) -> CacheConfig {
        match self {
            CacheType::MarketData => CacheConfig {
                max_age: 30,                      // 30 seconds for market data
                s_maxage: Some(30),               // Same for edge caching
                stale_while_revalidate: Some(60), // Allow stale for 1 minute while revalidating
                stale_if_error: Some(300),        // Allow stale for 5 minutes if origin fails
                must_revalidate: false,
                no_cache: false,
                no_store: false,
                public: true,
                immutable: false,
                vary: vec!["Accept-Encoding".to_string()],
                etag_generation: true,
            },
            CacheType::Opportunities => CacheConfig {
                max_age: 15,        // 15 seconds for opportunities
                s_maxage: Some(15), // Very fresh for trading opportunities
                stale_while_revalidate: Some(30),
                stale_if_error: Some(120),
                must_revalidate: false,
                no_cache: false,
                no_store: false,
                public: true,
                immutable: false,
                vary: vec!["Accept-Encoding".to_string(), "X-User-ID".to_string()],
                etag_generation: true,
            },
            CacheType::UserProfile => CacheConfig {
                max_age: 300, // 5 minutes for user profiles
                s_maxage: Some(300),
                stale_while_revalidate: Some(600),
                stale_if_error: Some(1800),
                must_revalidate: false,
                no_cache: false,
                no_store: false,
                public: false, // Private user data
                immutable: false,
                vary: vec!["Accept-Encoding".to_string(), "X-User-ID".to_string()],
                etag_generation: true,
            },
            CacheType::Analytics => CacheConfig {
                max_age: 900, // 15 minutes for analytics
                s_maxage: Some(900),
                stale_while_revalidate: Some(1800),
                stale_if_error: Some(3600),
                must_revalidate: false,
                no_cache: false,
                no_store: false,
                public: true,
                immutable: false,
                vary: vec!["Accept-Encoding".to_string()],
                etag_generation: true,
            },
            CacheType::Static => CacheConfig {
                max_age: 31536000, // 1 year for static content
                s_maxage: Some(31536000),
                stale_while_revalidate: None,
                stale_if_error: None,
                must_revalidate: false,
                no_cache: false,
                no_store: false,
                public: true,
                immutable: true, // Static content is immutable
                vary: vec!["Accept-Encoding".to_string()],
                etag_generation: false, // Not needed for immutable content
            },
            CacheType::Health => CacheConfig {
                max_age: 5, // 5 seconds for health checks
                s_maxage: Some(5),
                stale_while_revalidate: Some(10),
                stale_if_error: Some(30),
                must_revalidate: false,
                no_cache: false,
                no_store: false,
                public: true,
                immutable: false,
                vary: vec!["Accept-Encoding".to_string()],
                etag_generation: false, // Health checks change frequently
            },
            CacheType::Trading => CacheConfig {
                max_age: 10, // 10 seconds for trading data
                s_maxage: Some(10),
                stale_while_revalidate: Some(20),
                stale_if_error: Some(60),
                must_revalidate: false,
                no_cache: false,
                no_store: false,
                public: false, // Private trading data
                immutable: false,
                vary: vec!["Accept-Encoding".to_string(), "X-User-ID".to_string()],
                etag_generation: true,
            },
            CacheType::AiResponse => CacheConfig {
                max_age: 1800, // 30 minutes for AI responses
                s_maxage: Some(1800),
                stale_while_revalidate: Some(3600),
                stale_if_error: Some(7200),
                must_revalidate: false,
                no_cache: false,
                no_store: false,
                public: false, // User-specific AI responses
                immutable: false,
                vary: vec!["Accept-Encoding".to_string(), "X-User-ID".to_string()],
                etag_generation: true,
            },
            CacheType::Admin => CacheConfig {
                max_age: 0,
                s_maxage: Some(0),
                stale_while_revalidate: None,
                stale_if_error: None,
                must_revalidate: true,
                no_cache: true,
                no_store: true, // Never cache admin endpoints
                public: false,
                immutable: false,
                vary: vec!["Accept-Encoding".to_string(), "X-User-ID".to_string()],
                etag_generation: false,
            },
            CacheType::Auth => CacheConfig {
                max_age: 0,
                s_maxage: Some(0),
                stale_while_revalidate: None,
                stale_if_error: None,
                must_revalidate: true,
                no_cache: true,
                no_store: true, // Never cache auth endpoints
                public: false,
                immutable: false,
                vary: vec!["Accept-Encoding".to_string()],
                etag_generation: false,
            },
        }
    }

    /// Get cache type label for headers
    pub fn label(&self) -> &'static str {
        match self {
            CacheType::MarketData => "market-data",
            CacheType::Opportunities => "opportunities",
            CacheType::UserProfile => "user-profile",
            CacheType::Analytics => "analytics",
            CacheType::Static => "static",
            CacheType::Health => "health",
            CacheType::Trading => "trading",
            CacheType::AiResponse => "ai-response",
            CacheType::Admin => "admin",
            CacheType::Auth => "auth",
        }
    }
}

/// Add comprehensive cache headers to a response
pub fn add_cache_headers(
    response: &mut Response,
    cache_type: CacheType,
) -> Result<(), worker::Error> {
    let config = cache_type.config();
    let headers = response.headers_mut();

    // Build Cache-Control header
    let mut cache_control_parts = Vec::new();

    // Public/Private
    if config.public {
        cache_control_parts.push("public".to_string());
    } else {
        cache_control_parts.push("private".to_string());
    }

    // No-cache/No-store
    if config.no_store {
        cache_control_parts.push("no-store".to_string());
    } else if config.no_cache {
        cache_control_parts.push("no-cache".to_string());
    } else {
        // Max-age
        cache_control_parts.push(format!("max-age={}", config.max_age));

        // S-maxage for shared caches (Cloudflare)
        if let Some(s_maxage) = config.s_maxage {
            cache_control_parts.push(format!("s-maxage={}", s_maxage));
        }

        // Stale-while-revalidate
        if let Some(swr) = config.stale_while_revalidate {
            cache_control_parts.push(format!("stale-while-revalidate={}", swr));
        }

        // Stale-if-error
        if let Some(sie) = config.stale_if_error {
            cache_control_parts.push(format!("stale-if-error={}", sie));
        }

        // Must-revalidate
        if config.must_revalidate {
            cache_control_parts.push("must-revalidate".to_string());
        }

        // Immutable
        if config.immutable {
            cache_control_parts.push("immutable".to_string());
        }
    }

    // Set Cache-Control header
    let cache_control = cache_control_parts.join(", ");
    headers.set("Cache-Control", &cache_control)?;

    // Set Vary header
    if !config.vary.is_empty() {
        let vary = config.vary.join(", ");
        headers.set("Vary", &vary)?;
    }

    // Set ETag if enabled
    if config.etag_generation && !config.no_cache && !config.no_store {
        let etag = generate_etag(&cache_type);
        headers.set("ETag", &etag)?;
    }

    // Set custom cache type header for debugging
    headers.set("X-Cache-Type", cache_type.label())?;

    // Set additional performance headers
    headers.set("X-Content-Type-Options", "nosniff")?;

    // Enable compression for Cloudflare
    if !config.vary.contains(&"Accept-Encoding".to_string()) {
        let mut vary = config.vary.clone();
        vary.push("Accept-Encoding".to_string());
        headers.set("Vary", &vary.join(", "))?;
    }

    Ok(())
}

/// Generate ETag for cache validation
fn generate_etag(cache_type: &CacheType) -> String {
    let timestamp = chrono::Utc::now().timestamp();
    let type_label = cache_type.label();

    // Create a weak ETag that incorporates both timestamp and cache type
    // This ensures ETags are unique across different content types
    format!("W/\"{}-{}\"", type_label, timestamp)
}

/// Add cache headers and return response (convenience function)
pub fn with_cache_headers(
    mut response: Response,
    cache_type: CacheType,
) -> Result<Response, worker::Error> {
    add_cache_headers(&mut response, cache_type)?;
    Ok(response)
}

/// Determine cache type from request path
pub fn cache_type_from_path(path: &str) -> CacheType {
    match path {
        // Market data endpoints
        p if p.starts_with("/api/v1/market") => CacheType::MarketData,

        // Opportunities endpoints
        p if p.starts_with("/api/v1/opportunities") => CacheType::Opportunities,

        // User endpoints
        p if p.starts_with("/api/v1/user") => CacheType::UserProfile,

        // Analytics endpoints
        p if p.starts_with("/api/v1/analytics") => CacheType::Analytics,

        // Trading endpoints
        p if p.starts_with("/api/v1/trading") => CacheType::Trading,

        // AI endpoints
        p if p.starts_with("/api/v1/ai") => CacheType::AiResponse,

        // Admin endpoints
        p if p.starts_with("/api/v1/admin") => CacheType::Admin,

        // Auth endpoints
        p if p.starts_with("/auth") => CacheType::Auth,

        // Health endpoints
        p if p.starts_with("/health") => CacheType::Health,

        // Static content
        p if p.ends_with(".js") || p.ends_with(".css") || p.ends_with(".wasm") => CacheType::Static,

        // Default to short cache for unknown endpoints
        _ => CacheType::Health,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_type_configs() {
        // Test market data cache config
        let market_config = CacheType::MarketData.config();
        assert_eq!(market_config.max_age, 30);
        assert!(market_config.public);
        assert!(!market_config.no_cache);
        assert!(market_config.etag_generation);

        // Test admin cache config
        let admin_config = CacheType::Admin.config();
        assert_eq!(admin_config.max_age, 0);
        assert!(!admin_config.public);
        assert!(admin_config.no_cache);
        assert!(admin_config.no_store);
        assert!(!admin_config.etag_generation);

        // Test static cache config
        let static_config = CacheType::Static.config();
        assert_eq!(static_config.max_age, 31536000);
        assert!(static_config.immutable);
        assert!(!static_config.etag_generation);
    }

    #[test]
    fn test_cache_type_from_path() {
        assert_eq!(
            cache_type_from_path("/api/v1/opportunities"),
            CacheType::Opportunities
        );
        assert_eq!(
            cache_type_from_path("/api/v1/user/profile"),
            CacheType::UserProfile
        );
        assert_eq!(
            cache_type_from_path("/api/v1/admin/users"),
            CacheType::Admin
        );
        assert_eq!(cache_type_from_path("/auth/login"), CacheType::Auth);
        assert_eq!(cache_type_from_path("/health"), CacheType::Health);
        assert_eq!(cache_type_from_path("/static/app.js"), CacheType::Static);
    }

    #[test]
    fn test_cache_type_labels() {
        assert_eq!(CacheType::MarketData.label(), "market-data");
        assert_eq!(CacheType::Opportunities.label(), "opportunities");
        assert_eq!(CacheType::UserProfile.label(), "user-profile");
        assert_eq!(CacheType::Admin.label(), "admin");
        assert_eq!(CacheType::Auth.label(), "auth");
    }

    #[test]
    fn test_etag_generation() {
        let etag1 = generate_etag(&CacheType::MarketData);
        let etag2 = generate_etag(&CacheType::Opportunities);

        // ETags should be different for different cache types
        assert_ne!(etag1, etag2);

        // ETags should be weak ETags
        assert!(etag1.starts_with("W/\""));
        assert!(etag2.starts_with("W/\""));

        // ETags should contain the cache type
        assert!(etag1.contains("market-data"));
        assert!(etag2.contains("opportunities"));
    }
}
