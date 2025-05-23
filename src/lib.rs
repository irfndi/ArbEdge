use worker::*;

// Module declarations
mod services;
mod types;
mod utils;

use serde_json::json;
use services::exchange::{ExchangeInterface, ExchangeService};
use services::opportunity::{OpportunityService, OpportunityServiceConfig};
use services::positions::{CreatePositionData, PositionsService, UpdatePositionData};
use services::telegram::TelegramService;
use std::sync::Arc;
use types::{ExchangeIdEnum, StructuredTradingPair};
use utils::{ArbitrageError, ArbitrageResult};
use std::collections::HashMap;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();

    match (req.method(), path) {
        // Health check
        (Method::Get, "/health") => {
            Response::ok("ArbEdge Rust Worker is running!")
        }

        // KV test endpoint
        (Method::Get, "/kv-test") => {
            let value = url.query().unwrap_or("default");
            let kv = env.kv("ArbEdgeKV")?;
            kv.put("test-key", value)?.execute().await?;
            let retrieved = kv.get("test-key").text().await?;
            Response::ok(retrieved.unwrap_or_default())
        }

        // Exchange API endpoints
        (Method::Get, "/exchange/markets") => {
            handle_get_markets(req, env).await
        }

        (Method::Get, "/exchange/ticker") => {
            handle_get_ticker(req, env).await
        }

        (Method::Get, "/exchange/funding") => {
            handle_funding_rate(req, env).await
        }

        // Opportunity finding endpoint
        (Method::Post, "/find-opportunities") => {
            handle_find_opportunities(req, env).await
        }

        // Telegram webhook endpoint
        (Method::Post, "/webhook") => {
            handle_telegram_webhook(req, env).await
        }

        // Position management endpoints
        (Method::Post, "/positions") => {
            handle_create_position(req, env).await
        }

        (Method::Get, "/positions") => {
            handle_get_all_positions(req, env).await
        }

        (Method::Get, path) if path.starts_with("/positions/") => {
            let id = path.strip_prefix("/positions/").unwrap();
            handle_get_position(req, env, id).await
        }

        (Method::Put, path) if path.starts_with("/positions/") => {
            let id = path.strip_prefix("/positions/").unwrap();
            handle_update_position(req, env, id).await
        }

        (Method::Delete, path) if path.starts_with("/positions/") => {
            let id = path.strip_prefix("/positions/").unwrap();
            handle_close_position(req, env, id).await
        }

        // Default response
        _ => Response::ok("Hello, ArbEdge in Rust! Available endpoints: /health, /kv-test, /exchange/*, /find-opportunities, /webhook, /positions"),
    }
}

#[event(scheduled)]
pub async fn scheduled(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    console_error_panic_hook::set_once();

    match event.cron().as_str() {
        // Monitor opportunities every minute
        "* * * * *" => {
            if let Err(e) = monitor_opportunities_scheduled(env).await {
                console_log!("Error in scheduled opportunity monitoring: {}", e);
            }
        }
        _ => {
            console_log!("Unknown scheduled event: {}", event.cron());
        }
    }
}

// Handler implementations

async fn handle_get_markets(req: Request, env: Env) -> Result<Response> {
    let custom_env = types::Env::new(env);
    let exchange_service = match ExchangeService::new(&custom_env) {
        Ok(service) => service,
        Err(e) => return Response::error(format!("Failed to create exchange service: {}", e), 500),
    };

    let url = req.url()?;
    let exchange_id = url
        .query_pairs()
        .find(|(key, _)| key == "exchange")
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| "binance".to_string());

    match exchange_service.get_markets(&exchange_id).await {
        Ok(markets) => {
            let market_count = markets.len();
            let sample_markets: Vec<_> = markets.into_iter().take(5).collect();
            let response = json!({
                "exchange": exchange_id,
                "total_markets": market_count,
                "sample_markets": sample_markets
            });
            Response::from_json(&response)
        }
        Err(e) => Response::error(format!("Failed to get markets: {}", e), 500),
    }
}

async fn handle_get_ticker(req: Request, env: Env) -> Result<Response> {
    let custom_env = types::Env::new(env);
    let exchange_service = match ExchangeService::new(&custom_env) {
        Ok(service) => service,
        Err(e) => return Response::error(format!("Failed to create exchange service: {}", e), 500),
    };

    let url = req.url()?;
    let query_pairs: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let exchange_id = query_pairs
        .get("exchange")
        .cloned()
        .unwrap_or_else(|| "binance".to_string());
    let symbol = query_pairs
        .get("symbol")
        .cloned()
        .unwrap_or_else(|| "BTCUSDT".to_string());

    match exchange_service.get_ticker(&exchange_id, &symbol).await {
        Ok(ticker) => Response::from_json(&ticker),
        Err(e) => Response::error(format!("Failed to get ticker: {}", e), 500),
    }
}

