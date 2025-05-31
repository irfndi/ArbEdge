// Data Coordinator - Main Orchestrator for Data Access Operations
// Coordinates all data access components and provides unified interface

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::{
    api_connector::{APIConnector, APIConnectorConfig, APIRequest, ExchangeType},
    cache_layer::{CacheEntryType, CacheLayer, CacheLayerConfig},
    data_source_manager::{DataSourceManager, DataSourceManagerConfig},
    data_validator::{DataValidator, DataValidatorConfig, ValidationResult},
};

/// Data access request with routing information
#[derive(Debug, Clone)]
pub struct DataAccessRequest {
    pub request_id: String,
    pub data_type: String,
    pub key: String,
    pub source_preference: Vec<DataSourceType>,
    pub cache_strategy: CacheStrategy,
    pub validation_required: bool,
    pub freshness_required: bool,
    pub timeout_ms: Option<u64>,
    pub priority: u8, // 1 = high, 2 = medium, 3 = low
    pub metadata: HashMap<String, String>,
}

/// Data source types for routing
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceType {
    KvStore,
    D1Database,
    ExternalAPI,
    Cache,
    Pipeline,
    Queue,
    LocalFallback,
}

/// Cache strategies for data access
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStrategy {
    CacheFirst,  // Try cache first, fallback to source
    SourceFirst, // Try source first, cache result
    CacheOnly,   // Only use cache
    SourceOnly,  // Only use source, no caching
    Refresh,     // Force refresh from source
}

/// Data access response with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAccessResponse {
    pub request_id: String,
    pub data: Option<serde_json::Value>,
    pub source_used: DataSourceType,
    pub cache_hit: bool,
    pub validation_result: Option<ValidationResult>,
    pub latency_ms: u64,
    pub timestamp: u64,
    pub metadata: HashMap<String, String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Coordination metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub validation_passes: u64,
    pub validation_failures: u64,
    pub source_usage: HashMap<DataSourceType, u64>,
    pub average_latency_ms: f64,
    pub error_rates_by_source: HashMap<DataSourceType, f32>,
    pub last_updated: u64,
}

impl Default for CoordinationMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            cache_hits: 0,
            cache_misses: 0,
            validation_passes: 0,
            validation_failures: 0,
            source_usage: HashMap::new(),
            average_latency_ms: 0.0,
            error_rates_by_source: HashMap::new(),
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for the data coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataCoordinatorConfig {
    pub enable_coordination: bool,
    pub enable_intelligent_routing: bool,
    pub enable_fallback_chain: bool,
    pub enable_caching: bool,
    pub default_timeout_seconds: u64,
    pub max_concurrent_operations: usize,
    pub operation_retry_attempts: u32,
    pub operation_retry_delay_ms: u64,
    pub enable_kv_storage: bool,
    pub kv_key_prefix: String,
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
    pub enable_metrics_collection: bool,
    pub metrics_retention_hours: u32,
    pub enable_health_monitoring: bool,
    pub health_check_interval_seconds: u64,
    pub enable_performance_optimization: bool,
    pub performance_threshold_ms: u64,
    pub enable_data_validation: bool,
    pub validation_rules: Vec<String>,
    pub enable_audit_logging: bool,
    pub audit_log_retention_days: u32,
}

impl Default for DataCoordinatorConfig {
    fn default() -> Self {
        Self {
            enable_coordination: true,
            enable_intelligent_routing: true,
            enable_fallback_chain: true,
            enable_caching: true,
            default_timeout_seconds: 30, // 30 seconds
            max_concurrent_operations: 1000,
            operation_retry_attempts: 3,
            operation_retry_delay_ms: 500, // 500ms
            enable_kv_storage: true,
            kv_key_prefix: "data_coordinator_".to_string(),
            enable_compression: true,
            compression_threshold_bytes: 1024, // 1KB
            enable_metrics_collection: true,
            metrics_retention_hours: 24,
            enable_health_monitoring: true,
            health_check_interval_seconds: 300, // 5 minutes
            enable_performance_optimization: true,
            performance_threshold_ms: 1000, // 1 second
            enable_data_validation: true,
            validation_rules: Vec::new(),
            enable_audit_logging: true,
            audit_log_retention_days: 7,
        }
    }
}

