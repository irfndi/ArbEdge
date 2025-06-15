// Comprehensive Opportunity Generation Monitoring Service
// Tracks opportunity pipeline, API calls, service initialization, and performance metrics

use crate::services::core::infrastructure::monitoring_module::metrics_collector::{
    MetricType, MetricValue, MetricsCollector,
};
use crate::types::ArbitrageOpportunity;
use crate::utils::ArbitrageResult;

/// Parameters for tracking API calls to reduce function argument count
#[derive(Debug, Clone)]
pub struct ApiCallTrackingParams {
    pub exchange: String,
    pub endpoint: String,
    pub method: String,
    pub status_code: Option<u16>,
    pub response_time_ms: u64,
    pub success: bool,
    pub error_message: Option<String>,
    pub data_source: String,
}

/// Parameters for tracking opportunity generation to reduce function argument count
#[derive(Debug, Clone)]
pub struct OpportunityGenerationTrackingParams {
    pub opportunities: Vec<ArbitrageOpportunity>,
    pub duplicates_detected: u64,
    pub duplicates_removed: u64,
    pub validation_failures: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub pipeline_duration_ms: u64,
}

/// Parameters for tracking errors to reduce function argument count
#[derive(Debug, Clone)]
pub struct ErrorTrackingParams {
    pub error_type: String,
    pub error_message: String,
    pub stack_trace: Option<String>,
    pub stage: String,
    pub exchange: Option<String>,
    pub symbol: Option<String>,
    pub request_id: Option<String>,
    pub severity: String,
}
use crate::utils::now_system_time;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use worker::console_log;

/// Opportunity generation pipeline stages for detailed tracking
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpportunityPipelineStage {
    Initialization,
    MarketDataFetch,
    ApiCall,
    DataProcessing,
    OpportunityGeneration,
    Deduplication,
    Validation,
    Caching,
    Completion,
    Error,
}

impl OpportunityPipelineStage {
    pub fn as_str(&self) -> &str {
        match self {
            OpportunityPipelineStage::Initialization => "initialization",
            OpportunityPipelineStage::MarketDataFetch => "market_data_fetch",
            OpportunityPipelineStage::ApiCall => "api_call",
            OpportunityPipelineStage::DataProcessing => "data_processing",
            OpportunityPipelineStage::OpportunityGeneration => "opportunity_generation",
            OpportunityPipelineStage::Deduplication => "deduplication",
            OpportunityPipelineStage::Validation => "validation",
            OpportunityPipelineStage::Caching => "caching",
            OpportunityPipelineStage::Completion => "completion",
            OpportunityPipelineStage::Error => "error",
        }
    }
}

/// API call tracking information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCallMetrics {
    pub exchange: String,
    pub endpoint: String,
    pub method: String,
    pub status_code: Option<u16>,
    pub response_time_ms: u64,
    pub success: bool,
    pub error_message: Option<String>,
    pub timestamp: u64,
    pub data_source: String, // "pipeline", "cache", "direct_api"
}

/// Service initialization tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInitMetrics {
    pub service_name: String,
    pub initialization_time_ms: u64,
    pub success: bool,
    pub error_message: Option<String>,
    pub timestamp: u64,
    pub request_id: Option<String>,
}

/// Opportunity generation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityGenerationMetrics {
    pub total_opportunities_generated: u64,
    pub opportunities_by_type: HashMap<String, u64>,
    pub opportunities_by_exchange: HashMap<String, u64>,
    pub duplicates_detected: u64,
    pub duplicates_removed: u64,
    pub validation_failures: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub pipeline_duration_ms: u64,
    pub timestamp: u64,
}

/// Performance metrics per request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestPerformanceMetrics {
    pub request_id: String,
    pub total_duration_ms: u64,
    pub stage_durations: HashMap<String, u64>,
    pub memory_usage_mb: Option<f64>,
    pub api_calls_count: u64,
    pub database_queries_count: u64,
    pub cache_operations_count: u64,
    pub timestamp: u64,
}

/// Error tracking information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    pub error_type: String,
    pub error_message: String,
    pub stack_trace: Option<String>,
    pub stage: String,
    pub exchange: Option<String>,
    pub symbol: Option<String>,
    pub request_id: Option<String>,
    pub timestamp: u64,
    pub severity: String, // "low", "medium", "high", "critical"
}