async fn handle_funding_rate(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let query_params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    
    let exchange_id = query_params.get("exchange").unwrap_or(&"binance".to_string()).clone();
    let symbol = query_params.get("symbol").unwrap_or(&"BTCUSDT".to_string()).clone();
    
    let custom_env = types::Env::new(env);
    let exchange_service = match ExchangeService::new(&custom_env) {
        Ok(service) => service,
        Err(e) => return Response::error(format!("Failed to create exchange service: {}", e), 500),
    };
    
    match exchange_service
        .fetch_funding_rates(&exchange_id, Some(&symbol))
        .await
    {
        Ok(rates) => Response::from_json(&rates),
        Err(e) => Response::error(format!("Failed to fetch funding rate: {}", e), 500),
    }
}

async fn handle_find_opportunities(mut req: Request, env: Env) -> Result<Response> {
    // Create custom env first 
    let custom_env = types::Env::new(env);
    
    // Parse configuration from environment
    let exchanges_str = custom_env.worker_env.var("EXCHANGES")?.to_string();
    let exchanges: Vec<ExchangeIdEnum> = exchanges_str
        .split(',')
        .filter_map(|s| match s.trim() {
            "binance" => Some(ExchangeIdEnum::Binance),
            "bybit" => Some(ExchangeIdEnum::Bybit),
            "okx" => Some(ExchangeIdEnum::OKX),
            "bitget" => Some(ExchangeIdEnum::Bitget),
            _ => None,
        })
        .collect();

    if exchanges.len() < 2 {
        return Response::error("At least two exchanges must be configured", 400);
    }

    // Parse request body for trading pairs
    let request_data: serde_json::Value = match req.json().await {
        Ok(data) => data,
        Err(_) => {
            // Default trading pairs if no body provided
            json!({
                "trading_pairs": ["BTCUSDT", "ETHUSDT", "ADAUSDT", "DOTUSDT", "SOLUSDT"],
                "min_threshold": 0.01
            })
        }
    };

    let _trading_pairs: Vec<String> = request_data["trading_pairs"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    let _min_threshold = request_data["min_threshold"]
        .as_f64()
        .unwrap_or(0.01);

    let monitored_pairs_str = custom_env.worker_env.var("MONITORED_PAIRS_CONFIG")?.to_string();
    let monitored_pairs: Vec<StructuredTradingPair> =
        match serde_json::from_str(&monitored_pairs_str) {
            Ok(pairs) => pairs,
            Err(e) => {
                return Response::error(format!("Failed to parse monitored pairs: {}", e), 400)
            }
        };

    let threshold: f64 = custom_env.worker_env
        .var("ARBITRAGE_THRESHOLD")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "0.001".to_string())
        .parse()
        .unwrap_or(0.001);

    // Create services
    let exchange_service = Arc::new(match ExchangeService::new(&custom_env) {
        Ok(service) => service,
        Err(e) => return Response::error(format!("Failed to create exchange service: {}", e), 500),
    });

    let telegram_service = if let (Ok(bot_token), Ok(chat_id)) =
        (custom_env.worker_env.var("TELEGRAM_BOT_TOKEN"), custom_env.worker_env.var("TELEGRAM_CHAT_ID"))
    {
        Some(Arc::new(TelegramService::new(
            services::telegram::TelegramConfig {
                bot_token: bot_token.to_string(),
                chat_id: chat_id.to_string(),
            },
        )))
    } else {
        None
    };

    let opportunity_config = OpportunityServiceConfig {
        exchanges,
        monitored_pairs,
        threshold,
    };

    let opportunity_service =
        OpportunityService::new(opportunity_config, exchange_service, telegram_service);

    // Find opportunities
    match opportunity_service.monitor_opportunities().await {
        Ok(opportunities) => {
            // Process opportunities (send notifications)
            if let Err(e) = opportunity_service
                .process_opportunities(&opportunities)
                .await
            {
                console_log!("Failed to process opportunities: {}", e);
            }

            let response = json!({
                "status": "success",
                "opportunities_found": opportunities.len(),
                "opportunities": opportunities
            });
            Response::from_json(&response)
        }
        Err(e) => Response::error(format!("Failed to find opportunities: {}", e), 500),
    }
}

async fn handle_telegram_webhook(mut req: Request, env: Env) -> Result<Response> {
    let update: serde_json::Value = req.json().await?;

    let telegram_service = if let (Ok(bot_token), Ok(chat_id)) =
        (env.var("TELEGRAM_BOT_TOKEN"), env.var("TELEGRAM_CHAT_ID"))
    {
        TelegramService::new(services::telegram::TelegramConfig {
            bot_token: bot_token.to_string(),
            chat_id: chat_id.to_string(),
        })
    } else {
        return Response::error("Telegram configuration not found", 500);
    };

    match telegram_service.handle_webhook(update).await {
        Ok(Some(response_text)) => Response::ok(response_text),
        Ok(None) => Response::ok("OK"),
        Err(e) => Response::error(format!("Webhook processing error: {}", e), 500),
    }
}

