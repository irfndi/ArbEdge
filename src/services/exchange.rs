// src/services/exchange.rs

use chrono::Utc;
use reqwest::{Client, Method, Url};
use serde_json::{json, Value};
use std::collections::HashMap;
use worker::Env;

use crate::types::*;
use crate::utils::{ArbitrageError, ArbitrageResult};

// Exchange authentication helper
use hmac::{Hmac, Mac};
use sha2::Sha256;
use hex;

type HmacSha256 = Hmac<Sha256>;

#[allow(dead_code)]
pub trait ExchangeInterface {
    async fn save_api_key(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
    ) -> ArbitrageResult<()>;

    async fn get_api_key(&self, exchange_id: &str) -> ArbitrageResult<Option<ExchangeCredentials>>;
    async fn delete_api_key(&self, exchange_id: &str) -> ArbitrageResult<()>;

    async fn get_markets(&self, exchange_id: &str) -> ArbitrageResult<Vec<Market>>;
    async fn get_ticker(&self, exchange_id: &str, symbol: &str) -> ArbitrageResult<Ticker>;
    async fn get_orderbook(
        &self,
        exchange_id: &str,
        symbol: &str,
        limit: Option<u32>,
    ) -> ArbitrageResult<OrderBook>;

    async fn fetch_funding_rates(
        &self,
        exchange_id: &str,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>>;

    async fn get_balance(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
    ) -> ArbitrageResult<Value>;

    async fn create_order(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: &str,
        side: &str,
        amount: f64,
        price: Option<f64>,
    ) -> ArbitrageResult<Value>;

    async fn cancel_order(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        order_id: &str,
        symbol: &str,
    ) -> ArbitrageResult<Value>;

    async fn get_open_orders(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>>;

    async fn get_open_positions(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>>;

    async fn set_leverage(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: &str,
        leverage: u32,
    ) -> ArbitrageResult<Value>;

    async fn get_trading_fees(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
        symbol: &str,
    ) -> ArbitrageResult<Value>;
}

pub struct ExchangeService {
    client: Client,
    kv: worker::kv::KvStore,
    markets_cache: HashMap<String, (Vec<Market>, std::time::Instant)>,
    cache_ttl: std::time::Duration,
}

impl ExchangeService {
    #[allow(clippy::result_large_err)]
    pub fn new(env: &Env) -> ArbitrageResult<Self> {
        let client = Client::new();
        let kv = env.kv("ARBITRAGE_KV").map_err(|e| {
            ArbitrageError::internal_error(format!(
                "Failed to get KV store: {}",
                e
            ))
        })?;

        Ok(Self {
            client,
            kv,
            markets_cache: HashMap::new(),
            cache_ttl: std::time::Duration::from_secs(300), // 5 minutes
        })
    }

    // Exchange-specific implementations
    async fn binance_request(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        let base_url = "https://api.binance.com";
        let url = format!("{}{}", base_url, endpoint);

        let mut request_url = Url::parse(&url)
            .map_err(|e| ArbitrageError::network_error(format!("Invalid URL: {}", e)))?;

        // Add query parameters
        if let Some(params) = params {
            if let Some(obj) = params.as_object() {
                for (key, value) in obj {
                    if let Some(str_val) = value.as_str() {
                        request_url.query_pairs_mut().append_pair(key, str_val);
                    } else {
                        request_url
                            .query_pairs_mut()
                            .append_pair(key, &value.to_string());
                    }
                }
            }
        }

        let mut request = self.client.request(method, request_url);

        // Add authentication if provided
        if let Some(creds) = auth {
            let timestamp = Utc::now().timestamp_millis();
            let query_string = format!("timestamp={}", timestamp);

            // Create signature
            let mut mac = Hmac::<Sha256>::new_from_slice(creds.secret.as_bytes()).map_err(|e| {
                ArbitrageError::authentication_error(format!("Invalid secret key: {}", e))
            })?;
            mac.update(query_string.as_bytes());
            let signature = hex::encode(mac.finalize().into_bytes());

            request = request.header("X-MBX-APIKEY", &creds.api_key);
            request = request.query(&[
                ("timestamp", timestamp.to_string()),
                ("signature", signature),
            ]);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ArbitrageError::network_error(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::api_error(format!(
                "Binance API error: {}",
                error_text
            )));
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| ArbitrageError::parse_error(format!("Failed to parse JSON: {}", e)))?;
        Ok(json)
    }

    async fn bybit_request(
        &self,
        endpoint: &str,
        method: Method,
        params: Option<Value>,
        auth: Option<&ExchangeCredentials>,
    ) -> ArbitrageResult<Value> {
        let base_url = "https://api.bybit.com";
        let url = format!("{}{}", base_url, endpoint);

        let mut request = self.client.request(method, url);

        // Add authentication if provided
        if let Some(creds) = auth {
            let timestamp = Utc::now().timestamp_millis();
            let recv_window = "5000";

            let param_str = if let Some(params) = &params {
                serde_json::to_string(params).unwrap_or_default()
            } else {
                "{}".to_string()
            };

            let sign_str = format!("{}{}{}{}", timestamp, &creds.api_key, recv_window, param_str);

            let mut mac = Hmac::<Sha256>::new_from_slice(creds.secret.as_bytes()).map_err(|e| {
                ArbitrageError::authentication_error(format!("Invalid secret key: {}", e))
            })?;
            mac.update(sign_str.as_bytes());
            let signature = hex::encode(mac.finalize().into_bytes());

            request = request
                .header("X-BAPI-API-KEY", &creds.api_key)
                .header("X-BAPI-SIGN", signature)
                .header("X-BAPI-TIMESTAMP", timestamp.to_string())
                .header("X-BAPI-RECV-WINDOW", recv_window)
                .header("Content-Type", "application/json");

            if let Some(params) = params {
                request = request.json(&params);
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| ArbitrageError::network_error(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ArbitrageError::api_error(format!(
                "Bybit API error: {}",
                error_text
            )));
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| ArbitrageError::parse_error(format!("Failed to parse JSON: {}", e)))?;
        Ok(json)
    }

    #[allow(clippy::result_large_err)]
    fn parse_binance_ticker(&self, data: &Value, symbol: &str) -> ArbitrageResult<Ticker> {
        Ok(Ticker {
            symbol: symbol.to_string(),
            bid: data["bidPrice"].as_str().and_then(|s| s.parse().ok()),
            ask: data["askPrice"].as_str().and_then(|s| s.parse().ok()),
            last: data["price"].as_str().and_then(|s| s.parse().ok()),
            high: data["highPrice"].as_str().and_then(|s| s.parse().ok()),
            low: data["lowPrice"].as_str().and_then(|s| s.parse().ok()),
            volume: data["volume"].as_str().and_then(|s| s.parse().ok()),
            timestamp: Some(Utc::now()),
            datetime: Some(Utc::now().to_rfc3339()),
        })
    }

    #[allow(clippy::result_large_err)]
    fn parse_bybit_ticker(&self, data: &Value, symbol: &str) -> ArbitrageResult<Ticker> {
        Ok(Ticker {
            symbol: symbol.to_string(),
            bid: data["bid1Price"].as_str().and_then(|s| s.parse().ok()),
            ask: data["ask1Price"].as_str().and_then(|s| s.parse().ok()),
            last: data["lastPrice"].as_str().and_then(|s| s.parse().ok()),
            high: data["highPrice24h"].as_str().and_then(|s| s.parse().ok()),
            low: data["lowPrice24h"].as_str().and_then(|s| s.parse().ok()),
            volume: data["volume24h"].as_str().and_then(|s| s.parse().ok()),
            timestamp: Some(Utc::now()),
            datetime: Some(Utc::now().to_rfc3339()),
        })
    }
}

impl ExchangeInterface for ExchangeService {
    async fn save_api_key(
        &self,
        exchange_id: &str,
        credentials: &ExchangeCredentials,
    ) -> ArbitrageResult<()> {
        let key = format!("exchange_credentials_{}", exchange_id);
        let value = serde_json::to_string(credentials).map_err(|e| {
            ArbitrageError::serialization_error(format!("Failed to serialize credentials: {}", e))
        })?;

        self.kv.put(&key, value)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to save credentials: {}", e))
            })?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute save: {}", e))
            })?;

        Ok(())
    }