/// Alert configuration for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    pub zero_opportunities_threshold_minutes: u64,
    pub high_error_rate_threshold_percent: f64,
    pub slow_response_threshold_ms: u64,
    pub duplicate_rate_threshold_percent: f64,
    pub api_failure_rate_threshold_percent: f64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            zero_opportunities_threshold_minutes: 10,
            high_error_rate_threshold_percent: 5.0,
            slow_response_threshold_ms: 5000,
            duplicate_rate_threshold_percent: 50.0,
            api_failure_rate_threshold_percent: 10.0,
        }
    }
}

/// Comprehensive opportunity monitoring service
pub struct OpportunityMonitor {
    metrics_collector: Arc<MetricsCollector>,
    alert_config: AlertConfig,

    // In-memory tracking for current session
    current_request_metrics: Arc<Mutex<HashMap<String, RequestPerformanceMetrics>>>,
    api_call_history: Arc<Mutex<Vec<ApiCallMetrics>>>,
    service_init_history: Arc<Mutex<Vec<ServiceInitMetrics>>>,
    error_history: Arc<Mutex<Vec<ErrorMetrics>>>,

    // Performance tracking
    last_opportunity_generation: Arc<Mutex<u64>>,
    consecutive_zero_generations: Arc<Mutex<u64>>,
}

impl OpportunityMonitor {
    pub fn new(metrics_collector: Arc<MetricsCollector>) -> Self {
        Self {
            metrics_collector,
            alert_config: AlertConfig::default(),
            current_request_metrics: Arc::new(Mutex::new(HashMap::new())),
            api_call_history: Arc::new(Mutex::new(Vec::new())),
            service_init_history: Arc::new(Mutex::new(Vec::new())),
            error_history: Arc::new(Mutex::new(Vec::new())),
            last_opportunity_generation: Arc::new(Mutex::new(0)),
            consecutive_zero_generations: Arc::new(Mutex::new(0)),
        }
    }

    /// Track opportunity generation pipeline stage
    pub async fn track_pipeline_stage(
        &self,
        request_id: &str,
        stage: OpportunityPipelineStage,
        duration_ms: u64,
        success: bool,
        metadata: Option<HashMap<String, String>>,
    ) -> ArbitrageResult<()> {
        console_log!(
            "📊 PIPELINE STAGE TRACKING - {} {} ({}ms) success={}",
            request_id,
            stage.as_str(),
            duration_ms,
            success
        );

        // Update request metrics
        if let Ok(mut metrics) = self.current_request_metrics.lock() {
            let request_metrics = metrics.entry(request_id.to_string()).or_insert_with(|| {
                RequestPerformanceMetrics {
                    request_id: request_id.to_string(),
                    total_duration_ms: 0,
                    stage_durations: HashMap::new(),
                    memory_usage_mb: None,
                    api_calls_count: 0,
                    database_queries_count: 0,
                    cache_operations_count: 0,
                    timestamp: now_system_time()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                }
            });

            request_metrics
                .stage_durations
                .insert(stage.as_str().to_string(), duration_ms);
            request_metrics.total_duration_ms += duration_ms;
        }

        // Collect metrics
        let mut metric_value = MetricValue::new(duration_ms as f64)
            .with_tag("stage".to_string(), stage.as_str().to_string())
            .with_tag("success".to_string(), success.to_string())
            .with_tag("request_id".to_string(), request_id.to_string());

        if let Some(meta) = metadata {
            for (key, value) in meta {
                metric_value = metric_value.with_tag(key, value);
            }
        }

        self.metrics_collector
            .collect_metric(
                "opportunity_pipeline_stage_duration".to_string(),
                MetricType::Timer,
                "opportunity_engine".to_string(),
                metric_value,
            )
            .await?;

        // Track stage success/failure
        self.metrics_collector
            .collect_metric(
                "opportunity_pipeline_stage_success".to_string(),
                MetricType::Counter,
                "opportunity_engine".to_string(),
                MetricValue::new(if success { 1.0 } else { 0.0 })
                    .with_tag("stage".to_string(), stage.as_str().to_string()),
            )
            .await?;

        Ok(())
    }

