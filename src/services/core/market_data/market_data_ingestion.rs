use crate::services::core::infrastructure::analytics_engine::AnalyticsEngineService;
use crate::services::core::infrastructure::cloudflare_pipelines::CloudflarePipelinesService;
use crate::services::core::market_data::coinmarketcap::CoinMarketCapService;
use crate::types::{ExchangeIdEnum, FundingRateInfo};
use crate::utils::logger::Logger;
use crate::utils::{ArbitrageError, ArbitrageResult};

use chrono::{TimeZone, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use worker::kv::KvStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataIngestionConfig {
    pub ingestion_interval_seconds: u32,
    pub max_ingestion_rate_mb_per_sec: u32,
    pub cache_ttl_seconds: u64,
    pub monitored_exchanges: Vec<ExchangeIdEnum>,
    pub monitored_pairs: Vec<String>,
    pub enable_funding_rates: bool,
    pub enable_price_data: bool,
    pub enable_volume_data: bool,
    pub enable_orderbook_snapshots: bool,
    pub batch_size: u32,
    pub retry_attempts: u32,
    pub timeout_seconds: u32,
}

impl Default for MarketDataIngestionConfig {
    fn default() -> Self {
        Self {
            ingestion_interval_seconds: 30,
            max_ingestion_rate_mb_per_sec: 100,
            cache_ttl_seconds: 60,
            monitored_exchanges: vec![
                ExchangeIdEnum::Binance,
                ExchangeIdEnum::Bybit,
                ExchangeIdEnum::OKX,
            ],
            monitored_pairs: vec![
                "BTC-USDT".to_string(),
                "ETH-USDT".to_string(),
                "BNB-USDT".to_string(),
                "SOL-USDT".to_string(),
                "XRP-USDT".to_string(),
                "ADA-USDT".to_string(),
                "DOGE-USDT".to_string(),
                "AVAX-USDT".to_string(),
                "DOT-USDT".to_string(),
                "MATIC-USDT".to_string(),
            ],
            enable_funding_rates: true,
            enable_price_data: true,
            enable_volume_data: true,
            enable_orderbook_snapshots: false, // Disabled by default due to high volume
            batch_size: 10,
            retry_attempts: 3,
            timeout_seconds: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataSnapshot {
    pub exchange: ExchangeIdEnum,
    pub symbol: String,
    pub timestamp: u64,
    pub price_data: Option<PriceData>,
    pub funding_rate_data: Option<FundingRateInfo>,
    pub volume_data: Option<VolumeData>,
    pub orderbook_data: Option<OrderbookSnapshot>,
    pub source: DataSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    pub price: f64,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub high_24h: Option<f64>,
    pub low_24h: Option<f64>,
    pub change_24h: Option<f64>,
    pub change_percentage_24h: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeData {
    pub volume_24h: f64,
    pub volume_24h_usd: Option<f64>,
    pub trades_count_24h: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookSnapshot {
    pub bids: Vec<[f64; 2]>, // [price, quantity]
    pub asks: Vec<[f64; 2]>, // [price, quantity]
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    RealAPI,
    Pipeline,
    Cache,
    CoinMarketCap,
}

impl std::fmt::Display for DataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSource::RealAPI => write!(f, "real_api"),
            DataSource::Pipeline => write!(f, "pipeline"),
            DataSource::Cache => write!(f, "cache"),
            DataSource::CoinMarketCap => write!(f, "coinmarketcap"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub cache_hits: u64,
    pub pipeline_hits: u64,
    pub api_calls: u64,
    pub data_volume_mb: f64,
    pub average_latency_ms: f64,
    pub last_ingestion_timestamp: u64,
}

pub struct MarketDataIngestionService {
    config: MarketDataIngestionConfig,
    analytics_engine: Option<AnalyticsEngineService>,
    cloudflare_pipelines_service: Option<CloudflarePipelinesService>, // Assuming this was the intent for pipelines_service
    #[allow(dead_code)] // Will be used for price data fallback
    cmc_service: Option<CoinMarketCapService>,
    kv_store: KvStore,
    logger: Logger,
    metrics: IngestionMetrics,
}

impl MarketDataIngestionService {
    pub fn new(
        config: MarketDataIngestionConfig,
        analytics_engine: Option<AnalyticsEngineService>,
        cloudflare_pipelines_service: Option<CloudflarePipelinesService>,
        coinmarketcap_service: Option<CoinMarketCapService>,
        kv_store: KvStore,
        logger: Logger,
    ) -> Self {
        Self {
            config,
            analytics_engine,
            cloudflare_pipelines_service,
            cmc_service: coinmarketcap_service,
            kv_store,
            logger,
            metrics: IngestionMetrics {
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                cache_hits: 0,
                pipeline_hits: 0,
                api_calls: 0,
                data_volume_mb: 0.0,
                average_latency_ms: 0.0,
                last_ingestion_timestamp: 0,
            },
        }
    }

    /// Set or update the pipelines service after initialization
    pub fn set_analytics_engine(&mut self, analytics_engine: Option<AnalyticsEngineService>) {
        self.analytics_engine = analytics_engine;
    }

    /// Main ingestion method implementing optimized batch processing for better performance
    pub async fn ingest_market_data(&mut self) -> ArbitrageResult<Vec<MarketDataSnapshot>> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let mut snapshots = Vec::new();

        self.logger
            .info("Starting optimized batch market data ingestion cycle");

        // Process in batches to reduce subrequests and improve performance
        let pairs = self.config.monitored_pairs.clone();
        let exchanges = self.config.monitored_exchanges.clone();
        let batch_size = self.config.batch_size as usize;

        // Create all exchange-pair combinations
        let mut combinations = Vec::new();
        for pair in &pairs {
            for exchange in &exchanges {
                combinations.push((exchange, pair.as_str()));
            }
        }

        // Process combinations in batches
        for batch in combinations.chunks(batch_size) {
            let mut batch_snapshots = Vec::new();

            // Process each item in the batch sequentially (but batches are processed efficiently)
            for (exchange, pair) in batch {
                match self.ingest_exchange_pair_data(exchange, pair).await {
                    Ok(snapshot) => {
                        batch_snapshots.push(snapshot);
                        self.metrics.successful_requests += 1;
                    }
                    Err(e) => {
                        self.logger.warn(&format!(
                            "Failed to ingest data for {}:{} - {}",
                            exchange.as_str(),
                            pair,
                            e
                        ));
                        self.metrics.failed_requests += 1;
                    }
                }
                self.metrics.total_requests += 1;
            }

            // Add batch results to main collection
            snapshots.extend(batch_snapshots);

            // Small delay between batches to prevent rate limiting
            if combinations.len() > batch_size {
                worker::Delay::from(std::time::Duration::from_millis(100)).await;
            }
        }

        // Store aggregated data to analytics engine
        if let Some(ref mut _analytics_engine) = self.analytics_engine {
            if let Err(e) = self.store_snapshots_to_pipeline(&snapshots).await {
                self.logger.warn(&format!(
                    "Failed to store snapshots to analytics engine: {}",
                    e
                ));
            }
        }

        // Update metrics
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        self.metrics.average_latency_ms = (end_time - start_time) as f64;
        self.metrics.last_ingestion_timestamp = end_time;

        self.logger.info(&format!(
            "Market data ingestion completed: {} snapshots, {} successful, {} failed",
            snapshots.len(),
            self.metrics.successful_requests,
            self.metrics.failed_requests
        ));

        Ok(snapshots)
    }

    /// Ingest data for a specific exchange-pair combination using hybrid access pattern
    async fn ingest_exchange_pair_data(
        &mut self,
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        // Step 1: Try cache first (fastest)
        if let Ok(cached) = self.get_cached_market_data(exchange, pair).await {
            self.metrics.cache_hits += 1;
            return Ok(cached);
        }

        // Step 2: (Pipeline data retrieval was here, removed as AnalyticsEngine is for sending events)
        // If data retrieval from a pipeline-like source is needed in the future,
        // it should be implemented via a different service or mechanism.

        // Step 3: Fetch from real APIs (slowest but most current)
        let snapshot = self.fetch_real_market_data(exchange, pair).await?;
        self.metrics.api_calls += 1;

        // Cache the fresh data
        let _ = self.cache_market_data(&snapshot).await;

        Ok(snapshot)
    }

    /// Fetch real market data from exchange APIs
    async fn fetch_real_market_data(
        &mut self, // Changed to &mut self
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        match exchange {
            ExchangeIdEnum::Binance => self.fetch_binance_data(pair).await,
            ExchangeIdEnum::Bybit => self.fetch_bybit_data(pair).await,
            ExchangeIdEnum::OKX => self.fetch_okx_data(pair).await,
            _ => Err(ArbitrageError::not_implemented(format!(
                "Exchange {} not supported for real market data fetching",
                exchange.as_str()
            ))),
        }
    }

    /// Fetch data from Binance API
    async fn fetch_binance_data(&mut self, pair: &str) -> ArbitrageResult<MarketDataSnapshot> {
        let binance_symbol = pair.replace("-", "").to_uppercase();
        let mut snapshot = MarketDataSnapshot {
            exchange: ExchangeIdEnum::Binance,
            symbol: pair.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            price_data: None,
            funding_rate_data: None,
            volume_data: None,
            orderbook_data: None,
            source: DataSource::RealAPI,
        };

        // Fetch price data
        if self.config.enable_price_data {
            snapshot.price_data = self.fetch_binance_price_data(&binance_symbol).await.ok();
        }

        // Fetch funding rate data
        if self.config.enable_funding_rates {
            snapshot.funding_rate_data =
                self.fetch_binance_funding_rate(&binance_symbol).await.ok();
        }

        // Fetch volume data
        if self.config.enable_volume_data {
            snapshot.volume_data = self.fetch_binance_volume_data(&binance_symbol).await.ok();
        }

        Ok(snapshot)
    }

    /// Fetch data from Bybit API
    async fn fetch_bybit_data(&mut self, pair: &str) -> ArbitrageResult<MarketDataSnapshot> {
        let bybit_symbol = pair.replace("-", "").to_uppercase();
        let mut snapshot = MarketDataSnapshot {
            exchange: ExchangeIdEnum::Bybit,
            symbol: pair.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            price_data: None,
            funding_rate_data: None,
            volume_data: None,
            orderbook_data: None,
            source: DataSource::RealAPI,
        };

        // Fetch price data
        if self.config.enable_price_data {
            snapshot.price_data = self.fetch_bybit_price_data(&bybit_symbol).await.ok();
        }

        // Fetch funding rate data
        if self.config.enable_funding_rates {
            snapshot.funding_rate_data = self.fetch_bybit_funding_rate(&bybit_symbol).await.ok();
        }

        // Fetch volume data
        if self.config.enable_volume_data {
            snapshot.volume_data = self.fetch_bybit_volume_data(&bybit_symbol).await.ok();
        }

        Ok(snapshot)
    }

    /// Fetch data from OKX API
    async fn fetch_okx_data(&mut self, pair: &str) -> ArbitrageResult<MarketDataSnapshot> {
        // Changed to &mut self
        let okx_symbol = pair.to_uppercase();
        let mut snapshot = MarketDataSnapshot {
            exchange: ExchangeIdEnum::OKX,
            symbol: pair.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            price_data: None,
            funding_rate_data: None,
            volume_data: None,
            orderbook_data: None,
            source: DataSource::RealAPI,
        };

        // Fetch price data
        if self.config.enable_price_data {
            snapshot.price_data = self.fetch_okx_price_data(&okx_symbol).await.ok();
        }

        // Note: OKX funding rate and volume data implementation would go here
        // For now, focusing on Binance and Bybit as requested

        Ok(snapshot)
    }

    /// Fetch Binance price data
    async fn fetch_binance_price_data(&mut self, symbol: &str) -> ArbitrageResult<PriceData> {
        self.metrics.api_calls += 1;
        let url = format!(
            "https://api.binance.com/api/v3/ticker/24hr?symbol={}",
            symbol.replace("-", "")
        );

        let client = Client::new();
        let request = client
            .get(&url)
            .header("User-Agent", "ArbEdgeBot/1.0")
            .build()
            .map_err(|e| {
                ArbitrageError::network_error(format!(
                    "Failed to build Binance price request for {}: {}",
                    url, e
                ))
            })?;

        let response = client.execute(request).await.map_err(|e| {
            ArbitrageError::network_error(format!(
                "Binance price request failed for {}: {}",
                symbol, e
            ))
        })?;

        let status = response.status();
        if status != 200 {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            self.logger.error(&format!(
                "Binance price API error for {}: {} - {}",
                symbol, status, error_body
            ));
            return Err(ArbitrageError::api_error(format!(
                "Binance price API error {}: {}",
                status, error_body
            )));
        }

        let data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse Binance price response for {}: {}",
                symbol, e
            ))
        })?;

        Ok(PriceData {
            price: data["lastPrice"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
            bid: data["bidPrice"].as_str().and_then(|s| s.parse().ok()),
            ask: data["askPrice"].as_str().and_then(|s| s.parse().ok()),
            high_24h: data["highPrice"].as_str().and_then(|s| s.parse().ok()),
            low_24h: data["lowPrice"].as_str().and_then(|s| s.parse().ok()),
            change_24h: data["priceChange"].as_str().and_then(|s| s.parse().ok()),
            change_percentage_24h: data["priceChangePercent"]
                .as_str()
                .and_then(|s| s.parse().ok()),
        })
    }

    /// Fetch Binance funding rate
    async fn fetch_binance_funding_rate(
        &mut self,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        self.metrics.api_calls += 1;
        let url = format!(
            "https://fapi.binance.com/fapi/v1/premiumIndex?symbol={}",
            symbol.replace("-", "")
        );

        let client = Client::new();
        let request = client
            .get(&url)
            .header("User-Agent", "ArbEdgeBot/1.0")
            .build()
            .map_err(|e| {
                ArbitrageError::network_error(format!(
                    "Failed to build Binance funding rate request for {}: {}",
                    url, e
                ))
            })?;

        let response = client.execute(request).await.map_err(|e| {
            ArbitrageError::network_error(format!(
                "Binance funding rate request failed for {}: {}",
                symbol, e
            ))
        })?;

        let status = response.status();
        if status != 200 {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            self.logger.error(&format!(
                "Binance funding rate API error for {}: {} - {}",
                symbol, status, error_body
            ));
            return Err(ArbitrageError::api_error(format!(
                "Binance funding rate API error {}: {}",
                status, error_body
            )));
        }

        // Binance premiumIndex can return a single object or an array of one object
        let response_text = response.text().await.map_err(|e| {
            ArbitrageError::network_error(format!(
                "Failed to read Binance funding response text for {}: {}",
                symbol, e
            ))
        })?;
        let data: Value = serde_json::from_str(&response_text).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse Binance funding response for {}: {}. Body: {}",
                symbol, e, response_text
            ))
        })?;

        let item = if data.is_array() {
            data.as_array().and_then(|arr| arr.first()).cloned()
        } else if data.is_object() {
            Some(data.clone()) // Clone the Value if it's already an object
        } else {
            None
        };

        if let Some(item_data) = item {
            // item_data is the actual JSON object for the funding rate
            let item_clone = item_data.clone(); // Clone for use in info field if needed
            Ok(FundingRateInfo {
                symbol: item_data["symbol"].as_str().unwrap_or_default().to_string(),
                funding_rate: item_data["lastFundingRate"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0),
                timestamp: item_data["time"].as_u64().unwrap_or(0),
                datetime: Utc
                    .timestamp_millis_opt(item_data["time"].as_i64().unwrap_or(0))
                    .single()
                    .map_or_else(|| Utc::now().to_rfc3339(), |dt| dt.to_rfc3339()),
                next_funding_time: item_data["nextFundingTime"].as_u64(),
                estimated_rate: None, // Not directly available in premiumIndex
                estimated_settle_price: None, // Not directly available
                exchange: ExchangeIdEnum::Binance,
                funding_interval_hours: 8, // Binance typical, might need to parse from interestInterval if available
                mark_price: item_data["markPrice"].as_str().and_then(|s| s.parse().ok()),
                index_price: item_data["indexPrice"]
                    .as_str()
                    .and_then(|s| s.parse().ok()),
                funding_countdown: None, // Calculate if needed: nextFundingTime - currentTime
                info: serde_json::json!({ "raw_data": item_clone }), // Added missing info field
            })
        } else {
            self.logger.warn(&format!(
                "No funding rate data found in Binance response for {}",
                symbol
            ));
            Err(ArbitrageError::api_error(format!(
                "No funding rate data in Binance response for {}",
                symbol
            )))
        }
    }

    /// Fetch Binance volume data
    async fn fetch_binance_volume_data(&mut self, symbol: &str) -> ArbitrageResult<VolumeData> {
        self.metrics.api_calls += 1;
        let url = format!(
            "https://api.binance.com/api/v3/ticker/24hr?symbol={}",
            symbol.replace("-", "")
        );
        let client = Client::new();
        let request = client
            .get(&url)
            .header("User-Agent", "ArbEdgeBot/1.0")
            .build()
            .map_err(|e| {
                ArbitrageError::network_error(format!(
                    "Failed to build Binance volume request for {}: {}",
                    url, e
                ))
            })?;

        let response = client.execute(request).await.map_err(|e| {
            ArbitrageError::network_error(format!(
                "Binance volume request failed for {}: {}",
                symbol, e
            ))
        })?;

        let status = response.status();
        if status != 200 {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            self.logger.error(&format!(
                "Binance volume API error for {}: {} - {}",
                symbol, status, error_body
            ));
            return Err(ArbitrageError::api_error(format!(
                "Binance volume API error {}: {}",
                status, error_body
            )));
        }

        let data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse Binance volume response for {}: {}",
                symbol, e
            ))
        })?;

        if let Some(result) = data.get("result") {
            if let Some(data) = result.get("list").and_then(|l| l.as_array()) {
                if let Some(item) = data.first() {
                    return Ok(VolumeData {
                        volume_24h: item["volume"]
                            .as_str()
                            .unwrap_or("0")
                            .parse()
                            .unwrap_or(0.0),
                        volume_24h_usd: item["quoteVolume"].as_str().and_then(|s| s.parse().ok()),
                        trades_count_24h: item["count"].as_u64(),
                    });
                }
            }
        }
        Err(ArbitrageError::parse_error(
            "Failed to extract volume data from Binance response".to_string(),
        ))
    }

    /// Fetch Bybit price data
    async fn fetch_bybit_price_data(&mut self, symbol: &str) -> ArbitrageResult<PriceData> {
        self.metrics.api_calls += 1;
        let url = format!(
            "https://api.bybit.com/v5/market/tickers?category=linear&symbol={}",
            symbol.replace("-", "")
        );

        let client = Client::new();
        let request = client
            .get(&url)
            .header("User-Agent", "ArbEdgeBot/1.0")
            .build()
            .map_err(|e| {
                ArbitrageError::network_error(format!(
                    "Failed to build Bybit price request for {}: {}",
                    url, e
                ))
            })?;

        let response = client.execute(request).await.map_err(|e| {
            ArbitrageError::network_error(format!(
                "Bybit price request failed for {}: {}",
                symbol, e
            ))
        })?;

        let status = response.status();
        if status != 200 {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            self.logger.error(&format!(
                "Bybit price API error for {}: {} - {}",
                symbol, status, error_body
            ));
            return Err(ArbitrageError::api_error(format!(
                "Bybit price API error {}: {}",
                status, error_body
            )));
        }

        let data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse Bybit price response for {}: {}",
                symbol, e
            ))
        })?;

        if let Some(ticker_list) = data["result"]["list"].as_array() {
            if let Some(ticker) = ticker_list.first() {
                // Assuming first item is the relevant one
                return Ok(PriceData {
                    price: ticker["lastPrice"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    bid: ticker["bid1Price"].as_str().and_then(|s| s.parse().ok()),
                    ask: ticker["ask1Price"].as_str().and_then(|s| s.parse().ok()),
                    high_24h: ticker["highPrice24h"].as_str().and_then(|s| s.parse().ok()),
                    low_24h: ticker["lowPrice24h"].as_str().and_then(|s| s.parse().ok()),
                    change_24h: None, // Bybit provides percentage, calc if needed: (lastPrice - prevPrice24h)
                    change_percentage_24h: ticker["price24hPcnt"]
                        .as_str()
                        .and_then(|s| s.parse::<f64>().ok().map(|p| p * 100.0)), // Bybit uses decimal e.g. 0.01 for 1%
                });
            }
        }
        self.logger.warn(&format!(
            "No price data found in Bybit response for {}",
            symbol
        ));
        Err(ArbitrageError::api_error(format!(
            "No price data in Bybit response for {}",
            symbol
        )))
    }

    /// Fetch Bybit funding rate
    async fn fetch_bybit_funding_rate(&mut self, symbol: &str) -> ArbitrageResult<FundingRateInfo> {
        self.metrics.api_calls += 1;
        let url = format!(
            "https://api.bybit.com/v5/market/funding/history?category=linear&symbol={}&limit=1",
            symbol.replace("-", "")
        );

        let client = Client::new();
        let request = client
            .get(&url)
            .header("User-Agent", "ArbEdgeBot/1.0")
            .build()
            .map_err(|e| {
                ArbitrageError::network_error(format!(
                    "Failed to build Bybit funding rate request for {}: {}",
                    url, e
                ))
            })?;

        let response = client.execute(request).await.map_err(|e| {
            ArbitrageError::network_error(format!(
                "Bybit funding rate request failed for {}: {}",
                symbol, e
            ))
        })?;

        let status = response.status();
        if status != 200 {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            self.logger.error(&format!(
                "Bybit funding rate API error for {}: {} - {}",
                symbol, status, error_body
            ));
            return Err(ArbitrageError::api_error(format!(
                "Bybit funding rate API error {}: {}",
                status, error_body
            )));
        }

        let data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse Bybit funding rate response for {}: {}",
                symbol, e
            ))
        })?;

        if let Some(rate_list) = data["result"]["list"].as_array() {
            if let Some(rate_item) = rate_list.first() {
                // Assuming first item is the latest funding rate
                let rate_item_clone = rate_item.clone(); // Clone for use in info field
                let symbol_clone = rate_item["symbol"].as_str().unwrap_or_default().to_string();
                let funding_rate_value: f64 = rate_item["fundingRate"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                let funding_timestamp_str = rate_item["fundingTime"].as_str().unwrap_or("0");
                let funding_timestamp = funding_timestamp_str.parse::<u64>().unwrap_or(0);
                let datetime_str = Utc
                    .timestamp_millis_opt(funding_timestamp as i64)
                    .single()
                    .map_or_else(|| Utc::now().to_rfc3339(), |dt| dt.to_rfc3339());

                return Ok(FundingRateInfo {
                    symbol: symbol_clone,
                    funding_rate: funding_rate_value,
                    timestamp: funding_timestamp,
                    datetime: datetime_str,
                    next_funding_time: None, // v5/market/funding/history doesn't provide next funding time easily
                    estimated_rate: None,
                    estimated_settle_price: None,
                    exchange: ExchangeIdEnum::Bybit,
                    funding_interval_hours: 8, // Bybit typical interval
                    mark_price: None,          // Not in funding/history endpoint
                    index_price: None,         // Not in funding/history endpoint
                    funding_countdown: None,
                    info: serde_json::json!({ "raw_data": rate_item_clone }), // Added info field
                });
            }
        }
        self.logger.warn(&format!(
            "No funding rate data found in Bybit response for {}",
            symbol
        ));
        Err(ArbitrageError::api_error(format!(
            "No funding rate data in Bybit response for {}",
            symbol
        )))
    }

    /// Fetch Bybit volume data
    async fn fetch_bybit_volume_data(&mut self, symbol: &str) -> ArbitrageResult<VolumeData> {
        self.metrics.api_calls += 1;
        let url = format!(
            "https://api.bybit.com/v5/market/tickers?category=linear&symbol={}",
            symbol.replace("-", "")
        );

        let client = Client::new();
        let request = client
            .get(&url)
            .header("User-Agent", "ArbEdgeBot/1.0")
            .build()
            .map_err(|e| {
                ArbitrageError::network_error(format!(
                    "Failed to build Bybit volume request for {}: {}",
                    url, e
                ))
            })?;

        let response = client.execute(request).await.map_err(|e| {
            ArbitrageError::network_error(format!(
                "Bybit volume request failed for {}: {}",
                symbol, e
            ))
        })?;

        let status = response.status();
        if status != 200 {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            self.logger.error(&format!(
                "Bybit volume API error for {}: {} - {}",
                symbol, status, error_body
            ));
            return Err(ArbitrageError::api_error(format!(
                "Bybit volume API error {}: {}",
                status, error_body
            )));
        }

        let data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse Bybit volume response for {}: {}",
                symbol, e
            ))
        })?;

        if let Some(ticker_list) = data["result"]["list"].as_array() {
            if let Some(ticker) = ticker_list.first() {
                // Assuming first item is the relevant one
                return Ok(VolumeData {
                    volume_24h: ticker["volume24h"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    volume_24h_usd: ticker["turnover24h"].as_str().and_then(|s| s.parse().ok()),
                    trades_count_24h: None, // Bybit v5 market/tickers doesn't provide trade count
                });
            }
        }
        self.logger.warn(&format!(
            "No volume data found in Bybit response for {}",
            symbol
        ));
        Err(ArbitrageError::api_error(format!(
            "No volume data in Bybit response for {}",
            symbol
        )))
    }

    /// Fetch OKX price data
    async fn fetch_okx_price_data(&mut self, symbol: &str) -> ArbitrageResult<PriceData> {
        self.metrics.api_calls += 1;
        let url = format!("https://www.okx.com/api/v5/market/ticker?instId={}", symbol);

        let client = Client::new();
        let request = client
            .get(&url)
            .header("User-Agent", "ArbEdgeBot/1.0")
            .build()
            .map_err(|e| {
                ArbitrageError::network_error(format!(
                    "Failed to build OKX price request for {}: {}",
                    url, e
                ))
            })?;

        let response = client.execute(request).await.map_err(|e| {
            ArbitrageError::network_error(format!("OKX price request failed for {}: {}", symbol, e))
        })?;

        let status = response.status();
        if status != 200 {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            self.logger.error(&format!(
                "OKX price API error for {}: {} - {}",
                symbol, status, error_body
            ));
            return Err(ArbitrageError::api_error(format!(
                "OKX price API error {}: {}",
                status, error_body
            )));
        }

        let data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to parse OKX price response for {}: {}",
                symbol, e
            ))
        })?;

        if let Some(ticker_list) = data["data"].as_array() {
            if let Some(ticker) = ticker_list.first() {
                // Assuming first item for the specified instId
                return Ok(PriceData {
                    price: ticker["last"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    bid: ticker["bidPx"].as_str().and_then(|s| s.parse().ok()),
                    ask: ticker["askPx"].as_str().and_then(|s| s.parse().ok()),
                    high_24h: ticker["high24h"].as_str().and_then(|s| s.parse().ok()),
                    low_24h: ticker["low24h"].as_str().and_then(|s| s.parse().ok()),
                    change_24h: None, // OKX provides open24h and last, can calculate if needed: last - open24h
                    change_percentage_24h: ticker["change24h"]
                        .as_str()
                        .and_then(|s| s.parse::<f64>().ok().map(|p| p * 100.0)),
                });
            }
        }
        self.logger.warn(&format!(
            "No price data found in OKX response for {}",
            symbol
        ));
        Err(ArbitrageError::api_error(format!(
            "No price data in OKX response for {}",
            symbol
        )))
    }

    /// Get cached market data
    async fn get_cached_market_data(
        &self,
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        let cache_key = format!("market_data:{}:{}", exchange.as_str(), pair);

        match self.kv_store.get(&cache_key).text().await {
            Ok(Some(string_data)) => {
                // Successfully retrieved string data from KV
                match serde_json::from_str::<MarketDataSnapshot>(&string_data) {
                    Ok(snapshot) => Ok(snapshot),
                    Err(e) => Err(ArbitrageError::parse_error(format!(
                        "Failed to parse cached market data for {}:{}: {}",
                        exchange.as_str(),
                        pair,
                        e
                    ))),
                }
            }
            Ok(None) => {
                // Key found in KV, but the text content was null or empty
                Err(ArbitrageError::parse_error(format!(
                    "Cached data for {}:{} is present but empty or not valid text",
                    exchange.as_str(),
                    pair
                )))
            }
            Err(e) => {
                // Error fetching from KV store (e.g., key not found, network issue)
                Err(ArbitrageError::not_found(format!(
                    "No cached data found for {}:{}. KV Error: {}",
                    exchange.as_str(),
                    pair,
                    e
                )))
            }
        }
    }

    /// Cache market data
    async fn cache_market_data(&self, snapshot: &MarketDataSnapshot) -> ArbitrageResult<()> {
        let cache_key = format!(
            "market_data:{}:{}",
            snapshot.exchange.as_str(),
            snapshot.symbol
        );
        let serialized = serde_json::to_string(snapshot)?;

        self.kv_store
            .put(&cache_key, serialized)?
            .expiration_ttl(self.config.cache_ttl_seconds)
            .execute()
            .await?;

        Ok(())
    }

    /// Get market data from pipeline
    #[allow(dead_code)] // Will be used for pipeline data integration
    async fn get_pipeline_market_data(
        &self,
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        self.logger.warn(&format!(
            "Attempted to use unsupported get_pipeline_market_data for {}:{} (exchange: {})",
            pair,
            exchange.as_str(),
            exchange.as_str()
        ));
        Err(ArbitrageError::service_unavailable(
            "Direct data retrieval from pipeline service is not supported. Fetch from cache or API.",
        ))
    }

    /// Store snapshots to pipeline with error collection
    async fn store_snapshots_to_pipeline(
        &mut self,
        snapshots: &[MarketDataSnapshot],
    ) -> ArbitrageResult<()> {
        if self.cloudflare_pipelines_service.is_none() || self.analytics_engine.is_none() {
            // Check if services are available
            return Ok(());
        }

        if snapshots.is_empty() {
            return Ok(());
        }

        if let Some(engine) = &mut self.analytics_engine {
            for snapshot in snapshots {
                engine.track_market_snapshot(snapshot).await?;
            }
        } else {
            self.logger
                .info("Analytics engine not configured, skipping pipeline storage.");
        }

        self.logger.info(&format!(
            "Successfully sent {} snapshots to pipeline",
            snapshots.len()
        ));

        Ok(())
    }

    /// Get ingestion metrics
    pub fn get_metrics(&self) -> &IngestionMetrics {
        &self.metrics
    }

    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = IngestionMetrics {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            cache_hits: 0,
            pipeline_hits: 0,
            api_calls: 0,
            data_volume_mb: 0.0,
            average_latency_ms: 0.0,
            last_ingestion_timestamp: 0,
        };
    }

    /// Get market data with hybrid access pattern (public interface)
    pub async fn get_market_data(
        &mut self,
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        self.ingest_exchange_pair_data(exchange, pair).await
    }

    /// Force refresh market data (bypass cache)
    pub async fn force_refresh_market_data(
        &mut self,
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        let snapshot = self.fetch_real_market_data(exchange, pair).await?;
        self.metrics.api_calls += 1;

        // Update cache
        let _ = self.cache_market_data(&snapshot).await;

        // Store to pipeline
        if let Some(ref _pipelines) = self.cloudflare_pipelines_service {
            let key = format!(
                "market_data_{}_{}",
                snapshot.exchange.as_str(),
                snapshot.symbol
            );
            self.logger.debug(&format!(
                "TODO: Implement pipeline storage for refreshed snapshot key {}",
                key
            ));
        }

        Ok(snapshot)
    }

    // Method to get all funding rates sequentially, dispatching by exchange
    pub async fn get_all_funding_rates_concurrently(
        &mut self,
        exchange: ExchangeIdEnum,
        pairs: Vec<String>,
    ) -> Vec<ArbitrageResult<FundingRateInfo>> {
        let mut results = Vec::new();

        for pair in pairs {
            let result = match exchange {
                ExchangeIdEnum::Binance => self.fetch_binance_funding_rate(&pair).await,
                ExchangeIdEnum::Bybit => self.fetch_bybit_funding_rate(&pair).await,
                // TODO: Add fetch_okx_funding_rate and other exchanges if they have funding rate methods
                _ => Err(ArbitrageError::service_unavailable(format!(
                    "Funding rates not supported for {:?} on pair {}",
                    exchange, pair
                ))),
            };
            results.push(result);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_data_ingestion_config_creation() {
        let config = MarketDataIngestionConfig::default();
        assert_eq!(config.ingestion_interval_seconds, 30);
        assert_eq!(config.max_ingestion_rate_mb_per_sec, 100);
        assert!(config.enable_funding_rates);
        assert!(config.enable_price_data);
        assert!(config.enable_volume_data);
        assert!(!config.enable_orderbook_snapshots);
    }

    #[test]
    fn test_market_data_snapshot_structure() {
        let snapshot = MarketDataSnapshot {
            exchange: ExchangeIdEnum::Binance,
            symbol: "BTC-USDT".to_string(),
            timestamp: 1640995200000,
            price_data: Some(PriceData {
                price: 50000.0,
                bid: Some(49999.0),
                ask: Some(50001.0),
                high_24h: Some(51000.0),
                low_24h: Some(49000.0),
                change_24h: Some(1000.0),
                change_percentage_24h: Some(2.0),
            }),
            funding_rate_data: None,
            volume_data: None,
            orderbook_data: None,
            source: DataSource::RealAPI,
        };

        assert_eq!(snapshot.exchange, ExchangeIdEnum::Binance);
        assert_eq!(snapshot.symbol, "BTC-USDT");
        assert!(snapshot.price_data.is_some());
        assert!(matches!(snapshot.source, DataSource::RealAPI));
    }

    #[test]
    fn test_price_data_structure() {
        let price_data = PriceData {
            price: 50000.0,
            bid: Some(49999.0),
            ask: Some(50001.0),
            high_24h: Some(51000.0),
            low_24h: Some(49000.0),
            change_24h: Some(1000.0),
            change_percentage_24h: Some(2.0),
        };

        assert_eq!(price_data.price, 50000.0);
        assert_eq!(price_data.bid, Some(49999.0));
        assert_eq!(price_data.ask, Some(50001.0));
        assert_eq!(price_data.change_percentage_24h, Some(2.0));
    }

    #[test]
    fn test_volume_data_structure() {
        let volume_data = VolumeData {
            volume_24h: 1000.0,
            volume_24h_usd: Some(50000000.0),
            trades_count_24h: Some(10000),
        };

        assert_eq!(volume_data.volume_24h, 1000.0);
        assert_eq!(volume_data.volume_24h_usd, Some(50000000.0));
        assert_eq!(volume_data.trades_count_24h, Some(10000));
    }

    #[test]
    fn test_ingestion_metrics_structure() {
        let metrics = IngestionMetrics {
            total_requests: 100,
            successful_requests: 95,
            failed_requests: 5,
            cache_hits: 30,
            pipeline_hits: 20,
            api_calls: 45,
            data_volume_mb: 10.5,
            average_latency_ms: 250.0,
            last_ingestion_timestamp: 1640995200000,
        };

        assert_eq!(metrics.total_requests, 100);
        assert_eq!(metrics.successful_requests, 95);
        assert_eq!(metrics.failed_requests, 5);
        assert_eq!(metrics.cache_hits, 30);
        assert_eq!(metrics.pipeline_hits, 20);
        assert_eq!(metrics.api_calls, 45);
    }

    #[test]
    fn test_data_source_enum() {
        assert!(matches!(DataSource::RealAPI, DataSource::RealAPI));
        assert!(matches!(DataSource::Pipeline, DataSource::Pipeline));
        assert!(matches!(DataSource::Cache, DataSource::Cache));
        assert!(matches!(
            DataSource::CoinMarketCap,
            DataSource::CoinMarketCap
        ));
    }
}
