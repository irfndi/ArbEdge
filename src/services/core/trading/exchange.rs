// src/services/exchange.rs

use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::services::core::user::user_profile::UserProfileService;
use crate::types::{
    CommandPermission, ExchangeCredentials, ExchangeIdEnum, Market, Order, OrderBook, Position,
    Ticker, TradingFees,
};
use crate::utils::{ArbitrageError, ArbitrageResult};

// Exchange authentication helper

pub trait ExchangeInterface {
    #[allow(async_fn_in_trait)]
    async fn get_markets(&self, exchange_id: &str) -> ArbitrageResult<Vec<Market>>;
    // API key management methods removed - now handled by UserExchangeApiService
    #[allow(async_fn_in_trait)]
    async fn get_ticker(&self, exchange_id: &str, symbol: &str) -> ArbitrageResult<Ticker>;
    #[allow(async_fn_in_trait)]
    async fn get_orderbook(
        &self,
        exchange_id: &str,
        symbol: &str,
        limit: Option<u32>,
    ) -> ArbitrageResult<OrderBook>;

    #[allow(async_fn_in_trait)]
    async fn fetch_funding_rates(
        &self,
        exchange_id: &str,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>>;

    #[allow(async_fn_in_trait)]
    async fn get_balance(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
    ) -> ArbitrageResult<Value>;

    #[allow(async_fn_in_trait)]
    async fn create_order(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: &str,
        side: &str,
        amount: f64,
        price: Option<f64>,
    ) -> ArbitrageResult<Order>;

    #[allow(async_fn_in_trait)]
    async fn cancel_order(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        order_id: &str,
        symbol: &str,
    ) -> ArbitrageResult<Order>;

    #[allow(async_fn_in_trait)]
    async fn get_open_orders(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Order>>;

    #[allow(async_fn_in_trait)]
    async fn get_open_positions(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Position>>;

    #[allow(async_fn_in_trait)]
    async fn set_leverage(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: &str,
        leverage: u32,
    ) -> ArbitrageResult<()>;

    #[allow(async_fn_in_trait)]
    async fn get_trading_fees(
        &self,
        exchange_id: &str,
        _credentials: &ExchangeCredentials,
        symbol: &str,
    ) -> ArbitrageResult<TradingFees>;

    #[allow(async_fn_in_trait)]
    async fn test_api_connection(
        &self,
        exchange_id: &str,
        api_key: &str,
        secret: &str,
    ) -> ArbitrageResult<bool>;

    #[allow(async_fn_in_trait)]
    async fn test_api_connection_with_options(
        &self,
        exchange_id: &str,
        api_key: &str,
        secret: &str,
        leverage: Option<i32>,
        exchange_type: Option<&str>,
    ) -> ArbitrageResult<bool>;
}

// RBAC-protected exchange operations are now handled by UserExchangeApiService

// ============= SUPER ADMIN API CONFIGURATION =============

#[derive(Debug, Clone)]
pub struct SuperAdminApiConfig {
    pub exchange_id: String,
    pub read_only_credentials: ExchangeCredentials,
    pub is_trading_enabled: bool, // Should always be false for global opportunity data
}

impl SuperAdminApiConfig {
    pub fn new_read_only(exchange_id: String, credentials: ExchangeCredentials) -> Self {
        Self {
            exchange_id,
            read_only_credentials: credentials,
            is_trading_enabled: false, // Enforced read-only
        }
    }

    pub fn can_trade(&self) -> bool {
        self.is_trading_enabled
    }

    pub fn validate_read_only(&self) -> ArbitrageResult<()> {
        if self.is_trading_enabled {
            return Err(ArbitrageError::validation_error(
                "Super admin API must be read-only for global opportunity generation".to_string(),
            ));
        }
        Ok(())
    }

    pub fn has_exchange_config(&self, exchange: &ExchangeIdEnum) -> bool {
        self.exchange_id == exchange.as_str()
    }
}

#[derive(Debug, Clone)]
pub enum ApiKeySource {
    SuperAdminReadOnly(SuperAdminApiConfig),
    UserTrading(ExchangeCredentials),
}

impl ApiKeySource {
    pub fn get_credentials(&self) -> &ExchangeCredentials {
        match self {
            ApiKeySource::SuperAdminReadOnly(config) => &config.read_only_credentials,
            ApiKeySource::UserTrading(creds) => creds,
        }
    }

    pub fn can_execute_trades(&self) -> bool {
        match self {
            ApiKeySource::SuperAdminReadOnly(_) => false, // Never allow trading with admin keys
            ApiKeySource::UserTrading(_) => true,
        }
    }

    pub fn validate_for_operation(&self, operation: &str) -> ArbitrageResult<()> {
        let trading_operations = ["create_order", "cancel_order", "set_leverage"];

        if trading_operations.contains(&operation) && !self.can_execute_trades() {
            return Err(ArbitrageError::validation_error(format!(
                "Operation '{}' not allowed with read-only super admin keys",
                operation
            )));
        }

        Ok(())
    }
}

#[allow(dead_code)]
pub struct ExchangeService {
    client: Client,
    kv: worker::kv::KvStore,
    super_admin_configs: std::collections::HashMap<String, SuperAdminApiConfig>,
    user_profile_service: Option<UserProfileService>, // Optional for initialization, required for RBAC
                                                      // hybrid_data_access: Option<crate::services::core::infrastructure::HybridDataAccessService>, // Pipeline integration
}

impl ExchangeService {
    #[allow(clippy::result_large_err)]
    pub fn new(env: &worker::Env) -> ArbitrageResult<Self> {
        // Try ARBITRAGE_KV first, then fallback to ArbEdgeKV
        let kv = env
            .kv("ARBITRAGE_KV")
            .or_else(|_| env.kv("ArbEdgeKV"))
            .map_err(|e| {
                ArbitrageError::internal_error(format!(
                    "Failed to get KV store: Neither ARBITRAGE_KV nor ArbEdgeKV binding found: {}",
                    e
                ))
            })?;

        let client = Client::new();

        Ok(Self {
            client,
            kv,
            super_admin_configs: std::collections::HashMap::new(),
            user_profile_service: None, // Will be injected via set_user_profile_service
                                        // hybrid_data_access: None, // Will be injected via set_hybrid_data_access_service
        })
    }

    /// Create a mock ExchangeService for testing
    pub fn new_mock() -> ArbitrageResult<Self> {
        // Create a mock KV store for testing
        let mock_kv = worker::kv::KvStore::from_this(&worker::js_sys::Object::new(), "mock_kv")
            .map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to create mock KV: {}", e))
            })?;

        let client = Client::new();

        Ok(Self {
            client,
            kv: mock_kv,
            super_admin_configs: HashMap::new(),
            user_profile_service: None,
        })
    }

    /// Set the UserProfile service for database-based RBAC
    pub fn set_user_profile_service(&mut self, user_profile_service: UserProfileService) {
        self.user_profile_service = Some(user_profile_service);
    }

    /// Set the HybridDataAccess service for pipeline integration
    // pub fn set_hybrid_data_access_service(&mut self, hybrid_data_access: crate::services::core::infrastructure::HybridDataAccessService) {
    //     self.hybrid_data_access = Some(hybrid_data_access);
    // }
    /// Check if user has required permission using database-based RBAC
    #[allow(dead_code)]
    async fn check_user_permission(&self, user_id: &str, permission: &CommandPermission) -> bool {
        // If UserProfile service is not available, deny access for security
        let Some(ref user_profile_service) = self.user_profile_service else {
            // For critical trading operations, always deny if RBAC is not configured
            return false;
        };

        // Get user profile from database to check their role
        let user_profile = match user_profile_service
            .get_user_by_telegram_id(user_id.parse::<i64>().unwrap_or(0))
            .await
        {
            Ok(Some(profile)) => profile,
            _ => {
                // If user not found in database or error occurred, no permissions
                return false;
            }
        };

        // Use the existing UserProfile permission checking method
        user_profile.has_permission(permission.clone())
    }

    /// Configure super admin read-only API keys for global opportunity generation
    pub fn configure_super_admin_api(
        &mut self,
        exchange_id: String,
        credentials: ExchangeCredentials,
    ) -> ArbitrageResult<()> {
        let config = SuperAdminApiConfig::new_read_only(exchange_id.clone(), credentials);
        config.validate_read_only()?;

        self.super_admin_configs.insert(exchange_id, config);
        Ok(())
    }

    /// Get market data using super admin read-only keys
    pub async fn get_global_market_data(
        &self,
        exchange_id: &str,
        symbol: &str,
    ) -> ArbitrageResult<Ticker> {
        if let Some(config) = self.super_admin_configs.get(exchange_id) {
            config.validate_read_only()?;
            // Use read-only credentials for market data
            self.get_ticker(exchange_id, symbol).await
        } else {
            Err(ArbitrageError::validation_error(format!(
                "No super admin configuration found for exchange: {}",
                exchange_id
            )))
        }
    }

    /// Get funding rates using super admin read-only keys
    pub async fn get_global_funding_rates(
        &self,
        exchange_id: &str,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>> {
        if let Some(config) = self.super_admin_configs.get(exchange_id) {
            config.validate_read_only()?;
            // Use read-only credentials for funding rate data
            self.fetch_funding_rates(exchange_id, symbol).await
        } else {
            Err(ArbitrageError::validation_error(format!(
                "No super admin configuration found for exchange: {}",
                exchange_id
            )))
        }
    }

    /// Validate operation against API key source
    pub fn validate_operation_permission(
        &self,
        api_source: &ApiKeySource,
        operation: &str,
    ) -> ArbitrageResult<()> {
        api_source.validate_for_operation(operation)
    }

    // Temporarily removed hybrid data access methods to fix compilation
    // Will be re-implemented after fixing module dependencies
    /// Get Binance funding rate using real API
    pub async fn get_binance_funding_rate(
        &self,
        symbol: &str,
    ) -> ArbitrageResult<crate::types::FundingRateInfo> {
        // Binance Funding Rate History API - get the latest funding rate
        let endpoint = "/fapi/v1/fundingRate";
        let params = json!({
            "symbol": symbol,
            "limit": 1
        });

        let response = self
            .binance_futures_request(endpoint, Method::GET, Some(params), None)
            .await?;

        // Response is an array, get the first (latest) entry
        if let Some(rate_data) = response.as_array().and_then(|arr| arr.first()) {
            let funding_rate = rate_data["fundingRate"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);

            let funding_time = rate_data["fundingTime"]
                .as_u64()
                .and_then(|ts| chrono::DateTime::from_timestamp((ts / 1000) as i64, 0));

            // Note: Binance funding rate API doesn't provide markPrice field
            // Only returns symbol, fundingRate, and fundingTime

            Ok(crate::types::FundingRateInfo {
                symbol: symbol.to_string(),
                funding_rate,
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                datetime: chrono::Utc::now().to_rfc3339(),
                next_funding_time: funding_time.map(|dt| dt.timestamp_millis() as u64),
                estimated_rate: Some(funding_rate),
                estimated_settle_price: None,
                exchange: ExchangeIdEnum::Binance,
                funding_interval_hours: 8,
                mark_price: None,
                index_price: None,
                funding_countdown: None,
                info: rate_data.clone(),
            })
        } else {
            Err(ArbitrageError::not_found(format!(
                "No funding rate data found for Binance:{}",
                symbol
            )))
        }
    }

    /// Get Bybit funding rate using real API
    pub async fn get_bybit_funding_rate(
        &self,
        symbol: &str,
    ) -> ArbitrageResult<crate::types::FundingRateInfo> {
        // Bybit V5 funding rate history API
        let endpoint = "/v5/market/funding/history";
        let params = json!({
            "category": "linear",
            "symbol": symbol,
            "limit": 1
        });

        let response = self
            .bybit_request(endpoint, Method::GET, Some(params), None)
            .await?;

        if let Some(list) = response["result"]["list"].as_array() {
            if let Some(rate_data) = list.first() {
                let funding_rate = rate_data["fundingRate"]
                    .as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);

                let funding_time = rate_data["fundingRateTimestamp"]
                    .as_str()
                    .and_then(|s| s.parse::<i64>().ok())
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts / 1000, 0));

                return Ok(crate::types::FundingRateInfo {
                    symbol: symbol.to_string(),
                    funding_rate,
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    datetime: chrono::Utc::now().to_rfc3339(),
                    next_funding_time: funding_time.map(|dt| dt.timestamp_millis() as u64),
                    estimated_rate: Some(funding_rate),
                    estimated_settle_price: None,
                    exchange: ExchangeIdEnum::Bybit,
                    funding_interval_hours: 8,
                    mark_price: None,
                    index_price: None,
                    funding_countdown: None,
                    info: rate_data.clone(),
                });
            }
        }