    /// Track API call attempts and responses
    pub async fn track_api_call(&self, params: ApiCallTrackingParams) -> ArbitrageResult<()> {
        let exchange = &params.exchange;
        let endpoint = &params.endpoint;
        let method = &params.method;
        let status_code = params.status_code;
        let response_time_ms = params.response_time_ms;
        let success = params.success;
        let error_message = params.error_message.clone();
        let data_source = &params.data_source;
        console_log!(
            "🌐 API CALL TRACKING - {} {} {} ({}ms) success={} source={}",
            exchange,
            method,
            endpoint,
            response_time_ms,
            success,
            data_source
        );

        let api_metrics = ApiCallMetrics {
            exchange: exchange.to_string(),
            endpoint: endpoint.to_string(),
            method: method.to_string(),
            status_code,
            response_time_ms,
            success,
            error_message: error_message.clone(),
            timestamp: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            data_source: data_source.to_string(),
        };

        // Store in history
        if let Ok(mut history) = self.api_call_history.lock() {
            history.push(api_metrics);
            // Keep only last 1000 API calls
            if history.len() > 1000 {
                history.drain(0..100);
            }
        }

        // Collect metrics
        self.metrics_collector
            .collect_metric(
                "api_call_duration".to_string(),
                MetricType::Timer,
                "exchange_service".to_string(),
                MetricValue::new(response_time_ms as f64)
                    .with_tag("exchange".to_string(), exchange.to_string())
                    .with_tag("endpoint".to_string(), endpoint.to_string())
                    .with_tag("method".to_string(), method.to_string())
                    .with_tag("success".to_string(), success.to_string())
                    .with_tag("data_source".to_string(), data_source.to_string()),
            )
            .await?;

        // Track API success rate
        self.metrics_collector
            .collect_metric(
                "api_call_success_rate".to_string(),
                MetricType::Rate,
                "exchange_service".to_string(),
                MetricValue::new(if success { 1.0 } else { 0.0 })
                    .with_tag("exchange".to_string(), exchange.to_string())
                    .with_tag("data_source".to_string(), data_source.to_string()),
            )
            .await?;

        // Track data source usage
        self.metrics_collector
            .collect_metric(
                "data_source_usage".to_string(),
                MetricType::Counter,
                "market_analyzer".to_string(),
                MetricValue::new(1.0)
                    .with_tag("data_source".to_string(), data_source.to_string())
                    .with_tag("exchange".to_string(), exchange.to_string()),
            )
            .await?;

        // Log error if API call failed
        if !success {
            self.track_error(ErrorTrackingParams {
                error_type: "api_call_failure".to_string(),
                error_message: error_message.unwrap_or("Unknown API error".to_string()),
                stack_trace: None,
                stage: "api_call".to_string(),
                exchange: Some(exchange.to_string()),
                symbol: None,
                request_id: None,
                severity: "medium".to_string(),
            })
            .await?;
        }

        Ok(())
    }

    /// Track service initialization patterns
    pub async fn track_service_initialization(
        &self,
        service_name: &str,
        initialization_time_ms: u64,
        success: bool,
        error_message: Option<String>,
        request_id: Option<String>,
    ) -> ArbitrageResult<()> {
        console_log!(
            "🚀 SERVICE INIT TRACKING - {} ({}ms) success={} request_id={:?}",
            service_name,
            initialization_time_ms,
            success,
            request_id
        );

        let init_metrics = ServiceInitMetrics {
            service_name: service_name.to_string(),
            initialization_time_ms,
            success,
            error_message: error_message.clone(),
            timestamp: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            request_id: request_id.clone(),
        };

        // Store in history
        if let Ok(mut history) = self.service_init_history.lock() {
            history.push(init_metrics);
            // Keep only last 500 initializations
            if history.len() > 500 {
                history.drain(0..50);
            }
        }

        // Collect metrics
        self.metrics_collector
            .collect_metric(
                "service_initialization_duration".to_string(),
                MetricType::Timer,
                "service_container".to_string(),
                MetricValue::new(initialization_time_ms as f64)
                    .with_tag("service_name".to_string(), service_name.to_string())
                    .with_tag("success".to_string(), success.to_string()),
            )
            .await?;

        // Track initialization frequency (potential inefficiency indicator)
        self.metrics_collector
            .collect_metric(
                "service_initialization_frequency".to_string(),
                MetricType::Counter,
                "service_container".to_string(),
                MetricValue::new(1.0)
                    .with_tag("service_name".to_string(), service_name.to_string()),
            )
            .await?;

        // Alert if service is being initialized too frequently
        if let Ok(history) = self.service_init_history.lock() {
            let recent_inits = history
                .iter()
                .filter(|init| {
                    init.service_name == service_name
                        && init.timestamp
                            > now_system_time()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64
                                - 60000 // Last minute
                })
                .count();

            if recent_inits > 10 {
                console_log!(
                    "⚠️ ALERT: Service {} initialized {} times in the last minute - potential inefficiency!",
                    service_name,
                    recent_inits
                );
            }
        }

        Ok(())
    }

