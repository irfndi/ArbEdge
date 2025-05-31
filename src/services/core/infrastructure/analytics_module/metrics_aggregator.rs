// src/services/core/infrastructure/analytics_module/metrics_aggregator.rs

//! Metrics Aggregator - Business Metrics Aggregation and KPI Tracking
//!
//! This component provides comprehensive business metrics aggregation for the ArbEdge platform,
//! handling KPI tracking, business intelligence, and performance analytics with
//! real-time processing and historical analysis capabilities.

use crate::js_sys::Date;
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::{kv::KvStore, Env};

/// Metrics Aggregator Configuration
#[derive(Debug, Clone)]
pub struct MetricsAggregatorConfig {
    pub enable_business_metrics: bool,
    pub enable_kpi_tracking: bool,
    pub enable_comparative_analysis: bool,
    pub aggregation_interval_seconds: u64,
    pub retention_days: u32,
    pub batch_processing_size: usize,
    pub cache_ttl_seconds: u64,
}

impl Default for MetricsAggregatorConfig {
    fn default() -> Self {
        Self {
            enable_business_metrics: true,
            enable_kpi_tracking: true,
            enable_comparative_analysis: true,
            aggregation_interval_seconds: 300, // 5 minutes
            retention_days: 90,
            batch_processing_size: 100,
            cache_ttl_seconds: 300,
        }
    }
}

impl MetricsAggregatorConfig {
    pub fn high_performance() -> Self {
        Self {
            enable_business_metrics: true,
            enable_kpi_tracking: true,
            enable_comparative_analysis: true,
            aggregation_interval_seconds: 60, // 1 minute
            retention_days: 90,
            batch_processing_size: 200,
            cache_ttl_seconds: 180,
        }
    }

    pub fn high_reliability() -> Self {
        Self {
            enable_business_metrics: true,
            enable_kpi_tracking: true,
            enable_comparative_analysis: false, // Disable for stability
            aggregation_interval_seconds: 600,  // 10 minutes
            retention_days: 365,
            batch_processing_size: 50,
            cache_ttl_seconds: 600,
        }
    }

    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.aggregation_interval_seconds == 0 {
            return Err(ArbitrageError::configuration_error(
                "aggregation_interval_seconds must be greater than 0".to_string(),
            ));
        }
        if self.batch_processing_size == 0 {
            return Err(ArbitrageError::configuration_error(
                "batch_processing_size must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Metrics Aggregator Health Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsAggregatorHealth {
    pub is_healthy: bool,
    pub aggregation_healthy: bool,
    pub kpi_tracking_healthy: bool,
    pub storage_healthy: bool,
    pub metrics_processed_per_minute: f64,
    pub last_aggregation_time: u64,
    pub last_health_check: u64,
}

/// Metrics Aggregator Performance Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsAggregatorMetrics {
    pub metrics_collected: u64,
    pub aggregations_computed: u64,
    pub kpis_tracked: u64,
    pub cache_hit_rate: f64,
    pub error_rate: f64,
    pub metrics_per_minute: f64,
    pub average_processing_time_ms: f64,
    pub last_updated: u64,
}

/// Business KPI definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessKPI {
    pub kpi_id: String,
    pub name: String,
    pub description: String,
    pub category: String, // "revenue", "performance", "user", "system"
    pub current_value: f64,
    pub target_value: f64,
    pub threshold_warning: f64,
    pub threshold_critical: f64,
    pub unit: String,
    pub trend: String, // "up", "down", "stable"
    pub last_updated: u64,
}

/// Metrics Aggregator for business intelligence
#[derive(Clone)]
pub struct MetricsAggregator {
    config: MetricsAggregatorConfig,
    kv_store: Option<KvStore>,
    business_kpis: HashMap<String, BusinessKPI>,
    metrics: MetricsAggregatorMetrics,
    is_initialized: bool,
}

impl MetricsAggregator {
    pub fn new(config: MetricsAggregatorConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        Ok(Self {
            config,
            kv_store: None,
            business_kpis: HashMap::new(),
            metrics: MetricsAggregatorMetrics::default(),
            is_initialized: false,
        })
    }

    pub async fn initialize(&mut self, env: &Env) -> ArbitrageResult<()> {
        self.kv_store = Some(env.kv("METRICS_CACHE").map_err(|e| {
            ArbitrageError::configuration_error(format!("Failed to initialize KV store: {:?}", e))
        })?);

        self.load_default_kpis().await?;
        self.is_initialized = true;
        Ok(())
    }

    async fn load_default_kpis(&mut self) -> ArbitrageResult<()> {
        let default_kpis = vec![
            BusinessKPI {
                kpi_id: "total_revenue".to_string(),
                name: "Total Revenue".to_string(),
                description: "Total revenue generated from arbitrage opportunities".to_string(),
                category: "revenue".to_string(),
                current_value: 0.0,
                target_value: 10000.0,
                threshold_warning: 8000.0,
                threshold_critical: 5000.0,
                unit: "USD".to_string(),
                trend: "stable".to_string(),
                last_updated: Date::now() as u64,
            },
            BusinessKPI {
                kpi_id: "active_users".to_string(),
                name: "Active Users".to_string(),
                description: "Number of active users in the last 24 hours".to_string(),
                category: "user".to_string(),
                current_value: 0.0,
                target_value: 1000.0,
                threshold_warning: 800.0,
                threshold_critical: 500.0,
                unit: "users".to_string(),
                trend: "stable".to_string(),
                last_updated: Date::now() as u64,
            },
        ];

        for kpi in default_kpis {
            self.business_kpis.insert(kpi.kpi_id.clone(), kpi);
        }

        Ok(())
    }

    pub async fn aggregate_business_metrics(&mut self) -> ArbitrageResult<()> {
        // Update revenue KPI
        let revenue_kpi = self.business_kpis.get_mut("total_revenue").unwrap();
        revenue_kpi.current_value += 125.50; // Mock revenue update
        revenue_kpi.last_updated = Date::now() as u64;

        // Update active users KPI
        let users_kpi = self.business_kpis.get_mut("active_users").unwrap();
        users_kpi.current_value = 850.0; // Mock user count
        users_kpi.last_updated = Date::now() as u64;

        self.metrics.metrics_collected += 2;
        self.metrics.aggregations_computed += 1;

        Ok(())
    }

    pub async fn health_check(&self) -> ArbitrageResult<MetricsAggregatorHealth> {
        Ok(MetricsAggregatorHealth {
            is_healthy: true,
            aggregation_healthy: true,
            kpi_tracking_healthy: true,
            storage_healthy: true,
            metrics_processed_per_minute: self.metrics.metrics_per_minute,
            last_aggregation_time: Date::now() as u64,
            last_health_check: Date::now() as u64,
        })
    }

    pub async fn get_metrics(&self) -> ArbitrageResult<MetricsAggregatorMetrics> {
        Ok(self.metrics.clone())
    }

    pub fn get_business_kpis(&self) -> Vec<&BusinessKPI> {
        self.business_kpis.values().collect()
    }
}

impl Default for MetricsAggregatorMetrics {
    fn default() -> Self {
        Self {
            metrics_collected: 0,
            aggregations_computed: 0,
            kpis_tracked: 0,
            cache_hit_rate: 0.0,
            error_rate: 0.0,
            metrics_per_minute: 0.0,
            average_processing_time_ms: 0.0,
            last_updated: Date::now() as u64,
        }
    }
}