    async fn get_api_key(&self, exchange_id: &str) -> ArbitrageResult<Option<ExchangeCredentials>> {
        let key = format!("exchange_credentials_{}", exchange_id);
        
        match self.kv.get(&key).text().await {
            Ok(Some(value)) => {
                let credentials: ExchangeCredentials = serde_json::from_str(&value).map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to deserialize credentials: {}", e))
                })?;
                Ok(Some(credentials))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(ArbitrageError::database_error(format!("Failed to get credentials: {}", e))),
        }
    }

    async fn delete_api_key(&self, exchange_id: &str) -> ArbitrageResult<()> {
        let key = format!("exchange_credentials_{}", exchange_id);
        self.kv.delete(&key)
            .await
            .map_err(|e| ArbitrageError::database_error(format!("Failed to delete credentials: {}", e)))?;
        Ok(())
    }

    async fn get_markets(&self, exchange_id: &str) -> ArbitrageResult<Vec<Market>> {
        // Check cache first
        if let Some((markets, timestamp)) = self.markets_cache.get(exchange_id) {
            if timestamp.elapsed() < self.cache_ttl {
                return Ok(markets.clone());
            }
        }

        let markets = match exchange_id {
            "binance" => {
                let response = self.binance_request("/api/v3/exchangeInfo", Method::GET, None, None).await?;
                let empty_vec = vec![];
                let symbols = response["symbols"].as_array().unwrap_or(&empty_vec);
                
                symbols.iter().map(|symbol| {
                    Market {
                        id: symbol["symbol"].as_str().unwrap_or("").to_string(),
                        symbol: symbol["symbol"].as_str().unwrap_or("").to_string(),
                        base: symbol["baseAsset"].as_str().unwrap_or("").to_string(),
                        quote: symbol["quoteAsset"].as_str().unwrap_or("").to_string(),
                        active: symbol["status"].as_str() == Some("TRADING"),
                        precision: Precision {
                            amount: symbol["baseAssetPrecision"].as_i64().map(|x| x as i32),
                            price: symbol["quotePrecision"].as_i64().map(|x| x as i32),
                        },
                        limits: Limits {
                            amount: MinMax { min: Some(0.0), max: None },
                            price: MinMax { min: Some(0.0), max: None },
                            cost: MinMax { min: Some(0.0), max: None },
                        },
                        fees: None,
                    }
                }).collect()
            }
            "bybit" => {
                let response = self.bybit_request("/v5/market/instruments-info", Method::GET, Some(json!({"category": "spot"})), None).await?;
                let empty_vec = vec![];
                let symbols = response["result"]["list"].as_array().unwrap_or(&empty_vec);
                
                symbols.iter().map(|symbol| {
                    Market {
                        id: symbol["symbol"].as_str().unwrap_or("").to_string(),
                        symbol: symbol["symbol"].as_str().unwrap_or("").to_string(),
                        base: symbol["baseCoin"].as_str().unwrap_or("").to_string(),
                        quote: symbol["quoteCoin"].as_str().unwrap_or("").to_string(),
                        active: symbol["status"].as_str() == Some("Trading"),
                        precision: Precision {
                            amount: None,
                            price: None,
                        },
                        limits: Limits {
                            amount: MinMax { min: Some(0.0), max: None },
                            price: MinMax { min: Some(0.0), max: None },
                            cost: MinMax { min: Some(0.0), max: None },
                        },
                        fees: None,
                    }
                }).collect()
            }
            _ => return Err(ArbitrageError::validation_error(format!("Unsupported exchange: {}", exchange_id))),
        };

        Ok(markets)
    }

    async fn get_ticker(&self, exchange_id: &str, symbol: &str) -> ArbitrageResult<Ticker> {
        match exchange_id {
            "binance" => {
                let response = self.binance_request(
                    "/api/v3/ticker/24hr",
                    Method::GET,
                    Some(json!({"symbol": symbol})),
                    None,
                ).await?;
                self.parse_binance_ticker(&response, symbol)
            }
            "bybit" => {
                let response = self.bybit_request(
                    "/v5/market/tickers",
                    Method::GET,
                    Some(json!({"category": "spot", "symbol": symbol})),
                    None,
                ).await?;
                
                if let Some(list) = response["result"]["list"].as_array() {
                    if let Some(ticker_data) = list.first() {
                        return self.parse_bybit_ticker(ticker_data, symbol);
                    }
                }
                Err(ArbitrageError::not_found(format!("Ticker not found for symbol: {}", symbol)))
            }
            _ => Err(ArbitrageError::validation_error(format!("Unsupported exchange: {}", exchange_id))),
        }
    }

    async fn get_orderbook(
        &self,
        exchange_id: &str,
        symbol: &str,
        limit: Option<u32>,
    ) -> ArbitrageResult<OrderBook> {
        let limit = limit.unwrap_or(100);
        
        match exchange_id {
            "binance" => {
                let response = self.binance_request(
                    "/api/v3/depth",
                    Method::GET,
                    Some(json!({"symbol": symbol, "limit": limit})),
                    None,
                ).await?;
                
                let empty_vec = vec![];
                let bids: Vec<[f64; 2]> = response["bids"].as_array()
                    .unwrap_or(&empty_vec)
                    .iter()
                    .filter_map(|bid| {
                        if let Some(arr) = bid.as_array() {
                            if arr.len() >= 2 {
                                let price = arr[0].as_str()?.parse().ok()?;
                                let amount = arr[1].as_str()?.parse().ok()?;
                                Some([price, amount])
                            } else { None }
                        } else { None }
                    })
                    .collect();
                
                let empty_vec2 = vec![];
                let asks: Vec<[f64; 2]> = response["asks"].as_array()
                    .unwrap_or(&empty_vec2)
                    .iter()
                    .filter_map(|ask| {
                        if let Some(arr) = ask.as_array() {
                            if arr.len() >= 2 {
                                let price = arr[0].as_str()?.parse().ok()?;
                                let amount = arr[1].as_str()?.parse().ok()?;
                                Some([price, amount])
                            } else { None }
                        } else { None }
                    })
                    .collect();
                
                Ok(OrderBook {
                    symbol: symbol.to_string(),
                    bids,
                    asks,
                    timestamp: Some(Utc::now()),
                    datetime: Some(Utc::now().to_rfc3339()),
                })
            }
            _ => Err(ArbitrageError::validation_error(format!("Unsupported exchange: {}", exchange_id))),
        }
    }

    async fn fetch_funding_rates(
        &self,
        exchange_id: &str,
        symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>> {
        match exchange_id {
            "binance" => {
                let mut params = json!({});
                if let Some(s) = symbol {
                    params["symbol"] = json!(s);
                }
                
                let response = self.binance_request(
                    "/fapi/v1/premiumIndex",
                    Method::GET,
                    Some(params),
                    None,
                ).await?;
                
                if response.is_array() {
                    Ok(response.as_array().unwrap().clone())
                } else {
                    Ok(vec![response])
                }
            }
            _ => Err(ArbitrageError::validation_error(format!("Unsupported exchange: {}", exchange_id))),
        }
    }

    async fn get_balance(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
    ) -> ArbitrageResult<Value> {
        // Placeholder implementation
        Ok(json!({}))
    }

    async fn create_order(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: &str,
        _side: &str,
        _amount: f64,
        _price: Option<f64>,
    ) -> ArbitrageResult<Value> {
        // Placeholder implementation
        Ok(json!({}))
    }

    async fn cancel_order(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _order_id: &str,
        _symbol: &str,
    ) -> ArbitrageResult<Value> {
        // Placeholder implementation
        Ok(json!({}))
    }

    async fn get_open_orders(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>> {
        // Placeholder implementation
        Ok(vec![])
    }

    async fn get_open_positions(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: Option<&str>,
    ) -> ArbitrageResult<Vec<Value>> {
        // Placeholder implementation
        Ok(vec![])
    }

    async fn set_leverage(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: &str,
        _leverage: u32,
    ) -> ArbitrageResult<Value> {
        // Placeholder implementation
        Ok(json!({}))
    }

    async fn get_trading_fees(
        &self,
        _exchange_id: &str,
        _credentials: &ExchangeCredentials,
        _symbol: &str,
    ) -> ArbitrageResult<Value> {
        // Placeholder implementation
        Ok(json!({}))
    }
}
