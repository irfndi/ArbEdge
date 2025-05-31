// src/services/interfaces/telegram/core/mod.rs

//! Core Telegram functionality
//! 
//! This module contains the core Telegram bot functionality including:
//! - Bot client for API communication
//! - Message handling and processing
//! - Webhook processing
//! - Basic bot operations

pub mod bot_client;
pub mod message_handler;
pub mod webhook_handler;

pub use bot_client::*;
pub use message_handler::*;
pub use webhook_handler::*; 