// src/services/core/infrastructure/analytics_module/mod.rs

//! Analytics Module - Revolutionary Real-Time Analytics and Reporting System
//!
//! This module provides comprehensive analytics capabilities for the ArbEdge platform,
//! replacing the monolithic analytics_engine.rs with a modular architecture optimized
//! for high-concurrency trading operations (1000-2500 concurrent users).
//!
//! ## Modular Architecture (4 Components):
//!
//! 1. **DataProcessor** - Real-time data processing and stream analytics
//! 2. **ReportGenerator** - Automated report generation and export
//! 3. **MetricsAggregator** - Business metrics aggregation and KPI tracking
//! 4. **AnalyticsCoordinator** - Main orchestrator for all analytics operations
//!
//! ## Revolutionary Features:
//! - **Real-Time Processing**: Sub-second analytics with stream processing
//! - **Multi-Format Export**: PDF, CSV, JSON report generation
//! - **Business Intelligence**: KPI tracking and predictive analytics
//! - **Performance Optimization**: Batch processing and intelligent caching
//! - **Chaos Engineering**: Circuit breakers and fallback strategies

pub mod analytics_coordinator;
pub mod data_processor;
pub mod metrics_aggregator;
pub mod report_generator;

pub use analytics_coordinator::{
    AnalyticsCoordinator, AnalyticsCoordinatorConfig, AnalyticsCoordinatorHealth,
    AnalyticsCoordinatorMetrics, AnalyticsQuery, AnalyticsQueryResult, QueryMetadata,
    QueryPriority, TimeRange,
};
pub use data_processor::{
    DataProcessor, DataProcessorConfig, DataProcessorHealth, DataProcessorMetrics,
};
pub use metrics_aggregator::{
    MetricsAggregator, MetricsAggregatorConfig, MetricsAggregatorHealth, MetricsAggregatorMetrics,
};
pub use report_generator::{
    ReportGenerator, ReportGeneratorConfig, ReportGeneratorHealth, ReportGeneratorMetrics,
};

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::Env;

/// Analytics Module Configuration for High-Performance Analytics
#[derive(Debug, Clone)]
pub struct AnalyticsModuleConfig {
    // Core analytics settings
    pub enable_real_time_processing: bool,
    pub enable_automated_reporting: bool,
    pub enable_business_intelligence: bool,
    pub enable_predictive_analytics: bool,

    // Performance settings optimized for 1000-2500 concurrent users
    pub max_concurrent_queries: u32,
    pub batch_processing_size: usize,
    pub cache_ttl_seconds: u64,
    pub retention_days: u32,

    // Component configurations
    pub data_processor_config: DataProcessorConfig,
    pub report_generator_config: ReportGeneratorConfig,
    pub metrics_aggregator_config: MetricsAggregatorConfig,
    pub analytics_coordinator_config: AnalyticsCoordinatorConfig,
}

impl Default for AnalyticsModuleConfig {
    fn default() -> Self {
        Self {
            enable_real_time_processing: true,
            enable_automated_reporting: true,
            enable_business_intelligence: true,
            enable_predictive_analytics: true,
            max_concurrent_queries: 100,
            batch_processing_size: 200,
            cache_ttl_seconds: 300,
            retention_days: 90,
            data_processor_config: DataProcessorConfig::default(),
            report_generator_config: ReportGeneratorConfig::default(),
            metrics_aggregator_config: MetricsAggregatorConfig::default(),
            analytics_coordinator_config: AnalyticsCoordinatorConfig::default(),
        }
    }
}

impl AnalyticsModuleConfig {
    /// High-performance configuration for 1000-2500 concurrent users
    pub fn high_performance() -> Self {
        Self {
            enable_real_time_processing: true,
            enable_automated_reporting: true,
            enable_business_intelligence: true,
            enable_predictive_analytics: true,
            max_concurrent_queries: 200,
            batch_processing_size: 500,
            cache_ttl_seconds: 180,
            retention_days: 90,
            data_processor_config: DataProcessorConfig::high_performance(),
            report_generator_config: ReportGeneratorConfig::high_performance(),
            metrics_aggregator_config: MetricsAggregatorConfig::high_performance(),
            analytics_coordinator_config: AnalyticsCoordinatorConfig::high_performance(),
        }
    }

