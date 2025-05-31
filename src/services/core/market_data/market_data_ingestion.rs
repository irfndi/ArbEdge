use crate::types::{ArbitrageResult, ArbitrageError, ExchangeIdEnum, FundingRateInfo, Ticker};
use crate::services::core::infrastructure::cloudflare_pipelines::CloudflarePipelinesService;
use crate::services::core::logging::Logger;
use crate::services::core::market_data::coinmarketcap::{CoinMarketCapService, CmcQuoteData, CmcGlobalMetrics};
use worker::kv::KvStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::*;
use chrono::{DateTime, Utc};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use futures::future;

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
    pipelines_service: Option<CloudflarePipelinesService>,
    coinmarketcap_service: Option<CoinMarketCapService>,
    kv_store: KvStore,
    logger: Logger,
    metrics: IngestionMetrics,
}

impl MarketDataIngestionService {
    pub fn new(
        config: MarketDataIngestionConfig,
        pipelines_service: Option<CloudflarePipelinesService>,
        coinmarketcap_service: Option<CoinMarketCapService>,
        kv_store: KvStore,
        logger: Logger,
    ) -> Self {
        Self {
            config,
            pipelines_service,
            coinmarketcap_service,
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
    pub fn set_pipelines_service(&mut self, pipelines_service: Option<CloudflarePipelinesService>) {
        self.pipelines_service = pipelines_service;
    }

    /// Main ingestion method implementing hybrid data access pattern
    pub async fn ingest_market_data(&mut self) -> ArbitrageResult<Vec<MarketDataSnapshot>> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let mut snapshots = Vec::new();

        self.logger.info("Starting market data ingestion cycle");

        // TODO: Implement concurrent processing for better performance
        // Current limitation: Rust borrowing rules prevent concurrent access to &mut self
        // Future improvement: Refactor to use Arc<Mutex<Self>> or separate the mutable state
        
        // Ingest data for all monitored pairs and exchanges
        for pair in &self.config.monitored_pairs.clone() {
            for exchange in &self.config.monitored_exchanges.clone() {
                match self.ingest_exchange_pair_data(exchange, pair).await {
                    Ok(snapshot) => {
                        snapshots.push(snapshot);
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
        }

        // Store aggregated data to pipelines
        if let Some(ref pipelines) = self.pipelines_service {
            if let Err(e) = self.store_snapshots_to_pipeline(pipelines, &snapshots).await {
                self.logger.warn(&format!("Failed to store snapshots to pipeline: {}", e));
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

        // Step 2: Try pipeline data (medium speed)
        if let Some(ref pipelines) = self.pipelines_service {
            if let Ok(pipeline_data) = self.get_pipeline_market_data(pipelines, exchange, pair).await {
                self.metrics.pipeline_hits += 1;
                // Cache the pipeline data for future use
                let _ = self.cache_market_data(&pipeline_data).await;
                return Ok(pipeline_data);
            }
        }

        // Step 3: Fetch from real APIs (slowest but most current)
        let snapshot = self.fetch_real_market_data(exchange, pair).await?;
        self.metrics.api_calls += 1;

        // Cache the fresh data
        let _ = self.cache_market_data(&snapshot).await;

        Ok(snapshot)
    }

    /// Fetch real market data from exchange APIs
    async fn fetch_real_market_data(
        &self,
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        match exchange {
            ExchangeIdEnum::Binance => self.fetch_binance_data(pair).await,
            ExchangeIdEnum::Bybit => self.fetch_bybit_data(pair).await,
            ExchangeIdEnum::OKX => self.fetch_okx_data(pair).await,
            _ => Err(ArbitrageError::not_implemented(&format!(
                "Exchange {} not supported for real market data fetching",
                exchange.as_str()
            ))),
        }
    }

    /// Fetch data from Binance API
    async fn fetch_binance_data(&self, pair: &str) -> ArbitrageResult<MarketDataSnapshot> {
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
            snapshot.funding_rate_data = self.fetch_binance_funding_rate(&binance_symbol).await.ok();
        }

        // Fetch volume data
        if self.config.enable_volume_data {
            snapshot.volume_data = self.fetch_binance_volume_data(&binance_symbol).await.ok();
        }

        Ok(snapshot)
    }

    /// Fetch data from Bybit API
    async fn fetch_bybit_data(&self, pair: &str) -> ArbitrageResult<MarketDataSnapshot> {
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
    async fn fetch_okx_data(&self, pair: &str) -> ArbitrageResult<MarketDataSnapshot> {
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
    async fn fetch_binance_price_data(&self, symbol: &str) -> ArbitrageResult<PriceData> {
        let url = format!("https://api.binance.com/api/v3/ticker/24hr?symbol={}", symbol);
        
        let request = Request::new_with_init(
            &url,
            RequestInit::new().with_method(Method::Get),
        )?;

        let mut response = Fetch::Request(request).send().await?;
        
        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(&format!(
                "Binance price API error: {}", response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let data: serde_json::Value = serde_json::from_str(&response_text)?;

        Ok(PriceData {
            price: data["lastPrice"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
            bid: data["bidPrice"].as_str().and_then(|s| s.parse().ok()),
            ask: data["askPrice"].as_str().and_then(|s| s.parse().ok()),
            high_24h: data["highPrice"].as_str().and_then(|s| s.parse().ok()),
            low_24h: data["lowPrice"].as_str().and_then(|s| s.parse().ok()),
            change_24h: data["priceChange"].as_str().and_then(|s| s.parse().ok()),
            change_percentage_24h: data["priceChangePercent"].as_str().and_then(|s| s.parse().ok()),
        })
    }

    /// Fetch Binance funding rate
    async fn fetch_binance_funding_rate(&self, symbol: &str) -> ArbitrageResult<FundingRateInfo> {
        let url = format!("https://fapi.binance.com/fapi/v1/premiumIndex?symbol={}", symbol);
        
        let request = Request::new_with_init(
            &url,
            RequestInit::new().with_method(Method::Get),
        )?;

        let mut response = Fetch::Request(request).send().await?;
        
        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(&format!(
                "Binance funding rate API error: {}", response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let data: serde_json::Value = serde_json::from_str(&response_text)?;

        let funding_rate = data["lastFundingRate"]
            .as_str()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);

        Ok(FundingRateInfo {
            symbol: symbol.to_string(),
            funding_rate,
            timestamp: Utc::now().timestamp_millis() as u64,
            datetime: Utc::now().to_rfc3339(),
            next_funding_time: data["nextFundingTime"]
                .as_u64()
                .and_then(|ts| chrono::DateTime::from_timestamp((ts / 1000) as i64, 0)),
            estimated_rate: data["estimatedSettlePrice"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok()),
        })
    }

    /// Fetch Binance volume data
    async fn fetch_binance_volume_data(&self, symbol: &str) -> ArbitrageResult<VolumeData> {
        let url = format!("https://api.binance.com/api/v3/ticker/24hr?symbol={}", symbol);
        
        let request = Request::new_with_init(
            &url,
            RequestInit::new().with_method(Method::Get),
        )?;

        let mut response = Fetch::Request(request).send().await?;
        
        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(&format!(
                "Binance volume API error: {}", response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let data: serde_json::Value = serde_json::from_str(&response_text)?;

        Ok(VolumeData {
            volume_24h: data["volume"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
            volume_24h_usd: data["quoteVolume"].as_str().and_then(|s| s.parse().ok()),
            trades_count_24h: data["count"].as_u64(),
        })
    }

    /// Fetch Bybit price data
    async fn fetch_bybit_price_data(&self, symbol: &str) -> ArbitrageResult<PriceData> {
        let url = format!("https://api.bybit.com/v5/market/tickers?category=spot&symbol={}", symbol);
        
        let request = Request::new_with_init(
            &url,
            RequestInit::new().with_method(Method::Get),
        )?;

        let mut response = Fetch::Request(request).send().await?;
        
        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(&format!(
                "Bybit price API error: {}", response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(result) = response_json.get("result") {
            if let Some(list) = result.get("list").and_then(|l| l.as_array()) {
                if let Some(ticker) = list.first() {
                    return Ok(PriceData {
                        price: ticker["lastPrice"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        bid: ticker["bid1Price"].as_str().and_then(|s| s.parse().ok()),
                        ask: ticker["ask1Price"].as_str().and_then(|s| s.parse().ok()),
                        high_24h: ticker["highPrice24h"].as_str().and_then(|s| s.parse().ok()),
                        low_24h: ticker["lowPrice24h"].as_str().and_then(|s| s.parse().ok()),
                        change_24h: ticker["price24hPcnt"].as_str().and_then(|s| s.parse().ok()),
                        change_percentage_24h: ticker["price24hPcnt"].as_str().and_then(|s| s.parse::<f64>().ok().map(|v| v * 100.0)),
                    });
                }
            }
        }

        Err(ArbitrageError::parse_error("Failed to parse Bybit price data"))
    }

    /// Fetch Bybit funding rate
    async fn fetch_bybit_funding_rate(&self, symbol: &str) -> ArbitrageResult<FundingRateInfo> {
        let url = format!(
            "https://api.bybit.com/v5/market/funding/history?category=linear&symbol={}&limit=1",
            symbol
        );
        
        let request = Request::new_with_init(
            &url,
            RequestInit::new().with_method(Method::Get),
        )?;

        let mut response = Fetch::Request(request).send().await?;
        
        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(&format!(
                "Bybit funding rate API error: {}", response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(result) = response_json.get("result") {
            if let Some(list) = result.get("list").and_then(|l| l.as_array()) {
                if let Some(funding_data) = list.first() {
                    let funding_rate = funding_data["fundingRate"]
                        .as_str()
                        .unwrap_or("0")
                        .parse::<f64>()
                        .unwrap_or(0.0);

                    return Ok(FundingRateInfo {
                        symbol: symbol.to_string(),
                        funding_rate,
                        timestamp: Utc::now().timestamp_millis() as u64,
                        datetime: Utc::now().to_rfc3339(),
                        next_funding_time: None,
                        estimated_rate: None,
                    });
                }
            }
        }

        Err(ArbitrageError::parse_error("Failed to parse Bybit funding rate data"))
    }

    /// Fetch Bybit volume data
    async fn fetch_bybit_volume_data(&self, symbol: &str) -> ArbitrageResult<VolumeData> {
        let url = format!("https://api.bybit.com/v5/market/tickers?category=spot&symbol={}", symbol);
        
        let request = Request::new_with_init(
            &url,
            RequestInit::new().with_method(Method::Get),
        )?;

        let mut response = Fetch::Request(request).send().await?;
        
        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(&format!(
                "Bybit volume API error: {}", response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(result) = response_json.get("result") {
            if let Some(list) = result.get("list").and_then(|l| l.as_array()) {
                if let Some(ticker) = list.first() {
                    return Ok(VolumeData {
                        volume_24h: ticker["volume24h"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        volume_24h_usd: ticker["turnover24h"].as_str().and_then(|s| s.parse().ok()),
                        trades_count_24h: None, // Bybit doesn't provide trade count in this endpoint
                    });
                }
            }
        }

        Err(ArbitrageError::parse_error("Failed to parse Bybit volume data"))
    }

    /// Fetch OKX price data
    async fn fetch_okx_price_data(&self, symbol: &str) -> ArbitrageResult<PriceData> {
        let url = format!("https://www.okx.com/api/v5/market/ticker?instId={}", symbol);
        
        let request = Request::new_with_init(
            &url,
            RequestInit::new().with_method(Method::Get),
        )?;

        let mut response = Fetch::Request(request).send().await?;
        
        if response.status_code() != 200 {
            return Err(ArbitrageError::api_error(&format!(
                "OKX price API error: {}", response.status_code()
            )));
        }

        let response_text = response.text().await?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(data) = response_json.get("data").and_then(|d| d.as_array()) {
            if let Some(ticker) = data.first() {
                return Ok(PriceData {
                    price: ticker["last"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                    bid: ticker["bidPx"].as_str().and_then(|s| s.parse().ok()),
                    ask: ticker["askPx"].as_str().and_then(|s| s.parse().ok()),
                    high_24h: ticker["high24h"].as_str().and_then(|s| s.parse().ok()),
                    low_24h: ticker["low24h"].as_str().and_then(|s| s.parse().ok()),
                    change_24h: None, // OKX provides percentage, not absolute change
                    change_percentage_24h: ticker["priceChangePercent"].as_str().and_then(|s| s.parse::<f64>().ok()),
                });
            }
        }

        Err(ArbitrageError::parse_error("Failed to parse OKX price data"))
    }

    /// Get cached market data
    async fn get_cached_market_data(
        &self,
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        let cache_key = format!("market_data:{}:{}", exchange.as_str(), pair);
        
        match self.kv_store.get(&cache_key).text().await {
            Ok(cached_data) => {
                match serde_json::from_str::<MarketDataSnapshot>(&cached_data) {
                    Ok(snapshot) => Ok(snapshot),
                    Err(e) => Err(ArbitrageError::parse_error(&format!(
                        "Failed to parse cached market data: {}", e
                    ))),
                }
            }
            Err(e) => Err(ArbitrageError::not_found(&format!(
                "No cached data for {}:{} - {}", exchange.as_str(), pair, e
            ))),
        }
    }

    /// Cache market data
    async fn cache_market_data(&self, snapshot: &MarketDataSnapshot) -> ArbitrageResult<()> {
        let cache_key = format!("market_data:{}:{}", snapshot.exchange.as_str(), snapshot.symbol);
        let serialized = serde_json::to_string(snapshot)?;
        
        self.kv_store
            .put(&cache_key, serialized)?
            .expiration_ttl(self.config.cache_ttl_seconds)
            .execute()
            .await?;
        
        Ok(())
    }

    /// Get market data from pipeline
    async fn get_pipeline_market_data(
        &self,
        pipelines: &CloudflarePipelinesService,
        exchange: &ExchangeIdEnum,
        pair: &str,
    ) -> ArbitrageResult<MarketDataSnapshot> {
        let data = pipelines.get_latest_data(&format!("market_data_{}_{}", exchange.as_str(), pair)).await?;
        
        // Parse pipeline data to MarketDataSnapshot
        match serde_json::from_value::<MarketDataSnapshot>(data) {
            Ok(snapshot) => Ok(snapshot),
            Err(e) => Err(ArbitrageError::parse_error(&format!(
                "Failed to parse pipeline market data: {}", e
            ))),
        }
    }

    /// Store snapshots to pipeline with error collection
    async fn store_snapshots_to_pipeline(
        &self,
        pipelines: &CloudflarePipelinesService,
        snapshots: &[MarketDataSnapshot],
    ) -> ArbitrageResult<()> {
        let mut errors = Vec::new();
        let mut successful_stores = 0;
        
        for snapshot in snapshots {
            let data = match serde_json::to_value(snapshot) {
                Ok(data) => data,
                Err(e) => {
                    let error_msg = format!(
                        "Failed to serialize snapshot for {}:{} - {}",
                        snapshot.exchange.as_str(), snapshot.symbol, e
                    );
                    self.logger.warn(&error_msg);
                    errors.push(error_msg);
                    continue;
                }
            };
            
            let key = format!("market_data_{}_{}", snapshot.exchange.as_str(), snapshot.symbol);
            
            match pipelines.store_market_data(&key, &data).await {
                Ok(_) => {
                    successful_stores += 1;
                    self.logger.debug(&format!("Successfully stored snapshot for {}", key));
                }
                Err(e) => {
                    let error_msg = format!(
                        "Failed to store snapshot to pipeline for {}: {}",
                        key, e
                    );
                    self.logger.warn(&error_msg);
                    errors.push(error_msg);
                }
            }
        }
        
        // Return aggregated error if any failures occurred
        if !errors.is_empty() {
            let summary = format!(
                "Pipeline storage completed with {} successes and {} failures. Errors: {}",
                successful_stores,
                errors.len(),
                errors.join("; ")
            );
            return Err(ArbitrageError::storage_error(summary));
        }
        
        self.logger.info(&format!(
            "Successfully stored {} snapshots to pipeline",
            successful_stores
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
        if let Some(ref pipelines) = self.pipelines_service {
            let data = serde_json::to_value(&snapshot)?;
            let key = format!("market_data_{}_{}", snapshot.exchange.as_str(), snapshot.symbol);
            let _ = pipelines.store_market_data(&key, &data).await;
        }
        
        Ok(snapshot)
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
        assert!(matches!(DataSource::CoinMarketCap, DataSource::CoinMarketCap));
    }
} 