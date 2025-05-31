use crate::services::core::trading::exchange::ExchangeInterface;
use crate::services::core::trading::exchange::ExchangeService;
use crate::services::core::trading::positions::{
    CreatePositionData, /* ProductionPositionsService, */ UpdatePositionData,
};
use crate::types::ExchangeIdEnum;
use crate::utils::ArbitrageResult;
use worker::{console_log, Env, Request, Response, Result};

/// Legacy market data handler
pub async fn handle_get_markets(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let exchange = url
        .query_pairs()
        .find(|(key, _)| key == "exchange")
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| "binance".to_string());

    let exchange_enum = match exchange.to_lowercase().as_str() {
        "binance" => ExchangeIdEnum::Binance,
        "bybit" => ExchangeIdEnum::Bybit,
        "okx" => ExchangeIdEnum::OKX,
        "bitget" => ExchangeIdEnum::Bitget,
        _ => return Response::error("Unsupported exchange", 400),
    };

    let exchange_service = ExchangeService::new(&env)?;
    match exchange_service
        .get_markets(&exchange_enum.to_string())
        .await
    {
        Ok(markets) => Response::from_json(&markets),
        Err(e) => Response::error(format!("Failed to get markets: {:?}", e), 500),
    }
}

/// Legacy ticker handler
pub async fn handle_get_ticker(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let query_pairs: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let exchange = query_pairs
        .get("exchange")
        .cloned()
        .unwrap_or_else(|| "binance".to_string());
    let symbol = query_pairs
        .get("symbol")
        .cloned()
        .unwrap_or_else(|| "BTCUSDT".to_string());

    let exchange_enum = match exchange.to_lowercase().as_str() {
        "binance" => ExchangeIdEnum::Binance,
        "bybit" => ExchangeIdEnum::Bybit,
        "okx" => ExchangeIdEnum::OKX,
        "bitget" => ExchangeIdEnum::Bitget,
        _ => return Response::error("Unsupported exchange", 400),
    };

    let exchange_service = ExchangeService::new(&env)?;
    match exchange_service
        .get_ticker(&exchange_enum.to_string(), &symbol)
        .await
    {
        Ok(ticker) => Response::from_json(&ticker),
        Err(e) => Response::error(format!("Failed to get ticker: {:?}", e), 500),
    }
}

/// Legacy funding rate handler
pub async fn handle_funding_rate(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let query_pairs: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let exchange = query_pairs
        .get("exchange")
        .cloned()
        .unwrap_or_else(|| "binance".to_string());
    let symbol = query_pairs
        .get("symbol")
        .cloned()
        .unwrap_or_else(|| "BTCUSDT".to_string());

    let exchange_enum = match exchange.to_lowercase().as_str() {
        "binance" => ExchangeIdEnum::Binance,
        "bybit" => ExchangeIdEnum::Bybit,
        "okx" => ExchangeIdEnum::OKX,
        "bitget" => ExchangeIdEnum::Bitget,
        _ => return Response::error("Unsupported exchange", 400),
    };

    let exchange_service = ExchangeService::new(&env)?;
    match exchange_service
        .fetch_funding_rates(&exchange_enum.to_string(), Some(&symbol))
        .await
    {
        Ok(funding_rates) => Response::from_json(&funding_rates),
        Err(e) => Response::error(format!("Failed to get funding rate: {:?}", e), 500),
    }
}

/// Legacy orderbook handler
pub async fn handle_get_orderbook(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let query_pairs: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let exchange = query_pairs
        .get("exchange")
        .cloned()
        .unwrap_or_else(|| "binance".to_string());
    let symbol = query_pairs
        .get("symbol")
        .cloned()
        .unwrap_or_else(|| "BTCUSDT".to_string());

    let exchange_enum = match exchange.to_lowercase().as_str() {
        "binance" => ExchangeIdEnum::Binance,
        "bybit" => ExchangeIdEnum::Bybit,
        "okx" => ExchangeIdEnum::OKX,
        "bitget" => ExchangeIdEnum::Bitget,
        _ => return Response::error("Unsupported exchange", 400),
    };

    let exchange_service = ExchangeService::new(&env)?;
    match exchange_service
        .get_orderbook(&exchange_enum.to_string(), &symbol, None)
        .await
    {
        Ok(orderbook) => Response::from_json(&orderbook),
        Err(e) => Response::error(format!("Failed to get orderbook: {:?}", e), 500),
    }
}

/// Legacy find opportunities handler
pub async fn handle_find_opportunities(mut req: Request, _env: Env) -> Result<Response> {
    let body: serde_json::Value = req.json().await?;
    let _pairs = body["pairs"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_else(|| vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]);

    // TODO: Replace with new modular opportunity engine
    Response::error(
        "Opportunity service temporarily disabled during modularization",
        503,
    )
}

/// Legacy telegram webhook handler - now using modular service
pub async fn handle_telegram_webhook(mut req: Request, env: Env) -> Result<Response> {
    let update: serde_json::Value = req.json().await?;
    console_log!("📱 Telegram webhook received: {}", update);

    // Use regular telegram service
    use crate::services::interfaces::telegram::TelegramService;

    match TelegramService::from_env(&env) {
        Ok(telegram_service) => match telegram_service.handle_webhook(update).await {
            Ok(result) => {
                console_log!("✅ Telegram webhook processed: {}", result);
                Response::ok("Webhook processed")
            }
            Err(e) => {
                console_log!("❌ Telegram webhook error: {:?}", e);
                Response::error(format!("Webhook processing failed: {:?}", e), 500)
            }
        },
        Err(e) => {
            console_log!("❌ Failed to create telegram service: {:?}", e);
            Response::error("Telegram service initialization failed", 500)
        }
    }
}

/// Legacy position handlers
pub async fn handle_create_position(mut req: Request, _env: Env) -> Result<Response> {
    let _position_data: CreatePositionData = req.json().await?;
    console_log!("📊 Position creation not implemented yet");
    Response::error("Position management not implemented", 501)
}

pub async fn handle_get_all_positions(_req: Request, _env: Env) -> Result<Response> {
    console_log!("📊 Position listing not implemented yet");
    Response::error("Position management not implemented", 501)
}

pub async fn handle_get_position(_req: Request, _env: Env, _id: &str) -> Result<Response> {
    console_log!("📊 Position retrieval not implemented yet");
    Response::error("Position management not implemented", 501)
}

pub async fn handle_update_position(mut req: Request, _env: Env, _id: &str) -> Result<Response> {
    let _update_data: UpdatePositionData = req.json().await?;
    console_log!("📊 Position update not implemented yet");
    Response::error("Position management not implemented", 501)
}

pub async fn handle_close_position(_req: Request, _env: Env, _id: &str) -> Result<Response> {
    console_log!("📊 Position closure not implemented yet");
    Response::error("Position management not implemented", 501)
}

/// Legacy scheduled monitoring
pub async fn monitor_opportunities_scheduled(_env: Env) -> ArbitrageResult<()> {
    console_log!("🔄 Starting scheduled opportunity monitoring...");

    // TODO: Use service container for scheduled tasks
    console_log!("⚠️ Scheduled monitoring temporarily disabled during modularization");

    console_log!("✅ Scheduled opportunity monitoring completed");
    Ok(())
}
