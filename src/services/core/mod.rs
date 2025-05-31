// src/services/core/mod.rs

pub mod admin;
pub mod auth;
pub mod infrastructure;
pub mod opportunities;
pub mod trading;
pub mod user;
pub mod analysis;
pub mod ai;
pub mod invitation;
pub mod market_data;
pub mod analytics;

// Re-export all services for convenience
pub use user::*;
pub use trading::*;
pub use opportunities::*;
pub use analysis::*;
pub use ai::*;
pub use infrastructure::*;
pub use invitation::*;
pub use market_data::*;
pub use auth::*;
pub use admin::*;

// Re-export admin services for easier access
pub use admin::{AdminService, UserManagementService, SystemConfigService, MonitoringService, AuditService}; 