    /// High-reliability configuration with enhanced data retention
    pub fn high_reliability() -> Self {
        Self {
            enable_real_time_processing: true,
            enable_automated_reporting: true,
            enable_business_intelligence: true,
            enable_predictive_analytics: false, // Disable for stability
            max_concurrent_queries: 50,
            batch_processing_size: 100,
            cache_ttl_seconds: 600,
            retention_days: 365,
            data_processor_config: DataProcessorConfig::high_reliability(),
            report_generator_config: ReportGeneratorConfig::high_reliability(),
            metrics_aggregator_config: MetricsAggregatorConfig::high_reliability(),
            analytics_coordinator_config: AnalyticsCoordinatorConfig::high_reliability(),
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.max_concurrent_queries == 0 {
            return Err(ArbitrageError::configuration_error(
                "max_concurrent_queries must be greater than 0".to_string(),
            ));
        }
        if self.batch_processing_size == 0 {
            return Err(ArbitrageError::configuration_error(
                "batch_processing_size must be greater than 0".to_string(),
            ));
        }
        if self.retention_days == 0 {
            return Err(ArbitrageError::configuration_error(
                "retention_days must be greater than 0".to_string(),
            ));
        }

        // Validate component configurations
        self.data_processor_config.validate()?;
        self.report_generator_config.validate()?;
        self.metrics_aggregator_config.validate()?;
        self.analytics_coordinator_config.validate()?;

        Ok(())
    }
}

/// Analytics Module Health Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsModuleHealth {
    pub overall_health: bool,
    pub health_percentage: f64,
    pub component_health: HashMap<String, bool>,
    pub data_processor_health: DataProcessorHealth,
    pub report_generator_health: ReportGeneratorHealth,
    pub metrics_aggregator_health: MetricsAggregatorHealth,
    pub analytics_coordinator_health: AnalyticsCoordinatorHealth,
    pub last_health_check: u64,
    pub uptime_seconds: u64,
}

/// Analytics Module Performance Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsModuleMetrics {
    // Overall metrics
    pub total_queries_processed: u64,
    pub total_reports_generated: u64,
    pub total_metrics_collected: u64,
    pub average_query_latency_ms: f64,
    pub cache_hit_rate: f64,

    // Component metrics
    pub data_processor_metrics: DataProcessorMetrics,
    pub report_generator_metrics: ReportGeneratorMetrics,
    pub metrics_aggregator_metrics: MetricsAggregatorMetrics,
    pub analytics_coordinator_metrics: AnalyticsCoordinatorMetrics,

    // Performance tracking
    pub queries_per_second: f64,
    pub reports_per_hour: f64,
    pub metrics_per_minute: f64,
    pub error_rate: f64,
    pub last_updated: u64,
}

/// Main Analytics Module orchestrating all analytics components
pub struct AnalyticsModule {
    config: AnalyticsModuleConfig,
    data_processor: DataProcessor,
    report_generator: ReportGenerator,
    metrics_aggregator: MetricsAggregator,
    analytics_coordinator: AnalyticsCoordinator,
    is_initialized: bool,
    startup_time: Option<u64>,
}

impl AnalyticsModule {
    /// Create new Analytics Module with configuration
    pub fn new(config: AnalyticsModuleConfig) -> ArbitrageResult<Self> {
        // Validate configuration
        config.validate()?;

        // Create components
        let data_processor = DataProcessor::new(config.data_processor_config.clone())?;
        let report_generator = ReportGenerator::new(config.report_generator_config.clone())?;
        let metrics_aggregator = MetricsAggregator::new(config.metrics_aggregator_config.clone())?;
        let analytics_coordinator =
            AnalyticsCoordinator::new(config.analytics_coordinator_config.clone())?;

        Ok(Self {
            config,
            data_processor,
            report_generator,
            metrics_aggregator,
            analytics_coordinator,
            is_initialized: false,
            startup_time: None,
        })
    }

    /// Initialize the Analytics Module with environment
    pub async fn initialize(&mut self, env: &Env) -> ArbitrageResult<()> {
        let start_time = worker::Date::now().as_millis();

        // Initialize all components
        self.data_processor.initialize(env).await?;
        self.report_generator.initialize(env).await?;
        self.metrics_aggregator.initialize(env).await?;
        self.analytics_coordinator
            .initialize(
                env,
                self.data_processor.clone(),
                self.report_generator.clone(),
                self.metrics_aggregator.clone(),
            )
            .await?;

        self.is_initialized = true;
        self.startup_time = Some(start_time);

        Ok(())
    }

    /// Get comprehensive health status
    pub async fn health_check(&self) -> ArbitrageResult<AnalyticsModuleHealth> {
        let data_processor_health = self.data_processor.health_check().await?;
        let report_generator_health = self.report_generator.health_check().await?;
        let metrics_aggregator_health = self.metrics_aggregator.health_check().await?;
        let analytics_coordinator_health = self.analytics_coordinator.health_check().await?;

        let mut component_health = HashMap::new();
        component_health.insert(
            "data_processor".to_string(),
            data_processor_health.is_healthy,
        );
        component_health.insert(
            "report_generator".to_string(),
            report_generator_health.is_healthy,
        );
        component_health.insert(
            "metrics_aggregator".to_string(),
            metrics_aggregator_health.is_healthy,
        );
        component_health.insert(
            "analytics_coordinator".to_string(),
            analytics_coordinator_health.is_healthy,
        );

        let healthy_components = component_health.values().filter(|&&h| h).count();
        let total_components = component_health.len();
        let health_percentage = (healthy_components as f64 / total_components as f64) * 100.0;
        let overall_health = health_percentage >= 75.0; // 75% threshold

        let uptime_seconds = if let Some(startup_time) = self.startup_time {
            (worker::Date::now().as_millis() - startup_time) / 1000
        } else {
            0
        };

        Ok(AnalyticsModuleHealth {
            overall_health,
            health_percentage,
            component_health,
            data_processor_health,
            report_generator_health,
            metrics_aggregator_health,
            analytics_coordinator_health,
            last_health_check: worker::Date::now().as_millis(),
            uptime_seconds,
        })
    }

