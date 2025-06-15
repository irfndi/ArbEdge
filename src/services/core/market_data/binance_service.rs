use crate::services::core::infrastructure::enhanced_kv_cache::KvCacheManager;
use crate::types::Ticker;
use crate::utils::error::{ArbitrageError, ArbitrageResult};
use chrono;
use futures;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use worker::console_log;

/// Binance API response structure for ticker data
#[derive(Debug, Deserialize, Serialize)]
pub struct BinanceTicker {
    pub symbol: String,
    #[serde(rename = "priceChange")]
    pub price_change: String,
    #[serde(rename = "priceChangePercent")]
    pub price_change_percent: String,
    #[serde(rename = "weightedAvgPrice")]
    pub weighted_avg_price: String,
    #[serde(rename = "prevClosePrice")]
    pub prev_close_price: String,
    #[serde(rename = "lastPrice")]
    pub last_price: String,
    #[serde(rename = "lastQty")]
    pub last_qty: String,
    #[serde(rename = "bidPrice")]
    pub bid_price: String,
    #[serde(rename = "bidQty")]
    pub bid_qty: String,
    #[serde(rename = "askPrice")]
    pub ask_price: String,
    #[serde(rename = "askQty")]
    pub ask_qty: String,
    #[serde(rename = "openPrice")]
    pub open_price: String,
    #[serde(rename = "highPrice")]
    pub high_price: String,
    #[serde(rename = "lowPrice")]
    pub low_price: String,
    pub volume: String,
    #[serde(rename = "quoteVolume")]
    pub quote_volume: String,
    #[serde(rename = "openTime")]
    pub open_time: u64,
    #[serde(rename = "closeTime")]
    pub close_time: u64,
    #[serde(rename = "firstId")]
    pub first_id: u64,
    #[serde(rename = "lastId")]
    pub last_id: u64,
    pub count: u64,
}

/// Binance exchange service with cache-first architecture
pub struct BinanceService {
    pub client: Client,
    pub base_url: String,
    pub api_key: Option<String>,
    pub secret_key: Option<String>,
    pub cache_manager: Option<Arc<KvCacheManager>>,
}