impl DataCoordinatorConfig {
    /// Create configuration optimized for high throughput
    pub fn high_throughput() -> Self {
        Self {
            max_concurrent_operations: 2000,
            operation_retry_attempts: 5,
            operation_retry_delay_ms: 250, // 250ms
            default_timeout_seconds: 15,   // 15 seconds
            enable_caching: true,
            enable_intelligent_routing: true,
            enable_performance_optimization: true,
            enable_data_validation: true,
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            enable_fallback_chain: true,
            enable_intelligent_routing: true,
            enable_caching: true,
            enable_data_validation: true,
            enable_performance_optimization: true,
            enable_health_monitoring: true,
            health_check_interval_seconds: 60, // 1 minute
            default_timeout_seconds: 60,       // 60 seconds
            max_concurrent_operations: 500,
            operation_retry_attempts: 5,
            operation_retry_delay_ms: 250, // 250ms
            enable_coordination: true,
            enable_kv_storage: true,
            kv_key_prefix: "data_coordinator_".to_string(),
            enable_compression: true,
            compression_threshold_bytes: 1024, // 1KB
            enable_metrics_collection: true,
            metrics_retention_hours: 24,
            performance_threshold_ms: 1000, // 1 second
            validation_rules: Vec::new(),
            enable_audit_logging: true,
            audit_log_retention_days: 7,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.default_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "default_timeout_seconds must be greater than 0",
            ));
        }
        if self.max_concurrent_operations == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_operations must be greater than 0",
            ));
        }
        if self.operation_retry_delay_ms == 0 {
            return Err(ArbitrageError::validation_error(
                "operation_retry_delay_ms must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Main data coordinator orchestrating all data access operations
pub struct DataCoordinator {
    config: DataCoordinatorConfig,
    logger: crate::utils::logger::Logger,

    // Component instances
    data_source_manager: Arc<DataSourceManager>,
    cache_layer: Arc<CacheLayer>,
    api_connector: Arc<APIConnector>,
    data_validator: Arc<DataValidator>,

    // Coordination state
    metrics: Arc<std::sync::Mutex<CoordinationMetrics>>,
    active_requests: Arc<std::sync::Mutex<HashMap<String, u64>>>, // request_id -> start_time
}

impl DataCoordinator {
    /// Create new DataCoordinator instance
    pub async fn new(
        config: DataCoordinatorConfig,
        data_source_config: DataSourceManagerConfig,
        cache_config: CacheLayerConfig,
        api_config: APIConnectorConfig,
        validator_config: DataValidatorConfig,
        kv_store: worker::kv::KvStore,
    ) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize components
        let data_source_manager = Arc::new(DataSourceManager::new(
            kv_store.clone(),
            data_source_config,
        )?);
        let cache_layer = Arc::new(CacheLayer::new(kv_store, cache_config)?);
        let api_connector = Arc::new(APIConnector::new(api_config)?);
        let data_validator = Arc::new(DataValidator::new(validator_config)?);

        let coordinator = Self {
            config,
            logger,
            data_source_manager,
            cache_layer,
            api_connector,
            data_validator,
            metrics: Arc::new(std::sync::Mutex::new(CoordinationMetrics::default())),
            active_requests: Arc::new(std::sync::Mutex::new(HashMap::new())),
        };

        coordinator.logger.info(&format!(
            "DataCoordinator initialized: intelligent_routing={}, max_concurrent={}, operation_retry_delay_ms={}",
            coordinator.config.enable_intelligent_routing,
            coordinator.config.max_concurrent_operations,
            coordinator.config.operation_retry_delay_ms
        ));

        Ok(coordinator)
    }

    /// Process data access request with intelligent routing
    pub async fn process_request(
        &self,
        request: DataAccessRequest,
    ) -> ArbitrageResult<DataAccessResponse> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check concurrent request limit
        if !self
            .check_concurrent_limit(&request.request_id, start_time)
            .await
        {
            return Err(ArbitrageError::parse_error(
                "Maximum concurrent requests reached",
            ));
        }

        // Check for request deduplication
        if self.config.enable_coordination {
            if let Some(existing_response) = self.check_duplicate_request(&request).await {
                self.cleanup_request(&request.request_id).await;
                return Ok(existing_response);
            }
        }

        let mut response = DataAccessResponse {
            request_id: request.request_id.clone(),
            data: None,
            source_used: DataSourceType::Cache, // Default
            cache_hit: false,
            validation_result: None,
            latency_ms: 0,
            timestamp: start_time,
            metadata: request.metadata.clone(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // Determine optimal data source routing
        let routing_plan = if self.config.enable_intelligent_routing {
            self.create_intelligent_routing_plan(&request).await
        } else {
            request.source_preference.clone()
        };

        // Execute data access with fallback strategy
        let mut last_error = None;
        for source_type in routing_plan {
            match self.access_data_source(&request, &source_type).await {
                Ok(data) => {
                    response.data = Some(data);
                    response.source_used = source_type.clone();

                    // Handle caching based on strategy
                    if self.should_cache(&request, &source_type) {
                        if let Err(e) = self
                            .cache_data(&request, response.data.as_ref().unwrap())
                            .await
                        {
                            response
                                .warnings
                                .push(format!("Failed to cache data: {}", e));
                        }
                    }

                    break;
                }
                Err(e) => {
                    last_error = Some(e.clone());
                    response.errors.push(format!(
                        "Failed to access {}: {}",
                        source_type_to_string(&source_type),
                        e
                    ));

                    if !self.config.enable_fallback_chain {
                        break;
                    }
                }
            }
        }

        // Validate data if required and data was retrieved
        if request.validation_required && response.data.is_some() {
            match self
                .validate_data(&request, response.data.as_ref().unwrap())
                .await
            {
                Ok(validation_result) => {
                    response.validation_result = Some(validation_result.clone());
                    if !validation_result.is_valid {
                        response.warnings.push("Data validation failed".to_string());
                    }
                    if request.freshness_required && !validation_result.is_fresh {
                        response
                            .warnings
                            .push("Data freshness check failed".to_string());
                    }
                }
                Err(e) => {
                    response
                        .warnings
                        .push(format!("Data validation error: {}", e));
                }
            }
        }

        // Calculate final response metrics
        response.latency_ms = chrono::Utc::now().timestamp_millis() as u64 - start_time;

        // Record metrics
        self.record_request_metrics(&request, &response, start_time)
            .await;

        // Cleanup request tracking
        self.cleanup_request(&request.request_id).await;

        // Return error if no data was retrieved
        if response.data.is_none() {
            return Err(last_error
                .unwrap_or_else(|| ArbitrageError::parse_error("No data sources available")));
        }

        Ok(response)
    }

    /// Create intelligent routing plan based on performance metrics
    async fn create_intelligent_routing_plan(
        &self,
        request: &DataAccessRequest,
    ) -> Vec<DataSourceType> {
        let mut plan = Vec::new();

        // Start with cache if cache strategy allows
        match request.cache_strategy {
            CacheStrategy::CacheFirst | CacheStrategy::CacheOnly => {
                plan.push(DataSourceType::Cache);
                if request.cache_strategy == CacheStrategy::CacheOnly {
                    return plan;
                }
            }
            _ => {}
        }

        // Add other sources based on preference and performance
        for source in &request.source_preference {
            if *source != DataSourceType::Cache && !plan.contains(source) {
                plan.push(source.clone());
            }
        }

        // Add remaining sources as fallback if not already included
        let all_sources = vec![
            DataSourceType::KvStore,
            DataSourceType::D1Database,
            DataSourceType::ExternalAPI,
            DataSourceType::Pipeline,
            DataSourceType::Queue,
            DataSourceType::LocalFallback,
        ];
        for source in all_sources {
            if !plan.contains(&source) {
                plan.push(source);
            }
        }

        plan
    }

    /// Access data from specific source type
    async fn access_data_source(
        &self,
        request: &DataAccessRequest,
        source_type: &DataSourceType,
    ) -> ArbitrageResult<serde_json::Value> {
        match source_type {
            DataSourceType::Cache => {
                let cache_type = self.determine_cache_type(&request.data_type);
                match self
                    .cache_layer
                    .get::<serde_json::Value>(&request.key, cache_type)
                    .await?
                {
                    Some(data) => Ok(data),
                    None => Err(ArbitrageError::not_found("Data not found in cache")),
                }
            }
            DataSourceType::KvStore => {
                // Use data source manager for KV store access
                self.data_source_manager
                    .get_data::<serde_json::Value>(&request.key, &request.data_type)
                    .await?
                    .ok_or_else(|| ArbitrageError::not_found("Data not found in KV store"))
            }
            DataSourceType::D1Database => {
                // Use data source manager for D1 database access
                self.data_source_manager
                    .get_data::<serde_json::Value>(&request.key, &request.data_type)
                    .await?
                    .ok_or_else(|| ArbitrageError::not_found("Data not found in D1 database"))
            }
            DataSourceType::Pipeline => {
                // Use data source manager for pipeline access
                self.data_source_manager
                    .get_data::<serde_json::Value>(&request.key, &request.data_type)
                    .await?
                    .ok_or_else(|| ArbitrageError::not_found("Data not found in pipeline"))
            }
            DataSourceType::Queue => {
                // Use data source manager for queue access
                self.data_source_manager
                    .get_data::<serde_json::Value>(&request.key, &request.data_type)
                    .await?
                    .ok_or_else(|| ArbitrageError::not_found("Data not found in queue"))
            }
            DataSourceType::LocalFallback => {
                // Use data source manager for local fallback access
                self.data_source_manager
                    .get_data::<serde_json::Value>(&request.key, &request.data_type)
                    .await?
                    .ok_or_else(|| ArbitrageError::not_found("Data not found in local fallback"))
            }
            DataSourceType::ExternalAPI => {
                // Use API connector for external API access
                let api_request = self.create_api_request(request)?;
                let api_response = self.api_connector.make_request(api_request).await?;
                serde_json::from_str(&api_response.body).map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to parse API response: {}", e))
                })
            }
        }
    }

    /// Create API request from data access request
    fn create_api_request(&self, request: &DataAccessRequest) -> ArbitrageResult<APIRequest> {
        // Extract exchange and endpoint from metadata or key
        let exchange = request
            .metadata
            .get("exchange")
            .map(|e| match e.as_str() {
                "binance" => ExchangeType::Binance,
                "bybit" => ExchangeType::Bybit,
                "okx" => ExchangeType::OKX,
                _ => ExchangeType::Generic,
            })
            .unwrap_or(ExchangeType::Generic);

        let endpoint = request
            .metadata
            .get("endpoint")
            .cloned()
            .unwrap_or_else(|| format!("/api/v1/data/{}", request.key));

        let method = request
            .metadata
            .get("method")
            .cloned()
            .unwrap_or_else(|| "GET".to_string());

        Ok(APIRequest::new(exchange, endpoint, method).with_priority(request.priority))
    }

    /// Determine cache type from data type
    fn determine_cache_type(&self, data_type: &str) -> CacheEntryType {
        match data_type {
            "market_data" => CacheEntryType::MarketData,
            "funding_rates" => CacheEntryType::FundingRates,
            "analytics" => CacheEntryType::Analytics,
            "user_preferences" => CacheEntryType::UserPreferences,
            "opportunities" => CacheEntryType::Opportunities,
            "ai_analysis" => CacheEntryType::AIAnalysis,
            "system_config" => CacheEntryType::SystemConfig,
            "trading_pairs" => CacheEntryType::TradingPairs,
            _ => CacheEntryType::MarketData, // Default
        }
    }

    /// Check if data should be cached based on strategy and source
    fn should_cache(&self, request: &DataAccessRequest, source_type: &DataSourceType) -> bool {
        match request.cache_strategy {
            CacheStrategy::CacheOnly => false,  // Already in cache
            CacheStrategy::SourceOnly => false, // Explicitly no caching
            CacheStrategy::Refresh => true,     // Cache the refreshed data
            CacheStrategy::SourceFirst | CacheStrategy::CacheFirst => {
                // Cache if data came from non-cache source
                *source_type != DataSourceType::Cache
            }
        }
    }

    /// Cache data with appropriate TTL
    async fn cache_data(
        &self,
        request: &DataAccessRequest,
        data: &serde_json::Value,
    ) -> ArbitrageResult<()> {
        let cache_type = self.determine_cache_type(&request.data_type);
        self.cache_layer
            .set(&request.key, data, cache_type, None)
            .await
    }

    /// Validate data using data validator
    async fn validate_data(
        &self,
        request: &DataAccessRequest,
        data: &serde_json::Value,
    ) -> ArbitrageResult<ValidationResult> {
        let data_source = request
            .metadata
            .get("source")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let timestamp = request
            .metadata
            .get("timestamp")
            .and_then(|t| t.parse::<u64>().ok());

        self.data_validator
            .validate_data(data, &request.data_type, &data_source, timestamp)
            .await
    }

    /// Check for duplicate requests
    async fn check_duplicate_request(
        &self,
        _request: &DataAccessRequest,
    ) -> Option<DataAccessResponse> {
        // For now, return None (no deduplication implemented)
        // In a real implementation, this would check for identical pending requests
        None
    }

    /// Check concurrent request limit
    async fn check_concurrent_limit(&self, request_id: &str, start_time: u64) -> bool {
        if let Ok(mut active) = self.active_requests.lock() {
            if active.len() >= self.config.max_concurrent_operations {
                false
            } else {
                active.insert(request_id.to_string(), start_time);
                true
            }
        } else {
            false
        }
    }

    /// Cleanup request tracking
    async fn cleanup_request(&self, request_id: &str) {
        if let Ok(mut active) = self.active_requests.lock() {
            active.remove(request_id);
        }
    }

    /// Record request metrics
    async fn record_request_metrics(
        &self,
        _request: &DataAccessRequest,
        response: &DataAccessResponse,
        _start_time: u64,
    ) {
        if !self.config.enable_metrics_collection {
            return;
        }

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_requests += 1;

            if response.data.is_some() {
                metrics.successful_requests += 1;
            } else {
                metrics.failed_requests += 1;
            }

            // Track cache hits/misses
            if response.source_used == DataSourceType::Cache {
                metrics.cache_hits += 1;
            } else {
                metrics.cache_misses += 1;
            }

            // Track validation results
            if let Some(validation) = &response.validation_result {
                if validation.is_valid {
                    metrics.validation_passes += 1;
                } else {
                    metrics.validation_failures += 1;
                }
            }

            // Track source usage
            *metrics
                .source_usage
                .entry(response.source_used.clone())
                .or_default() += 1;

            // Update average latency
            let total = metrics.total_requests as f64;
            metrics.average_latency_ms =
                (metrics.average_latency_ms * (total - 1.0) + response.latency_ms as f64) / total;

            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Get coordination metrics
    pub async fn get_metrics(&self) -> CoordinationMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            CoordinationMetrics::default()
        }
    }

    /// Health check for data coordinator
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Check all component health
        let data_source_healthy = self
            .data_source_manager
            .health_check()
            .await
            .unwrap_or(false);
        let cache_healthy = self.cache_layer.health_check().await.unwrap_or(false);
        let api_healthy = self.api_connector.health_check().await.unwrap_or(false);
        let validator_healthy = self.data_validator.health_check().await.unwrap_or(false);

        // Consider healthy if at least 75% of components are healthy
        let healthy_components = [
            data_source_healthy,
            cache_healthy,
            api_healthy,
            validator_healthy,
        ]
        .iter()
        .filter(|&&h| h)
        .count();

        Ok(healthy_components >= 3)
    }

    /// Get comprehensive health summary
    pub async fn get_health_summary(&self) -> ArbitrageResult<serde_json::Value> {
        let metrics = self.get_metrics().await;

        // Get component health
        let data_source_health = self
            .data_source_manager
            .get_health_summary()
            .await
            .unwrap_or_default();
        let cache_health = self.cache_layer.get_health().await;
        let api_health = self
            .api_connector
            .get_health_summary()
            .await
            .unwrap_or_default();
        let validator_health = self
            .data_validator
            .get_health_summary()
            .await
            .unwrap_or_default();

        let success_rate = if metrics.total_requests > 0 {
            metrics.successful_requests as f32 / metrics.total_requests as f32 * 100.0
        } else {
            100.0
        };

        let cache_hit_rate = if metrics.cache_hits + metrics.cache_misses > 0 {
            metrics.cache_hits as f32 / (metrics.cache_hits + metrics.cache_misses) as f32 * 100.0
        } else {
            0.0
        };

        let active_requests = if let Ok(active) = self.active_requests.lock() {
            active.len()
        } else {
            0
        };

        Ok(serde_json::json!({
            "overall_health": success_rate >= 80.0,
            "success_rate_percent": success_rate,
            "cache_hit_rate_percent": cache_hit_rate,
            "total_requests": metrics.total_requests,
            "active_requests": active_requests,
            "max_concurrent_operations": self.config.max_concurrent_operations,
            "average_latency_ms": metrics.average_latency_ms,
            "source_usage": metrics.source_usage,
            "validation_success_rate": if metrics.validation_passes + metrics.validation_failures > 0 {
                metrics.validation_passes as f32 / (metrics.validation_passes + metrics.validation_failures) as f32 * 100.0
            } else {
                100.0
            },
            "component_health": {
                "data_source_manager": data_source_health,
                "cache_layer": cache_health,
                "api_connector": api_health,
                "data_validator": validator_health
            },
            "last_updated": metrics.last_updated
        }))
    }

    /// Batch process multiple requests for efficiency
    pub async fn batch_process_requests(
        &self,
        requests: Vec<DataAccessRequest>,
    ) -> ArbitrageResult<Vec<DataAccessResponse>> {
        if !self.config.enable_coordination {
            // Process individually if coordination is disabled
            let mut responses = Vec::new();
            for request in requests {
                responses.push(self.process_request(request).await?);
            }
            return Ok(responses);
        }

        // Group requests by data type and source for efficient batching
        let mut grouped_requests: HashMap<String, Vec<DataAccessRequest>> = HashMap::new();
        for request in requests {
            let group_key = format!(
                "{}:{}",
                request.data_type,
                request
                    .source_preference
                    .first()
                    .map(source_type_to_string)
                    .unwrap_or_else(|| "unknown".to_string())
            );
            grouped_requests.entry(group_key).or_default().push(request);
        }

        let mut all_responses = Vec::new();

        // Process each group
        for (_, group_requests) in grouped_requests {
            // Process in batches of configured size
            for chunk in group_requests.chunks(self.config.max_concurrent_operations) {
                let mut batch_responses = Vec::new();

                // Process batch concurrently (simplified - in real implementation use proper async batching)
                for request in chunk {
                    match self.process_request(request.clone()).await {
                        Ok(response) => batch_responses.push(response),
                        Err(e) => {
                            // Create error response
                            batch_responses.push(DataAccessResponse {
                                request_id: request.request_id.clone(),
                                data: None,
                                source_used: DataSourceType::Cache,
                                cache_hit: false,
                                validation_result: None,
                                latency_ms: 0,
                                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                metadata: request.metadata.clone(),
                                errors: vec![e.to_string()],
                                warnings: Vec::new(),
                            });
                        }
                    }
                }

                all_responses.extend(batch_responses);
            }
        }

        Ok(all_responses)
    }

    /// Get the underlying KvStore for direct access when needed
    pub fn get_kv_store(&self) -> worker::kv::KvStore {
        self.data_source_manager.get_kv_store()
    }

    /// Get coordinator configuration
    pub fn get_config(&self) -> &DataCoordinatorConfig {
        &self.config
    }
}

