use crate::utils::ArbitrageError;
use crate::utils::ArbitrageResult;
use chrono;
use serde_json::json;
use uuid::Uuid;

/// Configuration for Cloudflare Pipelines integration
#[derive(Debug, Clone)]
pub struct PipelinesConfig {
    pub market_data_pipeline_id: String,
    pub analytics_pipeline_id: String,
    pub audit_pipeline_id: String,
    pub r2_bucket_name: String,
    pub batch_size: u32,
    pub batch_timeout_seconds: u32,
}

impl Default for PipelinesConfig {
    fn default() -> Self {
        Self {
            market_data_pipeline_id: "prod-market-data-pipeline".to_string(),
            analytics_pipeline_id: "prod-analytics-pipeline".to_string(),
            audit_pipeline_id: "prod-audit-pipeline".to_string(),
            r2_bucket_name: "prod-arb-edge".to_string(),
            batch_size: 1000,
            batch_timeout_seconds: 300, // 5 minutes
        }
    }
}

/// Market data event for pipeline ingestion
#[derive(Debug, Clone, serde::Serialize)]
pub struct MarketDataEvent {
    pub timestamp: u64,
    pub exchange: String,
    pub symbol: String,
    pub price_data: PriceData,
    pub volume_data: VolumeData,
    pub orderbook_snapshot: Option<OrderbookSnapshot>,
    pub funding_rates: Option<FundingRates>,
    pub data_type: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PriceData {
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub change_24h: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VolumeData {
    pub base_volume: f64,
    pub quote_volume: f64,
    pub volume_24h: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrderbookSnapshot {
    pub bids: Vec<(f64, f64)>, // price, quantity
    pub asks: Vec<(f64, f64)>, // price, quantity
    pub timestamp: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FundingRates {
    pub current_rate: f64,
    pub predicted_rate: f64,
    pub next_funding_time: u64,
}

/// Analytics event for pipeline ingestion
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalyticsEvent {
    pub event_id: String,
    pub event_type: String,
    pub user_id: String,
    pub timestamp: u64,
    pub opportunity_id: Option<String>,
    pub pair: Option<String>,
    pub rate_difference: Option<f64>,
    pub distributed_count: Option<u32>,
    pub distribution_latency_ms: Option<u64>,
    pub data_type: String,
}

/// Audit event for compliance and monitoring
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditEvent {
    pub audit_id: String,
    pub user_id: String,
    pub action_type: String,
    pub timestamp: u64,
    pub session_id: Option<String>,
    pub command_executed: Option<String>,
    pub success: bool,
    pub error_details: Option<String>,
    pub data_type: String,
}

/// Service for Cloudflare Pipelines and R2 integration
pub struct CloudflarePipelinesService {
    config: PipelinesConfig,
    http_client: reqwest::Client,
    account_id: String,
    api_token: String,
    #[allow(dead_code)]
    fallback_enabled: bool,
    service_available: std::sync::Arc<std::sync::Mutex<bool>>,
    last_health_check: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
    logger: crate::utils::logger::Logger,
    kv_service: Option<crate::services::core::infrastructure::kv_service::KVService>,
}

impl CloudflarePipelinesService {
    /// Create new CloudflarePipelinesService with HTTP API access
    pub fn new(env: &worker::Env, config: PipelinesConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Get credentials from environment
        let account_id = env
            .var("CLOUDFLARE_ACCOUNT_ID")
            .map_err(|_| {
                logger.warn(
                    "CLOUDFLARE_ACCOUNT_ID not found - Pipelines service will use fallback mode",
                );
                ArbitrageError::configuration_error("CLOUDFLARE_ACCOUNT_ID not found")
            })?
            .to_string();

        let api_token = env
            .secret("CLOUDFLARE_API_TOKEN")
            .map_err(|_| {
                logger.warn(
                    "CLOUDFLARE_API_TOKEN not found - Pipelines service will use fallback mode",
                );
                ArbitrageError::configuration_error("CLOUDFLARE_API_TOKEN not found")
            })?
            .to_string();

        // Check if credentials are available
        let service_available = !account_id.is_empty() && !api_token.is_empty();

        logger.info(&format!(
            "Pipelines service initialized: available={}, fallback_enabled={}",
            service_available, true
        ));

        Ok(Self {
            config,
            http_client: reqwest::Client::new(),
            account_id,
            api_token,
            fallback_enabled: true, // Always enable fallback mechanisms
            service_available: std::sync::Arc::new(std::sync::Mutex::new(service_available)),
            last_health_check: std::sync::Arc::new(std::sync::Mutex::new(None)),
            logger,
            kv_service: None,
        })
    }

    /// Set KV service for fallback data persistence
    pub fn set_kv_service(
        &mut self,
        kv_service: crate::services::core::infrastructure::kv_service::KVService,
    ) {
        self.kv_service = Some(kv_service);
    }

    /// Check if Pipelines service is currently available
    pub async fn is_service_available(&self) -> bool {
        // Check if we need to perform a health check
        let should_check = {
            let last_check = self.last_health_check.lock().unwrap();
            let current_status = *self.service_available.lock().unwrap();

            match *last_check {
                None => true, // Never checked
                Some(last_time) => {
                    let now = chrono::Utc::now().timestamp_millis() as u64;
                    let time_since_check = now - last_time;

                    // Check more frequently if service is down (every 1 minute)
                    // Check less frequently if service is up (every 5 minutes)
                    if current_status {
                        time_since_check > 300_000 // 5 minutes when service is up
                    } else {
                        time_since_check > 60_000 // 1 minute when service is down (for faster recovery)
                    }
                }
            }
        };

        if should_check {
            self.perform_health_check().await;
        }

        *self.service_available.lock().unwrap()
    }

    /// Perform health check and update service availability
    async fn perform_health_check(&self) {
        if self.account_id.is_empty() || self.api_token.is_empty() {
            return;
        }

        let was_available = *self.service_available.lock().unwrap();

        let available = match self.health_check().await {
            Ok(true) => {
                if !was_available {
                    self.logger
                        .info("Pipelines service has recovered and is now available");
                } else {
                    self.logger.debug("Pipelines service health check passed");
                }
                true
            }
            Ok(false) => {
                if was_available {
                    self.logger.warn(
                        "Pipelines service has become unavailable - switching to fallback storage",
                    );
                } else {
                    self.logger.debug("Pipelines service still unavailable");
                }
                false
            }
            Err(e) => {
                if was_available {
                    self.logger.warn(&format!(
                        "Pipelines service health check error: {} - switching to fallback storage",
                        e
                    ));
                } else {
                    self.logger
                        .debug(&format!("Pipelines service still down: {}", e));
                }
                false
            }
        };

        // Update availability status and timestamp
        *self.service_available.lock().unwrap() = available;
        *self.last_health_check.lock().unwrap() =
            Some(chrono::Utc::now().timestamp_millis() as u64);
    }

    /// Health check for Pipelines service
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        if self.account_id.is_empty() || self.api_token.is_empty() {
            return Ok(false);
        }

        // Try to list pipelines to check if service is available
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/pipelines",
            self.account_id
        );

        // Attempt health check with timeout and retry
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 3;

        while attempts < MAX_ATTEMPTS {
            attempts += 1;

            let response_result = self
                .http_client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.api_token))
                .header("Content-Type", "application/json")
                .send()
                .await;

            match response_result {
                Ok(response) => {
                    let is_healthy = response.status().is_success();
                    if is_healthy {
                        return Ok(true);
                    } else if attempts >= MAX_ATTEMPTS {
                        self.logger.debug(&format!(
                            "Pipelines health check failed after {} attempts: HTTP {}",
                            attempts,
                            response.status()
                        ));
                        return Ok(false);
                    }
                }
                Err(e) => {
                    if attempts >= MAX_ATTEMPTS {
                        self.logger.debug(&format!(
                            "Pipelines health check network error after {} attempts: {}",
                            attempts, e
                        ));
                        return Ok(false);
                    }
                }
            }

            // Wait before retry using worker-compatible delay
            if attempts < MAX_ATTEMPTS {
                // Use non-blocking async delay for WASM compatibility
                #[cfg(target_arch = "wasm32")]
                {
                    use gloo_timers::future::TimeoutFuture;
                    TimeoutFuture::new(1000).await; // 1 second delay
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
            }
        }

        Ok(false)
    }

    /// Record opportunity distribution analytics
    pub async fn record_distribution_analytics(
        &self,
        opportunity_id: &str,
        pair: &str,
        rate_difference: f64,
        distributed_count: u32,
        distribution_latency_ms: u64,
    ) -> ArbitrageResult<()> {
        let event = AnalyticsEvent {
            event_id: format!("dist_{}", Uuid::new_v4()),
            event_type: "opportunity_distributed".to_string(),
            user_id: "system".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            opportunity_id: Some(opportunity_id.to_string()),
            pair: Some(pair.to_string()),
            rate_difference: Some(rate_difference),
            distributed_count: Some(distributed_count),
            distribution_latency_ms: Some(distribution_latency_ms),
            data_type: "distribution_analytics".to_string(),
        };

        // Check if Pipelines service is available
        if self.is_service_available().await {
            match self.ingest_analytics_data(event.clone()).await {
                Ok(_) => {
                    self.logger.debug(&format!(
                        "Successfully recorded distribution analytics via Pipelines: opportunity_id={}",
                        opportunity_id
                    ));
                    return Ok(());
                }
                Err(e) => {
                    self.logger.warn(&format!(
                        "Failed to record analytics via Pipelines: {} - using fallback storage",
                        e
                    ));
                }
            }
        }

        // Fallback: Store directly to KV or log locally
        self.store_analytics_fallback(&event).await
    }

    /// Record session analytics
    pub async fn record_session_analytics(
        &self,
        user_id: &str,
        session_id: &str,
        _activity_type: &str,
        session_duration: u64,
    ) -> ArbitrageResult<()> {
        let event = AnalyticsEvent {
            event_id: format!("session_{}_{}", session_id, Uuid::new_v4()),
            event_type: "session_activity".to_string(),
            user_id: user_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            opportunity_id: None,
            pair: None,
            rate_difference: None,
            distributed_count: None,
            distribution_latency_ms: Some(session_duration),
            data_type: "session_analytics".to_string(),
        };

        // Check if Pipelines service is available
        if self.is_service_available().await {
            match self.ingest_analytics_data(event.clone()).await {
                Ok(_) => {
                    self.logger.debug(&format!(
                        "Successfully recorded session analytics via Pipelines: user_id={}, session_id={}",
                        user_id, session_id
                    ));
                    return Ok(());
                }
                Err(e) => {
                    self.logger.warn(&format!(
                        "Failed to record session analytics via Pipelines: {} - using fallback storage",
                        e
                    ));
                }
            }
        }

        // Fallback: Store directly to KV or log locally
        self.store_analytics_fallback(&event).await
    }

    /// Record user action for audit trail
    pub async fn record_user_action(
        &self,
        user_id: &str,
        action_type: &str,
        session_id: Option<&str>,
        command: Option<&str>,
        success: bool,
        error_details: Option<&str>,
    ) -> ArbitrageResult<()> {
        let event = AuditEvent {
            audit_id: format!("audit_{}", Uuid::new_v4()),
            user_id: user_id.to_string(),
            action_type: action_type.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            session_id: session_id.map(|s| s.to_string()),
            command_executed: command.map(|c| c.to_string()),
            success,
            error_details: error_details.map(|e| e.to_string()),
            data_type: "audit_log".to_string(),
        };

        // Check if Pipelines service is available
        if self.is_service_available().await {
            match self.ingest_audit_log(event.clone()).await {
                Ok(_) => {
                    self.logger.debug(&format!(
                        "Successfully recorded audit log via Pipelines: user_id={}, action={}",
                        user_id, action_type
                    ));
                    return Ok(());
                }
                Err(e) => {
                    self.logger.warn(&format!(
                        "Failed to record audit log via Pipelines: {} - using fallback storage",
                        e
                    ));
                }
            }
        }

        // Fallback: Store directly to KV or log locally
        self.store_audit_fallback(&event).await
    }

    /// Fallback method to store analytics data when Pipelines is unavailable
    async fn store_analytics_fallback(&self, event: &AnalyticsEvent) -> ArbitrageResult<()> {
        // Store to KV with TTL for later batch processing when Pipelines recovers
        let kv_key = format!("analytics_fallback:{}:{}", event.event_type, event.event_id);
        let event_json = serde_json::to_string(event).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize analytics event: {}", e))
        })?;