impl BinanceService {
    /// Create new Binance service instance
    pub fn new(api_key: Option<String>, secret_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: "https://api.binance.com".to_string(),
            api_key,
            secret_key,
            cache_manager: None,
        }
    }

    /// Set cache manager for cache-first operations
    pub fn set_cache_manager(&mut self, cache_manager: Arc<KvCacheManager>) {
        self.cache_manager = Some(cache_manager);
    }

    /// Get ticker data with cache-first approach
    pub async fn get_ticker(
        &self,
        symbol: &str,
        kv_store: &worker::kv::KvStore,
    ) -> ArbitrageResult<Ticker> {
        let cache_key = format!("binance:ticker:{}", symbol);

        // Use cache manager if available
        if let Some(cache_manager) = &self.cache_manager {
            // Try to get from cache first
            if let Ok(Some(cached_data)) = cache_manager.get(kv_store, &cache_key).await {
                if let Ok(ticker) = serde_json::from_str::<Ticker>(&cached_data) {
                    console_log!("✅ Cache hit for Binance ticker: {}", symbol);
                    return Ok(ticker);
                }
            }
        }

        // Cache miss - fetch from API
        console_log!(
            "⚠️ Cache miss for Binance ticker: {}, fetching from API",
            symbol
        );
        let ticker = self.fetch_ticker_direct(symbol).await?;

        // Cache the result if cache manager is available
        if let Some(cache_manager) = &self.cache_manager {
            let ticker_json = serde_json::to_string(&ticker).unwrap_or_default();
            let _ = cache_manager
                .put(kv_store, &cache_key, &ticker_json, Some(30))
                .await; // 30 second TTL
        }

        Ok(ticker)
    }

    /// Direct API call to fetch ticker (private method)
    async fn fetch_ticker_direct(&self, symbol: &str) -> ArbitrageResult<Ticker> {
        let url = format!("{}/api/v3/ticker/24hr?symbol={}", self.base_url, symbol);
        console_log!("🔗 Binance API call: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(ArbitrageError::exchange_error(
                "binance",
                format!(
                    "API error: {} - {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                ),
            ));
        }

        let ticker_data: BinanceTicker = response.json().await?;

        Ok(Ticker {
            symbol: ticker_data.symbol,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            datetime: chrono::Utc::now().to_rfc3339(),
            bid: Some(ticker_data.bid_price.parse().unwrap_or(0.0)),
            ask: Some(ticker_data.ask_price.parse().unwrap_or(0.0)),
            volume: Some(ticker_data.volume.parse().unwrap_or(0.0)),
            bid_volume: Some(ticker_data.bid_qty.parse().unwrap_or(0.0)),
            ask_volume: Some(ticker_data.ask_qty.parse().unwrap_or(0.0)),
            vwap: Some(ticker_data.weighted_avg_price.parse().unwrap_or(0.0)),
            open: Some(ticker_data.open_price.parse().unwrap_or(0.0)),
            close: Some(ticker_data.last_price.parse().unwrap_or(0.0)),
            last: Some(ticker_data.last_price.parse().unwrap_or(0.0)),
            previous_close: Some(ticker_data.prev_close_price.parse().unwrap_or(0.0)),
            high: Some(ticker_data.high_price.parse().unwrap_or(0.0)),
            low: Some(ticker_data.low_price.parse().unwrap_or(0.0)),
            change: Some(ticker_data.price_change.parse().unwrap_or(0.0)),
            percentage: Some(ticker_data.price_change_percent.parse().unwrap_or(0.0)),
            average: Some(
                (ticker_data.bid_price.parse().unwrap_or(0.0)
                    + ticker_data.ask_price.parse().unwrap_or(0.0))
                    / 2.0,
            ),
            base_volume: Some(ticker_data.volume.parse().unwrap_or(0.0)),
            quote_volume: Some(ticker_data.quote_volume.parse().unwrap_or(0.0)),
            info: serde_json::json!({
                "count": ticker_data.count,
                "prev_close_price": ticker_data.prev_close_price,
                "open_time": ticker_data.open_time,
                "close_time": ticker_data.close_time
            }),
        })
    }

    /// Get multiple tickers with batch optimization
    pub async fn get_tickers(
        &self,
        symbols: &[String],
        kv_store: &worker::kv::KvStore,
    ) -> ArbitrageResult<Vec<Ticker>> {
        let mut tickers = Vec::new();

        // Use concurrent requests for better performance
        let futures: Vec<_> = symbols
            .iter()
            .map(|symbol| self.get_ticker(symbol, kv_store))
            .collect();

        let results = futures::future::join_all(futures).await;

        for result in results {
            match result {
                Ok(ticker) => tickers.push(ticker),
                Err(e) => {
                    console_log!("❌ Failed to fetch ticker: {}", e);
                    // Continue with other tickers instead of failing completely
                }
            }
        }

        Ok(tickers)
    }

    /// Get order book data
    pub async fn get_order_book(
        &self,
        symbol: &str,
        limit: Option<u16>,
    ) -> ArbitrageResult<serde_json::Value> {
        let limit_param = limit.unwrap_or(100);
        let url = format!(
            "{}/api/v3/depth?symbol={}&limit={}",
            self.base_url, symbol, limit_param
        );

        console_log!("🔗 Binance order book API call: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(ArbitrageError::exchange_error(
                "binance",
                format!(
                    "Order book API error: {} - {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                ),
            ));
        }

        let order_book: serde_json::Value = response.json().await?;
        Ok(order_book)
    }

    /// Get recent trades
    pub async fn get_recent_trades(
        &self,
        symbol: &str,
        limit: Option<u16>,
    ) -> ArbitrageResult<serde_json::Value> {
        let limit_param = limit.unwrap_or(500);
        let url = format!(
            "{}/api/v3/trades?symbol={}&limit={}",
            self.base_url, symbol, limit_param
        );

        console_log!("🔗 Binance trades API call: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(ArbitrageError::exchange_error(
                "binance",
                format!(
                    "Trades API error: {} - {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                ),
            ));
        }

        let trades: serde_json::Value = response.json().await?;
        Ok(trades)
    }

    /// Health check for the service
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let url = format!("{}/api/v3/ping", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

// Add From implementation for reqwest::Error
impl From<reqwest::Error> for ArbitrageError {
    fn from(err: reqwest::Error) -> Self {
        ArbitrageError::network_error(format!("HTTP request failed: {}", err))
    }
}