/// Helper function to convert DataSourceType to string
fn source_type_to_string(source_type: &DataSourceType) -> String {
    match source_type {
        DataSourceType::KvStore => "kv_store".to_string(),
        DataSourceType::D1Database => "d1_database".to_string(),
        DataSourceType::Cache => "cache".to_string(),
        DataSourceType::Pipeline => "pipeline".to_string(),
        DataSourceType::Queue => "queue".to_string(),
        DataSourceType::LocalFallback => "local_fallback".to_string(),
        DataSourceType::ExternalAPI => "external_api".to_string(),
    }
}

impl DataAccessRequest {
    /// Create a new data access request
    pub fn new(request_id: String, data_type: String, key: String) -> Self {
        Self {
            request_id,
            data_type,
            key,
            source_preference: vec![
                DataSourceType::Cache,
                DataSourceType::Pipeline,
                DataSourceType::Queue,
            ],
            cache_strategy: CacheStrategy::CacheFirst,
            validation_required: false,
            freshness_required: false,
            timeout_ms: None,
            priority: 2, // Medium priority
            metadata: HashMap::new(),
        }
    }

    /// Set source preference order
    pub fn with_source_preference(mut self, sources: Vec<DataSourceType>) -> Self {
        self.source_preference = sources;
        self
    }

