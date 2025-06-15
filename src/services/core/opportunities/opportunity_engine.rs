// src/services/core/opportunities/opportunity_engine.rs

use crate::log_info;
use crate::services::core::ai::ai_beta_integration::AiBetaIntegrationService;
use crate::services::core::infrastructure::monitoring_module::opportunity_monitor::{
    OpportunityMonitor, OpportunityPipelineStage,
};
use crate::services::core::opportunities::{
    access_manager::AccessManager,
    ai_enhancer::AIEnhancer,
    funding_rate_manager::FundingRateManager,
    historical_data_manager::HistoricalDataManager,
    market_analyzer::MarketAnalyzer,
    opportunity_builders::OpportunityBuilder,
    opportunity_core::{OpportunityConfig, OpportunityContext},
    opportunity_scoring_engine::OpportunityScoringEngine,
};
use crate::services::core::trading::exchange::ExchangeService;
use crate::services::core::user::user_access::UserAccessService;
use crate::services::core::user::UserProfileService;
use crate::services::CacheManager;
use crate::types::{
    ArbitrageOpportunity, ArbitrageType, ChatContext, ExchangeIdEnum, GlobalOpportunity,
    GroupedOpportunity, OpportunitySource, TechnicalOpportunity,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::Utc;
use serde_json;
use std::sync::Arc;
use worker::kv::KvStore;
use worker::{console_log, D1Database};

/// Unified opportunity engine that orchestrates all opportunity services
/// Eliminates redundancy by consolidating logic from personal, group, global, and legacy services
#[derive(Clone)]
pub struct OpportunityEngine {
    // Core components
    market_analyzer: Arc<MarketAnalyzer>,
    access_manager: Arc<AccessManager>,
    ai_enhancer: Arc<AIEnhancer>,
    cache_manager: Arc<CacheManager>,
    opportunity_builder: Arc<OpportunityBuilder>,

    // Strategic enhancement modules
    funding_rate_manager: Arc<FundingRateManager>,
    _historical_data_manager: Arc<HistoricalDataManager>,
    opportunity_scoring_engine: Arc<OpportunityScoringEngine>,

    // Monitoring and observability
    opportunity_monitor: Option<Arc<OpportunityMonitor>>,

    // Configuration
    config: OpportunityConfig,

    // Services
    user_profile_service: Arc<UserProfileService>,
    #[allow(dead_code)]
    kv_store: KvStore,
}

impl OpportunityEngine {
    pub fn new(
        user_profile_service: Arc<UserProfileService>,
        user_access_service: Arc<UserAccessService>,
        ai_service: Arc<AiBetaIntegrationService>,
        exchange_service: Arc<ExchangeService>,
        kv_store: KvStore,
        database: Option<Arc<D1Database>>,
        config: OpportunityConfig,
    ) -> ArbitrageResult<Self> {
        let access_manager = Arc::new(AccessManager::new(
            user_profile_service.clone(),
            user_access_service,
            Arc::new(kv_store.clone()),
        ));

        // Create cache manager for pipeline-first architecture
        let cache_manager = Arc::new(CacheManager::new(kv_store.clone()));

        // Create market analyzer with pipeline-first architecture components
        let mut market_analyzer = MarketAnalyzer::new(exchange_service.clone());

        // Inject cache manager for cache-first patterns
        market_analyzer = market_analyzer.with_cache_manager(cache_manager.clone());

        // TODO: Inject pipeline manager and data ingestion module when available
        // This will be done in the service container initialization
        // market_analyzer = market_analyzer.with_pipeline_manager(pipeline_manager);
        // market_analyzer = market_analyzer.with_data_ingestion_module(data_ingestion_module);

        let market_analyzer = Arc::new(market_analyzer);

        let ai_enhancer = Arc::new(AIEnhancer::new(ai_service, access_manager.clone()));
        let opportunity_builder = Arc::new(OpportunityBuilder::new(config.clone()));

        // Initialize strategic enhancement modules
        let funding_rate_manager = Arc::new(FundingRateManager::new(
            database.clone(),
            Some((*exchange_service).clone()),
        ));
        let historical_data_manager = Arc::new(HistoricalDataManager::new(database.clone()));
        let opportunity_scoring_engine = Arc::new(OpportunityScoringEngine::new());

        console_log!("✅ OPPORTUNITY ENGINE INITIALIZED - Pipeline-first architecture enabled");

        Ok(Self {
            market_analyzer,
            access_manager,
            ai_enhancer,
            cache_manager,
            opportunity_builder,
            funding_rate_manager,
            _historical_data_manager: historical_data_manager,
            opportunity_scoring_engine,
            opportunity_monitor: None, // Will be injected by service container
            config,
            user_profile_service,
            kv_store,
        })
    }

    // Personal Opportunity Generation (replaces PersonalOpportunityService)

    /// Generate personal arbitrage opportunities for a user
    pub async fn generate_personal_arbitrage_opportunities(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        pairs: Option<Vec<String>>,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        // Validate user access
        let access_result = self
            .access_manager
            .validate_user_access(user_id, "arbitrage", chat_context)
            .await?;

        if !access_result.can_access {
            return Err(ArbitrageError::access_denied(
                access_result.reason.unwrap_or("Access denied".to_string()),
            ));
        }

        // Check cache first
        let cache_key = format!("user_arbitrage_opportunities_{}", user_id);
        if let Ok(Some(cached_opportunities)) = self
            .cache_manager
            .get::<Vec<ArbitrageOpportunity>>(&cache_key)
            .await
        {
            log_info!(
                "Retrieved cached personal arbitrage opportunities",
                serde_json::json!({
                    "user_id": user_id,
                    "count": cached_opportunities.len(),
                    "cache_hit": true
                })
            );
            return Ok(cached_opportunities);
        }

        // Get user's exchange APIs
        let user_exchanges = self.access_manager.get_user_exchange_apis(user_id).await?;
        if user_exchanges.len() < 2 {
            return Err(ArbitrageError::validation_error(
                "At least 2 exchange APIs required for arbitrage opportunities".to_string(),
            ));
        }

        // Use default pairs if none provided
        let trading_pairs = pairs.unwrap_or_else(|| self.config.default_pairs.clone());

        // Analyze market data and detect opportunities
        let mut opportunities = Vec::new();
        for pair in &trading_pairs {
            let pair_opportunities = self.market_analyzer.build_price_arbitrage(pair).await?;

            for market_opp in pair_opportunities {
                // Use PRICE ARBITRAGE instead of funding rate arbitrage for market analyzer results
                let opportunity = self.opportunity_builder.build_price_arbitrage(
                    market_opp.pair,
                    market_opp.long_exchange,
                    market_opp.short_exchange,
                    market_opp.buy_price,
                    market_opp.sell_price,
                    &OpportunityContext::Personal {
                        user_id: user_id.to_string(),
                    },
                )?;
                opportunities.push(opportunity);
            }
        }

        // Apply subscription-based filtering
        opportunities = self
            .access_manager
            .filter_opportunities_by_subscription(user_id, opportunities)
            .await?;

        // Enhance with AI if available
        opportunities = self
            .ai_enhancer
            .enhance_arbitrage_opportunities(user_id, opportunities, "personal_arbitrage")
            .await?;

        // Cache the results
        let cache_key = format!("user_arbitrage_opportunities_{}", user_id);
        let _ = self
            .cache_manager
            .set(&cache_key, &opportunities, Some(300))
            .await;

        // Record opportunity generation for rate limiting
        let _ = self
            .access_manager
            .record_opportunity_received(user_id, "arbitrage", chat_context)
            .await;

        log_info!(
            "Generated personal arbitrage opportunities",
            serde_json::json!({
                "user_id": user_id,
                "count": opportunities.len(),
                "pairs": trading_pairs,
                "ai_enhanced": self.ai_enhancer.is_ai_available_for_user(user_id).await.unwrap_or(false)
            })
        );

        Ok(opportunities)
    }

    /// Generate personal technical opportunities for a user
    pub async fn generate_personal_technical_opportunities(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        pairs: Option<Vec<String>>,
    ) -> ArbitrageResult<Vec<TechnicalOpportunity>> {
        // Validate user access
        let access_result = self
            .access_manager
            .validate_user_access(user_id, "technical", chat_context)
            .await?;

        if !access_result.can_access {
            return Err(ArbitrageError::access_denied(
                access_result.reason.unwrap_or("Access denied".to_string()),
            ));
        }

        // Check cache first
        let cache_key = format!("user_technical_opportunities_{}", user_id);
        if let Ok(Some(cached_opportunities)) = self
            .cache_manager
            .get::<Vec<TechnicalOpportunity>>(&cache_key)
            .await
        {
            return Ok(cached_opportunities);
        }

        // Get user's exchange APIs
        let user_exchanges = self.access_manager.get_user_exchange_apis(user_id).await?;
        if user_exchanges.is_empty() {
            return Err(ArbitrageError::validation_error(
                "At least 1 exchange API required for technical opportunities".to_string(),
            ));
        }

        let trading_pairs = pairs.unwrap_or_else(|| self.config.default_pairs.clone());

        // Analyze technical signals
        let mut technical_opportunities = Vec::new();
        for pair in &trading_pairs {
            for (exchange, _) in &user_exchanges {
                // Get ticker data for this exchange and pair
                if let Ok((ticker, _)) = self
                    .market_analyzer
                    .get_market_data_pipeline_first(exchange, pair)
                    .await
                {
                    // Get funding rate if available
                    let funding_rate = None; // TODO: Implement funding rate retrieval

                    // Generate technical opportunities for this pair
                    let technical_analysis = self
                        .market_analyzer
                        .analyze_technical_signal(&ticker, &funding_rate);

                    // Convert technical analysis to technical opportunity
                    let technical_opportunity = TechnicalOpportunity {
                        id: format!("tech_{}_{}", pair, exchange.as_str()),
                        trading_pair: pair.clone(),
                        exchanges: vec![exchange.as_str().to_string()],
                        signal_type: crate::types::TechnicalSignalType::Buy,
                        confidence: technical_analysis.confidence,
                        risk_level: technical_analysis.risk_level.clone(),
                        entry_price: ticker.last.unwrap_or(0.0),
                        target_price: technical_analysis.target_price,
                        stop_loss: technical_analysis.stop_loss,
                        created_at: chrono::Utc::now().timestamp_millis() as u64,
                        expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + 300000), // 5 minutes
                        pair: pair.clone(),
                        expected_return_percentage: technical_analysis.expected_return,
                        details: Some(format!("Technical signal: {}", technical_analysis.signal)),
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                        metadata: serde_json::json!({
                            "exchange": exchange.as_str(),
                            "funding_rate": funding_rate,
                            "signal": technical_analysis.signal,
                            "market_conditions": technical_analysis.market_conditions
                        }),
                        timeframe: "1h".to_string(),
                        indicators: serde_json::json!({
                            "confidence": technical_analysis.confidence,
                            "risk_level": technical_analysis.risk_level
                        }),
                    };
                    technical_opportunities.push(technical_opportunity);
                }
            }
        }

        // Apply subscription-based filtering
        technical_opportunities = self
            .access_manager
            .filter_opportunities_by_subscription(user_id, technical_opportunities)
            .await?;

        // Enhance with AI if available
        technical_opportunities = self
            .ai_enhancer
            .enhance_technical_opportunities(user_id, technical_opportunities, "personal_technical")
            .await?;

        // Cache the results
        let cache_key = format!("user_technical_opportunities_{}", user_id);
        let _ = self
            .cache_manager
            .set(&cache_key, &technical_opportunities, Some(300))
            .await;

        // Record opportunity generation
        let _ = self
            .access_manager
            .record_opportunity_received(user_id, "technical", chat_context)
            .await;

        log_info!(
            "Generated personal technical opportunities",
            serde_json::json!({
                "user_id": user_id,
                "count": technical_opportunities.len(),
                "pairs": trading_pairs
            })
        );

        Ok(technical_opportunities)
    }

    // Group Opportunity Generation (replaces GroupOpportunityService)

    /// Generate group arbitrage opportunities for group members
    pub async fn generate_group_arbitrage_opportunities(
        &self,
        group_admin_id: &str,
        chat_context: &ChatContext,
        pairs: Option<Vec<String>>,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        // Validate group admin access
        let access_result = self
            .access_manager
            .validate_group_admin_access(group_admin_id, "arbitrage", chat_context)
            .await?;

        if !access_result.can_access {
            return Err(ArbitrageError::access_denied(
                access_result
                    .reason
                    .unwrap_or("Group admin access denied".to_string()),
            ));
        }

        // Check cache first
        let group_id = chat_context
            .get_group_id()
            .unwrap_or("unknown_group".to_string());
        let cache_key = format!("group_arbitrage_opportunities_{}", group_id);
        if let Ok(Some(cached_opportunities)) = self
            .cache_manager
            .get::<Vec<ArbitrageOpportunity>>(&cache_key)
            .await
        {
            return Ok(cached_opportunities);
        }

        // Get group admin's exchange APIs
        let admin_exchanges = self
            .access_manager
            .get_group_admin_exchange_apis(group_admin_id)
            .await?;

        if admin_exchanges.len() < 2 {
            return Err(ArbitrageError::validation_error(
                "Group admin needs at least 2 exchange APIs for group arbitrage".to_string(),
            ));
        }

        let trading_pairs = pairs.unwrap_or_else(|| self.config.default_pairs.clone());

        // Generate opportunities using admin's APIs
        let mut opportunities = Vec::new();
        for pair in &trading_pairs {
            let pair_opportunities = self.market_analyzer.build_price_arbitrage(pair).await?;

            for market_opp in pair_opportunities {
                // Use PRICE ARBITRAGE instead of funding rate arbitrage for market analyzer results
                let mut opportunity = self.opportunity_builder.build_price_arbitrage(
                    market_opp.pair,
                    market_opp.long_exchange,
                    market_opp.short_exchange,
                    market_opp.buy_price,
                    market_opp.sell_price,
                    &OpportunityContext::Group {
                        admin_id: group_admin_id.to_string(),
                        chat_context: chat_context.clone(),
                    },
                )?;

                // Apply group multiplier (2x opportunities)
                if let Some(profit) = opportunity.potential_profit_value {
                    opportunity.potential_profit_value = Some(profit * 2.0);
                }

                opportunities.push(opportunity);
            }
        }

        // Enhance with AI using admin's access level
        opportunities = self
            .ai_enhancer
            .enhance_arbitrage_opportunities(group_admin_id, opportunities, "group_arbitrage")
            .await?;

        // Cache the results
        let cache_key = format!("group_arbitrage_opportunities_{}", group_id);
        let _ = self
            .cache_manager
            .set(&cache_key, &opportunities, Some(300))
            .await;

        log_info!(
            "Generated group arbitrage opportunities",
            serde_json::json!({
                "group_admin_id": group_admin_id,
                "group_id": group_id,
                "count": opportunities.len(),
                "multiplier_applied": true
            })
        );

        Ok(opportunities)
    }

    // Global Opportunity Generation (replaces GlobalOpportunityService)

    /// Generate global opportunities for system-wide distribution
    pub async fn generate_global_opportunities(
        &self,
    ) -> Result<Vec<ArbitrageOpportunity>, Box<dyn std::error::Error + Send + Sync>> {
        let enhanced_logging =
            crate::utils::feature_flags::is_feature_enabled("opportunity_engine.enhanced_logging")
                .unwrap_or(false);

        // Generate unique request ID for tracking
        let request_id = format!("global_opp_{}", chrono::Utc::now().timestamp_millis());
        let pipeline_start_time = std::time::SystemTime::now();

        if enhanced_logging {
            worker::console_log!(
                "🌍 GLOBAL_OPPORTUNITIES - Starting generation ({})",
                request_id
            );
        }

        // Track pipeline initialization
        if let Some(monitor) = &self.opportunity_monitor {
            let _ = monitor
                .track_pipeline_stage(
                    &request_id,
                    OpportunityPipelineStage::Initialization,
                    0, // Will be updated when we know the duration
                    true,
                    Some(std::collections::HashMap::from([
                        (
                            "pipeline_type".to_string(),
                            "global_opportunities".to_string(),
                        ),
                        ("enhanced_logging".to_string(), enhanced_logging.to_string()),
                    ])),
                )
                .await;
        }

        let mut all_opportunities = Vec::new();
        let trading_pairs = self.get_dynamic_trading_pairs();
        let monitored_exchanges = &self.config.monitored_exchanges;

        if enhanced_logging {
            worker::console_log!(
                "📈 GLOBAL_OPPORTUNITIES - Pairs: {:?}, Exchanges: {:?}",
                trading_pairs,
                monitored_exchanges
            );
        }

        // PRIORITY 1: Generate funding rate arbitrage opportunities (PRIMARY TYPE)
        if enhanced_logging {
            worker::console_log!(
                "💰 FUNDING_RATE_ARBITRAGE - Starting primary opportunity generation"
            );
        }

        // Generate pure funding rate arbitrage opportunities first
        let monitored_pairs = self.get_monitored_pairs().await.unwrap_or_default();
        let funding_opportunities = self
            .funding_rate_manager
            .detect_funding_arbitrage(&monitored_pairs)
            .await
            .unwrap_or_default();
        let funding_arbitrage_opportunities =
            self.create_arbitrage_from_funding(&funding_opportunities);

        if enhanced_logging {
            worker::console_log!(
                "💰 FUNDING_RATE_ARBITRAGE - Generated {} pure funding rate opportunities",
                funding_arbitrage_opportunities.len()
            );
        }

        all_opportunities.extend(funding_arbitrage_opportunities);

        // Track market data fetch stage
        let _market_data_start = std::time::SystemTime::now();
        if let Some(monitor) = &self.opportunity_monitor {
            let _ = monitor
                .track_pipeline_stage(
                    &request_id,
                    OpportunityPipelineStage::MarketDataFetch,
                    0, // Will be updated when stage completes
                    true,
                    Some(std::collections::HashMap::from([
                        ("pairs_count".to_string(), trading_pairs.len().to_string()),
                        (
                            "exchanges_count".to_string(),
                            monitored_exchanges.len().to_string(),
                        ),
                    ])),
                )
                .await;
        }

        let mut _api_calls_count = 0;
        let mut cache_hits = 0;
        let mut cache_misses = 0;

        for pair in &trading_pairs {
            if enhanced_logging {
                worker::console_log!("🔄 PROCESSING_PAIR - {}", pair);
            }

            // Generate arbitrage opportunities for this pair
            for exchange_a in monitored_exchanges.iter() {
                for exchange_b in monitored_exchanges.iter() {
                    if exchange_a != exchange_b {
                        // Track API calls for monitoring
                        let api_call_start = std::time::SystemTime::now();

                        // Get ticker data for both exchanges
                        if let (Ok((ticker_a, source_a)), Ok((ticker_b, source_b))) = (
                            self.market_analyzer
                                .get_market_data_pipeline_first(exchange_a, pair)
                                .await,
                            self.market_analyzer
                                .get_market_data_pipeline_first(exchange_b, pair)
                                .await,
                        ) {
                            _api_calls_count += 2;

                            // Track cache hits/misses
                            if source_a == crate::services::core::opportunities::market_analyzer::DataSource::Cache {
                                cache_hits += 1;
                            } else {
                                cache_misses += 1;
                            }
                            if source_b == crate::services::core::opportunities::market_analyzer::DataSource::Cache {
                                cache_hits += 1;
                            } else {
                                cache_misses += 1;
                            }

                            // Track API call metrics
                            if let Some(monitor) = &self.opportunity_monitor {
                                let api_duration =
                                    api_call_start.elapsed().unwrap_or_default().as_millis() as u64;
                                let source_a_str = match source_a {
                                    crate::services::core::opportunities::market_analyzer::DataSource::Pipeline => "pipeline",
                                    crate::services::core::opportunities::market_analyzer::DataSource::Cache => "cache",
                                    crate::services::core::opportunities::market_analyzer::DataSource::DirectAPI => "direct_api",
                                };
                                let source_b_str = match source_b {
                                    crate::services::core::opportunities::market_analyzer::DataSource::Pipeline => "pipeline",
                                    crate::services::core::opportunities::market_analyzer::DataSource::Cache => "cache",
                                    crate::services::core::opportunities::market_analyzer::DataSource::DirectAPI => "direct_api",
                                };

                                let _ = monitor
                                    .track_api_call(
                                        crate::services::core::infrastructure::monitoring_module::opportunity_monitor::ApiCallTrackingParams {
                                            exchange: exchange_a.as_str().to_string(),
                                            endpoint: format!("/ticker/{}", pair),
                                            method: "GET".to_string(),
                                            status_code: Some(200),
                                            response_time_ms: api_duration / 2, // Approximate per exchange
                                            success: true,
                                            error_message: None,
                                            data_source: source_a_str.to_string(),
                                        }
                                    )
                                    .await;
                                let _ = monitor
                                    .track_api_call(
                                        crate::services::core::infrastructure::monitoring_module::opportunity_monitor::ApiCallTrackingParams {
                                            exchange: exchange_b.as_str().to_string(),
                                            endpoint: format!("/ticker/{}", pair),
                                            method: "GET".to_string(),
                                            status_code: Some(200),
                                            response_time_ms: api_duration / 2, // Approximate per exchange
                                            success: true,
                                            error_message: None,
                                            data_source: source_b_str.to_string(),
                                        }
                                    )
                                    .await;
                            }
                            // Analyze arbitrage opportunity
                            if let Ok(analysis) =
                                self.market_analyzer.analyze_arbitrage_opportunity(
                                    pair, &ticker_a, &ticker_b, exchange_a, exchange_b,
                                )
                            {
                                // Convert analysis to opportunity if profitable
                                if analysis.price_difference_percent
                                    > self.config.min_rate_difference
                                {
                                    // Create deterministic ID for deduplication (without timestamp)
                                    let deterministic_id = format!(
                                        "arb_{}_{}_{}",
                                        pair.replace("/", ""),
                                        exchange_a.as_str(),
                                        exchange_b.as_str()
                                    );

                                    let opportunity = ArbitrageOpportunity {
                                        id: deterministic_id,
                                        trading_pair: pair.clone(),
                                        exchanges: vec![
                                            exchange_a.as_str().to_string(),
                                            exchange_b.as_str().to_string(),
                                        ],
                                        buy_exchange: exchange_a.as_str().to_string(),
                                        sell_exchange: exchange_b.as_str().to_string(),
                                        buy_price: ticker_a.last.unwrap_or(0.0),
                                        sell_price: ticker_b.last.unwrap_or(0.0),
                                        profit_percentage: analysis.price_difference_percent,
                                        confidence_score: analysis.confidence,
                                        volume: ticker_a
                                            .volume
                                            .unwrap_or(0.0)
                                            .max(ticker_b.volume.unwrap_or(0.0)),
                                        risk_level: "medium".to_string(), // Default risk level since not in analysis
                                        expires_at: Some(
                                            chrono::Utc::now().timestamp_millis() as u64 + 300000,
                                        ), // 5 minutes
                                        created_at: chrono::Utc::now().timestamp_millis() as u64,
                                        // Unified modular fields
                                        pair: pair.clone(),
                                        long_exchange: *exchange_a,
                                        short_exchange: *exchange_b,
                                        long_rate: None,
                                        short_rate: None,
                                        rate_difference: analysis.price_difference_percent,
                                        net_rate_difference: Some(
                                            analysis.price_difference_percent,
                                        ),
                                        potential_profit_value: Some(
                                            analysis.price_difference_percent * 1000.0,
                                        ),
                                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                        detected_at: chrono::Utc::now().timestamp_millis() as u64,
                                        r#type: crate::types::ArbitrageType::Price,
                                        details: Some(format!(
                                            "Price arbitrage between {} and {}",
                                            exchange_a.as_str(),
                                            exchange_b.as_str()
                                        )),
                                        min_exchanges_required: 2,
                                    };
                                    all_opportunities.push(opportunity);
                                }
                            }
                        }
                    }
                }
            }
        }

        // PRIORITY 2: Enhance price arbitrage opportunities with funding rate data
        if enhanced_logging {
            worker::console_log!("🔗 FUNDING_ENHANCEMENT - Enhancing {} price arbitrage opportunities with funding rate data", all_opportunities.len());
        }

        // Populate funding rates for all opportunities
        self.populate_funding_rates(&mut all_opportunities).await;

        // Integrate funding rate arbitrage with existing opportunities
        self.integrate_funding_with_arbitrage(&mut all_opportunities, &funding_opportunities)
            .await;

        // Track deduplication stage
        let dedup_start = std::time::SystemTime::now();
        let opportunities_before_dedup = all_opportunities.len();

        if let Some(monitor) = &self.opportunity_monitor {
            let _ = monitor
                .track_pipeline_stage(
                    &request_id,
                    OpportunityPipelineStage::Deduplication,
                    0, // Will be updated when stage completes
                    true,
                    Some(std::collections::HashMap::from([(
                        "opportunities_before_dedup".to_string(),
                        opportunities_before_dedup.to_string(),
                    )])),
                )
                .await;
        }

        // CRITICAL FIX: Implement proper deduplication using HashMap
        let mut deduplicated_opportunities: std::collections::HashMap<
            String,
            ArbitrageOpportunity,
        > = std::collections::HashMap::new();
        let mut duplicates_removed = 0;

        for opportunity in all_opportunities {
            let dedup_key = format!(
                "{}_{}_{}",
                opportunity.pair.replace("/", ""),
                opportunity.long_exchange.as_str(),
                opportunity.short_exchange.as_str()
            );

            match deduplicated_opportunities.get(&dedup_key) {
                Some(existing) => {
                    duplicates_removed += 1;
                    // Keep the opportunity with higher profit percentage
                    if opportunity.profit_percentage > existing.profit_percentage {
                        deduplicated_opportunities.insert(dedup_key, opportunity);
                    }
                }
                None => {
                    deduplicated_opportunities.insert(dedup_key, opportunity);
                }
            }
        }

        // Convert back to Vec
        let mut all_opportunities: Vec<ArbitrageOpportunity> =
            deduplicated_opportunities.into_values().collect();

        if enhanced_logging {
            worker::console_log!(
                "🔄 DEDUPLICATION - Reduced to {} unique opportunities after deduplication",
                all_opportunities.len()
            );
        }

        // Complete deduplication stage tracking
        if let Some(monitor) = &self.opportunity_monitor {
            let dedup_duration = dedup_start.elapsed().unwrap_or_default().as_millis() as u64;
            let _ = monitor
                .track_pipeline_stage(
                    &request_id,
                    OpportunityPipelineStage::Deduplication,
                    dedup_duration,
                    true,
                    Some(std::collections::HashMap::from([
                        (
                            "opportunities_before_dedup".to_string(),
                            opportunities_before_dedup.to_string(),
                        ),
                        (
                            "opportunities_after_dedup".to_string(),
                            all_opportunities.len().to_string(),
                        ),
                        (
                            "duplicates_removed".to_string(),
                            duplicates_removed.to_string(),
                        ),
                    ])),
                )
                .await;
        }

        // Apply feature flag-based limits
        let max_opportunities = crate::utils::feature_flags::get_numeric_feature_value(
            "opportunity_engine.max_opportunities",
            self.config.max_opportunities as f64,
        ) as usize;

        // Sort by profit and limit
        crate::services::core::opportunities::opportunity_core::OpportunityUtils::sort_arbitrage_by_profit(&mut all_opportunities);
        all_opportunities.truncate(max_opportunities);

        if enhanced_logging {
            worker::console_log!(
                "🎯 GLOBAL_OPPORTUNITIES - Complete. Generated {} opportunities (limit: {})",
                all_opportunities.len(),
                max_opportunities
            );
        }

        // Track final completion and comprehensive metrics
        if let Some(monitor) = &self.opportunity_monitor {
            let total_duration = pipeline_start_time
                .elapsed()
                .unwrap_or_default()
                .as_millis() as u64;

            // Track completion stage
            let _ = monitor
                .track_pipeline_stage(
                    &request_id,
                    OpportunityPipelineStage::Completion,
                    total_duration,
                    true,
                    Some(std::collections::HashMap::from([
                        (
                            "final_opportunities_count".to_string(),
                            all_opportunities.len().to_string(),
                        ),
                        (
                            "max_opportunities_limit".to_string(),
                            max_opportunities.to_string(),
                        ),
                        (
                            "total_pipeline_duration_ms".to_string(),
                            total_duration.to_string(),
                        ),
                    ])),
                )
                .await;

            // Track comprehensive opportunity generation metrics
            let duplicates_detected = opportunities_before_dedup as u64;
            let _ = monitor
                .track_opportunity_generation(
                    crate::services::core::infrastructure::monitoring_module::opportunity_monitor::OpportunityGenerationTrackingParams {
                        opportunities: all_opportunities.clone(),
                        duplicates_detected,
                        duplicates_removed: duplicates_removed as u64,
                        validation_failures: 0, // we don't track this separately yet
                        cache_hits,
                        cache_misses,
                        pipeline_duration_ms: total_duration,
                    }
                )
                .await;

            // Check for zero opportunities alert
            if all_opportunities.is_empty() {
                let _ = monitor
                    .track_error(
                        crate::services::core::infrastructure::monitoring_module::opportunity_monitor::ErrorTrackingParams {
                            error_type: "ZeroOpportunities".to_string(),
                            error_message: "No opportunities generated in global scan".to_string(),
                            stack_trace: None,
                            stage: "OpportunityGeneration".to_string(),
                            exchange: None,
                            symbol: None,
                            request_id: Some(request_id.clone()),
                            severity: "warning".to_string(),
                        }
                    )
                    .await;
            }
        }

        Ok(all_opportunities)
    }

    /// Get dynamic trading pairs based on market conditions and feature flags
    pub fn get_dynamic_trading_pairs(&self) -> Vec<String> {
        // Use feature flags to determine which pairs to include
        let enhanced_logging =
            crate::utils::feature_flags::is_feature_enabled("opportunity_engine.enhanced_logging")
                .unwrap_or(false);

        // Production-ready symbol list prioritizing main trading pairs
        let mut trading_pairs = vec![
            "BTC/USDT".to_string(),
            "ETH/USDT".to_string(),
            "BNB/USDT".to_string(),
            "SOL/USDT".to_string(),
            "ADA/USDT".to_string(),
            "MATIC/USDT".to_string(),
            "AVAX/USDT".to_string(),
            "DOT/USDT".to_string(),
        ];

        // Add additional pairs based on feature flags
        if crate::utils::feature_flags::is_feature_enabled("opportunity_engine.extended_pairs")
            .unwrap_or(false)
        {
            trading_pairs.extend(vec![
                "LINK/USDT".to_string(),
                "UNI/USDT".to_string(),
                "ATOM/USDT".to_string(),
                "FTM/USDT".to_string(),
            ]);
        }

        if enhanced_logging {
            worker::console_log!(
                "📋 DYNAMIC_PAIRS - Generated {} trading pairs: {:?}",
                trading_pairs.len(),
                trading_pairs
            );
        }

        trading_pairs
    }

    // Unified Opportunity Generation

    /// Generate opportunities of specified type
    pub async fn generate_opportunities_by_type(
        &self,
        user_id: &str,
        chat_context: &ChatContext,
        opportunity_type: Option<String>,
    ) -> ArbitrageResult<(Vec<ArbitrageOpportunity>, Vec<TechnicalOpportunity>)> {
        let opp_type = opportunity_type.unwrap_or_else(|| "both".to_string());

        let (arbitrage_opportunities, technical_opportunities) = match opp_type.as_str() {
            "arbitrage" => {
                let arb_opps = self
                    .generate_personal_arbitrage_opportunities(user_id, chat_context, None)
                    .await?;
                (arb_opps, Vec::new())
            }
            "technical" => {
                let tech_opps = self
                    .generate_personal_technical_opportunities(user_id, chat_context, None)
                    .await?;
                (Vec::new(), tech_opps)
            }
            _ => {
                // Generate both types
                let arb_opps = self
                    .generate_personal_arbitrage_opportunities(user_id, chat_context, None)
                    .await
                    .unwrap_or_default();
                let tech_opps = self
                    .generate_personal_technical_opportunities(user_id, chat_context, None)
                    .await
                    .unwrap_or_default();
                (arb_opps, tech_opps)
            }
        };

        log_info!(
            "Generated opportunities by type",
            serde_json::json!({
                "user_id": user_id,
                "type": opp_type,
                "arbitrage_count": arbitrage_opportunities.len(),
                "technical_count": technical_opportunities.len()
            })
        );

        Ok((arbitrage_opportunities, technical_opportunities))
    }

    // Utility Methods

    /// Get user's opportunity statistics
    pub async fn get_user_opportunity_stats(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<serde_json::Value> {
        let user_profile = self.user_profile_service.get_user_profile(user_id).await?;
        let ai_access_level = self
            .access_manager
            .get_user_ai_access_level(user_id)
            .await?;
        let user_exchanges = self.access_manager.get_user_exchange_apis(user_id).await?;

        Ok(serde_json::json!({
            "user_id": user_id,
            "subscription_tier": user_profile.map(|p| p.subscription.tier),
            "ai_access_level": format!("{:?}", ai_access_level),
            "exchange_count": user_exchanges.len(),
            "can_generate_arbitrage": user_exchanges.len() >= 2,
            "can_generate_technical": !user_exchanges.is_empty(),
            "ai_available": self.ai_enhancer.is_ai_available_for_user(user_id).await.unwrap_or(false)
        }))
    }

    /// Invalidate user caches
    pub async fn invalidate_user_caches(&self, user_id: &str) -> ArbitrageResult<()> {
        let cache_key = format!("user_arbitrage_opportunities_{}", user_id);
        self.cache_manager
            .delete::<Vec<ArbitrageOpportunity>>(&cache_key)
            .await
            .map(|_| ())
    }

    /// Invalidate group caches
    pub async fn invalidate_group_caches(&self, group_id: &str) -> ArbitrageResult<()> {
        let cache_key = format!("group_arbitrage_opportunities_{}", group_id);
        self.cache_manager
            .delete::<Vec<ArbitrageOpportunity>>(&cache_key)
            .await
            .map(|_| ())
    }

    /// Get engine configuration
    pub fn get_config(&self) -> &OpportunityConfig {
        &self.config
    }

    /// Update engine configuration
    pub fn update_config(&mut self, new_config: OpportunityConfig) {
        self.config = new_config;
    }

    /// Inject pipeline components into the market analyzer for pipeline-first architecture
    pub fn inject_pipeline_components(
        &mut self,
        data_ingestion_module: Arc<
            crate::services::core::infrastructure::data_ingestion_module::DataIngestionModule,
        >,
        pipeline_manager: Arc<crate::services::core::infrastructure::data_ingestion_module::pipeline_manager::PipelineManager>,
    ) {
        // Create a new market analyzer with pipeline components
        let mut new_analyzer = MarketAnalyzer::new(self.market_analyzer.get_exchange_service())
            .with_pipeline_manager(pipeline_manager)
            .with_data_ingestion_module(data_ingestion_module);

        // Preserve existing cache manager if available
        new_analyzer = new_analyzer.with_cache_manager(self.cache_manager.clone());

        self.market_analyzer = Arc::new(new_analyzer);
        console_log!("✅ Pipeline components injected into OpportunityEngine MarketAnalyzer");
    }

    /// Inject opportunity monitor for comprehensive tracking
    pub fn inject_opportunity_monitor(&mut self, monitor: Arc<OpportunityMonitor>) {
        console_log!("🔧 OPPORTUNITY ENGINE - Injecting opportunity monitor");
        self.opportunity_monitor = Some(monitor);
        console_log!("✅ OPPORTUNITY ENGINE - Opportunity monitor injected successfully");
    }

    /// Get monitored trading pairs for funding rate arbitrage
    pub async fn get_monitored_pairs(&self) -> ArbitrageResult<Vec<String>> {
        // Return the dynamic trading pairs configured for the system
        Ok(self.get_dynamic_trading_pairs())
    }

    /// Get monitored exchanges for funding rate arbitrage
    pub async fn get_monitored_exchanges(&self) -> ArbitrageResult<Vec<ExchangeIdEnum>> {
        // Return all supported exchanges that have funding rate capabilities
        Ok(vec![
            ExchangeIdEnum::Binance,
            ExchangeIdEnum::Bybit,
            ExchangeIdEnum::OKX,
            ExchangeIdEnum::Bitget,
        ])
    }

    /// Clean up expired opportunities from the database and cache
    pub async fn cleanup_expired_opportunities(&self) -> ArbitrageResult<u32> {
        console_log!("🧹 CLEANUP - Starting expired opportunities cleanup...");

        let current_time = Utc::now().timestamp_millis() as u64;
        let mut cleaned_count = 0;

        // Get all opportunities from cache first
        if let Ok(Some(cached_opportunities)) = self
            .cache_manager
            .get::<Vec<GlobalOpportunity>>("global_opportunities")
            .await
        {
            // Filter out expired opportunities
            let valid_opportunities: Vec<GlobalOpportunity> = cached_opportunities
                .into_iter()
                .filter(|opp| {
                    let is_valid = opp.expires_at > current_time;
                    if !is_valid {
                        cleaned_count += 1;
                    }
                    is_valid
                })
                .collect();

            // Update cache with only valid opportunities
            let _ = self
                .cache_manager
                .set("global_opportunities", &valid_opportunities, Some(30))
                .await;

            console_log!(
                "✅ CLEANUP - Removed {} expired opportunities from cache, {} remain valid",
                cleaned_count,
                valid_opportunities.len()
            );
        }

        // TODO: Add database cleanup & backup to R2 when database migration is resolved
        // This would involve deleting/archiving expired opportunities from the database D1 to R2
        // Check Cloudflare Pipelines, Durable Objects, and R2 to keep in sync effective & efficient

        Ok(cleaned_count)
    }

    /// Filter opportunities to remove expired ones and group by symbol to reduce confusion
    pub async fn get_fresh_opportunities_grouped(
        &self,
    ) -> ArbitrageResult<Vec<GroupedOpportunity>> {
        console_log!("📊 GROUPING - Fetching and grouping fresh opportunities...");

        // Get all opportunities
        let all_opportunities = match self.generate_global_opportunities().await {
            Ok(opportunities) => opportunities,
            Err(e) => {
                console_log!("❌ Failed to generate global opportunities: {}", e);
                return Err(ArbitrageError::internal_error(format!(
                    "Failed to generate opportunities: {}",
                    e
                )));
            }
        };
        let current_time = Utc::now().timestamp_millis() as u64;

        // Filter out expired opportunities and convert to GlobalOpportunity
        let fresh_opportunities: Vec<GlobalOpportunity> = all_opportunities
            .into_iter()
            .map(|arb_opp| {
                GlobalOpportunity::from_arbitrage(
                    arb_opp,
                    OpportunitySource::GlobalScanner,
                    current_time + 1800000, // 30 minutes from now
                )
            })
            .filter(|opp| opp.expires_at > current_time)
            .collect();

        console_log!(
            "⏰ FRESHNESS - Found {} fresh opportunities (expired ones filtered out)",
            fresh_opportunities.len()
        );

        // Extract arbitrage opportunities for strategic processing
        let arbitrage_opportunities: Vec<ArbitrageOpportunity> = fresh_opportunities
            .iter()
            .filter_map(|global_opp| {
                if let crate::types::OpportunityData::Arbitrage(arb_opp) =
                    &global_opp.opportunity_data
                {
                    Some(arb_opp.clone())
                } else {
                    None
                }
            })
            .collect();

        // Use strategic enhancement engine for intelligent grouping
        let best_opportunities = self
            .opportunity_scoring_engine
            .get_best_opportunities_by_symbol(&arbitrage_opportunities);

        // Convert ArbitrageOpportunity to GroupedOpportunity
        let grouped_opportunities: Vec<GroupedOpportunity> = best_opportunities
            .into_iter()
            .map(|arb_opp| {
                // Find the corresponding GlobalOpportunity
                let global_opp = fresh_opportunities
                    .iter()
                    .find(|global| {
                        if let crate::types::OpportunityData::Arbitrage(global_arb) =
                            &global.opportunity_data
                        {
                            global_arb.id == arb_opp.id
                        } else {
                            false
                        }
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        // Create a default GlobalOpportunity if not found
                        GlobalOpportunity::from_arbitrage(
                            arb_opp.clone(),
                            OpportunitySource::GlobalScanner,
                            current_time + 1800000, // 30 minutes from now
                        )
                    });

                // Calculate expires_in_minutes
                let expires_in_minutes = if global_opp.expires_at > current_time {
                    ((global_opp.expires_at - current_time) / 60000) as u32
                } else {
                    0
                };

                GroupedOpportunity {
                    symbol: arb_opp.pair.clone(),
                    best_opportunity: global_opp,
                    total_opportunities: 1, // Each grouped opportunity represents the best one for that symbol
                    available_exchanges: vec![
                        arb_opp.long_exchange.as_str().to_string(),
                        arb_opp.short_exchange.as_str().to_string(),
                    ],
                    expires_in_minutes,
                }
            })
            .collect();

        console_log!(
            "📈 GROUPING - Created {} grouped opportunities from {} total opportunities",
            grouped_opportunities.len(),
            arbitrage_opportunities.len()
        );

        Ok(grouped_opportunities)
    }

    /// Integrate funding rate data with existing arbitrage opportunities
    /// This is the PRIMARY opportunity type as per user requirements
    pub async fn integrate_funding_with_arbitrage(
        &self,
        arbitrage_opportunities: &mut [ArbitrageOpportunity],
        funding_opportunities: &[crate::services::core::opportunities::funding_rate_manager::FundingRateArbitrage],
    ) {
        for arb_opp in arbitrage_opportunities.iter_mut() {
            // Find matching funding opportunity by symbol and exchanges
            if let Some(funding_opp) = funding_opportunities.iter().find(|f| {
                f.symbol == arb_opp.pair
                    && ((f.long_exchange == arb_opp.long_exchange.as_str()
                        && f.short_exchange == arb_opp.short_exchange.as_str())
                        || (f.short_exchange == arb_opp.long_exchange.as_str()
                            && f.long_exchange == arb_opp.short_exchange.as_str()))
            }) {
                // Get actual funding rates for each exchange
                let long_exchange_str = arb_opp.long_exchange.as_str();
                let short_exchange_str = arb_opp.short_exchange.as_str();

                let long_funding_rate = self
                    .funding_rate_manager
                    .get_funding_rate_for_exchange(&arb_opp.pair, long_exchange_str)
                    .await
                    .unwrap_or(None)
                    .unwrap_or(0.0);

                let short_funding_rate = self
                    .funding_rate_manager
                    .get_funding_rate_for_exchange(&arb_opp.pair, short_exchange_str)
                    .await
                    .unwrap_or(None)
                    .unwrap_or(0.0);

                // Set the actual funding rates
                arb_opp.long_rate = Some(long_funding_rate);
                arb_opp.short_rate = Some(short_funding_rate);

                // Update profit calculations to include funding rate benefits
                let funding_profit = funding_opp.profit_potential;
                let combined_profit = arb_opp.profit_percentage + funding_profit;

                arb_opp.profit_percentage = combined_profit;
                arb_opp.rate_difference = combined_profit;
                arb_opp.net_rate_difference = Some(combined_profit);
                arb_opp.potential_profit_value = Some(combined_profit * 1000.0); // Assuming $1000 position size

                // Update details with funding information
                let funding_details = format!(
                    "Enhanced with funding rate arbitrage: {:.4}% additional profit. Long {} ({:.4}%), Short {} ({:.4}%). Next funding: {} minutes",
                    funding_profit,
                    long_exchange_str,
                    long_funding_rate * 100.0,
                    short_exchange_str,
                    short_funding_rate * 100.0,
                    (funding_opp.time_to_funding / 60).max(1)
                );

                arb_opp.details = Some(match &arb_opp.details {
                    Some(existing) => format!("{}. {}", existing, funding_details),
                    None => funding_details,
                });

                console_log!(
                    "🔗 ENHANCED OPPORTUNITY - {} with {:.4}% funding rate benefit (Long: {:.4}%, Short: {:.4}%)",
                    arb_opp.pair,
                    funding_profit,
                    long_funding_rate * 100.0,
                    short_funding_rate * 100.0
                );
            }
        }
    }

    /// Populate funding rates for arbitrage opportunities
    pub async fn populate_funding_rates(
        &self,
        arbitrage_opportunities: &mut [ArbitrageOpportunity],
    ) {
        for arb_opp in arbitrage_opportunities.iter_mut() {
            let long_exchange_str = arb_opp.long_exchange.as_str();
            let short_exchange_str = arb_opp.short_exchange.as_str();

            let long_funding_rate = self
                .funding_rate_manager
                .get_funding_rate_for_exchange(&arb_opp.pair, long_exchange_str)
                .await
                .unwrap_or(None)
                .unwrap_or(0.0);

            let short_funding_rate = self
                .funding_rate_manager
                .get_funding_rate_for_exchange(&arb_opp.pair, short_exchange_str)
                .await
                .unwrap_or(None)
                .unwrap_or(0.0);

            arb_opp.long_rate = Some(long_funding_rate);
            arb_opp.short_rate = Some(short_funding_rate);

            console_log!(
                "💰 POPULATED FUNDING RATES - {} Long: {:.4}%, Short: {:.4}%",
                arb_opp.pair,
                long_funding_rate * 100.0,
                short_funding_rate * 100.0
            );
        }
    }

    /// Create new arbitrage opportunities from pure funding rate arbitrage
    pub fn create_arbitrage_from_funding(
        &self,
        funding_opportunities: &[crate::services::core::opportunities::funding_rate_manager::FundingRateArbitrage],
    ) -> Vec<ArbitrageOpportunity> {
        funding_opportunities
            .iter()
            .map(|funding_opp| {
                let timestamp = crate::utils::get_current_timestamp();
                let expires_at = funding_opp.expires_at as u64;

                // Convert exchange strings to ExchangeIdEnum
                let long_exchange = ExchangeIdEnum::from_string(&funding_opp.long_exchange)
                    .unwrap_or(ExchangeIdEnum::Binance);
                let short_exchange = ExchangeIdEnum::from_string(&funding_opp.short_exchange)
                    .unwrap_or(ExchangeIdEnum::Bybit);

                ArbitrageOpportunity {
                    id: funding_opp.id.clone(),
                    trading_pair: funding_opp.symbol.clone(),
                    exchanges: vec![funding_opp.long_exchange.clone(), funding_opp.short_exchange.clone()],
                    profit_percentage: funding_opp.profit_potential,
                    confidence_score: funding_opp.confidence_score as f64,
                    risk_level: "medium".to_string(),
                    buy_exchange: funding_opp.long_exchange.clone(),
                    sell_exchange: funding_opp.short_exchange.clone(),
                    buy_price: 0.0, // Will be filled by market data
                    sell_price: 0.0, // Will be filled by market data
                    volume: 1000.0, // Default volume
                    created_at: timestamp,
                    expires_at: Some(expires_at),
                    // Unified modular fields
                    pair: funding_opp.symbol.clone(),
                    long_exchange,
                    short_exchange,
                    long_rate: None,
                    short_rate: None,
                    rate_difference: funding_opp.rate_difference,
                    net_rate_difference: Some(funding_opp.profit_potential),
                    potential_profit_value: Some(funding_opp.profit_potential * 1000.0),
                    timestamp,
                    detected_at: timestamp,
                    r#type: ArbitrageType::FundingRate,
                    details: Some(format!(
                        "Funding rate arbitrage: {:.4}% rate difference. Long {} (lower rate), Short {} (higher rate). Next funding in {} minutes",
                        funding_opp.rate_difference * 100.0,
                        funding_opp.long_exchange,
                        funding_opp.short_exchange,
                        (funding_opp.time_to_funding / 60).max(1)
                    )),
                    min_exchanges_required: 2,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SubscriptionTier, UserAccessLevel, UserProfile};
    use chrono::Utc;

    fn create_test_user_profile(user_id: &str) -> UserProfile {
        UserProfile {
            user_id: user_id.to_string(),
            telegram_user_id: Some(123456789),
            username: Some("testuser".to_string()),
            email: Some("test@example.com".to_string()),
            subscription_tier: SubscriptionTier::Free,
            access_level: UserAccessLevel::Registered,
            is_active: true,
            created_at: Utc::now().timestamp_millis() as u64,
            last_login: None,
            preferences: crate::types::UserPreferences::default(),
            risk_profile: crate::types::RiskProfile::default(),
            configuration: crate::types::UserConfiguration::default(),
            api_keys: Vec::new(),
            invitation_code: None,
            beta_expires_at: None,
            updated_at: Utc::now().timestamp_millis() as u64,
            last_active: Utc::now().timestamp_millis() as u64, // Corrected: last_active is u64, not Option<u64>
            invitation_code_used: None,
            invited_by: None,
            total_invitations_sent: 0,
            successful_invitations: 0,
            total_trades: 0,
            total_pnl_usdt: 0.0,
            account_balance_usdt: 0.0,
            profile_metadata: None,
            telegram_username: Some("testuser".to_string()), // This was duplicated, user_id is already test_user
            subscription: crate::types::Subscription::default(), // Corrected to use Subscription::default()
            group_admin_roles: Vec::new(),
            is_beta_active: false,
        }
    }

    #[test]
    fn test_opportunity_context_mapping() {
        // Test that different contexts are properly handled
        let personal_context = OpportunityContext::Personal {
            user_id: "test_user".to_string(),
        };
        let group_context = OpportunityContext::Group {
            admin_id: "admin_user".to_string(),
            chat_context: crate::types::ChatContext {
                chat_id: -123456789,
                chat_type: "group".to_string(),
                user_id: Some("test_user".to_string()),
                username: Some("testuser".to_string()),
                is_group: true,
                group_title: Some("Test Group".to_string()),
                message_id: Some(1),
                reply_to_message_id: None,
            },
        };
        let global_context = OpportunityContext::Global { system_level: true };

        assert!(matches!(
            personal_context,
            OpportunityContext::Personal { .. }
        ));
        assert!(matches!(group_context, OpportunityContext::Group { .. }));
        assert!(matches!(global_context, OpportunityContext::Global { .. }));
    }

    #[test]
    fn test_user_profile_structure() {
        let user_profile = create_test_user_profile("test_user");

        assert_eq!(user_profile.user_id, "test_user");
        assert_eq!(user_profile.subscription.tier, SubscriptionTier::Free);
        assert!(user_profile.is_active);
        assert_eq!(user_profile.account_balance_usdt, 0.0);
    }

    #[test]
    fn test_opportunity_config_defaults() {
        // Create test config without calling WASM-specific functions
        let config = OpportunityConfig {
            symbols: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            max_opportunities: 100,
            enable_ai: true,
            enable_caching: true,
            cache_ttl_seconds: 300,
            min_confidence_threshold: 0.7,
            max_risk_level: 0.8,
            default_pairs: vec![
                "BTC/USDT".to_string(),
                "ETH/USDT".to_string(),
                "BNB/USDT".to_string(),
                "SOL/USDT".to_string(),
                "ADA/USDT".to_string(),
            ],
            min_rate_difference: 0.05,
            monitored_exchanges: vec![
                ExchangeIdEnum::Binance,
                ExchangeIdEnum::OKX,
                ExchangeIdEnum::Bybit,
                ExchangeIdEnum::Coinbase,
            ],
            opportunity_ttl_minutes: 15,
            max_participants_per_opportunity: 100,
        };

        assert!(config.min_rate_difference > 0.0);
        assert!(!config.default_pairs.is_empty());
        assert!(!config.monitored_exchanges.is_empty());
        assert!(config.opportunity_ttl_minutes > 0);
        assert!(config.max_participants_per_opportunity > 0);
    }
}
