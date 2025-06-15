// src/services/exchange.rs

use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use worker::Method;

use crate::services::core::infrastructure::cache_manager::CacheManager;
use crate::services::core::infrastructure::enhanced_kv_cache::{
    CacheTier, DataType, KvCacheManager,
};
use crate::services::core::infrastructure::monitoring_module::opportunity_monitor::{
    ApiCallTrackingParams, OpportunityMonitor,
};
use crate::services::core::user::user_exchange_api::RateLimitInfo;
use crate::services::core::user::user_profile::UserProfileService;
use crate::types::{
    CommandPermission, ExchangeCredentials, ExchangeIdEnum, FundingRateInfo, Market, MarketLimits,
    MarketPrecision, MinMax, Order, OrderBook, Position, Ticker, TradingFees,
};
use crate::utils::{now_system_time, ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use worker::console_log;

// Simple global market data structure for internal use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMarketData {
    pub symbol: String,
    pub binance: Option<Ticker>,
    pub bybit: Option<Ticker>,
    pub okx: Option<Ticker>,
    pub coinbase: Option<Ticker>,
    pub timestamp: u64,
}

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
    ) -> ArbitrageResult<(bool, bool, Option<RateLimitInfo>)>;

    #[allow(async_fn_in_trait)]
    async fn test_api_connection_with_options(
        &self,
        exchange_id: &str,
        api_key: &str,
        secret: &str,
        leverage: Option<i32>,
        exchange_type: Option<&str>,
    ) -> ArbitrageResult<(bool, bool, Option<RateLimitInfo>)>;
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
#[derive(Clone)]
pub struct ExchangeService {
    client: Client,
    kv: worker::kv::KvStore,
    super_admin_configs: std::collections::HashMap<String, SuperAdminApiConfig>,
    user_profile_service: Option<UserProfileService>, // Optional for initialization, required for RBAC
    env: worker::Env,                                 // Add environment access for secrets
    cache_manager: Option<std::sync::Arc<CacheManager>>, // Basic cache manager for compatibility
    enhanced_cache: Option<std::sync::Arc<KvCacheManager>>, // Enhanced KV cache for performance
    opportunity_monitor: Option<std::sync::Arc<OpportunityMonitor>>, // Monitoring for API calls
}

impl ExchangeService {
    #[allow(clippy::result_large_err)]
    pub fn new(kv: worker::kv::KvStore, env: worker::Env) -> Self {
        Self {
            client: Client::new(),
            kv,
            super_admin_configs: std::collections::HashMap::new(),
            user_profile_service: None, // Will be injected later
            env,
            cache_manager: None, // Will be injected later for cache-first patterns
            enhanced_cache: None, // Will be injected later for enhanced KV cache
            opportunity_monitor: None, // Will be injected later for monitoring
        }
    }

    /// Create a mock ExchangeService for testing
    pub fn new_mock() -> ArbitrageResult<Self> {
        // Create a mock KV store for testing
        let mock_kv = worker::kv::KvStore::from_this(&worker::js_sys::Object::new(), "mock_kv")
            .map_err(|e| {
                ArbitrageError::internal_error(format!("Failed to create mock KV: {}", e))
            })?;

        let client = Client::new();

        // Create a mock environment for testing
        // In a real test environment, this would be provided by the test framework
        let mock_env = worker::Env::from(worker::wasm_bindgen::JsValue::from(
            worker::js_sys::Object::new(),
        ));

        Ok(Self {
            client,
            kv: mock_kv,
            super_admin_configs: HashMap::new(),
            user_profile_service: None,
            env: mock_env,
            cache_manager: None,
            enhanced_cache: None,
            opportunity_monitor: None,
        })
    }

    /// Set the UserProfile service for database-based RBAC
    pub fn set_user_profile_service(&mut self, user_profile_service: UserProfileService) {
        self.user_profile_service = Some(user_profile_service);
    }

    /// Set cache manager for dependency injection
    pub fn set_cache_manager(&mut self, cache_manager: std::sync::Arc<CacheManager>) {
        self.cache_manager = Some(cache_manager);
    }

    /// Set enhanced KV cache manager for advanced caching features
    pub fn set_enhanced_cache(&mut self, enhanced_cache: std::sync::Arc<KvCacheManager>) {
        self.enhanced_cache = Some(enhanced_cache);
    }

    /// Set opportunity monitor for API call tracking
    pub fn set_opportunity_monitor(&mut self, monitor: std::sync::Arc<OpportunityMonitor>) {
        self.opportunity_monitor = Some(monitor);
    }

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

    /// Get global market data with enhanced multi-tier caching optimization
    pub async fn get_global_market_data(&self, symbol: &str) -> ArbitrageResult<GlobalMarketData> {
        // Use enhanced cache if available, fallback to basic cache
        if let Some(enhanced_cache) = &self.enhanced_cache {
            self.get_global_market_data_enhanced(symbol, enhanced_cache)
                .await
        } else if let Some(cache_manager) = &self.cache_manager {
            self.get_global_market_data_basic(symbol, cache_manager)
                .await
        } else {
            // No cache available - direct API call
            self.fetch_ticker_from_api(symbol).await
        }
    }

    /// Enhanced caching implementation with tier management and warming
    async fn get_global_market_data_enhanced(
        &self,
        symbol: &str,
        enhanced_cache: &KvCacheManager,
    ) -> ArbitrageResult<GlobalMarketData> {
        let cache_key = format!("market_data:global:{}", symbol.to_uppercase());

        // Try to get from enhanced cache with tier preference (Hot tier for real-time data)
        match enhanced_cache
            .get_with_tier(
                &self.kv,
                &cache_key,
                Some(CacheTier::Hot),
                &DataType::MarketData,
            )
            .await
        {
            Ok(Some(cache_entry)) => {
                // Parse cached data
                match serde_json::from_str::<GlobalMarketData>(&cache_entry.value) {
                    Ok(market_data) => {
                        console_log!(
                            "Enhanced cache HIT for market data: {} (tier: {:?})",
                            symbol,
                            cache_entry.tier
                        );
                        return Ok(market_data);
                    }
                    Err(e) => {
                        console_log!("Enhanced cache parse error for {}: {}", symbol, e);
                    }
                }
            }
            Ok(None) => {
                console_log!("Enhanced cache MISS for market data: {}", symbol);
            }
            Err(e) => {
                console_log!("Enhanced cache error for {}: {}", symbol, e);
            }
        }

        // Cache miss - fetch from API
        let market_data = self.fetch_ticker_from_api(symbol).await?;

        // Store in enhanced cache with Hot tier (5-minute TTL for real-time market data)
        match serde_json::to_string(&market_data) {
            Ok(serialized_data) => {
                let ttl_seconds = 300; // 5 minutes for market data
                match enhanced_cache
                    .put_with_tier(
                        &self.kv,
                        &cache_key,
                        &serialized_data,
                        CacheTier::Hot,
                        &DataType::MarketData,
                        Some(ttl_seconds),
                    )
                    .await
                {
                    Ok(_) => {
                        console_log!(
                            "Enhanced cache stored market data for {} (Hot tier, TTL: {}s)",
                            symbol,
                            ttl_seconds
                        );
                    }
                    Err(e) => {
                        console_log!("Failed to store in enhanced cache for {}: {}", symbol, e);
                    }
                }
            }
            Err(e) => {
                console_log!("Failed to serialize market data for {}: {}", symbol, e);
            }
        }

        Ok(market_data)
    }

    /// Basic caching implementation for fallback compatibility
    async fn get_global_market_data_basic(
        &self,
        symbol: &str,
        cache_manager: &CacheManager,
    ) -> ArbitrageResult<GlobalMarketData> {
        let cache_key = format!("market_data:global:{}", symbol.to_uppercase());

        // Try to get from basic cache
        match cache_manager.get::<String>(&cache_key).await {
            Ok(Some(cached_data)) => match serde_json::from_str::<GlobalMarketData>(&cached_data) {
                Ok(market_data) => {
                    console_log!("Basic cache HIT for market data: {}", symbol);
                    return Ok(market_data);
                }
                Err(e) => {
                    console_log!("Basic cache parse error for {}: {}", symbol, e);
                }
            },
            Ok(None) => {
                console_log!("Basic cache MISS for market data: {}", symbol);
            }
            Err(e) => {
                console_log!("Basic cache error for {}: {}", symbol, e);
            }
        }

        // Cache miss - fetch from API
        let market_data = self.fetch_ticker_from_api(symbol).await?;

        // Store in basic cache
        match serde_json::to_string(&market_data) {
            Ok(serialized_data) => {
                let ttl_seconds = 300; // 5 minutes
                match cache_manager
                    .set(&cache_key, &serialized_data, Some(ttl_seconds))
                    .await
                {
                    Ok(_) => {
                        console_log!(
                            "Basic cache stored market data for {} (TTL: {}s)",
                            symbol,
                            ttl_seconds
                        );
                    }
                    Err(e) => {
                        console_log!("Failed to store in basic cache for {}: {}", symbol, e);
                    }
                }
            }
            Err(e) => {
                console_log!("Failed to serialize market data for {}: {}", symbol, e);
            }
        }

        Ok(market_data)
    }