    /// Track opportunity generation results
    pub async fn track_opportunity_generation(
        &self,
        params: OpportunityGenerationTrackingParams,
    ) -> ArbitrageResult<()> {
        let opportunities = &params.opportunities;
        let duplicates_detected = params.duplicates_detected;
        let duplicates_removed = params.duplicates_removed;
        let validation_failures = params.validation_failures;
        let cache_hits = params.cache_hits;
        let cache_misses = params.cache_misses;
        let pipeline_duration_ms = params.pipeline_duration_ms;
        let total_opportunities = opportunities.len() as u64;

        console_log!(
            "🎯 OPPORTUNITY GENERATION TRACKING - {} opportunities, {} duplicates removed, {}ms pipeline",
            total_opportunities,
            duplicates_removed,
            pipeline_duration_ms
        );

        // Update last generation timestamp
        if let Ok(mut last_gen) = self.last_opportunity_generation.lock() {
            *last_gen = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
        }

        // Track consecutive zero generations
        if let Ok(mut zero_count) = self.consecutive_zero_generations.lock() {
            if total_opportunities == 0 {
                *zero_count += 1;
                console_log!(
                    "⚠️ ZERO OPPORTUNITIES GENERATED - consecutive count: {}",
                    *zero_count
                );
            } else {
                *zero_count = 0;
            }
        }

        // Count opportunities by type and exchange
        let mut opportunities_by_type = HashMap::new();
        let mut opportunities_by_exchange = HashMap::new();

        for opp in opportunities {
            let opp_type = match opp.r#type {
                crate::types::ArbitrageType::FundingRate => "funding",
                crate::types::ArbitrageType::SpotFutures => "spot_futures",
                crate::types::ArbitrageType::CrossExchange => "cross_exchange",
                crate::types::ArbitrageType::Price => "price",
            };
            *opportunities_by_type
                .entry(opp_type.to_string())
                .or_insert(0) += 1;

            *opportunities_by_exchange
                .entry(opp.long_exchange.as_str().to_string())
                .or_insert(0) += 1;
            *opportunities_by_exchange
                .entry(opp.short_exchange.as_str().to_string())
                .or_insert(0) += 1;
        }

