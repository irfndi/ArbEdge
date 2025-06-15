// src/utils/mod.rs

pub mod cache_headers;
pub mod calculations;
pub mod core_architecture;
pub mod error;
pub mod feature_flags;
pub mod formatter;
pub mod helpers;
pub mod kv_standards;
pub mod logger;
pub mod time; // Added time module

// Re-export commonly used items
pub use cache_headers::{add_cache_headers, cache_type_from_path, with_cache_headers, CacheType};
pub use core_architecture::{
    CoreServiceArchitecture, HealthCheckResult, HealthCheckable, ServiceConfig, ServiceDependency,
    ServiceInfo, ServiceLifecycle, ServiceRegistryEntry, ServiceStatus, ServiceType,
    SystemHealthOverview,
};
pub use error::{ArbitrageError, ArbitrageResult};
pub use helpers::{generate_api_key, generate_secret_key, generate_uuid, validate_api_key};
pub use time::{get_current_timestamp, TimeService}; // Added re-export for TimeService and get_current_timestamp
