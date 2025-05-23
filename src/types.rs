// src/types.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::js_sys::Date;

/// Exchange identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::upper_case_acronyms)]
pub enum ExchangeIdEnum {
    Binance,
    Bybit,
    OKX,
    Bitget,
    // Add other exchanges as needed
}

impl ExchangeIdEnum {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExchangeIdEnum::Binance => "binance",
            ExchangeIdEnum::Bybit => "bybit",
            ExchangeIdEnum::OKX => "okx",
            ExchangeIdEnum::Bitget => "bitget",
        }
    }
}

impl std::fmt::Display for ExchangeIdEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// String alias for exchange identifiers (for compatibility with CCXT-like interface)
pub type ExchangeId = String;
pub type TradingPairSymbol = String;

/// Types of arbitrage opportunities
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArbitrageType {
    FundingRate,
    SpotFutures,
    CrossExchange,
}

/// Core arbitrage opportunity structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbitrageOpportunity {
    pub id: String,
    pub pair: String,
    pub long_exchange: Option<ExchangeIdEnum>,
    pub short_exchange: Option<ExchangeIdEnum>,
    pub long_rate: Option<f64>,
    pub short_rate: Option<f64>,
    pub rate_difference: f64,
    pub net_rate_difference: Option<f64>,
    pub potential_profit_value: Option<f64>,
    pub timestamp: u64, // Unix timestamp in milliseconds
    pub r#type: ArbitrageType,
    pub details: Option<String>,
}

impl ArbitrageOpportunity {
    pub fn new(
        pair: String,
        long_exchange: Option<ExchangeIdEnum>,
        short_exchange: Option<ExchangeIdEnum>,
        long_rate: Option<f64>,
        short_rate: Option<f64>,
        rate_difference: f64,
        r#type: ArbitrageType,
    ) -> Self {
        Self {
            id: String::new(),
            pair,
            long_exchange,
            short_exchange,
            long_rate,
            short_rate,
            rate_difference,
            net_rate_difference: None,
            potential_profit_value: None,
            timestamp: Date::now() as u64,
            r#type,
            details: None,
        }
    }

    pub fn with_net_difference(mut self, net_rate_difference: f64) -> Self {
        self.net_rate_difference = Some(net_rate_difference);
        self
    }

    pub fn with_potential_profit(mut self, potential_profit_value: f64) -> Self {
        self.potential_profit_value = Some(potential_profit_value);
        self
    }

    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }
}

/// Exchange rate data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub exchange: ExchangeIdEnum,
    pub pair: String,
    pub rate: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionSide {
    Long,
    Short,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionStatus {
    Open,
    Closed,
    Pending,
}

/// Position data for tracking open positions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbitragePosition {
    pub id: String,
    pub exchange: ExchangeIdEnum,
    pub pair: String,
    pub side: PositionSide,
    pub size: f64,
    pub entry_price: f64,
    pub current_price: Option<f64>,
    pub pnl: Option<f64>,
    pub status: PositionStatus,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Configuration for the application
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub environment: String,
    pub usdt_amount: f64,
    pub bybit_leverage: u32,
    pub binance_leverage: u32,
    pub log_level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            environment: "development".to_string(),
            usdt_amount: 10.0,
            bybit_leverage: 20,
            binance_leverage: 20,
            log_level: "info".to_string(),
        }
    }
}

// Exchange trading types (CCXT-like interface)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    StopMarket,
    StopLimit,
    TrailingStop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderStatus {
    Open,
    Closed,
    Canceled,
    Expired,
    Rejected,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub active: bool,
    pub precision: Precision,
    pub limits: Limits,
    pub fees: Option<TradingFee>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Precision {
    pub amount: Option<i32>,
    pub price: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    pub amount: MinMax,
    pub price: MinMax,
    pub cost: MinMax,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinMax {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol: String,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub last: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub volume: Option<f64>,
    pub timestamp: Option<DateTime<Utc>>,
    pub datetime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<[f64; 2]>, // [price, amount]
    pub asks: Vec<[f64; 2]>, // [price, amount]
    pub timestamp: Option<DateTime<Utc>>,
    pub datetime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub free: f64,
    pub used: f64,
    pub total: f64,
}

pub type Balances = HashMap<String, Balance>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub r#type: OrderType,
    pub side: OrderSide,
    pub amount: f64,
    pub price: Option<f64>,
    pub cost: Option<f64>,
    pub filled: f64,
    pub remaining: f64,
    pub status: OrderStatus,
    pub timestamp: Option<DateTime<Utc>>,
    pub datetime: Option<String>,
    pub fee: Option<Fee>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: Option<String>,
    pub symbol: String,
    pub side: PositionSide,
    pub size: f64,
    pub notional: f64,
    pub entry_price: f64,
    pub mark_price: Option<f64>,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub leverage: f64,
    pub margin: f64,
    pub timestamp: Option<DateTime<Utc>>,
    pub datetime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fee {
    pub currency: String,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingFee {
    pub maker: f64,
    pub taker: f64,
    pub percentage: bool,
}

pub type TradingFeeInterface = TradingFee;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateInfo {
    pub symbol: String,
    pub funding_rate: f64,
    pub timestamp: Option<DateTime<Utc>>,
    pub datetime: Option<String>,
    pub next_funding_time: Option<DateTime<Utc>>,
    pub estimated_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeCredentials {
    pub api_key: String,
    pub secret: String,
    pub default_leverage: i32,
    pub exchange_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredTradingPair {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub exchange_id: String,
}

// Environment and configuration types
pub struct LoggerInterface {
    // Implementation will be in logger.rs
}

pub struct Env {
    // Worker environment interface
    pub arb_edge_kv: worker::Env,
}

// Error types
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Order not found: {0}")]
    OrderNotFound(String),
    #[error("Invalid symbol: {0}")]
    InvalidSymbol(String),
    #[error("Insufficient funds")]
    InsufficientFunds,
    #[error("Rate limit exceeded")]
    RateLimit,
    #[error("Exchange not supported: {0}")]
    ExchangeNotSupported(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub type ExchangeResult<T> = Result<T, ExchangeError>;