        let generation_metrics = OpportunityGenerationMetrics {
            total_opportunities_generated: total_opportunities,
            opportunities_by_type,
            opportunities_by_exchange,
            duplicates_detected,
            duplicates_removed,
            validation_failures,
            cache_hits,
            cache_misses,
            pipeline_duration_ms,
            timestamp: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        // Collect comprehensive metrics
        self.metrics_collector
            .collect_metric(
                "opportunities_generated_total".to_string(),
                MetricType::Gauge,
                "opportunity_engine".to_string(),
                MetricValue::new(total_opportunities as f64),
            )
            .await?;

        self.metrics_collector
            .collect_metric(
                "opportunity_generation_duration".to_string(),
                MetricType::Timer,
                "opportunity_engine".to_string(),
                MetricValue::new(pipeline_duration_ms as f64),
            )
            .await?;

        self.metrics_collector
            .collect_metric(
                "duplicates_detected".to_string(),
                MetricType::Counter,
                "opportunity_engine".to_string(),
                MetricValue::new(duplicates_detected as f64),
            )
            .await?;

        self.metrics_collector
            .collect_metric(
                "duplicates_removed".to_string(),
                MetricType::Counter,
                "opportunity_engine".to_string(),
                MetricValue::new(duplicates_removed as f64),
            )
            .await?;

        // Calculate and track duplicate rate
        let duplicate_rate = if duplicates_detected > 0 {
            (duplicates_removed as f64 / duplicates_detected as f64) * 100.0
        } else {
            0.0
        };

        self.metrics_collector
            .collect_metric(
                "duplicate_removal_rate".to_string(),
                MetricType::Percentage,
                "opportunity_engine".to_string(),
                MetricValue::new(duplicate_rate),
            )
            .await?;

        // Track cache efficiency
        let cache_hit_rate = if cache_hits + cache_misses > 0 {
            (cache_hits as f64 / (cache_hits + cache_misses) as f64) * 100.0
        } else {
            0.0
        };

        self.metrics_collector
            .collect_metric(
                "cache_hit_rate".to_string(),
                MetricType::Percentage,
                "cache_manager".to_string(),
                MetricValue::new(cache_hit_rate),
            )
            .await?;

        // Check for alerts
        self.check_opportunity_generation_alerts(&generation_metrics)
            .await?;

        Ok(())
    }

    /// Track errors with detailed context
    pub async fn track_error(&self, params: ErrorTrackingParams) -> ArbitrageResult<()> {
        let error_type = params.error_type;
        let error_message = params.error_message;
        let stack_trace = params.stack_trace;
        let stage = params.stage;
        let exchange = params.exchange;
        let symbol = params.symbol;
        let request_id = params.request_id;
        let severity = params.severity;
        console_log!(
            "❌ ERROR TRACKING - {} {} at {} severity={} exchange={:?} symbol={:?}",
            error_type,
            error_message,
            stage,
            severity,
            exchange,
            symbol
        );

        let error_metrics = ErrorMetrics {
            error_type: error_type.clone(),
            error_message: error_message.clone(),
            stack_trace,
            stage: stage.clone(),
            exchange: exchange.clone(),
            symbol: symbol.clone(),
            request_id: request_id.clone(),
            timestamp: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            severity: severity.clone(),
        };

        // Store in history
        if let Ok(mut history) = self.error_history.lock() {
            history.push(error_metrics);
            // Keep only last 1000 errors
            if history.len() > 1000 {
                history.drain(0..100);
            }
        }

        // Collect error metrics
        self.metrics_collector
            .collect_metric(
                "error_count".to_string(),
                MetricType::Counter,
                "error_tracking".to_string(),
                MetricValue::new(1.0)
                    .with_tag("error_type".to_string(), error_type)
                    .with_tag("stage".to_string(), stage)
                    .with_tag("severity".to_string(), severity),
            )
            .await?;

        if let Some(exchange_name) = exchange {
            self.metrics_collector
                .collect_metric(
                    "exchange_error_count".to_string(),
                    MetricType::Counter,
                    "exchange_service".to_string(),
                    MetricValue::new(1.0).with_tag("exchange".to_string(), exchange_name),
                )
                .await?;
        }

        Ok(())
    }

    /// Check for alerts based on opportunity generation metrics
    async fn check_opportunity_generation_alerts(
        &self,
        metrics: &OpportunityGenerationMetrics,
    ) -> ArbitrageResult<()> {
        // Alert for zero opportunities
        if metrics.total_opportunities_generated == 0 {
            if let Ok(zero_count) = self.consecutive_zero_generations.lock() {
                if *zero_count >= self.alert_config.zero_opportunities_threshold_minutes {
                    console_log!(
                        "🚨 CRITICAL ALERT: Zero opportunities generated for {} consecutive cycles!",
                        *zero_count
                    );
                }
            }
        }

        // Alert for high duplicate rate
        let duplicate_rate = if metrics.duplicates_detected > 0 {
            (metrics.duplicates_removed as f64 / metrics.duplicates_detected as f64) * 100.0
        } else {
            0.0
        };

        if duplicate_rate > self.alert_config.duplicate_rate_threshold_percent {
            console_log!(
                "⚠️ ALERT: High duplicate rate detected: {:.2}% (threshold: {:.2}%)",
                duplicate_rate,
                self.alert_config.duplicate_rate_threshold_percent
            );
        }

        // Alert for slow pipeline
        if metrics.pipeline_duration_ms > self.alert_config.slow_response_threshold_ms {
            console_log!(
                "⚠️ ALERT: Slow opportunity generation pipeline: {}ms (threshold: {}ms)",
                metrics.pipeline_duration_ms,
                self.alert_config.slow_response_threshold_ms
            );
        }

        Ok(())
    }