    /// Get comprehensive performance metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<AnalyticsModuleMetrics> {
        let data_processor_metrics = self.data_processor.get_metrics().await?;
        let report_generator_metrics = self.report_generator.get_metrics().await?;
        let metrics_aggregator_metrics = self.metrics_aggregator.get_metrics().await?;
        let analytics_coordinator_metrics = self.analytics_coordinator.get_metrics().await?;

        // Calculate overall metrics
        let total_queries_processed = data_processor_metrics.queries_processed
            + analytics_coordinator_metrics.queries_processed;
        let total_reports_generated = report_generator_metrics.reports_generated;
        let total_metrics_collected = metrics_aggregator_metrics.metrics_collected;

        let average_query_latency_ms = (data_processor_metrics.average_processing_time_ms
            + analytics_coordinator_metrics.average_query_time_ms)
            / 2.0;

        let cache_hit_rate = (data_processor_metrics.cache_hit_rate
            + metrics_aggregator_metrics.cache_hit_rate)
            / 2.0;

        let queries_per_second = data_processor_metrics.processing_rate_per_second;
        let reports_per_hour = report_generator_metrics.reports_per_hour;
        let metrics_per_minute = metrics_aggregator_metrics.metrics_per_minute;

        let error_rate = (data_processor_metrics.error_rate
            + report_generator_metrics.error_rate
            + metrics_aggregator_metrics.error_rate
            + analytics_coordinator_metrics.error_rate)
            / 4.0;

        Ok(AnalyticsModuleMetrics {
            total_queries_processed,
            total_reports_generated,
            total_metrics_collected,
            average_query_latency_ms,
            cache_hit_rate,
            data_processor_metrics,
            report_generator_metrics,
            metrics_aggregator_metrics,
            analytics_coordinator_metrics,
            queries_per_second,
            reports_per_hour,
            metrics_per_minute,
            error_rate,
            last_updated: worker::Date::now().as_millis(),
        })
    }

    /// Access to data processor component
    pub fn data_processor(&self) -> &DataProcessor {
        &self.data_processor
    }

    /// Access to report generator component
    pub fn report_generator(&self) -> &ReportGenerator {
        &self.report_generator
    }

    /// Access to metrics aggregator component
    pub fn metrics_aggregator(&self) -> &MetricsAggregator {
        &self.metrics_aggregator
    }

    /// Access to analytics coordinator component
    pub fn analytics_coordinator(&self) -> &AnalyticsCoordinator {
        &self.analytics_coordinator
    }

    /// Check if module is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Get module configuration
    pub fn config(&self) -> &AnalyticsModuleConfig {
        &self.config
    }

    /// Get startup time
    pub fn startup_time(&self) -> Option<u64> {
        self.startup_time
    }
}

/// Analytics Module utility functions
pub mod utils {
    use super::*;

    /// Create high-performance analytics configuration
    pub fn create_high_performance_config() -> AnalyticsModuleConfig {
        AnalyticsModuleConfig::high_performance()
    }

    /// Create high-reliability analytics configuration
    pub fn create_high_reliability_config() -> AnalyticsModuleConfig {
        AnalyticsModuleConfig::high_reliability()
    }

    /// Create development analytics configuration
    pub fn create_development_config() -> AnalyticsModuleConfig {
        AnalyticsModuleConfig {
            max_concurrent_queries: 10,
            batch_processing_size: 50,
            retention_days: 30,
            enable_predictive_analytics: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analytics_module_config_default() {
        let config = AnalyticsModuleConfig::default();
        assert!(config.enable_real_time_processing);
        assert!(config.enable_automated_reporting);
        assert_eq!(config.max_concurrent_queries, 100);
        assert_eq!(config.batch_processing_size, 200);
    }

    #[test]
    fn test_high_performance_config() {
        let config = AnalyticsModuleConfig::high_performance();
        assert_eq!(config.max_concurrent_queries, 200);
        assert_eq!(config.batch_processing_size, 500);
        assert_eq!(config.cache_ttl_seconds, 180);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = AnalyticsModuleConfig::high_reliability();
        assert_eq!(config.max_concurrent_queries, 50);
        assert_eq!(config.retention_days, 365);
        assert!(!config.enable_predictive_analytics);
    }

    #[test]
    fn test_config_validation() {
        let mut config = AnalyticsModuleConfig::default();
        assert!(config.validate().is_ok());

        config.max_concurrent_queries = 0;
        assert!(config.validate().is_err());
    }
}
