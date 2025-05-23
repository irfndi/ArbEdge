// src/services/mod.rs

pub mod exchange;
pub mod opportunity;
pub mod positions;
pub mod telegram;

// Re-export main service structs
pub use exchange::{ExchangeInterface, ExchangeService};
pub use opportunity::{OpportunityService, OpportunityServiceConfig};
pub use positions::{PositionsService, CreatePositionData, UpdatePositionData};
pub use telegram::{TelegramService, TelegramConfig};