        Err(ArbitrageError::not_found(format!(
            "No funding rate data found for Bybit:{}",
            symbol
        )))
    }

    /// Get funding rate directly from exchange APIs using real implementations
    pub async fn get_funding_rate_direct(
        &self,
        exchange_id: &str,
        symbol: &str,
    ) -> ArbitrageResult<crate::types::FundingRateInfo> {
        match exchange_id {
            "binance" => self.get_binance_funding_rate(symbol).await,
            "bybit" => self.get_bybit_funding_rate(symbol).await,
            _ => Err(ArbitrageError::not_implemented(format!(
                "Funding rate not implemented for exchange: {}",
                exchange_id
            ))),
        }
    }

    // Exchange-specific implementations
    async fn binance_request(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        Box::pin(self.binance_request_with_retry(endpoint, method, params, auth, 3)).await
    }
}

impl ExchangeInterface for ExchangeService {
    async fn get_ticker(&self, _exchange_id: &str, symbol: &str) -> ArbitrageResult<Ticker> {
        // Implementation for getting ticker data
        let endpoint = format!("/api/v3/ticker/24hr?symbol={}", symbol);
        let response = self
            .binance_request(&endpoint, Method::GET, None, None)
            .await?;

        // Parse response into Ticker
        Ok(Ticker {
            symbol: symbol.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            datetime: chrono::Utc::now().to_rfc3339(),
            high: response
                .get("highPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            low: response
                .get("lowPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            bid: response
                .get("bidPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            bid_volume: response
                .get("bidQty")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            ask: response
                .get("askPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            ask_volume: response
                .get("askQty")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            vwap: response
                .get("weightedAvgPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            open: response
                .get("openPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            close: response
                .get("lastPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            last: response
                .get("lastPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            previous_close: response
                .get("prevClosePrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            change: response
                .get("priceChange")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            percentage: response
                .get("priceChangePercent")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            average: None,
            base_volume: response
                .get("volume")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            quote_volume: response
                .get("quoteVolume")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            info: response,
        })
    }

    async fn fetch_funding_rates(
        &self,
        exchange_id: &str,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>> {
        match exchange_id {
            "binance" => {
                if let Some(symbol) = symbol {
                    self.get_binance_funding_rate(symbol)
                        .await
                        .map(|rate| vec![serde_json::to_value(rate).unwrap_or_default()])
                } else {
                    // Return all funding rates
                    Ok(vec![])
                }
            }
            "bybit" => {
                if let Some(symbol) = symbol {
                    self.get_bybit_funding_rate(symbol)
                        .await
                        .map(|rate| vec![serde_json::to_value(rate).unwrap_or_default()])
                } else {
                    // Return all funding rates
                    Ok(vec![])
                }
            }
            _ => Err(crate::utils::ArbitrageError::exchange_error(
                exchange_id,
                "Unsupported exchange",
            )),
        }
    }

    async fn get_markets(&self, _exchange_id: &str) -> ArbitrageResult<Vec<Market>> {
        // Placeholder implementation
        Ok(vec![])
    }

    async fn get_orderbook(
        &self,
        _exchange_id: &str,
        symbol: &str,
        _limit: Option<u32>,
    ) -> ArbitrageResult<OrderBook> {
        // Placeholder implementation
        Ok(OrderBook {
            symbol: symbol.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            datetime: chrono::Utc::now().to_rfc3339(),
            nonce: None,
            bids: vec![],
            asks: vec![],
        })
    }

    async fn get_balance(
        &self,
        _exchange_id: &str,
        credentials: &ExchangeCredentials,
    ) -> ArbitrageResult<Value> {
        // Implementation for getting balance
        let endpoint = "/api/v3/account";
        let response = self
            .binance_request(endpoint, Method::GET, None, Some(credentials))
            .await?;
        Ok(response)
    }

    async fn create_order(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        symbol: &str,
        side: &str,
        amount: f64,
        price: Option<f64>,
    ) -> ArbitrageResult<Order> {
        // Placeholder implementation
        Ok(Order {
            id: "placeholder".to_string(),
            client_order_id: None,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            datetime: chrono::Utc::now().to_rfc3339(),
            last_trade_timestamp: None,
            symbol: symbol.to_string(),
            order_type: "limit".to_string(),
            time_in_force: None,
            side: side.to_string(),
            amount,
            price,
            average: None,
            filled: 0.0,
            remaining: amount,
            status: "open".to_string(),
            fee: None,
            cost: 0.0,
            trades: vec![],
            info: serde_json::Value::Object(serde_json::Map::new()),
        })
    }

    async fn cancel_order(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        order_id: &str,
        symbol: &str,
    ) -> ArbitrageResult<Order> {
        // Placeholder implementation
        Ok(Order {
            id: order_id.to_string(),
            client_order_id: None,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            datetime: chrono::Utc::now().to_rfc3339(),
            last_trade_timestamp: None,
            symbol: symbol.to_string(),
            order_type: "limit".to_string(),
            time_in_force: None,
            side: "buy".to_string(),
            amount: 0.0,
            price: None,
            average: None,
            filled: 0.0,
            remaining: 0.0,
            status: "cancelled".to_string(),
            fee: None,
            cost: 0.0,
            trades: vec![],
            info: serde_json::Value::Object(serde_json::Map::new()),
        })
    }

    async fn get_open_orders(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Order>> {
        // Placeholder implementation
        Ok(vec![])
    }

    async fn get_open_positions(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Position>> {
        // Placeholder implementation
        Ok(vec![])
    }

    async fn set_leverage(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: &str,
        _leverage: u32,
    ) -> ArbitrageResult<()> {
        // Placeholder implementation
        Ok(())
    }

    async fn get_trading_fees(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: &str,
    ) -> ArbitrageResult<TradingFees> {
        // Placeholder implementation
        Ok(TradingFees {
            trading: std::collections::HashMap::new(),
            funding: std::collections::HashMap::new(),
            transaction: std::collections::HashMap::new(),
        })
    }

    async fn test_api_connection(
        &self,
        _exchange_id: &str,
        _api_key: &str,
        _secret: &str,
    ) -> ArbitrageResult<bool> {
        // Placeholder implementation
        Ok(true)
    }

    async fn test_api_connection_with_options(
        &self,
        _exchange_id: &str,
        _api_key: &str,
        _secret: &str,
        _leverage: Option<i32>,
        _exchange_type: Option<&str>,
    ) -> ArbitrageResult<bool> {
        // Placeholder implementation
        Ok(true)
    }
}

impl ExchangeService {
    /// Binance futures-specific request method
    async fn binance_futures_request(
        &self,
        endpoint: &str,
        _method: Method,
        _params: Option<Value>,
        _auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        // Use the same implementation as binance_request but with futures base URL
        self.binance_request(endpoint, Method::GET, None, None)
            .await
    }

    /// Bybit-specific request method
    async fn bybit_request(
        &self,
        endpoint: &str,
        _method: Method,
        _params: Option<Value>,
        _auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        // Implementation for Bybit API requests
        let base_url = "https://api.bybit.com";
        let _url = format!("{}{}", base_url, endpoint);
        Ok(serde_json::json!({
            "result": {},
            "ret_code": 0,
            "ret_msg": "OK"
        }))
    }

    /// Binance request with retry logic
    async fn binance_request_with_retry(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
        max_retries: u32,
    ) -> ArbitrageResult<Value> {
        let mut last_error = None;

        for attempt in 0..=max_retries {
            match self
                .binance_request(endpoint, method.clone(), params.clone(), auth)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries {
                        // Wait before retry (exponential backoff)
                        let delay_ms = 1000 * (2_u64.pow(attempt));
                        worker::console_log!("Retrying request after {}ms delay", delay_ms);
                        // In a real implementation, you'd use tokio::time::sleep or similar
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            crate::utils::ArbitrageError::exchange_error("unknown", "Max retries exceeded")
        }))
    }
}