    /// Fetch ticker data from multiple exchanges with error handling
    async fn fetch_ticker_from_api(&self, symbol: &str) -> ArbitrageResult<GlobalMarketData> {
        let binance_result = self.get_binance_ticker(symbol).await;
        let bybit_result = self.get_bybit_ticker(symbol).await;
        let okx_result = self.get_okx_ticker(symbol).await;

        let mut global_data = GlobalMarketData {
            symbol: symbol.to_string(),
            binance: None,
            bybit: None,
            okx: None,
            coinbase: None,
            timestamp: worker::Date::now().as_millis(),
        };

        if let Ok(binance_ticker) = binance_result {
            global_data.binance = Some(binance_ticker);
        }

        if let Ok(bybit_ticker) = bybit_result {
            global_data.bybit = Some(bybit_ticker);
        }

        if let Ok(okx_ticker) = okx_result {
            global_data.okx = Some(okx_ticker);
        }

        // Ensure we have at least some data
        if global_data.binance.is_none() && global_data.bybit.is_none() && global_data.okx.is_none()
        {
            return Err(ArbitrageError::not_found(format!(
                "No market data available for symbol: {}",
                symbol
            )));
        }

        console_log!(
            "✅ Fetched global market data for {}: Binance={}, Bybit={}, OKX={}",
            symbol,
            global_data.binance.is_some(),
            global_data.bybit.is_some(),
            global_data.okx.is_some()
        );

        Ok(global_data)
    }

    /// Get Binance ticker data with error handling
    async fn get_binance_ticker(&self, symbol: &str) -> ArbitrageResult<Ticker> {
        let endpoint = "/api/v3/ticker/24hr";
        let params = serde_json::json!({
            "symbol": symbol.replace("/", "")
        });

        // Use super admin credentials if available for global market data
        let auth = self
            .super_admin_configs
            .get("binance")
            .map(|config| &config.read_only_credentials);

        match self
            .binance_request(endpoint, Method::Get, Some(params), auth)
            .await
        {
            Ok(response) => {
                let timestamp = worker::Date::now().as_millis();
                Ok(Ticker {
                    symbol: symbol.to_string(),
                    timestamp,
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
                    ask: response
                        .get("askPrice")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    last: response
                        .get("lastPrice")
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
                    change: response
                        .get("priceChange")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    percentage: response
                        .get("priceChangePercent")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    volume: response
                        .get("volume")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    quote_volume: response
                        .get("quoteVolume")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    info: response,
                    // Optional fields set to None for now
                    bid_volume: None,
                    ask_volume: None,
                    vwap: None,
                    previous_close: None,
                    average: None,
                    base_volume: None,
                })
            }
            Err(e) => {
                console_log!("Failed to fetch Binance ticker for {}: {}", symbol, e);
                Err(e)
            }
        }
    }

