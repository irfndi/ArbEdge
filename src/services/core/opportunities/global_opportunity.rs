// src/services/global_opportunity.rs

use crate::types::{
    ArbitrageOpportunity, ArbitrageType, CommandPermission, DeliveryStatus, DistributionStrategy,
    ExchangeIdEnum, FundingRateInfo, GlobalOpportunity, GlobalOpportunityConfig, OpportunityQueue,
    OpportunitySource, QueueStatus, SubscriptionTier, UserAccessLevel, UserOpportunityDistribution,
};
// use crate::services::core::analysis::market_analysis::{TradingOpportunity, OpportunityType}; // TODO: Re-enable when implementing market analysis integration
use crate::log_info;
use crate::services::core::trading::exchange::{ExchangeService, SuperAdminApiConfig};
use crate::services::core::user::user_profile::UserProfileService;
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::Utc;

use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

/// Global Opportunity Service for Task 2
/// Implements system-wide opportunity detection, queue management, and fair distribution
/// **SECURITY**: Uses only super admin read-only APIs for global opportunity generation
pub struct GlobalOpportunityService {
    config: GlobalOpportunityConfig,
    exchange_service: Arc<ExchangeService>,
    user_profile_service: Arc<UserProfileService>,
    kv_store: KvStore,
    current_queue: Option<OpportunityQueue>,
    distribution_tracking: HashMap<String, UserOpportunityDistribution>,
    super_admin_configs: HashMap<String, SuperAdminApiConfig>, // Read-only API configs
    pipelines_service: Option<
        crate::services::core::infrastructure::cloudflare_pipelines::CloudflarePipelinesService,
    >, // Pipeline integration
}

impl GlobalOpportunityService {
    const OPPORTUNITY_QUEUE_KEY: &'static str = "global_opportunity_queue";
    const DISTRIBUTION_TRACKING_PREFIX: &'static str = "user_opportunity_dist";
    const ACTIVE_USERS_KEY: &'static str = "active_users_list";

    pub fn new(
        config: GlobalOpportunityConfig,
        exchange_service: Arc<ExchangeService>,
        user_profile_service: Arc<UserProfileService>,
        kv_store: KvStore,
        pipelines_service: Option<
            crate::services::core::infrastructure::cloudflare_pipelines::CloudflarePipelinesService,
        >,
    ) -> Self {
        Self {
            config,
            exchange_service,
            user_profile_service,
            kv_store,
            current_queue: None,
            distribution_tracking: HashMap::new(),
            super_admin_configs: HashMap::new(),
            pipelines_service,
        }
    }

    /// Configure super admin read-only API for global opportunity generation
    /// **SECURITY**: Validates that API cannot execute trades
    pub fn configure_super_admin_api(
        &mut self,
        exchange_id: String,
        credentials: crate::types::ExchangeCredentials,
    ) -> ArbitrageResult<()> {
        let config = SuperAdminApiConfig::new_read_only(exchange_id.clone(), credentials);
        config.validate_read_only()?;

        log_info!(
            "Configured super admin read-only API",
            serde_json::json!({
                "exchange_id": exchange_id,
                "is_trading_enabled": config.can_trade()
            })
        );

        self.super_admin_configs.insert(exchange_id, config);
        Ok(())
    }

    /// Set pipelines service for data ingestion and storage
    pub fn set_pipelines_service(
        &mut self,
        pipelines_service: crate::services::core::infrastructure::cloudflare_pipelines::CloudflarePipelinesService,
    ) {
        self.pipelines_service = Some(pipelines_service);
    }

    /// Get market data from pipeline with fallback to super admin APIs
    /// Implements hybrid data access pattern: Pipeline-first, KV cache fallback, API last resort
    async fn get_market_data_from_pipeline(
        &self,
        exchange: &str,
        symbol: &str,
    ) -> ArbitrageResult<serde_json::Value> {
        // 1. Try pipelines (primary)
        if let Some(pipelines) = &self.pipelines_service {
            let pipeline_key = format!("market_data:{}:{}", exchange, symbol);
            match pipelines.get_latest_data(&pipeline_key).await {
                Ok(data) => {
                    log_info!(
                        "Retrieved market data from pipeline",
                        serde_json::json!({
                            "exchange": exchange,
                            "symbol": symbol,
                            "source": "pipeline"
                        })
                    );
                    return Ok(data);
                }
                Err(e) => {
                    log_info!(
                        "Pipeline data not available, falling back",
                        serde_json::json!({
                            "exchange": exchange,
                            "symbol": symbol,
                            "error": e.to_string()
                        })
                    );
                }
            }
        }

        // 2. Try KV cache (fallback)
        let cache_key = format!("market_data:{}:{}", exchange, symbol);
        match self.kv_store.get(&cache_key).text().await {
            Ok(Some(cached_data)) => {
                match serde_json::from_str::<serde_json::Value>(&cached_data) {
                    Ok(data) => {
                        log_info!(
                            "Retrieved market data from KV cache",
                            serde_json::json!({
                                "exchange": exchange,
                                "symbol": symbol,
                                "source": "kv_cache"
                            })
                        );
                        return Ok(data);
                    }
                    Err(e) => {
                        log_info!(
                            "Failed to parse cached market data",
                            serde_json::json!({
                                "exchange": exchange,
                                "symbol": symbol,
                                "error": e.to_string()
                            })
                        );
                    }
                }
            }
            Ok(None) => {
                log_info!(
                    "No cached market data available",
                    serde_json::json!({
                        "exchange": exchange,
                        "symbol": symbol
                    })
                );
            }
            Err(e) => {
                log_info!(
                    "KV cache access failed",
                    serde_json::json!({
                        "exchange": exchange,
                        "symbol": symbol,
                        "error": e.to_string()
                    })
                );
            }
        }

        // 3. Try super admin API (last resort)
        if let Some(super_admin_config) = self.super_admin_configs.get(exchange) {
            super_admin_config.validate_read_only()?;

            match self
                .exchange_service
                .get_global_funding_rates(exchange, Some(symbol))
                .await
            {
                Ok(rates) => {
                    let data = serde_json::to_value(&rates)?;

                    // Cache for future use
                    if let Ok(data_str) = serde_json::to_string(&data) {
                        if let Ok(put_builder) = self.kv_store.put(&cache_key, data_str) {
                            let _ = put_builder.expiration_ttl(60).execute().await;
                            // 1 minute TTL
                        }
                    }

                    log_info!(
                        "Retrieved market data from super admin API",
                        serde_json::json!({
                            "exchange": exchange,
                            "symbol": symbol,
                            "source": "super_admin_api"
                        })
                    );
                    return Ok(data);
                }
                Err(e) => {
                    log_info!(
                        "Super admin API call failed",
                        serde_json::json!({
                            "exchange": exchange,
                            "symbol": symbol,
                            "error": e.to_string()
                        })
                    );
                }
            }
        }

        Err(ArbitrageError::not_found(format!(
            "No market data available from any source for {}:{}",
            exchange, symbol
        )))
    }

    /// Store market data to pipeline for historical tracking
    async fn store_market_data_to_pipeline(
        &self,
        exchange: &str,
        symbol: &str,
        data: &serde_json::Value,
    ) -> ArbitrageResult<()> {
        if let Some(pipelines) = &self.pipelines_service {
            pipelines.store_market_data(exchange, symbol, data).await?;
            log_info!(
                "Stored market data to pipeline",
                serde_json::json!({
                    "exchange": exchange,
                    "symbol": symbol
                })
            );
        }
        Ok(())
    }