    /// Set cache strategy
    pub fn with_cache_strategy(mut self, strategy: CacheStrategy) -> Self {
        self.cache_strategy = strategy;
        self
    }

    /// Enable validation
    pub fn with_validation(mut self, required: bool) -> Self {
        self.validation_required = required;
        self
    }

    /// Enable freshness check
    pub fn with_freshness_check(mut self, required: bool) -> Self {
        self.freshness_required = required;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_access_request_builder() {
        let request = DataAccessRequest::new(
            "req_123".to_string(),
            "market_data".to_string(),
            "BTC/USD".to_string(),
        )
        .with_cache_strategy(CacheStrategy::SourceFirst)
        .with_validation(true)
        .with_priority(1);

        assert_eq!(request.request_id, "req_123");
        assert_eq!(request.cache_strategy, CacheStrategy::SourceFirst);
        assert!(request.validation_required);
        assert_eq!(request.priority, 1);
    }

    #[test]
    fn test_data_coordinator_config_validation() {
        let config = DataCoordinatorConfig::default();
        assert!(config.validate().is_ok());

        let mut invalid_config = config;
        invalid_config.default_timeout_seconds = 0;
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_coordination_metrics_default() {
        let metrics = CoordinationMetrics::default();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.cache_hits, 0);
        assert_eq!(metrics.average_latency_ms, 0.0);
    }

    #[test]
    fn test_source_type_to_string() {
        assert_eq!(source_type_to_string(&DataSourceType::Cache), "cache");
        assert_eq!(
            source_type_to_string(&DataSourceType::ExternalAPI),
            "external_api"
        );
        assert_eq!(source_type_to_string(&DataSourceType::Pipeline), "pipeline");
    }
}