    /// Get comprehensive monitoring report
    pub async fn get_monitoring_report(&self) -> ArbitrageResult<serde_json::Value> {
        let api_history = self.api_call_history.lock().unwrap().clone();
        let service_history = self.service_init_history.lock().unwrap().clone();
        let error_history = self.error_history.lock().unwrap().clone();

        // Calculate API success rate
        let total_api_calls = api_history.len();
        let successful_api_calls = api_history.iter().filter(|call| call.success).count();
        let api_success_rate = if total_api_calls > 0 {
            (successful_api_calls as f64 / total_api_calls as f64) * 100.0
        } else {
            0.0
        };

        // Calculate average response times
        let avg_api_response_time = if !api_history.is_empty() {
            api_history
                .iter()
                .map(|call| call.response_time_ms)
                .sum::<u64>() as f64
                / api_history.len() as f64
        } else {
            0.0
        };

        // Count errors by severity
        let mut errors_by_severity = HashMap::new();
        for error in &error_history {
            *errors_by_severity
                .entry(error.severity.clone())
                .or_insert(0) += 1;
        }

        Ok(serde_json::json!({
            "timestamp": now_system_time().duration_since(UNIX_EPOCH).unwrap().as_millis(),
            "api_metrics": {
                "total_calls": total_api_calls,
                "success_rate_percent": api_success_rate,
                "average_response_time_ms": avg_api_response_time,
                "recent_calls": api_history.iter().rev().take(10).collect::<Vec<_>>()
            },
            "service_metrics": {
                "total_initializations": service_history.len(),
                "recent_initializations": service_history.iter().rev().take(10).collect::<Vec<_>>()
            },
            "error_metrics": {
                "total_errors": error_history.len(),
                "errors_by_severity": errors_by_severity,
                "recent_errors": error_history.iter().rev().take(10).collect::<Vec<_>>()
            },
            "alert_status": {
                "consecutive_zero_generations": *self.consecutive_zero_generations.lock().unwrap(),
                "last_opportunity_generation": *self.last_opportunity_generation.lock().unwrap()
            }
        }))
    }

    /// Update alert configuration
    pub fn update_alert_config(&mut self, config: AlertConfig) {
        self.alert_config = config;
        console_log!("📋 Alert configuration updated");
    }

    /// Get current alert configuration
    pub fn get_alert_config(&self) -> &AlertConfig {
        &self.alert_config
    }

    /// Clear monitoring history (for maintenance)
    pub fn clear_history(&self) {
        if let Ok(mut api_history) = self.api_call_history.lock() {
            api_history.clear();
        }
        if let Ok(mut service_history) = self.service_init_history.lock() {
            service_history.clear();
        }
        if let Ok(mut error_history) = self.error_history.lock() {
            error_history.clear();
        }
        if let Ok(mut request_metrics) = self.current_request_metrics.lock() {
            request_metrics.clear();
        }
        console_log!("🧹 Monitoring history cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opportunity_pipeline_stage_as_str() {
        assert_eq!(
            OpportunityPipelineStage::Initialization.as_str(),
            "initialization"
        );
        assert_eq!(OpportunityPipelineStage::ApiCall.as_str(), "api_call");
        assert_eq!(OpportunityPipelineStage::Error.as_str(), "error");
    }

    #[test]
    fn test_alert_config_default() {
        let config = AlertConfig::default();
        assert_eq!(config.zero_opportunities_threshold_minutes, 10);
        assert_eq!(config.high_error_rate_threshold_percent, 5.0);
    }
}