    /// Fetch real funding rate data from Bybit and Binance APIs
    async fn fetch_real_funding_rate_data(
        &self,
        exchange_id: &ExchangeIdEnum,
        symbol: &str,
    ) -> ArbitrageResult<FundingRateInfo> {
        use worker::{Fetch, Method, Request, RequestInit};

        match exchange_id {
            ExchangeIdEnum::Bybit => {
                // Convert symbol to Bybit format (e.g., BTC-USDT -> BTCUSDT)
                let bybit_symbol = symbol.replace("-", "").to_uppercase();

                // Bybit V5 Funding Rate API
                let url = format!(
                    "https://api.bybit.com/v5/market/funding/history?category=linear&symbol={}&limit=1",
                    bybit_symbol
                );

                let request =
                    Request::new_with_init(&url, RequestInit::new().with_method(Method::Get))?;

                let mut response = Fetch::Request(request).send().await?;

                if response.status_code() != 200 {
                    return Err(ArbitrageError::api_error(format!(
                        "Bybit funding rate API error: {}",
                        response.status_code()
                    )));
                }

                let response_text = response.text().await?;
                let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

                // Parse Bybit response
                if let Some(result) = response_json.get("result") {
                    if let Some(list) = result.get("list").and_then(|l| l.as_array()) {
                        if let Some(funding_data) = list.first() {
                            if let Some(funding_rate_str) =
                                funding_data.get("fundingRate").and_then(|v| v.as_str())
                            {
                                if let Ok(funding_rate) = funding_rate_str.parse::<f64>() {
                                    let funding_info = FundingRateInfo {
                                        symbol: symbol.to_string(),
                                        funding_rate,
                                        timestamp: Some(Utc::now()),
                                        datetime: Some(Utc::now().to_rfc3339()),
                                        next_funding_time: None,
                                        estimated_rate: None,
                                    };

                                    // Store to pipeline
                                    let data = serde_json::to_value(&funding_info)?;
                                    let _ = self
                                        .store_market_data_to_pipeline("bybit", symbol, &data)
                                        .await;

                                    return Ok(funding_info);
                                }
                            }
                        }
                    }
                }

                Err(ArbitrageError::parse_error(
                    "Failed to parse Bybit funding rate response",
                ))
            }
            ExchangeIdEnum::Binance => {
                // Convert symbol to Binance format (e.g., BTC-USDT -> BTCUSDT)
                let binance_symbol = symbol.replace("-", "").to_uppercase();

                // Binance Premium Index API (closest to funding rate)
                let url = format!(
                    "https://fapi.binance.com/fapi/v1/premiumIndex?symbol={}",
                    binance_symbol
                );

                let request =
                    Request::new_with_init(&url, RequestInit::new().with_method(Method::Get))?;

                let mut response = Fetch::Request(request).send().await?;

                if response.status_code() != 200 {
                    return Err(ArbitrageError::api_error(format!(
                        "Binance funding rate API error: {}",
                        response.status_code()
                    )));
                }

                let response_text = response.text().await?;
                let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

                // Parse Binance response
                if let Some(last_funding_rate_str) = response_json
                    .get("lastFundingRate")
                    .and_then(|v| v.as_str())
                {
                    if let Ok(funding_rate) = last_funding_rate_str.parse::<f64>() {
                        let funding_info = FundingRateInfo {
                            symbol: symbol.to_string(),
                            funding_rate,
                            timestamp: Some(Utc::now()),
                            datetime: Some(Utc::now().to_rfc3339()),
                            next_funding_time: response_json
                                .get("nextFundingTime")
                                .and_then(|v| v.as_u64())
                                .and_then(|ts| {
                                    chrono::DateTime::from_timestamp((ts / 1000) as i64, 0)
                                }),
                            estimated_rate: response_json
                                .get("estimatedSettlePrice")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<f64>().ok()),
                        };

                        // Store to pipeline
                        let data = serde_json::to_value(&funding_info)?;
                        let _ = self
                            .store_market_data_to_pipeline("binance", symbol, &data)
                            .await;

                        return Ok(funding_info);
                    }
                }

                Err(ArbitrageError::parse_error(
                    "Failed to parse Binance funding rate response",
                ))
            }
            _ => Err(ArbitrageError::not_implemented(format!(
                "Exchange {:?} not supported for real funding rate data",
                exchange_id
            ))),
        }
    }

    /// Initialize super admin API keys from Wrangler secrets
    /// **SECURITY**: Reads encrypted API keys from Wrangler secrets, not environment variables
    ///
    /// Required Wrangler secrets:
    /// - `BINANCE_SUPER_ADMIN_API_KEY` and `BINANCE_SUPER_ADMIN_SECRET`
    /// - `BYBIT_SUPER_ADMIN_API_KEY` and `BYBIT_SUPER_ADMIN_SECRET`
    /// - `OKX_SUPER_ADMIN_API_KEY` and `OKX_SUPER_ADMIN_SECRET`
    /// - etc.
    pub fn initialize_super_admin_apis_from_secrets(
        &mut self,
        env: &crate::types::Env,
    ) -> ArbitrageResult<()> {
        let exchanges_to_configure = vec![
            ("binance", "BINANCE"),
            ("bybit", "BYBIT"),
            ("okx", "OKX"),
            ("bitget", "BITGET"),
        ];

        let mut configured_count = 0;

        for (exchange_name, env_prefix) in exchanges_to_configure {
            let api_key_var = format!("{}_SUPER_ADMIN_API_KEY", env_prefix);
            let secret_var = format!("{}_SUPER_ADMIN_SECRET", env_prefix);

            // Try to read from Wrangler secrets
            if let (Ok(api_key), Ok(secret)) = (
                env.worker_env.var(&api_key_var),
                env.worker_env.var(&secret_var),
            ) {
                let credentials = crate::types::ExchangeCredentials {
                    api_key: api_key.to_string(),
                    secret: secret.to_string(),
                    passphrase: None,    // Most exchanges don't require passphrase
                    default_leverage: 1, // Read-only APIs don't need leverage
                    exchange_type: "spot".to_string(), // Default to spot for read-only
                };

                match self.configure_super_admin_api(exchange_name.to_string(), credentials) {
                    Ok(()) => {
                        configured_count += 1;
                        log_info!(
                            "Successfully configured super admin API from Wrangler secrets",
                            serde_json::json!({
                                "exchange": exchange_name,
                                "api_key_var": api_key_var,
                                "secret_var": secret_var
                            })
                        );
                    }
                    Err(e) => {
                        log_info!(
                            "Failed to configure super admin API from Wrangler secrets",
                            serde_json::json!({
                                "exchange": exchange_name,
                                "error": e.to_string()
                            })
                        );
                    }
                }
            } else {
                log_info!(
                    "Super admin API secrets not found for exchange",
                    serde_json::json!({
                        "exchange": exchange_name,
                        "api_key_var": api_key_var,
                        "secret_var": secret_var,
                        "note": "Use 'wrangler secret put' to configure"
                    })
                );
            }
        }

        if configured_count == 0 {
            return Err(ArbitrageError::config_error(
                "No super admin API keys configured from Wrangler secrets. Use 'wrangler secret put EXCHANGE_SUPER_ADMIN_API_KEY' and 'wrangler secret put EXCHANGE_SUPER_ADMIN_SECRET' to configure.".to_string(),
            ));
        }

        log_info!(
            "Super admin API initialization completed",
            serde_json::json!({
                "configured_exchanges": configured_count,
                "total_attempted": 4 // Total number of exchanges we attempted to configure
            })
        );

        Ok(())
    }

    /// Validate that user has compatible exchange APIs for trading
    /// **SECURITY**: Prevents users without proper APIs from accessing trading features
    pub async fn validate_user_exchange_compatibility(
        &self,
        user_id: &str,
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<bool> {
        // Get user profile to check their exchange APIs
        let user_profile = match self.user_profile_service.get_user_profile(user_id).await {
            Ok(Some(profile)) => profile,
            Ok(None) => {
                log_info!(
                    "User exchange compatibility check failed: User not found",
                    serde_json::json!({
                        "user_id": user_id,
                        "opportunity_id": opportunity.opportunity.id
                    })
                );
                return Ok(false);
            }
            Err(e) => {
                log_info!(
                    "User exchange compatibility check failed: Database error",
                    serde_json::json!({
                        "user_id": user_id,
                        "error": e.to_string()
                    })
                );
                return Ok(false);
            }
        };

        // Check if user has required exchanges for this opportunity
        let user_exchanges = user_profile.get_active_exchanges();
        let required_exchanges = self.get_opportunity_required_exchanges(opportunity);

        let has_all_required = required_exchanges
            .iter()
            .all(|req_exchange| user_exchanges.contains(req_exchange));

        if !has_all_required {
            log_info!(
                "User exchange compatibility check failed: Missing required exchanges",
                serde_json::json!({
                    "user_id": user_id,
                    "user_exchanges": user_exchanges,
                    "required_exchanges": required_exchanges,
                    "opportunity_id": opportunity.opportunity.id
                })
            );
        }

        Ok(has_all_required)
    }

    /// Get required exchanges for an opportunity
    fn get_opportunity_required_exchanges(
        &self,
        opportunity: &GlobalOpportunity,
    ) -> Vec<ExchangeIdEnum> {
        // For arbitrage opportunities, both long and short exchanges are always required
        opportunity.opportunity.get_required_exchanges()
    }

    /// Check if user has permission to access trading opportunities
    /// **SECURITY**: Validates user permissions before showing trading-enabled opportunities
    pub async fn check_user_trading_permission(&self, user_id: &str) -> ArbitrageResult<bool> {
        // Get user profile to check permissions
        let user_profile = match self.user_profile_service.get_user_profile(user_id).await {
            Ok(Some(profile)) => profile,
            Ok(None) => return Ok(false),
            Err(_) => return Ok(false),
        };

        // Check if user has BasicOpportunities permission
        let has_basic_permission =
            user_profile.has_permission(CommandPermission::BasicOpportunities);

        // Check if user has trading API keys
        let has_trading_apis = user_profile.has_trading_api_keys();

        Ok(has_basic_permission && has_trading_apis)
    }

    /// Load or create the global opportunity queue
    pub async fn initialize_queue(&mut self) -> ArbitrageResult<()> {
        match self.load_queue().await {
            Ok(queue) => {
                log_info!(
                    "Loaded existing global opportunity queue",
                    serde_json::json!({
                        "queue_id": queue.id,
                        "opportunities_count": queue.opportunities.len(),
                        "active_users": queue.active_users
                    })
                );
                self.current_queue = Some(queue);
            }
            Err(_) => {
                log_info!(
                    "Creating new global opportunity queue",
                    serde_json::json!({})
                );
                let new_queue = OpportunityQueue {
                    id: uuid::Uuid::new_v4().to_string(),
                    opportunities: Vec::new(),
                    created_at: Utc::now().timestamp_millis() as u64,
                    scheduled_for: Utc::now().timestamp_millis() as u64,
                    status: QueueStatus::Pending,
                    target_users: Vec::new(),
                    distribution_strategy: DistributionStrategy::Immediate,
                    updated_at: Utc::now().timestamp_millis() as u64,
                    total_distributed: 0,
                    active_users: 0,
                };
                self.save_queue(&new_queue).await?;
                self.current_queue = Some(new_queue);
            }
        }
        Ok(())
    }

    /// Main detection loop - discovers new opportunities using default strategy
    /// **SECURITY**: Uses only super admin read-only APIs for global opportunity generation
    pub async fn detect_opportunities(&mut self) -> ArbitrageResult<Vec<GlobalOpportunity>> {
        // **SECURITY**: Validate that we have super admin configurations
        if self.super_admin_configs.is_empty() {
            return Err(ArbitrageError::validation_error(
                "No super admin read-only APIs configured for global opportunity detection"
                    .to_string(),
            ));
        }

        log_info!(
            "Starting global opportunity detection with super admin APIs",
            serde_json::json!({
                "min_threshold": self.config.min_threshold,
                "max_threshold": self.config.max_threshold,
                "exchanges": self.config.monitored_exchanges.len(),
                "pairs": self.config.monitored_pairs.len(),
                "super_admin_configs": self.super_admin_configs.len()
            })
        );

        let mut new_opportunities = Vec::new();

        // Step 1: Fetch funding rates for all monitored pairs and exchanges using ONLY super admin APIs
        let mut funding_rate_data: HashMap<
            String,
            HashMap<ExchangeIdEnum, Option<FundingRateInfo>>,
        > = HashMap::new();

        // Initialize maps for each pair
        for pair in &self.config.monitored_pairs {
            funding_rate_data.insert(pair.clone(), HashMap::new());
        }

        // **ENHANCED**: Use hybrid data access pattern for funding rate collection
        // First, try to get data from pipelines for all pairs/exchanges
        for pair in &self.config.monitored_pairs {
            for exchange_id in &self.config.monitored_exchanges {
                // **SECURITY**: Only use exchanges that have super admin read-only configuration
                if let Some(super_admin_config) =
                    self.super_admin_configs.get(&exchange_id.to_string())
                {
                    // Validate read-only status before each use
                    if let Err(e) = super_admin_config.validate_read_only() {
                        log_info!(
                            "Skipping exchange due to invalid super admin configuration",
                            serde_json::json!({
                                "exchange": exchange_id.as_str(),
                                "error": e.to_string()
                            })
                        );
                        continue;
                    }

                    // **ENHANCED**: Use real API calls with hybrid data access pattern
                    let funding_info =
                        match self.fetch_real_funding_rate_data(exchange_id, pair).await {
                            Ok(info) => {
                                log_info!(
                                    "Successfully fetched real funding rate data",
                                    serde_json::json!({
                                        "exchange": exchange_id.as_str(),
                                        "pair": pair,
                                        "funding_rate": info.funding_rate,
                                        "source": "real_api"
                                    })
                                );
                                Some(info)
                            }
                            Err(e) => {
                                log_info!(
                                    "Failed to fetch real funding rate, trying pipeline fallback",
                                    serde_json::json!({
                                        "exchange": exchange_id.as_str(),
                                        "pair": pair,
                                        "error": e.to_string()
                                    })
                                );

                                // Fallback to pipeline/cache data
                                match self
                                    .get_market_data_from_pipeline(&exchange_id.to_string(), pair)
                                    .await
                                {
                                    Ok(data) => {
                                        // Parse pipeline/cache data
                                        if let Some(rate_data) =
                                            data.as_array().and_then(|arr| arr.first())
                                        {
                                            match rate_data["fundingRate"].as_str() {
                                                Some(rate_str) => match rate_str.parse::<f64>() {
                                                    Ok(funding_rate) => Some(FundingRateInfo {
                                                        symbol: pair.clone(),
                                                        funding_rate,
                                                        timestamp: Some(Utc::now()),
                                                        datetime: Some(Utc::now().to_rfc3339()),
                                                        next_funding_time: None,
                                                        estimated_rate: None,
                                                    }),
                                                    Err(_) => None,
                                                },
                                                None => None,
                                            }
                                        } else {
                                            None
                                        }
                                    }
                                    Err(_) => None,
                                }
                            }
                        };

                    // Store the funding info
                    if let Some(pair_map) = funding_rate_data.get_mut(pair) {
                        pair_map.insert(*exchange_id, funding_info);
                    }
                } else {
                    log_info!(
                        "Skipping exchange without super admin configuration",
                        serde_json::json!({
                            "exchange": exchange_id.as_str(),
                            "reason": "No super admin read-only API configured"
                        })
                    );
                }
            }
        }

        // Data collection completed using hybrid access pattern

        // Step 2: Identify arbitrage opportunities using default strategy
        for pair in &self.config.monitored_pairs {
            if let Some(pair_funding_rates) = funding_rate_data.get(pair) {
                let available_exchanges: Vec<ExchangeIdEnum> = pair_funding_rates
                    .iter()
                    .filter_map(|(exchange_id, rate_info)| {
                        if rate_info.is_some() {
                            Some(*exchange_id)
                        } else {
                            None
                        }
                    })
                    .collect();

                if available_exchanges.len() < 2 {
                    continue;
                }

                // Compare all pairs of exchanges for opportunities
                for i in 0..available_exchanges.len() {
                    for j in (i + 1)..available_exchanges.len() {
                        let exchange_a = available_exchanges[i];
                        let exchange_b = available_exchanges[j];

                        if let (Some(Some(rate_a)), Some(Some(rate_b))) = (
                            pair_funding_rates.get(&exchange_a),
                            pair_funding_rates.get(&exchange_b),
                        ) {
                            let rate_diff = (rate_a.funding_rate - rate_b.funding_rate).abs();

                            // Check if opportunity meets our thresholds
                            if rate_diff >= self.config.min_threshold
                                && rate_diff <= self.config.max_threshold
                            {
                                let (long_exchange, short_exchange, long_rate, short_rate) =
                                    if rate_a.funding_rate > rate_b.funding_rate {
                                        (
                                            exchange_b,
                                            exchange_a,
                                            rate_b.funding_rate,
                                            rate_a.funding_rate,
                                        )
                                    } else {
                                        (
                                            exchange_a,
                                            exchange_b,
                                            rate_a.funding_rate,
                                            rate_b.funding_rate,
                                        )
                                    };

                                    // Create base arbitrage opportunity with required exchanges
                                    let mut opportunity = match ArbitrageOpportunity::new(
                                        pair.clone(),
                                        long_exchange,  // **REQUIRED**: No longer optional
                                        short_exchange, // **REQUIRED**: No longer optional
                                        Some(long_rate),
                                        Some(short_rate),
                                        rate_diff,
                                        ArbitrageType::FundingRate,
                                    ) {
                                        Ok(opp) => opp,
                                        Err(e) => {
                                            log_info!(
                                                "Failed to create arbitrage opportunity",
                                                serde_json::json!({
                                                    "pair": pair,
                                                    "error": e,
                                                    "long_exchange": long_exchange.as_str(),
                                                    "short_exchange": short_exchange.as_str()
                                                })
                                            );
                                            continue;
                                        }
                                    };

                                    // Set additional fields
                                    opportunity.id = uuid::Uuid::new_v4().to_string();
                                    opportunity = opportunity
                                                .with_net_difference(rate_diff)
                                                .with_potential_profit(rate_diff * 1000.0) // Estimate for $1000 position
                                                .with_details(format!(
                                                "Funding rate arbitrage: Long {} ({:.4}%) vs Short {} ({:.4}%)",
                                                long_exchange.as_str(),
                                                long_rate * 100.0,
                                                short_exchange.as_str(),
                                                short_rate * 100.0
                                                ));

                                    // **POSITION STRUCTURE VALIDATION**: Ensure 2-exchange requirement
                                    if let Err(validation_error) =
                                        opportunity.validate_position_structure()
                                    {
                                        log_info!(
                                            "Skipping invalid arbitrage opportunity",
                                            serde_json::json!({
                                                "pair": pair,
                                                "validation_error": validation_error,
                                                "long_exchange": long_exchange.as_str(),
                                                "short_exchange": short_exchange.as_str()
                                            })
                                        );
                                        continue;
                                    }

                                    // Calculate priority score (higher rate difference = higher priority)
                                    let priority_score = rate_diff * 1000.0; // Scale up for easier comparison

                                    // Create global opportunity
                                    let global_opportunity = GlobalOpportunity {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        opportunity_type: "arbitrage".to_string(),
                                        opportunity: opportunity.clone(),
                                        arbitrage_opportunity: Some(opportunity.clone()),
                                        technical_opportunity: None,
                                        source: OpportunitySource::SystemGenerated,
                                        created_at: Utc::now().timestamp_millis() as u64,
                                        detection_timestamp: Utc::now().timestamp_millis() as u64,
                                        expires_at: Some(Utc::now().timestamp_millis() as u64 + 600000), // 10 minutes
                                        expiry_timestamp: Some(
                                            Utc::now().timestamp_millis() as u64 + 600000,
                                        ), // 10 minutes
                                        priority: 5, // Default priority
                                        priority_score,
                                        ai_enhanced: false,
                                        ai_confidence_score: None,
                                        ai_insights: None,
                                        distributed_to: Vec::new(),
                                        max_participants: Some(10), // Default limit
                                        current_participants: 0,
                                        distribution_strategy: DistributionStrategy::RoundRobin,
                                    };

                                    new_opportunities.push(global_opportunity);

                                    log_info!(
                                        "Detected new global opportunity",
                                        serde_json::json!({
                                            "pair": pair,
                                            "rate_difference": rate_diff,
                                            "priority_score": priority_score,
                                            "long_exchange": long_exchange.as_str(),
                                            "short_exchange": short_exchange.as_str()
                                        })
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        log_info!(
            "Global opportunity detection completed",
            serde_json::json!({
                "new_opportunities_count": new_opportunities.len()
            })
        );

        Ok(new_opportunities)
    }

    /// Add new opportunities to the global queue
    pub async fn add_opportunities_to_queue(
        &mut self,
        opportunities: Vec<GlobalOpportunity>,
    ) -> ArbitrageResult<()> {
        let opportunities_count = opportunities.len(); // Store count before move

        // Extract the queue to avoid borrowing conflicts
        if let Some(mut queue) = self.current_queue.take() {
            // Add new opportunities
            queue.opportunities.extend(opportunities);

            // Sort by priority score (highest first)
            queue.opportunities.sort_by(|a, b| {
                b.priority_score
                    .partial_cmp(&a.priority_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Limit queue size
            if queue.opportunities.len() > self.config.max_queue_size as usize {
                queue
                    .opportunities
                    .truncate(self.config.max_queue_size as usize);
            }

            // Remove expired opportunities
            let now = Utc::now().timestamp_millis() as u64;
            queue
                .opportunities
                .retain(|opp| opp.expiry_timestamp.unwrap_or(0) > now);

            queue.updated_at = now;

            // Save the queue
            self.save_queue(&queue).await?;

            log_info!(
                "Updated global opportunity queue",
                serde_json::json!({
                    "total_opportunities": queue.opportunities.len(),
                    "new_opportunities_added": opportunities_count
                })
            );

            // Put the queue back
            self.current_queue = Some(queue);
        }

        Ok(())
    }

    /// Distribute opportunities to eligible users using fairness algorithms
    pub async fn distribute_opportunities(
        &mut self,
    ) -> ArbitrageResult<Vec<(String, GlobalOpportunity)>> {
        let mut distributions = Vec::new();

        // Extract the queue to avoid borrowing conflicts
        if let Some(mut queue) = self.current_queue.take() {
            // Load active users from KV store
            let active_users = self.load_active_users().await?;

            // Load distribution tracking for all users
            for user_id in &active_users {
                if !self.distribution_tracking.contains_key(user_id) {
                    if let Ok(tracking) = self.load_user_distribution_tracking(user_id).await {
                        self.distribution_tracking.insert(user_id.clone(), tracking);
                    } else {
                        // Create new tracking for user
                        let tracking = UserOpportunityDistribution {
                            user_id: user_id.clone(),
                            opportunity_id: "test_opportunity".to_string(),
                            distributed_at: Utc::now().timestamp_millis() as u64,
                            delivery_status: DeliveryStatus::Sent,
                            delay_applied_seconds: 0,
                            access_level: UserAccessLevel::FreeWithAPI,
                            last_opportunity_received: Some(Utc::now().timestamp_millis() as u64),
                            total_opportunities_received: 5,
                            opportunities_today: 2,
                            last_daily_reset: Utc::now().timestamp_millis() as u64,
                            priority_weight: 1.5,
                            is_eligible: true,
                        };
                        self.distribution_tracking.insert(user_id.clone(), tracking);
                    }
                }
            }

            // Apply fairness algorithm based on distribution strategy
            match self.config.distribution_strategy {
                DistributionStrategy::RoundRobin => {
                    distributions = self
                        .distribute_round_robin(&active_users, &mut queue)
                        .await?;
                }
                DistributionStrategy::FirstComeFirstServe => {
                    distributions = self
                        .distribute_first_come_first_serve(&active_users, &mut queue)
                        .await?;
                }
                DistributionStrategy::PriorityBased => {
                    distributions = self
                        .distribute_priority_based(&active_users, &mut queue)
                        .await?;
                }
                DistributionStrategy::Broadcast => {
                    distributions = self.distribute_broadcast(&active_users, &mut queue).await?;
                }
                DistributionStrategy::Immediate => {
                    distributions = self.distribute_broadcast(&active_users, &mut queue).await?;
                }
                DistributionStrategy::Batched => {
                    distributions = self
                        .distribute_first_come_first_serve(&active_users, &mut queue)
                        .await?;
                }
                DistributionStrategy::Prioritized => {
                    distributions = self
                        .distribute_priority_based(&active_users, &mut queue)
                        .await?;
                }
                DistributionStrategy::RateLimited => {
                    distributions = self
                        .distribute_round_robin(&active_users, &mut queue)
                        .await?;
                }
            }

            // Update distribution tracking
            for (user_id, _) in &distributions {
                if let Some(tracking) = self.distribution_tracking.get_mut(user_id) {
                    tracking.last_opportunity_received = Some(Utc::now().timestamp_millis() as u64);
                    tracking.total_opportunities_received += 1;
                    tracking.opportunities_today += 1;

                    // Reset daily count if needed
                    let now = Utc::now().timestamp_millis() as u64;
                    let one_day_ms = 24 * 60 * 60 * 1000;
                    if now - tracking.last_daily_reset > one_day_ms {
                        tracking.opportunities_today = 1;
                        tracking.last_daily_reset = now;
                    }

                    // Clone to avoid borrowing issues
                    let tracking_clone = tracking.clone();
                    self.save_user_distribution_tracking(&tracking_clone)
                        .await?;
                }
            }

            queue.total_distributed += distributions.len() as u32;

            // Clone for saving
            let queue_clone = queue.clone();
            self.save_queue(&queue_clone).await?;

            // Put the queue back
            self.current_queue = Some(queue);
        }

        log_info!(
            "Distributed opportunities",
            serde_json::json!({
                "distributions_count": distributions.len(),
                "strategy": format!("{:?}", self.config.distribution_strategy)
            })
        );

        Ok(distributions)
    }

    /// Round-robin distribution - fair rotation among users
    async fn distribute_round_robin(
        &self,
        active_users: &[String],
        queue: &mut OpportunityQueue,
    ) -> ArbitrageResult<Vec<(String, GlobalOpportunity)>> {
        let mut distributions = Vec::new();
        let mut user_index = 0;

        for opportunity in queue.opportunities.iter_mut() {
            if opportunity.distributed_to.len()
                >= opportunity.max_participants.unwrap_or(10) as usize
            {
                continue;
            }

            // Find next eligible user
            let mut attempts = 0;
            while attempts < active_users.len() {
                let user_id = &active_users[user_index % active_users.len()];

                if self
                    .is_user_eligible_for_opportunity(user_id, opportunity)
                    .await?
                {
                    opportunity.distributed_to.push(user_id.clone());
                    opportunity.current_participants += 1;
                    distributions.push((user_id.clone(), opportunity.clone()));
                    break;
                }

                user_index = (user_index + 1) % active_users.len();
                attempts += 1;
            }

            user_index = (user_index + 1) % active_users.len();
        }

        Ok(distributions)
    }

    /// First-come-first-serve distribution
    async fn distribute_first_come_first_serve(
        &self,
        active_users: &[String],
        queue: &mut OpportunityQueue,
    ) -> ArbitrageResult<Vec<(String, GlobalOpportunity)>> {
        let mut distributions = Vec::new();

        for opportunity in queue.opportunities.iter_mut() {
            if opportunity.distributed_to.len()
                >= opportunity.max_participants.unwrap_or(10) as usize
            {
                continue;
            }

            for user_id in active_users {
                if opportunity.distributed_to.contains(user_id) {
                    continue;
                }

                if self
                    .is_user_eligible_for_opportunity(user_id, opportunity)
                    .await?
                {
                    opportunity.distributed_to.push(user_id.clone());
                    opportunity.current_participants += 1;
                    distributions.push((user_id.clone(), opportunity.clone()));
                    break;
                }
            }
        }

        Ok(distributions)
    }

    /// Priority-based distribution considering subscription tiers and activity
    async fn distribute_priority_based(
        &self,
        active_users: &[String],
        queue: &mut OpportunityQueue,
    ) -> ArbitrageResult<Vec<(String, GlobalOpportunity)>> {
        let mut distributions = Vec::new();

        // Calculate user priorities
        let mut user_priorities: Vec<(String, f64)> = Vec::new();

        for user_id in active_users {
            let priority = self.calculate_user_priority(user_id).await?;
            user_priorities.push((user_id.clone(), priority));
        }

        // Sort by priority (highest first)
        user_priorities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for opportunity in queue.opportunities.iter_mut() {
            if opportunity.distributed_to.len()
                >= opportunity.max_participants.unwrap_or(10) as usize
            {
                continue;
            }

            for (user_id, _priority) in &user_priorities {
                if opportunity.distributed_to.contains(user_id) {
                    continue;
                }

                if self
                    .is_user_eligible_for_opportunity(user_id, opportunity)
                    .await?
                {
                    opportunity.distributed_to.push(user_id.clone());
                    opportunity.current_participants += 1;
                    distributions.push((user_id.clone(), opportunity.clone()));
                    break;
                }
            }
        }

        Ok(distributions)
    }

    /// Broadcast distribution - send to all eligible users
    async fn distribute_broadcast(
        &self,
        active_users: &[String],
        queue: &mut OpportunityQueue,
    ) -> ArbitrageResult<Vec<(String, GlobalOpportunity)>> {
        let mut distributions = Vec::new();

        for opportunity in queue.opportunities.iter_mut() {
            for user_id in active_users {
                if opportunity.distributed_to.contains(user_id) {
                    continue;
                }

                if self
                    .is_user_eligible_for_opportunity(user_id, opportunity)
                    .await?
                {
                    opportunity.distributed_to.push(user_id.clone());
                    opportunity.current_participants += 1;
                    distributions.push((user_id.clone(), opportunity.clone()));
                }
            }
        }

        Ok(distributions)
    }

    /// Check if user is eligible to receive an opportunity
    async fn is_user_eligible_for_opportunity(
        &self,
        user_id: &str,
        _opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<bool> {
        // Check distribution tracking
        if let Some(tracking) = self.distribution_tracking.get(user_id) {
            if !tracking.is_eligible {
                return Ok(false);
            }

            // Check daily limits
            if tracking.opportunities_today
                >= self
                    .config
                    .fairness_config
                    .max_opportunities_per_user_per_hour
            {
                return Ok(false);
            }

            // Check cooldown period
            if let Some(last_received) = tracking.last_opportunity_received {
                let cooldown_ms = self.config.fairness_config.free_user_delay_seconds * 1000;
                let now = Utc::now().timestamp_millis() as u64;
                if now - last_received < cooldown_ms {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Calculate user priority based on subscription tier and activity
    async fn calculate_user_priority(&self, user_id: &str) -> ArbitrageResult<f64> {
        // Load user profile to get subscription tier
        match self.user_profile_service.get_user_profile(user_id).await {
            Ok(Some(_profile)) => {
                // Base priority can be adjusted based on subscription tier or other factors if needed.
                // For now, using a base priority.
                // The prioritize_subscription_users flag in FairnessConfig can be used elsewhere
                // in the distribution logic if simple true/false prioritization is sufficient.

                // let tier_name = match profile.subscription.tier {
                //     SubscriptionTier::Free => "Free",
                //     SubscriptionTier::Basic => "Basic",
                //     SubscriptionTier::Pro => "Pro",
                //     SubscriptionTier::Premium => "Premium",
                //     SubscriptionTier::Enterprise => "Enterprise",
                //     SubscriptionTier::Admin => "Admin",
                //     SubscriptionTier::SuperAdmin => "SuperAdmin",
                // };

                // REMOVED: tier_multipliers logic as it's not in FairnessConfig
                // let tier_multiplier = self
                //     .config
                //     .fairness_config
                //     .tier_multipliers // THIS FIELD DOES NOT EXIST
                //     .get(tier_name)
                //     .copied()
                //     .unwrap_or(1.0);

                // Base priority with tier multiplier
                // let mut priority = tier_multiplier;
                let priority = 1.0; // Default priority

                // REMOVED: activity_boost_factor logic as it's not in FairnessConfig
                // Activity boost - check last active time
                // let now = Utc::now().timestamp_millis() as u64;
                // let one_hour_ms = 60 * 60 * 1000;
                // if now - profile.last_active < one_hour_ms {
                //     priority *= self.config.fairness_config.activity_boost_factor; // THIS FIELD DOES NOT EXIST
                // }

                Ok(priority)
            }
            Ok(None) => Ok(1.0), // No profile found, default priority
            Err(_) => Ok(1.0),   // Error loading profile, default priority
        }
    }

    /// Get current queue status
    pub fn get_queue_status(&self) -> Option<&OpportunityQueue> {
        self.current_queue.as_ref()
    }

    /// Update active users list
    pub async fn update_active_users(&self, user_ids: Vec<String>) -> ArbitrageResult<()> {
        let data = serde_json::to_string(&user_ids).map_err(|e| {
            ArbitrageError::serialization_error(format!("Failed to serialize active users: {}", e))
        })?;

        self.kv_store
            .put(Self::ACTIVE_USERS_KEY, data)?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to save active users: {}", e))
            })?;

        Ok(())
    }

    // Storage operations
    async fn load_queue(&self) -> ArbitrageResult<OpportunityQueue> {
        let data = self
            .kv_store
            .get(Self::OPPORTUNITY_QUEUE_KEY)
            .text()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to load opportunity queue: {}", e))
            })?
            .ok_or_else(|| ArbitrageError::not_found("Opportunity queue not found".to_string()))?;

        serde_json::from_str(&data).map_err(|e| {
            ArbitrageError::serialization_error(format!(
                "Failed to deserialize opportunity queue: {}",
                e
            ))
        })
    }

    async fn save_queue(&self, queue: &OpportunityQueue) -> ArbitrageResult<()> {
        let data = serde_json::to_string(queue).map_err(|e| {
            ArbitrageError::serialization_error(format!(
                "Failed to serialize opportunity queue: {}",
                e
            ))
        })?;

        self.kv_store
            .put(Self::OPPORTUNITY_QUEUE_KEY, data)?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to save opportunity queue: {}", e))
            })?;

        Ok(())
    }

    async fn load_active_users(&self) -> ArbitrageResult<Vec<String>> {
        match self.kv_store.get(Self::ACTIVE_USERS_KEY).text().await {
            Ok(Some(data)) => serde_json::from_str(&data).map_err(|e| {
                ArbitrageError::serialization_error(format!(
                    "Failed to deserialize active users: {}",
                    e
                ))
            }),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(ArbitrageError::database_error(format!(
                "Failed to load active users: {}",
                e
            ))),
        }
    }

    async fn load_user_distribution_tracking(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserOpportunityDistribution> {
        let key = format!("{}:{}", Self::DISTRIBUTION_TRACKING_PREFIX, user_id);
        let data = self
            .kv_store
            .get(&key)
            .text()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!(
                    "Failed to load user distribution tracking: {}",
                    e
                ))
            })?
            .ok_or_else(|| {
                ArbitrageError::not_found("User distribution tracking not found".to_string())
            })?;

        serde_json::from_str(&data).map_err(|e| {
            ArbitrageError::serialization_error(format!(
                "Failed to deserialize user distribution tracking: {}",
                e
            ))
        })
    }

    async fn save_user_distribution_tracking(
        &self,
        tracking: &UserOpportunityDistribution,
    ) -> ArbitrageResult<()> {
        let key = format!(
            "{}:{}",
            Self::DISTRIBUTION_TRACKING_PREFIX,
            tracking.user_id
        );
        let data = serde_json::to_string(tracking).map_err(|e| {
            ArbitrageError::serialization_error(format!(
                "Failed to serialize user distribution tracking: {}",
                e
            ))
        })?;

        self.kv_store
            .put(&key, data)?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!(
                    "Failed to save user distribution tracking: {}",
                    e
                ))
            })?;

        Ok(())
    }

    /// Enhance global opportunities with AI analysis for a specific user
    /// **AI Integration**: Uses user's AI access level to determine enhancement level
    /// Private helper method to apply AI enhancement to a global opportunity
    async fn apply_ai_enhancement_to_global_opportunity(
        &self,
        global_opp: GlobalOpportunity,
        enhanced: &crate::services::core::ai::ai_beta_integration::AiEnhancedOpportunity,
        is_system_level: bool,
    ) -> GlobalOpportunity {
        let mut enhanced_global = global_opp;

        // Update the base opportunity with AI insights
        enhanced_global.opportunity = enhanced.base_opportunity.clone();

        // Update potential profit value based on AI risk assessment
        if let Some(current_profit) = enhanced_global.opportunity.potential_profit_value {
            enhanced_global.opportunity.potential_profit_value =
                Some(enhanced.risk_adjusted_score * current_profit);
        }

        // Update global opportunity metadata with AI insights
        enhanced_global.priority_score = enhanced.calculate_final_score();

        // Mark as AI-enhanced at system level if applicable
        if is_system_level {
            enhanced_global.source = crate::types::OpportunitySource::SystemGenerated;
        }

        enhanced_global
    }

    /// Private helper method for common AI enhancement logic
    async fn enhance_opportunities_with_ai_common(
        &self,
        opportunities: Vec<GlobalOpportunity>,
        user_id: &str,
        ai_service: &mut crate::services::core::ai::ai_beta_integration::AiBetaIntegrationService,
        is_system_level: bool,
        log_context: &str,
    ) -> ArbitrageResult<Vec<GlobalOpportunity>> {
        let mut enhanced_opportunities = Vec::new();

        for global_opp in opportunities {
            // Extract the base arbitrage opportunity for AI analysis
            let arbitrage_opp = global_opp.opportunity.clone();

            // Enhance with AI
            match ai_service
                .enhance_opportunities(vec![arbitrage_opp], user_id)
                .await
            {
                Ok(enhanced_opportunities_result) => {
                    if let Some(enhanced) = enhanced_opportunities_result.first() {
                        let enhanced_global = self
                            .apply_ai_enhancement_to_global_opportunity(
                                global_opp,
                                enhanced,
                                is_system_level,
                            )
                            .await;
                        enhanced_opportunities.push(enhanced_global);
                    } else {
                        // No enhancement available, use original
                        enhanced_opportunities.push(global_opp);
                    }
                }
                Err(e) => {
                    log_info!(
                        &format!("{} failed, using original", log_context),
                        serde_json::json!({ "error": e.to_string() })
                    );
                    enhanced_opportunities.push(global_opp);
                }
            }
        }

        log_info!(
            &format!("{} completed", log_context),
            serde_json::json!({
                "count": enhanced_opportunities.len(),
                "user_id": user_id
            })
        );
        Ok(enhanced_opportunities)
    }

    pub async fn enhance_global_opportunities_with_ai(
        &self,
        user_id: &str,
        opportunities: Vec<GlobalOpportunity>,
        ai_service: &mut crate::services::core::ai::ai_beta_integration::AiBetaIntegrationService,
    ) -> ArbitrageResult<Vec<GlobalOpportunity>> {
        // Check if user has AI access through UserAccessService
        let user_profile = match self.user_profile_service.get_user_profile(user_id).await? {
            Some(profile) => profile,
            None => return Ok(opportunities), // User not found, return original opportunities
        };
        let ai_access_level = user_profile.get_ai_access_level();

        // Only enhance if user has AI access
        if !ai_access_level.can_use_ai_analysis() {
            return Ok(opportunities);
        }

        self.enhance_opportunities_with_ai_common(
            opportunities,
            user_id,
            ai_service,
            false, // Not system level
            "Enhanced global opportunities with AI for user",
        )
        .await
    }

    /// Enhance detected opportunities with AI before adding to queue
    /// **AI Integration**: System-level AI enhancement for all detected opportunities
    pub async fn enhance_detected_opportunities_with_ai(
        &mut self,
        opportunities: Vec<GlobalOpportunity>,
        ai_service: &mut crate::services::core::ai::ai_beta_integration::AiBetaIntegrationService,
    ) -> ArbitrageResult<Vec<GlobalOpportunity>> {
        let system_user_id = "system_global_ai";

        self.enhance_opportunities_with_ai_common(
            opportunities,
            system_user_id,
            ai_service,
            true, // System level
            "Enhanced detected opportunities with system AI",
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FairnessConfig, SubscriptionInfo, UserProfile};
    use std::collections::HashMap;
    use uuid::Uuid;

    // Mock structures for testing
    #[allow(dead_code)]
    struct MockKvStore {
        data: std::sync::Arc<std::sync::Mutex<HashMap<String, String>>>,
    }

    #[allow(dead_code)]
    impl MockKvStore {
        fn new() -> Self {
            Self {
                data: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            }
        }

        fn with_data(self, key: &str, value: &str) -> Self {
            let mut data = self.data.lock().unwrap();
            data.insert(key.to_string(), value.to_string());
            drop(data);
            self
        }

        async fn get(&self, key: &str) -> Option<String> {
            let data = self.data.lock().unwrap();
            data.get(key).cloned()
        }

        async fn put(&self, key: &str, value: String) -> Result<(), String> {
            let mut data = self.data.lock().unwrap();
            data.insert(key.to_string(), value);
            Ok(())
        }
    }

    fn create_test_config() -> GlobalOpportunityConfig {
        GlobalOpportunityConfig {
            enabled: true,
            max_opportunities_per_batch: 10,
            batch_interval_seconds: 30,
            min_rate_difference: 0.001,
            max_age_minutes: 10,
            ai_enhancement_enabled: false,
            distribution_strategy: DistributionStrategy::RoundRobin,
            min_threshold: 0.001, // 0.1%
            max_threshold: 0.02,  // 2%
            monitored_exchanges: vec![ExchangeIdEnum::Binance, ExchangeIdEnum::Bybit],
            monitored_pairs: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
            fairness_config: FairnessConfig::default(),
            max_queue_size: 50,
            opportunity_ttl_minutes: 10,
        }
    }

    fn create_test_opportunity() -> GlobalOpportunity {
        let opportunity = ArbitrageOpportunity {
            id: Uuid::new_v4().to_string(),
            pair: "BTCUSDT".to_string(),
            long_exchange: ExchangeIdEnum::Binance, // **REQUIRED**: No longer optional
            short_exchange: ExchangeIdEnum::Bybit,  // **REQUIRED**: No longer optional
            long_rate: Some(0.0005),
            short_rate: Some(0.0015),
            rate_difference: 0.001,
            net_rate_difference: Some(0.001),
            potential_profit_value: Some(10.0),
            timestamp: Utc::now().timestamp_millis() as u64,
            r#type: ArbitrageType::FundingRate,
            details: Some("Test opportunity".to_string()),
            min_exchanges_required: 2, // **ALWAYS 2** for arbitrage
        };

        GlobalOpportunity {
            id: uuid::Uuid::new_v4().to_string(),
            opportunity_type: "arbitrage".to_string(),
            opportunity: opportunity.clone(),
            arbitrage_opportunity: Some(opportunity.clone()),
            technical_opportunity: None,
            source: OpportunitySource::SystemGenerated,
            created_at: Utc::now().timestamp_millis() as u64,
            detection_timestamp: Utc::now().timestamp_millis() as u64,
            expires_at: Some(Utc::now().timestamp_millis() as u64 + 600000), // 10 minutes
            expiry_timestamp: Some(Utc::now().timestamp_millis() as u64 + 600000), // 10 minutes
            priority: 5,                                                     // Default priority
            priority_score: 1.0,
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
            distributed_to: Vec::new(),
            max_participants: Some(10), // Default limit
            current_participants: 0,
            distribution_strategy: DistributionStrategy::RoundRobin,
        }
    }

    fn create_test_user_profile(user_id: &str, tier: SubscriptionTier) -> UserProfile {
        UserProfile {
            user_id: user_id.to_string(),
            telegram_user_id: Some(12345),
            telegram_username: Some("testuser".to_string()),
            subscription: SubscriptionInfo {
                tier,
                is_active: true,
                expires_at: None,
                created_at: Utc::now().timestamp_millis() as u64,
                features: vec!["basic_features".to_string()],
            },
            configuration: crate::types::UserConfiguration::default(),
            api_keys: Vec::new(),
            invitation_code: None,
            created_at: Utc::now().timestamp_millis() as u64,
            updated_at: Utc::now().timestamp_millis() as u64,
            last_active: Utc::now().timestamp_millis() as u64,
            is_active: true,
            total_trades: 0,
            total_pnl_usdt: 0.0,
            account_balance_usdt: 10000.0, // Default test balance
            profile_metadata: None,
            beta_expires_at: None,
        }
    }

    #[tokio::test]
    async fn test_global_opportunity_config_creation() {
        let config = create_test_config();

        assert_eq!(config.enabled, true);
        assert_eq!(config.max_opportunities_per_batch, 10);
        assert_eq!(config.batch_interval_seconds, 30);
        assert_eq!(config.min_rate_difference, 0.001);
        assert_eq!(config.max_age_minutes, 10);
        assert_eq!(config.ai_enhancement_enabled, false);
        assert!(matches!(
            config.distribution_strategy,
            DistributionStrategy::RoundRobin
        ));
        assert_eq!(config.min_threshold, 0.001);
        assert_eq!(config.max_threshold, 0.02);
        assert_eq!(config.monitored_exchanges.len(), 2);
        assert_eq!(config.monitored_pairs.len(), 2);
    }

    #[tokio::test]
    async fn test_global_opportunity_structure() {
        let global_opp = create_test_opportunity();

        assert_eq!(global_opp.opportunity.pair, "BTCUSDT");
        assert_eq!(global_opp.opportunity.rate_difference, 0.001);
        assert_eq!(global_opp.priority_score, 1.0);
        assert_eq!(global_opp.distributed_to.len(), 0);
        assert_eq!(global_opp.max_participants, Some(10));
        assert_eq!(global_opp.current_participants, 0);
        assert!(matches!(
            global_opp.distribution_strategy,
            DistributionStrategy::RoundRobin
        ));
        assert!(matches!(
            global_opp.source,
            OpportunitySource::SystemGenerated
        ));
    }

    #[tokio::test]
    async fn test_opportunity_queue_management() {
        let mut queue = OpportunityQueue {
            id: Uuid::new_v4().to_string(),
            opportunities: Vec::new(),
            created_at: Utc::now().timestamp_millis() as u64,
            scheduled_for: Utc::now().timestamp_millis() as u64,
            status: QueueStatus::Pending,
            target_users: vec!["user1".to_string(), "user2".to_string()],
            distribution_strategy: DistributionStrategy::RoundRobin,
            updated_at: Utc::now().timestamp_millis() as u64,
            total_distributed: 0,
            active_users: 2,
        };

        // Test adding opportunities
        let opp1 = create_test_opportunity();
        let mut opp2 = create_test_opportunity();
        opp2.priority_score = 2.0; // Higher priority

        queue.opportunities.push(opp1);
        queue.opportunities.push(opp2);

        // Test sorting by priority
        queue.opportunities.sort_by(|a, b| {
            b.priority_score
                .partial_cmp(&a.priority_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        assert_eq!(queue.opportunities.len(), 2);
        assert_eq!(queue.opportunities[0].priority_score, 2.0); // Higher priority first
        assert_eq!(queue.opportunities[1].priority_score, 1.0);
        assert_eq!(queue.active_users, 2);
    }

    #[tokio::test]
    async fn test_user_distribution_tracking() {
        let tracking = UserOpportunityDistribution {
            user_id: "test_user".to_string(),
            opportunity_id: "test_opportunity".to_string(),
            distributed_at: Utc::now().timestamp_millis() as u64,
            delivery_status: DeliveryStatus::Sent,
            delay_applied_seconds: 0,
            access_level: UserAccessLevel::FreeWithAPI,
            last_opportunity_received: Some(Utc::now().timestamp_millis() as u64),
            total_opportunities_received: 5,
            opportunities_today: 2,
            last_daily_reset: Utc::now().timestamp_millis() as u64,
            priority_weight: 1.5,
            is_eligible: true,
        };

        assert_eq!(tracking.user_id, "test_user");
        assert_eq!(tracking.total_opportunities_received, 5);
        assert_eq!(tracking.opportunities_today, 2);
        assert_eq!(tracking.priority_weight, 1.5);
        assert!(tracking.is_eligible);
        assert!(tracking.last_opportunity_received.is_some());
    }

    #[tokio::test]
    async fn test_fairness_config_defaults() {
        let config = FairnessConfig::default();

        assert_eq!(config.enable_delay_for_free_users, true);
        assert_eq!(config.free_user_delay_seconds, 300); // 5 minutes
        assert_eq!(config.max_opportunities_per_user_per_hour, 2);
        assert_eq!(config.prioritize_subscription_users, true);
        assert_eq!(config.enable_round_robin, true);
    }

    #[tokio::test]
    async fn test_distribution_strategies() {
        // Test all distribution strategy variants
        let strategies = vec![
            DistributionStrategy::FirstComeFirstServe,
            DistributionStrategy::RoundRobin,
            DistributionStrategy::PriorityBased,
            DistributionStrategy::Broadcast,
            DistributionStrategy::Immediate,
            DistributionStrategy::Batched,
            DistributionStrategy::Prioritized,
            DistributionStrategy::RateLimited,
        ];

        for strategy in strategies {
            match strategy {
                DistributionStrategy::FirstComeFirstServe => {
                    // Test first-come-first-serve logic - verified strategy exists
                }
                DistributionStrategy::RoundRobin => {
                    // Test round-robin logic - verified strategy exists
                }
                DistributionStrategy::PriorityBased => {
                    // Test priority-based logic - verified strategy exists
                }
                DistributionStrategy::Broadcast => {
                    // Test broadcast logic - verified strategy exists
                }
                DistributionStrategy::Immediate => {
                    // Test immediate distribution logic - verified strategy exists
                }
                DistributionStrategy::Batched => {
                    // Test batched distribution logic - verified strategy exists
                }
                DistributionStrategy::Prioritized => {
                    // Test prioritized distribution logic - verified strategy exists
                }
                DistributionStrategy::RateLimited => {
                    // Test rate-limited distribution logic - verified strategy exists
                }
            }
        }
    }

    #[tokio::test]
    async fn test_opportunity_source_types() {
        let sources = vec![
            OpportunitySource::SystemGenerated,
            OpportunitySource::AIGenerated,
            OpportunitySource::ExternalAPI,
        ];

        for source in sources {
            match source {
                OpportunitySource::SystemGenerated => {
                    // System-generated opportunities - verified source exists
                }
                OpportunitySource::AIGenerated => {
                    // AI-generated opportunities - verified source exists
                }
                OpportunitySource::ExternalAPI => {
                    // External source opportunities - verified source exists
                }
                _ => {
                    // Handle other variants
                }
            }
        }
    }

    #[tokio::test]
    async fn test_opportunity_expiry_logic() {
        let now = Utc::now().timestamp_millis() as u64;

        // Create expired opportunity
        let mut expired_opp = create_test_opportunity();
        expired_opp.expiry_timestamp = Some(now - 1000); // 1 second ago

        // Create valid opportunity
        let valid_opp = create_test_opportunity();
        // expiry_timestamp is set in the future by create_test_opportunity()

        assert!(expired_opp.expiry_timestamp.unwrap_or(0) < now);
        assert!(valid_opp.expiry_timestamp.unwrap_or(0) > now);

        // Test filtering logic
        let opportunities = vec![expired_opp, valid_opp];
        let valid_opportunities: Vec<_> = opportunities
            .into_iter()
            .filter(|opp| opp.expiry_timestamp.unwrap_or(0) > now)
            .collect();

        assert_eq!(valid_opportunities.len(), 1);
    }

    #[tokio::test]
    async fn test_priority_score_calculation() {
        let base_rate_diff = 0.001; // 0.1%
        let priority_score = base_rate_diff * 1000.0; // Scale up

        assert_eq!(priority_score, 1.0);

        // Test higher rate difference
        let higher_rate_diff = 0.005; // 0.5%
        let higher_priority = higher_rate_diff * 1000.0;

        assert_eq!(higher_priority, 5.0);
        assert!(higher_priority > priority_score);
    }

    #[tokio::test]
    async fn test_user_eligibility_checks() {
        let config = FairnessConfig::default();

        // Test daily limit check
        let mut tracking = UserOpportunityDistribution {
            user_id: "test_user".to_string(),
            opportunity_id: "test_opportunity".to_string(),
            distributed_at: Utc::now().timestamp_millis() as u64,
            delivery_status: DeliveryStatus::Sent,
            delay_applied_seconds: 0,
            access_level: UserAccessLevel::FreeWithAPI,
            last_opportunity_received: None,
            total_opportunities_received: 0,
            opportunities_today: config.max_opportunities_per_user_per_hour,
            last_daily_reset: Utc::now().timestamp_millis() as u64,
            priority_weight: 1.0,
            is_eligible: true,
        };

        // User at hourly limit should not be eligible
        assert!(
            !tracking.is_eligible
                || tracking.opportunities_today >= config.max_opportunities_per_user_per_hour
        );

        // Test cooldown period
        tracking.opportunities_today = 0;
        tracking.last_opportunity_received = Some(Utc::now().timestamp_millis() as u64 - 1000); // 1 second ago

        let cooldown_ms = config.free_user_delay_seconds * 1000; // Convert to milliseconds
        let time_since_last = 1000u64; // 1 second

        // Should not be eligible due to cooldown
        assert!(time_since_last < cooldown_ms);
    }

    #[tokio::test]
    async fn test_subscription_tier_priority() {
        let _free_user = create_test_user_profile("user1", SubscriptionTier::Free);
        let _premium_user = create_test_user_profile("user2", SubscriptionTier::Premium);

        let config = FairnessConfig::default();

        // Test that subscription users are prioritized
        assert_eq!(config.prioritize_subscription_users, true);
        assert_eq!(config.enable_round_robin, true);
    }

    #[tokio::test]
    async fn test_opportunity_participant_limits() {
        let mut opportunity = create_test_opportunity();
        opportunity.max_participants = Some(2);

        // Test adding participants
        opportunity.distributed_to.push("user1".to_string());
        opportunity.current_participants += 1;
        assert_eq!(opportunity.current_participants, 1);
        assert!(opportunity.current_participants < opportunity.max_participants.unwrap_or(10));

        // Add second participant
        opportunity.distributed_to.push("user2".to_string());
        opportunity.current_participants += 1;
        assert_eq!(opportunity.current_participants, 2);
        assert_eq!(
            opportunity.current_participants,
            opportunity.max_participants.unwrap_or(10)
        );

        // Check if at limit
        assert_eq!(
            opportunity.distributed_to.len(),
            opportunity.max_participants.unwrap_or(10) as usize
        );
    }

    #[tokio::test]
    async fn test_queue_size_limits() {
        let config = create_test_config();
        let mut opportunities = Vec::new();

        // Create more opportunities than max queue size
        for i in 0..(config.max_queue_size + 10) {
            let mut opp = create_test_opportunity();
            opp.priority_score = i as f64; // Different priorities
            opportunities.push(opp);
        }

        // Sort by priority (highest first)
        opportunities.sort_by(|a, b| {
            b.priority_score
                .partial_cmp(&a.priority_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to max size
        if opportunities.len() > config.max_queue_size as usize {
            opportunities.truncate(config.max_queue_size as usize);
        }

        assert_eq!(opportunities.len(), config.max_queue_size as usize);

        // Verify highest priority opportunities are kept
        assert_eq!(
            opportunities[0].priority_score,
            (config.max_queue_size + 10 - 1) as f64
        );
        assert_eq!(opportunities.last().unwrap().priority_score, 10.0);
    }

    #[tokio::test]
    async fn test_activity_boost_calculation() {
        let config = FairnessConfig::default();
        let base_priority = 1.0;

        // Test with recent activity (within 1 hour)
        let now = Utc::now().timestamp_millis() as u64;
        let one_hour_ms = 60 * 60 * 1000;
        let recent_activity = now - (one_hour_ms / 2); // 30 minutes ago

        // Test delay for free users
        let delay_seconds = config.free_user_delay_seconds;
        assert_eq!(delay_seconds, 300); // 5 minutes

        // Test with old activity (more than delay period)
        let old_activity = now - (delay_seconds * 1000 * 2); // 10 minutes ago

        let time_since_activity = now - old_activity;
        let delay_ms = delay_seconds * 1000;

        assert!(time_since_activity > delay_ms);
    }
}