    /// Get Bybit ticker data with error handling
    async fn get_bybit_ticker(&self, symbol: &str) -> ArbitrageResult<Ticker> {
        let endpoint = "/v5/market/tickers";
        let params = serde_json::json!({
            "category": "spot",
            "symbol": symbol.replace("/", "")
        });

        // Use super admin credentials if available for global market data
        let auth = self
            .super_admin_configs
            .get("bybit")
            .map(|config| &config.read_only_credentials);

        match self
            .bybit_request(endpoint, Method::Get, Some(params), auth)
            .await
        {
            Ok(response) => {
                if let Some(result) = response
                    .get("result")
                    .and_then(|r| r.get("list"))
                    .and_then(|l| l.as_array())
                    .and_then(|arr| arr.first())
                {
                    let timestamp = worker::Date::now().as_millis();
                    Ok(Ticker {
                        symbol: symbol.to_string(),
                        timestamp,
                        datetime: chrono::Utc::now().to_rfc3339(),
                        high: result
                            .get("highPrice24h")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        low: result
                            .get("lowPrice24h")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        bid: result
                            .get("bid1Price")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        ask: result
                            .get("ask1Price")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        last: result
                            .get("lastPrice")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        open: result
                            .get("prevPrice24h")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        close: result
                            .get("lastPrice")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        change: result
                            .get("price24hPcnt")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<f64>().ok().map(|p| p * 100.0)),
                        percentage: result
                            .get("price24hPcnt")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<f64>().ok().map(|p| p * 100.0)),
                        volume: result
                            .get("volume24h")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        quote_volume: result
                            .get("turnover24h")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        info: result.clone(),
                        // Optional fields set to None for now
                        bid_volume: None,
                        ask_volume: None,
                        vwap: None,
                        previous_close: None,
                        average: None,
                        base_volume: None,
                    })
                } else {
                    Err(ArbitrageError::not_found(format!(
                        "No Bybit ticker data found for symbol: {}",
                        symbol
                    )))
                }
            }
            Err(e) => {
                console_log!("Failed to fetch Bybit ticker for {}: {}", symbol, e);
                Err(e)
            }
        }
    }

    /// Get OKX ticker data with error handling
    async fn get_okx_ticker(&self, symbol: &str) -> ArbitrageResult<Ticker> {
        let endpoint = "/api/v5/market/ticker";
        let params = serde_json::json!({
            "instId": symbol.replace("/", "-")
        });

        // Use super admin credentials if available for global market data
        let auth = self
            .super_admin_configs
            .get("okx")
            .map(|config| &config.read_only_credentials);

        match self
            .okx_request(endpoint, Method::Get, Some(params), auth)
            .await
        {
            Ok(response) => {
                // Parse OKX ticker response
                if let Some(data) = response.get("data").and_then(|d| d.as_array()) {
                    if let Some(ticker_data) = data.first() {
                        let last_price = ticker_data["last"]
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let bid_price = ticker_data["bidPx"]
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let ask_price = ticker_data["askPx"]
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let volume = ticker_data["vol24h"]
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);

                        let ticker = Ticker {
                            symbol: symbol.to_string(),
                            timestamp: chrono::Utc::now().timestamp_millis() as u64,
                            datetime: chrono::Utc::now().to_rfc3339(),
                            high: ticker_data["high24h"]
                                .as_str()
                                .and_then(|s| s.parse::<f64>().ok()),
                            low: ticker_data["low24h"]
                                .as_str()
                                .and_then(|s| s.parse::<f64>().ok()),
                            bid: Some(bid_price),
                            bid_volume: None, // OKX doesn't provide bid volume in ticker
                            ask: Some(ask_price),
                            ask_volume: None, // OKX doesn't provide ask volume in ticker
                            vwap: None,       // OKX doesn't provide VWAP in ticker
                            open: ticker_data["open24h"]
                                .as_str()
                                .and_then(|s| s.parse::<f64>().ok()),
                            close: Some(last_price),
                            last: Some(last_price),
                            previous_close: None,
                            change: None,
                            percentage: None,
                            average: None,
                            base_volume: Some(volume),
                            quote_volume: ticker_data["volCcy24h"]
                                .as_str()
                                .and_then(|s| s.parse::<f64>().ok()),
                            volume: Some(volume),
                            info: ticker_data.clone(),
                        };

                        console_log!(
                            "Successfully fetched OKX ticker for {}: ${}",
                            symbol,
                            last_price
                        );
                        Ok(ticker)
                    } else {
                        Err(ArbitrageError::not_found(format!(
                            "No OKX ticker data found for symbol: {}",
                            symbol
                        )))
                    }
                } else {
                    Err(ArbitrageError::api_error(format!(
                        "Invalid OKX ticker response format for symbol: {}",
                        symbol
                    )))
                }
            }
            Err(e) => {
                console_log!("Failed to fetch OKX ticker for {}: {}", symbol, e);
                Err(e)
            }
        }
    }

    /// Get Binance funding rate with enhanced multi-tier caching optimization
    pub async fn get_binance_funding_rate(&self, symbol: &str) -> ArbitrageResult<FundingRateInfo> {
        // Use enhanced cache if available, fallback to basic cache
        if let Some(enhanced_cache) = &self.enhanced_cache {
            self.get_binance_funding_rate_enhanced(symbol, enhanced_cache)
                .await
        } else if let Some(cache_manager) = &self.cache_manager {
            self.get_binance_funding_rate_basic(symbol, cache_manager)
                .await
        } else {
            // No cache available - direct API call
            self.fetch_binance_funding_rate_from_api(symbol).await
        }
    }

    /// Enhanced caching implementation for Binance funding rates
    async fn get_binance_funding_rate_enhanced(
        &self,
        symbol: &str,
        enhanced_cache: &KvCacheManager,
    ) -> ArbitrageResult<FundingRateInfo> {
        let cache_key = format!("funding_rate:binance:{}", symbol.to_uppercase());

        // Try to get from enhanced cache with Warm tier (funding rates change less frequently)
        match enhanced_cache
            .get_with_tier(
                &self.kv,
                &cache_key,
                Some(CacheTier::Warm),
                &DataType::FundingRate,
            )
            .await
        {
            Ok(Some(cache_entry)) => {
                match serde_json::from_str::<FundingRateInfo>(&cache_entry.value) {
                    Ok(funding_rate_info) => {
                        console_log!(
                            "Enhanced cache HIT for Binance funding rate: {} (tier: {:?})",
                            symbol,
                            cache_entry.tier
                        );
                        return Ok(funding_rate_info);
                    }
                    Err(e) => {
                        console_log!("Enhanced cache parse error for Binance {}: {}", symbol, e);
                    }
                }
            }
            Ok(None) => {
                console_log!("Enhanced cache MISS for Binance funding rate: {}", symbol);
            }
            Err(e) => {
                console_log!("Enhanced cache error for Binance {}: {}", symbol, e);
            }
        }

        // Cache miss - fetch from API
        let funding_rate_info = self.fetch_binance_funding_rate_from_api(symbol).await?;

        // Store in enhanced cache with Warm tier (1-hour TTL for funding rates)
        match serde_json::to_string(&funding_rate_info) {
            Ok(serialized_data) => {
                let ttl_seconds = 3600; // 1 hour for funding rates
                match enhanced_cache
                    .put_with_tier(
                        &self.kv,
                        &cache_key,
                        &serialized_data,
                        CacheTier::Warm,
                        &DataType::FundingRate,
                        Some(ttl_seconds),
                    )
                    .await
                {
                    Ok(_) => {
                        console_log!(
                            "Enhanced cache stored Binance funding rate for {} (Warm tier, TTL: {}s)",
                            symbol,
                            ttl_seconds
                        );
                    }
                    Err(e) => {
                        console_log!(
                            "Failed to store Binance funding rate in enhanced cache for {}: {}",
                            symbol,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                console_log!(
                    "Failed to serialize Binance funding rate for {}: {}",
                    symbol,
                    e
                );
            }
        }

        Ok(funding_rate_info)
    }

    /// Basic caching implementation for Binance funding rates
    async fn get_binance_funding_rate_basic(
        &self,
        symbol: &str,
        cache_manager: &CacheManager,
    ) -> ArbitrageResult<FundingRateInfo> {
        let cache_key = format!("funding_rate:binance:{}", symbol.to_uppercase());

        // Try to get from basic cache
        match cache_manager.get::<String>(&cache_key).await {
            Ok(Some(cached_data)) => match serde_json::from_str::<FundingRateInfo>(&cached_data) {
                Ok(funding_rate_info) => {
                    console_log!("Basic cache HIT for Binance funding rate: {}", symbol);
                    return Ok(funding_rate_info);
                }
                Err(e) => {
                    console_log!("Basic cache parse error for Binance {}: {}", symbol, e);
                }
            },
            Ok(None) => {
                console_log!("Basic cache MISS for Binance funding rate: {}", symbol);
            }
            Err(e) => {
                console_log!("Basic cache error for Binance {}: {}", symbol, e);
            }
        }

        // Cache miss - fetch from API
        let funding_rate_info = self.fetch_binance_funding_rate_from_api(symbol).await?;

        // Store in basic cache
        match serde_json::to_string(&funding_rate_info) {
            Ok(serialized_data) => {
                let ttl_seconds = 3600; // 1 hour
                match cache_manager
                    .set(&cache_key, &serialized_data, Some(ttl_seconds))
                    .await
                {
                    Ok(_) => {
                        console_log!(
                            "Basic cache stored Binance funding rate for {} (TTL: {}s)",
                            symbol,
                            ttl_seconds
                        );
                    }
                    Err(e) => {
                        console_log!(
                            "Failed to store Binance funding rate in basic cache for {}: {}",
                            symbol,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                console_log!(
                    "Failed to serialize Binance funding rate for {}: {}",
                    symbol,
                    e
                );
            }
        }

        Ok(funding_rate_info)
    }

    /// Fetch Binance funding rate directly from API
    async fn fetch_binance_funding_rate_from_api(
        &self,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        // Use super admin credentials if available for global funding rate data
        let auth = self
            .super_admin_configs
            .get("binance")
            .map(|config| &config.read_only_credentials);

        // Make request to Binance API
        match self
            .binance_request(
                "/fapi/v1/fundingRate",
                Method::Get,
                Some(serde_json::json!({
                    "symbol": symbol.replace("/", ""),
                    "limit": 1
                })),
                auth,
            )
            .await
        {
            Ok(response) => {
                // Parse the response array from Binance API
                if let Some(funding_rates) = response.as_array() {
                    if let Some(latest_rate) = funding_rates.first() {
                        let funding_rate = latest_rate
                            .get("fundingRate")
                            .and_then(|fr| fr.as_str())
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);

                        let funding_time = latest_rate
                            .get("fundingTime")
                            .and_then(|ft| ft.as_u64())
                            .unwrap_or(0);

                        return Ok(FundingRateInfo {
                            symbol: symbol.to_string(),
                            funding_rate,
                            timestamp: funding_time,
                            datetime: chrono::Utc::now().to_rfc3339(),
                            info: latest_rate.clone(),
                            next_funding_time: Some(funding_time),
                            estimated_rate: Some(funding_rate),
                            estimated_settle_price: None,
                            exchange: ExchangeIdEnum::Binance,
                            funding_interval_hours: 8,
                            mark_price: None,
                            index_price: None,
                            funding_countdown: None,
                        });
                    }
                }

                Err(ArbitrageError::not_found(format!(
                    "No funding rate data found for Binance:{}",
                    symbol
                )))
            }
            Err(e) => Err(e),
        }
    }

    /// Get Bybit funding rate with enhanced multi-tier caching optimization
    pub async fn get_bybit_funding_rate(&self, symbol: &str) -> ArbitrageResult<FundingRateInfo> {
        // Use enhanced cache if available, fallback to basic cache
        if let Some(enhanced_cache) = &self.enhanced_cache {
            self.get_bybit_funding_rate_enhanced(symbol, enhanced_cache)
                .await
        } else if let Some(cache_manager) = &self.cache_manager {
            self.get_bybit_funding_rate_basic(symbol, cache_manager)
                .await
        } else {
            // No cache available - direct API call
            self.fetch_bybit_funding_rate_from_api(symbol).await
        }
    }

    /// Enhanced caching implementation for Bybit funding rates
    async fn get_bybit_funding_rate_enhanced(
        &self,
        symbol: &str,
        enhanced_cache: &KvCacheManager,
    ) -> ArbitrageResult<FundingRateInfo> {
        let cache_key = format!("funding_rate:bybit:{}", symbol.to_uppercase());

        // Try to get from enhanced cache with Warm tier (funding rates change less frequently)
        match enhanced_cache
            .get_with_tier(
                &self.kv,
                &cache_key,
                Some(CacheTier::Warm),
                &DataType::FundingRate,
            )
            .await
        {
            Ok(Some(cache_entry)) => {
                match serde_json::from_str::<FundingRateInfo>(&cache_entry.value) {
                    Ok(funding_rate_info) => {
                        console_log!(
                            "Enhanced cache HIT for Bybit funding rate: {} (tier: {:?})",
                            symbol,
                            cache_entry.tier
                        );
                        return Ok(funding_rate_info);
                    }
                    Err(e) => {
                        console_log!("Enhanced cache parse error for Bybit {}: {}", symbol, e);
                    }
                }
            }
            Ok(None) => {
                console_log!("Enhanced cache MISS for Bybit funding rate: {}", symbol);
            }
            Err(e) => {
                console_log!("Enhanced cache error for Bybit {}: {}", symbol, e);
            }
        }

        // Cache miss - fetch from API
        let funding_rate_info = self.fetch_bybit_funding_rate_from_api(symbol).await?;

        // Store in enhanced cache with Warm tier (1-hour TTL for funding rates)
        match serde_json::to_string(&funding_rate_info) {
            Ok(serialized_data) => {
                let ttl_seconds = 3600; // 1 hour for funding rates
                match enhanced_cache
                    .put_with_tier(
                        &self.kv,
                        &cache_key,
                        &serialized_data,
                        CacheTier::Warm,
                        &DataType::FundingRate,
                        Some(ttl_seconds),
                    )
                    .await
                {
                    Ok(_) => {
                        console_log!(
                            "Enhanced cache stored Bybit funding rate for {} (Warm tier, TTL: {}s)",
                            symbol,
                            ttl_seconds
                        );
                    }
                    Err(e) => {
                        console_log!(
                            "Failed to store Bybit funding rate in enhanced cache for {}: {}",
                            symbol,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                console_log!(
                    "Failed to serialize Bybit funding rate for {}: {}",
                    symbol,
                    e
                );
            }
        }

        Ok(funding_rate_info)
    }

    /// Basic caching implementation for Bybit funding rates
    async fn get_bybit_funding_rate_basic(
        &self,
        symbol: &str,
        cache_manager: &CacheManager,
    ) -> ArbitrageResult<FundingRateInfo> {
        let cache_key = format!("funding_rate:bybit:{}", symbol.to_uppercase());

        // Try to get from basic cache
        match cache_manager.get::<String>(&cache_key).await {
            Ok(Some(cached_data)) => match serde_json::from_str::<FundingRateInfo>(&cached_data) {
                Ok(funding_rate_info) => {
                    console_log!("Basic cache HIT for Bybit funding rate: {}", symbol);
                    return Ok(funding_rate_info);
                }
                Err(e) => {
                    console_log!("Basic cache parse error for Bybit {}: {}", symbol, e);
                }
            },
            Ok(None) => {
                console_log!("Basic cache MISS for Bybit funding rate: {}", symbol);
            }
            Err(e) => {
                console_log!("Basic cache error for Bybit {}: {}", symbol, e);
            }
        }

        // Cache miss - fetch from API
        let funding_rate_info = self.fetch_bybit_funding_rate_from_api(symbol).await?;

        // Store in basic cache
        match serde_json::to_string(&funding_rate_info) {
            Ok(serialized_data) => {
                let ttl_seconds = 3600; // 1 hour
                match cache_manager
                    .set(&cache_key, &serialized_data, Some(ttl_seconds))
                    .await
                {
                    Ok(_) => {
                        console_log!(
                            "Basic cache stored Bybit funding rate for {} (TTL: {}s)",
                            symbol,
                            ttl_seconds
                        );
                    }
                    Err(e) => {
                        console_log!(
                            "Failed to store Bybit funding rate in basic cache for {}: {}",
                            symbol,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                console_log!(
                    "Failed to serialize Bybit funding rate for {}: {}",
                    symbol,
                    e
                );
            }
        }

        Ok(funding_rate_info)
    }

    /// Fetch Bybit funding rate directly from API
    async fn fetch_bybit_funding_rate_from_api(
        &self,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        // Use super admin credentials if available for global funding rate data
        let auth = self
            .super_admin_configs
            .get("bybit")
            .map(|config| &config.read_only_credentials);

        // Make request to Bybit API
        match self
            .bybit_request(
                "/v5/market/funding/history",
                Method::Get,
                Some(serde_json::json!({
                    "category": "linear",
                    "symbol": symbol.replace("/", ""),
                    "limit": 1
                })),
                auth,
            )
            .await
        {
            Ok(response) => {
                // Parse the response to get funding rate data from Bybit API format
                if let Some(result) = response.get("result") {
                    if let Some(list) = result.get("list").and_then(|l| l.as_array()) {
                        if let Some(latest) = list.first() {
                            let funding_rate = latest
                                .get("fundingRate")
                                .and_then(|fr| fr.as_str())
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);

                            let funding_time = latest
                                .get("fundingRateTimestamp")
                                .and_then(|ft| ft.as_str())
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(0);

                            return Ok(FundingRateInfo {
                                symbol: symbol.to_string(),
                                funding_rate,
                                timestamp: funding_time,
                                datetime: chrono::Utc::now().to_rfc3339(),
                                info: latest.clone(),
                                next_funding_time: Some(funding_time),
                                estimated_rate: Some(funding_rate),
                                estimated_settle_price: None,
                                exchange: ExchangeIdEnum::Bybit,
                                funding_interval_hours: 8,
                                mark_price: None,
                                index_price: None,
                                funding_countdown: None,
                            });
                        }
                    }
                }

                Err(ArbitrageError::not_found(format!(
                    "No funding rate data found for Bybit:{}",
                    symbol
                )))
            }
            Err(e) => Err(e),
        }
    }

    /// Get funding rate directly from exchange APIs using cached implementations
    pub async fn get_funding_rate_direct(
        &self,
        exchange_id: &str,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        match exchange_id {
            "binance" => self.get_binance_funding_rate(symbol).await,
            "bybit" => self.get_bybit_funding_rate(symbol).await,
            _ => Err(ArbitrageError::not_implemented(format!(
                "Funding rate not implemented for exchange: {}",
                exchange_id
            ))),
        }
    }

    /// Get global funding rates for a specific exchange and symbol
    /// Used by funding rate manager for OKX and other exchanges
    pub async fn get_global_funding_rates(
        &self,
        exchange_id: &str,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<serde_json::Value>> {
        match exchange_id {
            "okx" => {
                self.get_okx_funding_rates(symbol.unwrap_or("BTC-USDT"))
                    .await
            }
            "binance" => {
                // Convert to use existing binance funding rate method
                if let Some(symbol) = symbol {
                    match self.get_binance_funding_rate(symbol).await {
                        Ok(funding_info) => {
                            let funding_json = serde_json::json!({
                                "fundingRate": funding_info.funding_rate.to_string(),
                                "fundingTime": funding_info.timestamp,
                                "symbol": funding_info.symbol
                            });
                            Ok(vec![funding_json])
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    Err(ArbitrageError::validation_error(
                        "Symbol required for Binance funding rates".to_string(),
                    ))
                }
            }
            "bybit" => {
                // Convert to use existing bybit funding rate method
                if let Some(symbol) = symbol {
                    match self.get_bybit_funding_rate(symbol).await {
                        Ok(funding_info) => {
                            let funding_json = serde_json::json!({
                                "fundingRate": funding_info.funding_rate.to_string(),
                                "fundingTime": funding_info.timestamp,
                                "symbol": funding_info.symbol
                            });
                            Ok(vec![funding_json])
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    Err(ArbitrageError::validation_error(
                        "Symbol required for Bybit funding rates".to_string(),
                    ))
                }
            }
            _ => Err(ArbitrageError::not_implemented(format!(
                "Global funding rates not implemented for exchange: {}",
                exchange_id
            ))),
        }
    }

    /// Get OKX funding rates with caching optimization
    async fn get_okx_funding_rates(&self, symbol: &str) -> ArbitrageResult<Vec<serde_json::Value>> {
        // Build cache key for OKX funding rate data
        let cache_key = format!("funding_rate:okx:{}", symbol.to_uppercase());

        // Initialize cache if available - cache-first pattern
        if let Some(cache_manager) = &self.cache_manager {
            // Try to get from cache first
            match cache_manager.get::<String>(&cache_key).await {
                Ok(Some(cached_data)) => {
                    // Parse cached JSON data
                    match serde_json::from_str::<Vec<serde_json::Value>>(&cached_data) {
                        Ok(funding_rates) => {
                            // Log cache hit for debugging
                            console_log!("Cache HIT for OKX funding rate: {}", symbol);
                            return Ok(funding_rates);
                        }
                        Err(e) => {
                            // Log cache parsing error
                            console_log!("Cache parse error for OKX {}: {}", symbol, e);
                        }
                    }
                }
                Ok(None) => {
                    // Cache miss - log for debugging
                    console_log!("Cache MISS for OKX funding rate: {}", symbol);
                }
                Err(e) => {
                    // Cache error - log and continue with API call
                    console_log!("Cache error for OKX {}: {}", symbol, e);
                }
            }
        }

        // Cache miss or no cache available - fetch from API
        let funding_rates = self.fetch_okx_funding_rates_from_api(symbol).await?;

        // Cache the result for future requests
        if let Some(cache_manager) = &self.cache_manager {
            // Serialize funding rate data for caching
            match serde_json::to_string(&funding_rates) {
                Ok(serialized_data) => {
                    let ttl_seconds = 300; // 5 minutes TTL for funding rates
                    match cache_manager
                        .set(&cache_key, &serialized_data, Some(ttl_seconds))
                        .await
                    {
                        Ok(_) => {
                            console_log!(
                                "Cached OKX funding rate for {} (TTL: {}s)",
                                symbol,
                                ttl_seconds
                            );
                        }
                        Err(e) => {
                            console_log!("Failed to cache OKX funding rate for {}: {}", symbol, e);
                        }
                    }
                }
                Err(e) => {
                    console_log!("Failed to serialize OKX funding rate for {}: {}", symbol, e);
                }
            }
        }

        Ok(funding_rates)
    }

    /// Fetch OKX funding rates from API
    async fn fetch_okx_funding_rates_from_api(
        &self,
        symbol: &str,
    ) -> ArbitrageResult<Vec<serde_json::Value>> {
        // Use super admin credentials if available for global funding rate data
        let auth = self
            .super_admin_configs
            .get("okx")
            .map(|config| &config.read_only_credentials);

        // Make request to OKX API for funding rates
        match self
            .okx_request(
                "/api/v5/public/funding-rate",
                Method::Get,
                Some(serde_json::json!({
                    "instId": symbol.replace("/", "-")
                })),
                auth,
            )
            .await
        {
            Ok(response) => {
                // Parse the response to get funding rate data from OKX API format
                if let Some(data) = response.get("data").and_then(|d| d.as_array()) {
                    Ok(data.clone())
                } else {
                    Err(ArbitrageError::not_found(format!(
                        "No OKX funding rate data found for symbol: {}",
                        symbol
                    )))
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Generic request method for Binance API
    pub async fn binance_request(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        let start_time = now_system_time();
        let base_url = "https://api.binance.com";
        let url = format!("{}{}", base_url, endpoint);

        let mut request_builder = match method {
            Method::Get => self.client.get(&url),
            Method::Post => self.client.post(&url),
            Method::Put => self.client.put(&url),
            Method::Delete => self.client.delete(&url),
            Method::Head => self.client.head(&url),
            Method::Patch => self.client.patch(&url),
            Method::Options => self.client.request(reqwest::Method::OPTIONS, &url),
            _ => {
                return Err(ArbitrageError::validation_error(format!(
                    "Unsupported HTTP method: {:?}",
                    method
                )))
            }
        };

        // Add query parameters for GET requests
        if matches!(method, Method::Get) {
            if let Some(params) = &params {
                if let Ok(query_params) = serde_json::from_value::<
                    std::collections::HashMap<String, serde_json::Value>,
                >(params.clone())
                {
                    for (key, value) in query_params {
                        let value_str = match value {
                            serde_json::Value::String(s) => s,
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => value.to_string(),
                        };
                        request_builder = request_builder.query(&[(key, value_str)]);
                    }
                }
            }
        }

        // Add authentication headers if credentials provided
        if let Some(credentials) = auth {
            request_builder = request_builder.header("X-MBX-APIKEY", &credentials.api_key);

            // For authenticated requests, add signature
            if let Some(params) = &params {
                let query_string = if let Ok(query_params) = serde_json::from_value::<
                    std::collections::HashMap<String, serde_json::Value>,
                >(params.clone())
                {
                    query_params
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or(&v.to_string())))
                        .collect::<Vec<_>>()
                        .join("&")
                } else {
                    String::new()
                };

                let timestamp = chrono::Utc::now().timestamp_millis();
                let query_with_timestamp = if query_string.is_empty() {
                    format!("timestamp={}", timestamp)
                } else {
                    format!("{}&timestamp={}", query_string, timestamp)
                };

                // Create HMAC signature
                use hmac::{Hmac, Mac};
                use sha2::Sha256;
                type HmacSha256 = Hmac<Sha256>;

                let mut mac =
                    HmacSha256::new_from_slice(credentials.secret.as_bytes()).map_err(|e| {
                        ArbitrageError::internal_error(format!("Invalid secret key: {}", e))
                    })?;
                mac.update(query_with_timestamp.as_bytes());
                let signature = hex::encode(mac.finalize().into_bytes());

                request_builder = request_builder.query(&[
                    ("timestamp", timestamp.to_string()),
                    ("signature", signature),
                ]);
            }
        } else if let Some(params) = &params {
            // For non-authenticated requests, just add params as JSON body for non-GET
            if !matches!(method, Method::Get) {
                request_builder = request_builder.json(params);
            }
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| ArbitrageError::api_error(format!("Binance API request failed: {}", e)))?;

        let response_time_ms = start_time.elapsed().unwrap_or_default().as_millis() as u64;
        let status_code = response.status().as_u16();
        let success = response.status().is_success();

        if !success {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Track failed API call
            if let Some(monitor) = &self.opportunity_monitor {
                let _ = monitor
                    .track_api_call(ApiCallTrackingParams {
                        exchange: "binance".to_string(),
                        endpoint: endpoint.to_string(),
                        method: format!("{:?}", method),
                        status_code: Some(status_code),
                        response_time_ms,
                        success: false,
                        error_message: Some(error_text.clone()),
                        data_source: "direct_api".to_string(),
                    })
                    .await;
            }

            return Err(ArbitrageError::api_error(format!(
                "Binance API error {}: {}",
                status_code, error_text
            )));
        }

        let result = response.json().await.map_err(|e| {
            ArbitrageError::api_error(format!("Failed to parse Binance response: {}", e))
        });

        // Track successful API call
        if let Some(monitor) = &self.opportunity_monitor {
            let _ = monitor
                .track_api_call(ApiCallTrackingParams {
                    exchange: "binance".to_string(),
                    endpoint: endpoint.to_string(),
                    method: format!("{:?}", method),
                    status_code: Some(status_code),
                    response_time_ms,
                    success: result.is_ok(),
                    error_message: if result.is_err() {
                        Some("JSON parse error".to_string())
                    } else {
                        None
                    },
                    data_source: "direct_api".to_string(),
                })
                .await;
        }

        result
    }

    /// Binance request with retry logic
    pub async fn binance_request_with_retry(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
        retries: u32,
    ) -> ArbitrageResult<Value> {
        let mut last_error = None;

        for attempt in 0..=retries {
            match self
                .binance_request(endpoint, method.clone(), params.clone(), auth)
                .await
            {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < retries {
                        // Wait before retry (exponential backoff)
                        let delay_ms = 1000 * (2_u64.pow(attempt));
                        worker::Delay::from(std::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| ArbitrageError::network_error("Request failed after retries")))
    }

    /// Binance futures API request method
    async fn binance_futures_request(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        let base_url = "https://fapi.binance.com";
        let url = format!("{}{}", base_url, endpoint);

        let mut request_builder = match method {
            Method::Get => self.client.get(&url),
            Method::Post => self.client.post(&url),
            Method::Put => self.client.put(&url),
            Method::Delete => self.client.delete(&url),
            Method::Head => self.client.head(&url),
            Method::Patch => self.client.patch(&url),
            Method::Options => self.client.request(reqwest::Method::OPTIONS, &url),
            _ => {
                return Err(ArbitrageError::validation_error(format!(
                    "Unsupported HTTP method: {:?}",
                    method
                )))
            }
        };

        // Add query parameters for GET requests
        if matches!(method, Method::Get) {
            if let Some(params) = &params {
                if let Ok(query_params) = serde_json::from_value::<
                    std::collections::HashMap<String, serde_json::Value>,
                >(params.clone())
                {
                    for (key, value) in query_params {
                        let value_str = match value {
                            serde_json::Value::String(s) => s,
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => value.to_string(),
                        };
                        request_builder = request_builder.query(&[(key, value_str)]);
                    }
                }
            }
        }

        // Add authentication headers if credentials provided
        if let Some(credentials) = auth {
            request_builder = request_builder.header("X-MBX-APIKEY", &credentials.api_key);

            // For authenticated requests, add signature
            if let Some(params) = &params {
                let query_string = if let Ok(query_params) = serde_json::from_value::<
                    std::collections::HashMap<String, serde_json::Value>,
                >(params.clone())
                {
                    query_params
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or(&v.to_string())))
                        .collect::<Vec<_>>()
                        .join("&")
                } else {
                    String::new()
                };

                let timestamp = chrono::Utc::now().timestamp_millis();
                let query_with_timestamp = if query_string.is_empty() {
                    format!("timestamp={}", timestamp)
                } else {
                    format!("{}&timestamp={}", query_string, timestamp)
                };

                // Create HMAC signature
                use hmac::{Hmac, Mac};
                use sha2::Sha256;
                type HmacSha256 = Hmac<Sha256>;

                let mut mac =
                    HmacSha256::new_from_slice(credentials.secret.as_bytes()).map_err(|e| {
                        ArbitrageError::internal_error(format!("Invalid secret key: {}", e))
                    })?;
                mac.update(query_with_timestamp.as_bytes());
                let signature = hex::encode(mac.finalize().into_bytes());

                request_builder = request_builder.query(&[
                    ("timestamp", timestamp.to_string()),
                    ("signature", signature),
                ]);
            }
        } else if let Some(params) = &params {
            // For non-authenticated requests, just add params as JSON body for non-GET
            if !matches!(method, Method::Get) {
                request_builder = request_builder.json(params);
            }
        }

        let response = request_builder.send().await.map_err(|e| {
            ArbitrageError::api_error(format!("Binance futures API request failed: {}", e))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ArbitrageError::api_error(format!(
                "Binance futures API error {}: {}",
                status, error_text
            )));
        }

        response.json().await.map_err(|e| {
            ArbitrageError::api_error(format!("Failed to parse Binance futures response: {}", e))
        })
    }

    /// Generic request method for Bybit API  
    async fn bybit_request(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        let start_time = now_system_time();
        let base_url = "https://api.bybit.com";
        let url = format!("{}{}", base_url, endpoint);

        let mut request_builder = match method {
            Method::Get => self.client.get(&url),
            Method::Post => self.client.post(&url),
            Method::Put => self.client.put(&url),
            Method::Delete => self.client.delete(&url),
            Method::Head => self.client.head(&url),
            Method::Patch => self.client.patch(&url),
            Method::Options => self.client.request(reqwest::Method::OPTIONS, &url),
            _ => {
                return Err(ArbitrageError::validation_error(format!(
                    "Unsupported HTTP method: {:?}",
                    method
                )))
            }
        };

        // Add query parameters for GET requests
        if matches!(method, Method::Get) {
            if let Some(params) = &params {
                if let Ok(query_params) = serde_json::from_value::<
                    std::collections::HashMap<String, serde_json::Value>,
                >(params.clone())
                {
                    for (key, value) in query_params {
                        let value_str = match value {
                            serde_json::Value::String(s) => s,
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => value.to_string(),
                        };
                        request_builder = request_builder.query(&[(key, value_str)]);
                    }
                }
            }
        }

        // Add authentication headers if credentials provided
        if let Some(credentials) = auth {
            let timestamp = chrono::Utc::now().timestamp_millis().to_string();
            let recv_window = "5000";

            request_builder = request_builder
                .header("X-BAPI-API-KEY", &credentials.api_key)
                .header("X-BAPI-TIMESTAMP", &timestamp)
                .header("X-BAPI-RECV-WINDOW", recv_window);

            // For authenticated requests, add signature
            if let Some(params) = &params {
                let param_str = if let Ok(query_params) = serde_json::from_value::<
                    std::collections::HashMap<String, serde_json::Value>,
                >(params.clone())
                {
                    query_params
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or(&v.to_string())))
                        .collect::<Vec<_>>()
                        .join("&")
                } else {
                    String::new()
                };

                // Bybit signature format: timestamp + api_key + recv_window + param_str
                let sign_str = format!(
                    "{}{}{}{}",
                    timestamp, credentials.api_key, recv_window, param_str
                );

                // Create HMAC signature
                use hmac::{Hmac, Mac};
                use sha2::Sha256;
                type HmacSha256 = Hmac<Sha256>;

                let mut mac =
                    HmacSha256::new_from_slice(credentials.secret.as_bytes()).map_err(|e| {
                        ArbitrageError::internal_error(format!("Invalid secret key: {}", e))
                    })?;
                mac.update(sign_str.as_bytes());
                let signature = hex::encode(mac.finalize().into_bytes());

                request_builder = request_builder.header("X-BAPI-SIGN", signature);
            }
        } else if let Some(params) = &params {
            // For non-authenticated requests, just add params as JSON body for non-GET
            if !matches!(method, Method::Get) {
                request_builder = request_builder.json(params);
            }
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| ArbitrageError::api_error(format!("Bybit API request failed: {}", e)))?;

        let response_time_ms = start_time.elapsed().unwrap_or_default().as_millis() as u64;
        let status_code = response.status().as_u16();
        let success = response.status().is_success();

        if !success {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Track failed API call
            if let Some(monitor) = &self.opportunity_monitor {
                let _ = monitor
                    .track_api_call(ApiCallTrackingParams {
                        exchange: "bybit".to_string(),
                        endpoint: endpoint.to_string(),
                        method: format!("{:?}", method),
                        status_code: Some(status_code),
                        response_time_ms,
                        success: false,
                        error_message: Some(error_text.clone()),
                        data_source: "direct_api".to_string(),
                    })
                    .await;
            }

            return Err(ArbitrageError::api_error(format!(
                "Bybit API error {}: {}",
                status_code, error_text
            )));
        }

        let result = response.json().await.map_err(|e| {
            ArbitrageError::api_error(format!("Failed to parse Bybit response: {}", e))
        });

        // Track successful API call
        if let Some(monitor) = &self.opportunity_monitor {
            let _ = monitor
                .track_api_call(ApiCallTrackingParams {
                    exchange: "bybit".to_string(),
                    endpoint: endpoint.to_string(),
                    method: format!("{:?}", method),
                    status_code: Some(status_code),
                    response_time_ms,
                    success: result.is_ok(),
                    error_message: if result.is_err() {
                        Some("JSON parse error".to_string())
                    } else {
                        None
                    },
                    data_source: "direct_api".to_string(),
                })
                .await;
        }

        result
    }

    /// Generic request method for OKX API  
    async fn okx_request(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        let start_time = now_system_time();
        let base_url = "https://www.okx.com";
        let url = format!("{}{}", base_url, endpoint);

        let mut request_builder = match method {
            Method::Get => self.client.get(&url),
            Method::Post => self.client.post(&url),
            Method::Put => self.client.put(&url),
            Method::Delete => self.client.delete(&url),
            Method::Head => self.client.head(&url),
            Method::Patch => self.client.patch(&url),
            Method::Options => self.client.request(reqwest::Method::OPTIONS, &url),
            _ => {
                return Err(ArbitrageError::validation_error(format!(
                    "Unsupported HTTP method: {:?}",
                    method
                )))
            }
        };

        // Add query parameters for GET requests
        if matches!(method, Method::Get) {
            if let Some(params) = &params {
                if let Ok(query_params) = serde_json::from_value::<
                    std::collections::HashMap<String, serde_json::Value>,
                >(params.clone())
                {
                    for (key, value) in query_params {
                        let value_str = match value {
                            serde_json::Value::String(s) => s,
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => value.to_string(),
                        };
                        request_builder = request_builder.query(&[(key, value_str)]);
                    }
                }
            }
        }

        // Add authentication headers if credentials provided
        if let Some(credentials) = auth {
            let timestamp = chrono::Utc::now().to_rfc3339();

            request_builder = request_builder
                .header("OK-ACCESS-KEY", &credentials.api_key)
                .header("OK-ACCESS-TIMESTAMP", &timestamp)
                .header("OK-ACCESS-PASSPHRASE", &credentials.secret); // OKX uses passphrase differently

            // For authenticated requests, add signature
            if let Some(params) = &params {
                let body_str = if matches!(method, Method::Get) {
                    String::new()
                } else {
                    serde_json::to_string(params).unwrap_or_default()
                };

                // OKX signature format: timestamp + method + request_path + body
                let request_path = endpoint;
                let method_str = match method {
                    Method::Get => "GET",
                    Method::Post => "POST",
                    Method::Put => "PUT",
                    Method::Delete => "DELETE",
                    _ => "GET",
                };

                let sign_str = format!("{}{}{}{}", timestamp, method_str, request_path, body_str);

                // Create HMAC signature
                use base64::Engine;
                use hmac::{Hmac, Mac};
                use sha2::Sha256;
                type HmacSha256 = Hmac<Sha256>;

                let mut mac =
                    HmacSha256::new_from_slice(credentials.secret.as_bytes()).map_err(|e| {
                        ArbitrageError::internal_error(format!("Invalid secret key: {}", e))
                    })?;
                mac.update(sign_str.as_bytes());
                let signature =
                    base64::prelude::BASE64_STANDARD.encode(mac.finalize().into_bytes());

                request_builder = request_builder.header("OK-ACCESS-SIGN", signature);
            }
        } else if let Some(params) = &params {
            // For non-authenticated requests, just add params as JSON body for non-GET
            if !matches!(method, Method::Get) {
                request_builder = request_builder.json(params);
            }
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| ArbitrageError::api_error(format!("OKX API request failed: {}", e)))?;

        let response_time_ms = start_time.elapsed().unwrap_or_default().as_millis() as u64;
        let status_code = response.status().as_u16();
        let success = response.status().is_success();

        if !success {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Track failed API call
            if let Some(monitor) = &self.opportunity_monitor {
                let _ = monitor
                    .track_api_call(ApiCallTrackingParams {
                        exchange: "okx".to_string(),
                        endpoint: endpoint.to_string(),
                        method: format!("{:?}", method),
                        status_code: Some(status_code),
                        response_time_ms,
                        success: false,
                        error_message: Some(error_text.clone()),
                        data_source: "direct_api".to_string(),
                    })
                    .await;
            }

            return Err(ArbitrageError::api_error(format!(
                "OKX API error {}: {}",
                status_code, error_text
            )));
        }

        let result = response
            .json()
            .await
            .map_err(|e| ArbitrageError::api_error(format!("Failed to parse OKX response: {}", e)));

        // Track successful API call
        if let Some(monitor) = &self.opportunity_monitor {
            let _ = monitor
                .track_api_call(ApiCallTrackingParams {
                    exchange: "okx".to_string(),
                    endpoint: endpoint.to_string(),
                    method: format!("{:?}", method),
                    status_code: Some(status_code),
                    response_time_ms,
                    success: result.is_ok(),
                    error_message: if result.is_err() {
                        Some("JSON parse error".to_string())
                    } else {
                        None
                    },
                    data_source: "direct_api".to_string(),
                })
                .await;
        }

        result
    }
}

/// Implementation of ExchangeInterface trait for ExchangeService
impl ExchangeInterface for ExchangeService {
    /// Get ticker data using cache-first approach to reduce uncached subrequests
    async fn get_ticker(&self, exchange_id: &str, symbol: &str) -> ArbitrageResult<Ticker> {
        // Delegate to the global market data implementation - get_global_market_data only takes symbol
        let global_data = self.get_global_market_data(symbol).await?;

        // Convert GlobalMarketData to Ticker based on the exchange_id
        match exchange_id {
            "binance" => {
                if let Some(binance_data) = global_data.binance {
                    Ok(binance_data)
                } else {
                    Err(ArbitrageError::not_found(format!(
                        "No {} ticker data found for {}",
                        exchange_id, symbol
                    )))
                }
            }
            "bybit" => {
                if let Some(bybit_data) = global_data.bybit {
                    Ok(bybit_data)
                } else {
                    Err(ArbitrageError::not_found(format!(
                        "No {} ticker data found for {}",
                        exchange_id, symbol
                    )))
                }
            }
            "okx" => {
                if let Some(okx_data) = global_data.okx {
                    Ok(okx_data)
                } else {
                    Err(ArbitrageError::not_found(format!(
                        "No {} ticker data found for {}",
                        exchange_id, symbol
                    )))
                }
            }
            _ => {
                // For unsupported exchanges, try to return any available data
                if let Some(binance_data) = global_data.binance {
                    Ok(binance_data)
                } else if let Some(bybit_data) = global_data.bybit {
                    Ok(bybit_data)
                } else {
                    Err(ArbitrageError::not_found(format!(
                        "No ticker data available for {} on {}",
                        symbol, exchange_id
                    )))
                }
            }
        }
    }

    /// Get markets for a specific exchange
    async fn get_markets(&self, exchange_id: &str) -> ArbitrageResult<Vec<Market>> {
        match exchange_id.to_lowercase().as_str() {
            "binance" => {
                let response = self
                    .binance_request("/api/v3/exchangeInfo", Method::Get, None, None)
                    .await?;

                let symbols = response["symbols"]
                    .as_array()
                    .ok_or_else(|| ArbitrageError::api_error("Invalid response format"))?;

                let mut markets = Vec::new();
                for symbol in symbols {
                    if symbol["status"].as_str() == Some("TRADING") {
                        let base = symbol["baseAsset"].as_str().unwrap_or_default().to_string();
                        let quote = symbol["quoteAsset"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let symbol_name = symbol["symbol"].as_str().unwrap_or_default().to_string();

                        // Extract trading limits
                        let empty_filters = vec![];
                        let filters = symbol["filters"].as_array().unwrap_or(&empty_filters);
                        let mut min_qty = None;
                        let mut max_qty = None;
                        let mut min_price = None;
                        let mut max_price = None;
                        let mut min_notional = None;

                        for filter in filters {
                            match filter["filterType"].as_str() {
                                Some("LOT_SIZE") => {
                                    min_qty = filter["minQty"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok());
                                    max_qty = filter["maxQty"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok());
                                }
                                Some("PRICE_FILTER") => {
                                    min_price = filter["minPrice"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok());
                                    max_price = filter["maxPrice"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok());
                                }
                                Some("MIN_NOTIONAL") => {
                                    min_notional = filter["minNotional"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok());
                                }
                                _ => {}
                            }
                        }

                        let market = Market {
                            symbol: symbol_name,
                            base,
                            quote,
                            active: true,
                            type_: "spot".to_string(),
                            spot: true,
                            margin: false,
                            future: false,
                            option: false,
                            contract: false,
                            settle: None,
                            settle_id: None,
                            contract_size: None,
                            linear: None,
                            inverse: None,
                            taker: 0.001, // Default Binance taker fee
                            maker: 0.001, // Default Binance maker fee
                            percentage: true,
                            tier_based: false,
                            limits: MarketLimits {
                                amount: Some(MinMax {
                                    min: min_qty,
                                    max: max_qty,
                                }),
                                price: Some(MinMax {
                                    min: min_price,
                                    max: max_price,
                                }),
                                cost: Some(MinMax {
                                    min: min_notional,
                                    max: None,
                                }),
                                leverage: Some(MinMax {
                                    min: Some(1.0),
                                    max: Some(10.0),
                                }),
                            },
                            precision: MarketPrecision {
                                amount: symbol["baseAssetPrecision"].as_i64().map(|p| p as i32),
                                price: symbol["quotePrecision"].as_i64().map(|p| p as i32),
                                base: symbol["baseAssetPrecision"].as_i64().map(|p| p as i32),
                                quote: symbol["quotePrecision"].as_i64().map(|p| p as i32),
                            },
                            info: symbol.clone(),
                        };

                        markets.push(market);
                    }
                }

                Ok(markets)
            }
            "bybit" => {
                let response = self
                    .bybit_request(
                        "/v5/market/instruments-info?category=spot",
                        Method::Get,
                        None,
                        None,
                    )
                    .await?;

                let instruments = response["result"]["list"]
                    .as_array()
                    .ok_or_else(|| ArbitrageError::api_error("Invalid response format"))?;

                let mut markets = Vec::new();
                for instrument in instruments {
                    if instrument["status"].as_str() == Some("Trading") {
                        let symbol = instrument["symbol"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let base_coin = instrument["baseCoin"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let quote_coin = instrument["quoteCoin"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();

                        let market = Market {
                            symbol,
                            base: base_coin,
                            quote: quote_coin,
                            active: true,
                            type_: "spot".to_string(),
                            spot: true,
                            margin: false,
                            future: false,
                            option: false,
                            contract: false,
                            settle: None,
                            settle_id: None,
                            contract_size: None,
                            linear: None,
                            inverse: None,
                            taker: 0.001, // Default Bybit taker fee
                            maker: 0.001, // Default Bybit maker fee
                            percentage: true,
                            tier_based: false,
                            limits: MarketLimits {
                                amount: Some(MinMax {
                                    min: instrument["lotSizeFilter"]["minOrderQty"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok()),
                                    max: instrument["lotSizeFilter"]["maxOrderQty"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok()),
                                }),
                                price: Some(MinMax {
                                    min: instrument["priceFilter"]["minPrice"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok()),
                                    max: instrument["priceFilter"]["maxPrice"]
                                        .as_str()
                                        .and_then(|s| s.parse::<f64>().ok()),
                                }),
                                cost: None,
                                leverage: Some(MinMax {
                                    min: Some(1.0),
                                    max: Some(10.0),
                                }),
                            },
                            precision: MarketPrecision {
                                amount: instrument["lotSizeFilter"]["basePrecision"]
                                    .as_str()
                                    .and_then(|s| s.parse::<i32>().ok()),
                                price: instrument["priceFilter"]["tickSize"]
                                    .as_str()
                                    .and_then(|s| s.len().to_string().parse::<i32>().ok()),
                                base: instrument["lotSizeFilter"]["basePrecision"]
                                    .as_str()
                                    .and_then(|s| s.parse::<i32>().ok()),
                                quote: instrument["priceFilter"]["tickSize"]
                                    .as_str()
                                    .and_then(|s| s.len().to_string().parse::<i32>().ok()),
                            },
                            info: instrument.clone(),
                        };

                        markets.push(market);
                    }
                }

                Ok(markets)
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Exchange not supported: {}",
                exchange_id
            ))),
        }
    }

    /// Get orderbook for a specific exchange and symbol
    async fn get_orderbook(
        &self,
        exchange_id: &str,
        symbol: &str,
        limit: Option<u32>,
    ) -> ArbitrageResult<OrderBook> {
        match exchange_id.to_lowercase().as_str() {
            "binance" => {
                let limit_param = limit.unwrap_or(100).min(5000); // Binance max is 5000
                let endpoint = format!("/api/v3/depth?symbol={}&limit={}", symbol, limit_param);

                let response = self
                    .binance_request(&endpoint, Method::Get, None, None)
                    .await?;

                let bids = response["bids"]
                    .as_array()
                    .ok_or_else(|| ArbitrageError::api_error("Invalid bids format"))?
                    .iter()
                    .filter_map(|bid| {
                        if let (Some(price), Some(qty)) = (
                            bid.get(0).and_then(|v| v.as_str()),
                            bid.get(1).and_then(|v| v.as_str()),
                        ) {
                            if let (Ok(p), Ok(q)) = (price.parse::<f64>(), qty.parse::<f64>()) {
                                Some([p, q])
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                let asks = response["asks"]
                    .as_array()
                    .ok_or_else(|| ArbitrageError::api_error("Invalid asks format"))?
                    .iter()
                    .filter_map(|ask| {
                        if let (Some(price), Some(qty)) = (
                            ask.get(0).and_then(|v| v.as_str()),
                            ask.get(1).and_then(|v| v.as_str()),
                        ) {
                            if let (Ok(p), Ok(q)) = (price.parse::<f64>(), qty.parse::<f64>()) {
                                Some([p, q])
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                Ok(OrderBook {
                    symbol: symbol.to_string(),
                    bids,
                    asks,
                    timestamp: response["lastUpdateId"]
                        .as_u64()
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64),
                    datetime: chrono::Utc::now().to_rfc3339(),
                    nonce: response["lastUpdateId"].as_u64(),
                })
            }
            "bybit" => {
                let limit_param = limit.unwrap_or(25).min(500); // Bybit max is 500
                let endpoint = format!(
                    "/v5/market/orderbook?category=spot&symbol={}&limit={}",
                    symbol, limit_param
                );

                let response = self
                    .bybit_request(&endpoint, Method::Get, None, None)
                    .await?;

                let result = &response["result"];
                let bids = result["b"]
                    .as_array()
                    .ok_or_else(|| ArbitrageError::api_error("Invalid bids format"))?
                    .iter()
                    .filter_map(|bid| {
                        if let (Some(price), Some(qty)) = (
                            bid.get(0).and_then(|v| v.as_str()),
                            bid.get(1).and_then(|v| v.as_str()),
                        ) {
                            if let (Ok(p), Ok(q)) = (price.parse::<f64>(), qty.parse::<f64>()) {
                                Some([p, q])
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                let asks = result["a"]
                    .as_array()
                    .ok_or_else(|| ArbitrageError::api_error("Invalid asks format"))?
                    .iter()
                    .filter_map(|ask| {
                        if let (Some(price), Some(qty)) = (
                            ask.get(0).and_then(|v| v.as_str()),
                            ask.get(1).and_then(|v| v.as_str()),
                        ) {
                            if let (Ok(p), Ok(q)) = (price.parse::<f64>(), qty.parse::<f64>()) {
                                Some([p, q])
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                Ok(OrderBook {
                    symbol: symbol.to_string(),
                    bids,
                    asks,
                    timestamp: result["ts"]
                        .as_u64()
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64),
                    datetime: chrono::Utc::now().to_rfc3339(),
                    nonce: result["u"].as_u64(),
                })
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Exchange not supported: {}",
                exchange_id
            ))),
        }
    }

    /// Fetch funding rates for a specific exchange
    async fn fetch_funding_rates(
        &self,
        exchange_id: &str,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>> {
        match exchange_id.to_lowercase().as_str() {
            "binance" => {
                if let Some(sym) = symbol {
                    let funding_rate = self.get_binance_funding_rate(sym).await?;
                    Ok(vec![serde_json::to_value(funding_rate)?])
                } else {
                    Err(ArbitrageError::validation_error(
                        "Symbol required for Binance funding rates",
                    ))
                }
            }
            "bybit" => {
                if let Some(sym) = symbol {
                    let funding_rate = self.get_bybit_funding_rate(sym).await?;
                    Ok(vec![serde_json::to_value(funding_rate)?])
                } else {
                    Err(ArbitrageError::validation_error(
                        "Symbol required for Bybit funding rates",
                    ))
                }
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Funding rates not implemented for exchange: {}",
                exchange_id
            ))),
        }
    }

    /// Get balance for a specific exchange (requires credentials)
    async fn get_balance(
        &self,
        exchange_id: &str,
        _credentials: &ExchangeCredentials,
    ) -> ArbitrageResult<Value> {
        // TODO: Implement balance fetching for different exchanges
        Err(ArbitrageError::not_implemented(format!(
            "Balance not implemented for {}",
            exchange_id
        )))
    }

    /// Create order on a specific exchange (requires credentials)
    async fn create_order(
        &self,
        exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: &str,
        _side: &str,
        _amount: f64,
        _price: Option<f64>,
    ) -> ArbitrageResult<Order> {
        // TODO: Implement order creation for different exchanges
        Err(ArbitrageError::not_implemented(format!(
            "Order creation not implemented for {}",
            exchange_id
        )))
    }

    /// Cancel order on a specific exchange (requires credentials)
    async fn cancel_order(
        &self,
        exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _order_id: &str,
        _symbol: &str,
    ) -> ArbitrageResult<Order> {
        // TODO: Implement order cancellation for different exchanges
        Err(ArbitrageError::not_implemented(format!(
            "Order cancellation not implemented for {}",
            exchange_id
        )))
    }

    /// Get open orders for a specific exchange (requires credentials)
    async fn get_open_orders(
        &self,
        exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Order>> {
        // TODO: Implement open orders fetching for different exchanges
        Err(ArbitrageError::not_implemented(format!(
            "Open orders not implemented for {}",
            exchange_id
        )))
    }

    /// Get open positions for a specific exchange (requires credentials)
    async fn get_open_positions(
        &self,
        exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Position>> {
        // TODO: Implement open positions fetching for different exchanges
        Err(ArbitrageError::not_implemented(format!(
            "Open positions not implemented for {}",
            exchange_id
        )))
    }

    /// Set leverage for a specific exchange (requires credentials)
    async fn set_leverage(
        &self,
        exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: &str,
        _leverage: u32,
    ) -> ArbitrageResult<()> {
        // TODO: Implement leverage setting for different exchanges
        Err(ArbitrageError::not_implemented(format!(
            "Leverage setting not implemented for {}",
            exchange_id
        )))
    }

    /// Get trading fees for a specific exchange (requires credentials)
    async fn get_trading_fees(
        &self,
        exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: &str,
    ) -> ArbitrageResult<TradingFees> {
        // TODO: Implement trading fees fetching for different exchanges
        Err(ArbitrageError::not_implemented(format!(
            "Trading fees not implemented for {}",
            exchange_id
        )))
    }

    /// Test API connection for a specific exchange
    async fn test_api_connection(
        &self,
        exchange_id: &str,
        api_key: &str,
        secret: &str,
    ) -> ArbitrageResult<(bool, bool, Option<RateLimitInfo>)> {
        let credentials = ExchangeCredentials {
            exchange: ExchangeIdEnum::from_string(exchange_id).map_err(|e| {
                ArbitrageError::validation_error(format!("Invalid exchange ID: {}", e))
            })?,
            api_key: api_key.to_string(),
            api_secret: secret.to_string(),
            secret: secret.to_string(),
            passphrase: None,
            sandbox: false,
            is_testnet: false,
            default_leverage: 1,
            exchange_type: "spot".to_string(),
        };

        match exchange_id.to_lowercase().as_str() {
            "binance" => {
                // Test with account info endpoint (requires authentication)
                match self
                    .binance_request("/api/v3/account", Method::Get, None, Some(&credentials))
                    .await
                {
                    Ok(response) => {
                        // Check if response contains account info
                        let has_trading_permission = response["permissions"]
                            .as_array()
                            .map(|perms| perms.iter().any(|p| p.as_str() == Some("SPOT")))
                            .unwrap_or(false);

                        let rate_limit_info = RateLimitInfo {
                            requests_per_minute: 1200, // Binance default
                            requests_remaining: 1200,  // We don't get this from account endpoint
                            reset_time: chrono::Utc::now().timestamp() as u64 + 60,
                        };

                        Ok((true, has_trading_permission, Some(rate_limit_info)))
                    }
                    Err(e) => {
                        // Check if it's an authentication error vs network error
                        let error_msg = e.to_string().to_lowercase();
                        if error_msg.contains("invalid api") || error_msg.contains("signature") {
                            Ok((false, false, None)) // Invalid credentials
                        } else {
                            Err(e) // Network or other error
                        }
                    }
                }
            }
            "bybit" => {
                // Test with account info endpoint
                match self
                    .bybit_request("/v5/account/info", Method::Get, None, Some(&credentials))
                    .await
                {
                    Ok(response) => {
                        let has_trading_permission = response["result"]["unifiedMarginStatus"]
                            .as_u64()
                            .map(|status| status == 1)
                            .unwrap_or(false);

                        let rate_limit_info = RateLimitInfo {
                            requests_per_minute: 120, // Bybit default for private endpoints
                            requests_remaining: 120,
                            reset_time: chrono::Utc::now().timestamp() as u64 + 60,
                        };

                        Ok((true, has_trading_permission, Some(rate_limit_info)))
                    }
                    Err(e) => {
                        let error_msg = e.to_string().to_lowercase();
                        if error_msg.contains("invalid api") || error_msg.contains("signature") {
                            Ok((false, false, None))
                        } else {
                            Err(e)
                        }
                    }
                }
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Exchange not supported: {}",
                exchange_id
            ))),
        }
    }

    /// Test API connection with options for a specific exchange
    async fn test_api_connection_with_options(
        &self,
        exchange_id: &str,
        api_key: &str,
        secret: &str,
        leverage: Option<i32>,
        exchange_type: Option<&str>,
    ) -> ArbitrageResult<(bool, bool, Option<RateLimitInfo>)> {
        let credentials = ExchangeCredentials {
            exchange: ExchangeIdEnum::from_string(exchange_id).unwrap_or(ExchangeIdEnum::Binance),
            api_key: api_key.to_string(),
            api_secret: secret.to_string(),
            secret: secret.to_string(),
            passphrase: None,
            sandbox: false,
            is_testnet: false,
            default_leverage: 1,
            exchange_type: exchange_type.unwrap_or("spot").to_string(),
        };

        match exchange_id.to_lowercase().as_str() {
            "binance" => {
                // For Binance, test different endpoints based on exchange_type
                let endpoint = match exchange_type {
                    Some("futures") => "/fapi/v1/account",
                    Some("margin") => "/sapi/v1/margin/account",
                    _ => "/api/v3/account", // Default to spot
                };

                let request_result = if exchange_type == Some("futures") {
                    self.binance_futures_request(endpoint, Method::Get, None, Some(&credentials))
                        .await
                } else {
                    self.binance_request(endpoint, Method::Get, None, Some(&credentials))
                        .await
                };

                match request_result {
                    Ok(response) => {
                        let has_trading_permission;

                        // Check permissions based on exchange type
                        match exchange_type {
                            Some("futures") => {
                                has_trading_permission =
                                    response["canTrade"].as_bool().unwrap_or(false);

                                // Test leverage setting if provided
                                if let Some(lev) = leverage {
                                    if (1..=125).contains(&lev) {
                                        // Binance futures max leverage - leverage is valid
                                        // has_trading_permission remains as-is since leverage is valid
                                    }
                                }
                            }
                            Some("margin") => {
                                has_trading_permission =
                                    response["tradeEnabled"].as_bool().unwrap_or(false);
                            }
                            _ => {
                                has_trading_permission = response["permissions"]
                                    .as_array()
                                    .map(|perms| perms.iter().any(|p| p.as_str() == Some("SPOT")))
                                    .unwrap_or(false);
                            }
                        }

                        let rate_limit_info = RateLimitInfo {
                            requests_per_minute: if exchange_type == Some("futures") {
                                2400
                            } else {
                                1200
                            },
                            requests_remaining: if exchange_type == Some("futures") {
                                2400
                            } else {
                                1200
                            },
                            reset_time: chrono::Utc::now().timestamp() as u64 + 60,
                        };

                        Ok((true, has_trading_permission, Some(rate_limit_info)))
                    }
                    Err(e) => {
                        let error_msg = e.to_string().to_lowercase();
                        if error_msg.contains("invalid api") || error_msg.contains("signature") {
                            Ok((false, false, None))
                        } else {
                            Err(e)
                        }
                    }
                }
            }
            "bybit" => {
                // For Bybit, test based on category
                let endpoint = match exchange_type {
                    Some("linear") | Some("futures") => "/v5/account/info",
                    Some("inverse") => "/v5/account/info",
                    _ => "/v5/account/info", // Default endpoint
                };

                match self
                    .bybit_request(endpoint, Method::Get, None, Some(&credentials))
                    .await
                {
                    Ok(response) => {
                        let has_trading_permission = response["result"]["unifiedMarginStatus"]
                            .as_u64()
                            .map(|status| status == 1)
                            .unwrap_or(false);

                        // Validate leverage if provided
                        let leverage_valid = if let Some(lev) = leverage {
                            match exchange_type {
                                Some("linear") | Some("futures") => (1..=100).contains(&lev),
                                Some("inverse") => (1..=100).contains(&lev),
                                _ => (1..=10).contains(&lev), // Spot margin
                            }
                        } else {
                            true
                        };

                        let rate_limit_info = RateLimitInfo {
                            requests_per_minute: 120,
                            requests_remaining: 120,
                            reset_time: chrono::Utc::now().timestamp() as u64 + 60,
                        };

                        Ok((
                            true,
                            has_trading_permission && leverage_valid,
                            Some(rate_limit_info),
                        ))
                    }
                    Err(e) => {
                        let error_msg = e.to_string().to_lowercase();
                        if error_msg.contains("invalid api") || error_msg.contains("signature") {
                            Ok((false, false, None))
                        } else {
                            Err(e)
                        }
                    }
                }
            }
            _ => Err(ArbitrageError::validation_error(format!(
                "Exchange not supported: {}",
                exchange_id
            ))),
        }
    }
}