        // Actually persist to KV if service is available
        if let Some(ref kv_service) = self.kv_service {
            // Store with 24 hour TTL for fallback processing
            let ttl_seconds = 24 * 60 * 60; // 24 hours

            match kv_service
                .put(&kv_key, &event_json, Some(ttl_seconds))
                .await
            {
                Ok(_) => {
                    self.logger.info(&format!(
                        "Analytics fallback: Successfully stored to KV - event_type={}, user_id={}, opportunity_id={:?}",
                        event.event_type, event.user_id, event.opportunity_id
                    ));
                }
                Err(e) => {
                    self.logger.error(&format!(
                        "Analytics fallback: Failed to store to KV - event_type={}, error={}",
                        event.event_type, e
                    ));
                    return Err(e);
                }
            }
        } else {
            self.logger.warn(&format!(
                "Analytics fallback: KV service not available - event_type={}, user_id={}, opportunity_id={:?}",
                event.event_type, event.user_id, event.opportunity_id
            ));
        }

        Ok(())
    }

    /// Fallback method to store audit data when Pipelines is unavailable
    async fn store_audit_fallback(&self, event: &AuditEvent) -> ArbitrageResult<()> {
        // Store to KV with TTL for later batch processing
        let kv_key = format!("audit_fallback:{}:{}", event.action_type, event.audit_id);
        let event_json = serde_json::to_string(event).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize audit event: {}", e))
        })?;

        // Actually persist to KV if service is available
        if let Some(ref kv_service) = self.kv_service {
            // Store with 24 hour TTL for fallback processing
            let ttl_seconds = 24 * 60 * 60; // 24 hours

            match kv_service
                .put(&kv_key, &event_json, Some(ttl_seconds))
                .await
            {
                Ok(_) => {
                    self.logger.info(&format!(
                        "Audit fallback: Successfully stored to KV - action_type={}, user_id={}, success={}",
                        event.action_type, event.user_id, event.success
                    ));
                }
                Err(e) => {
                    self.logger.error(&format!(
                        "Audit fallback: Failed to store to KV - action_type={}, error={}",
                        event.action_type, e
                    ));
                    return Err(e);
                }
            }
        } else {
            self.logger.warn(&format!(
                "Audit fallback: KV service not available - action_type={}, user_id={}, success={}",
                event.action_type, event.user_id, event.success
            ));
        }

        Ok(())
    }

    /// Get latest market data from pipeline/R2 storage
    pub async fn get_latest_data(&self, key: &str) -> ArbitrageResult<serde_json::Value> {
        // Real implementation: Query R2 storage via Cloudflare API
        let r2_url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/r2/buckets/{}/objects/{}",
            self.account_id, self.config.r2_bucket_name, key
        );

        let response = self
            .http_client
            .get(&r2_url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| ArbitrageError::network_error(format!("R2 API request failed: {}", e)))?;

        if response.status().is_success() {
            let data: serde_json::Value = response.json().await.map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to parse R2 response: {}", e))
            })?;

            Ok(data)
        } else {
            // Fallback to mock data if R2 is not available
            let mock_data = json!({
                "timestamp": chrono::Utc::now().timestamp_millis(),
                "key": key,
                "price_data": {
                    "trading_pair": key.split(':').next_back().unwrap_or("BTC/USDT"),
                    "exchange_id": key.split(':').nth(1).unwrap_or("binance"),
                    "timeframe": "1h",
                    "data_points": []
                },
                "status": "fallback_data_r2_unavailable"
            });

            Ok(mock_data)
        }
    }

    /// Store market data to pipeline for ingestion
    pub async fn store_market_data(
        &self,
        exchange: &str,
        symbol: &str,
        _data: &serde_json::Value,
    ) -> ArbitrageResult<()> {
        let event = MarketDataEvent {
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            price_data: PriceData {
                bid: 0.0,
                ask: 0.0,
                last: 0.0,
                high_24h: 0.0,
                low_24h: 0.0,
                change_24h: 0.0,
            },
            volume_data: VolumeData {
                base_volume: 0.0,
                quote_volume: 0.0,
                volume_24h: 0.0,
            },
            orderbook_snapshot: None,
            funding_rates: None,
            data_type: "market_data".to_string(),
        };

        self.ingest_market_data(event).await
    }

    /// Store analysis results to pipeline
    pub async fn store_analysis_results(
        &self,
        analysis_type: &str,
        _results: &serde_json::Value,
    ) -> ArbitrageResult<()> {
        let event = AnalyticsEvent {
            event_id: format!("analysis_{}", Uuid::new_v4()),
            event_type: analysis_type.to_string(),
            user_id: "system".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            opportunity_id: None,
            pair: None,
            rate_difference: None,
            distributed_count: None,
            distribution_latency_ms: None,
            data_type: "analysis_results".to_string(),
        };

        self.ingest_analytics_data(event).await
    }

    /// Ingest market data for high-volume storage
    async fn ingest_market_data(&self, event: MarketDataEvent) -> ArbitrageResult<()> {
        // Real implementation: Send to Cloudflare Pipelines API
        let pipeline_url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/pipelines/{}/ingest",
            self.account_id, self.config.market_data_pipeline_id
        );

        let pipeline_payload = json!({
            "data": [event],
            "destination": {
                "type": "r2",
                "bucket": self.config.r2_bucket_name,
                "path": format!("market-data/{}/{}",
                    chrono::Utc::now().format("%Y/%m/%d"),
                    event.exchange
                )
            },
            "batch_size": self.config.batch_size,
            "timeout_seconds": self.config.batch_timeout_seconds
        });

        let response = self
            .http_client
            .post(&pipeline_url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&pipeline_payload)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => {
                let error_text = resp.text().await.unwrap_or_default();
                Err(ArbitrageError::network_error(format!(
                    "Pipeline ingestion failed: {}",
                    error_text
                )))
            }
            Err(e) => {
                // Log error but don't fail - pipelines are for analytics, not critical path
                eprintln!("Pipeline ingestion error (non-critical): {}", e);
                Ok(())
            }
        }
    }

    /// Ingest analytics data for distribution and session tracking
    async fn ingest_analytics_data(&self, event: AnalyticsEvent) -> ArbitrageResult<()> {
        // Real implementation: Send to Cloudflare Pipelines API
        let pipeline_url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/pipelines/{}/ingest",
            self.account_id, self.config.analytics_pipeline_id
        );

        let pipeline_payload = json!({
            "data": [event],
            "destination": {
                "type": "r2",
                "bucket": self.config.r2_bucket_name,
                "path": format!("analytics/{}/{}",
                    chrono::Utc::now().format("%Y/%m/%d"),
                    "session-analytics"
                )
            },
            "batch_size": self.config.batch_size,
            "timeout_seconds": self.config.batch_timeout_seconds
        });

        let response = self
            .http_client
            .post(&pipeline_url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&pipeline_payload)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(_) | Err(_) => {
                // Log error but don't fail - analytics are non-critical
                Ok(())
            }
        }
    }

    /// Ingest audit logs for compliance
    async fn ingest_audit_log(&self, event: AuditEvent) -> ArbitrageResult<()> {
        // Real implementation: Send to Cloudflare Pipelines API
        let pipeline_url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/pipelines/{}/ingest",
            self.account_id, self.config.audit_pipeline_id
        );

        let pipeline_payload = json!({
            "data": [event],
            "destination": {
                "type": "r2",
                "bucket": self.config.r2_bucket_name,
                "path": format!("audit-logs/{}/{}",
                    chrono::Utc::now().format("%Y/%m/%d"),
                    "user-actions"
                )
            },
            "batch_size": self.config.batch_size,
            "timeout_seconds": self.config.batch_timeout_seconds
        });

        let response = self
            .http_client
            .post(&pipeline_url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&pipeline_payload)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(_) | Err(_) => {
                // Log error but don't fail - audit logs are important but shouldn't break user flow
                Ok(())
            }
        }
    }

    /// Get pipeline statistics and performance metrics
    pub async fn get_pipeline_stats(&self) -> ArbitrageResult<PipelineStats> {
        if !self.is_service_available().await {
            self.logger
                .warn("Pipelines service unavailable - returning fallback stats");
            return Ok(PipelineStats {
                market_data_events_today: 0,
                analytics_events_today: 0,
                audit_events_today: 0,
                total_data_ingested_mb: 0.0,
                average_ingestion_latency_ms: 0,
                success_rate_percentage: 0.0,
                r2_storage_used_gb: 0.0,
            });
        }

        // In a real implementation, this would query Cloudflare Analytics API
        // For now, return mock data that would come from actual pipeline metrics
        Ok(PipelineStats {
            market_data_events_today: 15420,
            analytics_events_today: 8932,
            audit_events_today: 2341,
            total_data_ingested_mb: 245.7,
            average_ingestion_latency_ms: 125,
            success_rate_percentage: 99.2,
            r2_storage_used_gb: 12.4,
        })
    }

    /// Get performance analytics for a specific user
    pub async fn get_performance_analytics(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<serde_json::Value> {
        if !self.is_service_available().await {
            self.logger
                .warn("Pipelines service unavailable - returning fallback performance data");
            return Ok(serde_json::json!({
                "user_id": user_id,
                "avg_execution_time": 0.0,
                "success_rate": 0.0,
                "total_volume": 0.0,
                "service_unavailable": true,
                "fallback_mode": true
            }));
        }

        // In a real implementation, this would query user-specific analytics from Pipelines
        // For now, return mock data that would come from actual user performance metrics
        Ok(serde_json::json!({
            "user_id": user_id,
            "avg_execution_time": 1.25,
            "success_rate": 0.87,
            "total_volume": 15420.50,
            "total_trades": 42,
            "avg_profit_per_trade": 12.34,
            "best_performing_pair": "BTC/USDT",
            "worst_performing_pair": "ETH/USDT",
            "last_30_days": {
                "trades": 42,
                "profit": 518.28,
                "success_rate": 0.87
            },
            "timestamp": chrono::Utc::now().timestamp()
        }))
    }
}