async fn handle_create_position(mut req: Request, env: Env) -> Result<Response> {
    let kv = env.kv("ArbEdgeKV")?;
    let positions_service = PositionsService::new(kv);

    let position_data: CreatePositionData = req.json().await?;

    match positions_service.create_position(position_data).await {
        Ok(position) => Response::from_json(&position),
        Err(e) => Response::error(format!("Failed to create position: {}", e), 500),
    }
}

async fn handle_get_all_positions(_req: Request, env: Env) -> Result<Response> {
    let kv = env.kv("ArbEdgeKV")?;
    let positions_service = PositionsService::new(kv);

    match positions_service.get_all_positions().await {
        Ok(positions) => Response::from_json(&positions),
        Err(e) => Response::error(format!("Failed to get positions: {}", e), 500),
    }
}

async fn handle_get_position(_req: Request, env: Env, id: &str) -> Result<Response> {
    let kv = env.kv("ArbEdgeKV")?;
    let positions_service = PositionsService::new(kv);

    match positions_service.get_position(id).await {
        Ok(Some(position)) => Response::from_json(&position),
        Ok(None) => Response::error("Position not found", 404),
        Err(e) => Response::error(format!("Failed to get position: {}", e), 500),
    }
}

async fn handle_update_position(mut req: Request, env: Env, id: &str) -> Result<Response> {
    let kv = env.kv("ArbEdgeKV")?;
    let positions_service = PositionsService::new(kv);

    let update_data: UpdatePositionData = req.json().await?;

    match positions_service.update_position(id, update_data).await {
        Ok(Some(position)) => Response::from_json(&position),
        Ok(None) => Response::error("Position not found", 404),
        Err(e) => Response::error(format!("Failed to update position: {}", e), 500),
    }
}

async fn handle_close_position(_req: Request, env: Env, id: &str) -> Result<Response> {
    let kv = env.kv("ArbEdgeKV")?;
    let positions_service = PositionsService::new(kv);

    match positions_service.close_position(id).await {
        Ok(true) => Response::ok("Position closed"),
        Ok(false) => Response::error("Position not found", 404),
        Err(e) => Response::error(format!("Failed to close position: {}", e), 500),
    }
}

async fn monitor_opportunities_scheduled(env: Env) -> ArbitrageResult<()> {
    let custom_env = types::Env::new(env);
    let exchange_service = Arc::new(ExchangeService::new(&custom_env)?);

    // Parse configuration from environment
    let exchanges_str = custom_env.worker_env.var("EXCHANGES")?.to_string();
    let exchanges: Vec<ExchangeIdEnum> = exchanges_str
        .split(',')
        .filter_map(|s| match s.trim() {
            "binance" => Some(ExchangeIdEnum::Binance),
            "bybit" => Some(ExchangeIdEnum::Bybit),
            "okx" => Some(ExchangeIdEnum::OKX),
            "bitget" => Some(ExchangeIdEnum::Bitget),
            _ => None,
        })
        .collect();

    if exchanges.len() < 2 {
        return Err(ArbitrageError::config_error(
            "At least two exchanges must be configured",
        ));
    }

    let monitored_pairs_str = custom_env.worker_env.var("MONITORED_PAIRS_CONFIG")?.to_string();
    let monitored_pairs: Vec<StructuredTradingPair> =
        serde_json::from_str(&monitored_pairs_str.to_string()).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse monitored pairs: {}", e))
        })?;

    let threshold: f64 = custom_env.worker_env
        .var("ARBITRAGE_THRESHOLD")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "0.001".to_string())
        .parse()
        .unwrap_or(0.001);

    // Create services
    let telegram_service = if let (Ok(bot_token), Ok(chat_id)) =
        (custom_env.worker_env.var("TELEGRAM_BOT_TOKEN"), custom_env.worker_env.var("TELEGRAM_CHAT_ID"))
    {
        Some(Arc::new(TelegramService::new(
            services::telegram::TelegramConfig {
                bot_token: bot_token.to_string(),
                chat_id: chat_id.to_string(),
            },
        )))
    } else {
        None
    };

    let opportunity_config = OpportunityServiceConfig {
        exchanges,
        monitored_pairs,
        threshold,
    };

    let opportunity_service =
        OpportunityService::new(opportunity_config, exchange_service, telegram_service);

    // Find and process opportunities
    let opportunities = opportunity_service.monitor_opportunities().await?;

    if !opportunities.is_empty() {
        console_log!("Found {} opportunities", opportunities.len());
        opportunity_service
            .process_opportunities(&opportunities)
            .await?;
    }

    Ok(())
}