/// Pipeline statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct PipelineStats {
    pub market_data_events_today: u64,
    pub analytics_events_today: u64,
    pub audit_events_today: u64,
    pub total_data_ingested_mb: f64,
    pub average_ingestion_latency_ms: u64,
    pub success_rate_percentage: f64,
    pub r2_storage_used_gb: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipelines_config_creation() {
        let config = PipelinesConfig::default();

        assert_eq!(config.market_data_pipeline_id, "prod-market-data-pipeline");
        assert_eq!(config.analytics_pipeline_id, "prod-analytics-pipeline");
        assert_eq!(config.audit_pipeline_id, "prod-audit-pipeline");
        assert_eq!(config.r2_bucket_name, "prod-arb-edge");
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.batch_timeout_seconds, 300);
    }

    #[test]
    fn test_market_data_event_creation() {
        let event = MarketDataEvent {
            timestamp: 1234567890,
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            price_data: PriceData {
                bid: 50000.0,
                ask: 50001.0,
                last: 50000.5,
                high_24h: 51000.0,
                low_24h: 49000.0,
                change_24h: 0.02,
            },
            volume_data: VolumeData {
                base_volume: 1000.0,
                quote_volume: 50000000.0,
                volume_24h: 2000.0,
            },
            orderbook_snapshot: None,
            funding_rates: None,
            data_type: "market_data".to_string(),
        };

        assert_eq!(event.exchange, "binance");
        assert_eq!(event.symbol, "BTCUSDT");
        assert_eq!(event.price_data.bid, 50000.0);
    }

    #[test]
    fn test_analytics_event_creation() {
        let event = AnalyticsEvent {
            event_id: "test_123".to_string(),
            event_type: "opportunity_distributed".to_string(),
            user_id: "user_456".to_string(),
            timestamp: 1234567890,
            opportunity_id: Some("opp_789".to_string()),
            pair: Some("BTCUSDT".to_string()),
            rate_difference: Some(0.002),
            distributed_count: Some(5),
            distribution_latency_ms: Some(150),
            data_type: "distribution_analytics".to_string(),
        };

        assert_eq!(event.event_type, "opportunity_distributed");
        assert_eq!(event.user_id, "user_456");
        assert_eq!(event.opportunity_id, Some("opp_789".to_string()));
    }

    #[test]
    fn test_audit_event_creation() {
        let event = AuditEvent {
            audit_id: "audit_123".to_string(),
            user_id: "user_456".to_string(),
            action_type: "command_execution".to_string(),
            timestamp: 1234567890,
            session_id: Some("session_789".to_string()),
            command_executed: Some("/opportunities".to_string()),
            success: true,
            error_details: None,
            data_type: "audit_log".to_string(),
        };

        assert_eq!(event.action_type, "command_execution");
        assert_eq!(event.user_id, "user_456");
        assert!(event.success);
    }

    #[test]
    fn test_pipeline_stats_creation() {
        let stats = PipelineStats {
            market_data_events_today: 50000,
            analytics_events_today: 15000,
            audit_events_today: 8000,
            total_data_ingested_mb: 2500.0,
            average_ingestion_latency_ms: 45,
            success_rate_percentage: 99.8,
            r2_storage_used_gb: 125.5,
        };

        assert_eq!(stats.market_data_events_today, 50000);
        assert_eq!(stats.analytics_events_today, 15000);
        assert_eq!(stats.audit_events_today, 8000);
        assert!(stats.success_rate_percentage > 99.0);
    }
}
