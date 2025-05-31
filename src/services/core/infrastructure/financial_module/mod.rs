// src/services/core/infrastructure/financial_module/mod.rs

//! Financial Module - Real-Time Financial Monitoring and Analysis System
//!
//! This module provides comprehensive financial monitoring capabilities for the ArbEdge platform,
//! replacing the monolithic fund_monitoring.rs with a modular architecture optimized
//! for high-concurrency trading operations (1000-2500 concurrent users).
//!
//! ## Modular Architecture (3 Components):
//!
//! 1. **BalanceTracker** - Real-time balance monitoring across exchanges
//! 2. **FundAnalyzer** - Financial analysis and portfolio optimization
//! 3. **FinancialCoordinator** - Main orchestrator for financial operations
//!
//! ## Revolutionary Features:
//! - **Multi-Exchange Balance Tracking**: Real-time balance monitoring across all exchanges
//! - **Portfolio Analytics**: P&L calculation, risk assessment, performance metrics
//! - **Fund Optimization**: Intelligent fund allocation and rebalancing recommendations
//! - **Chaos Engineering**: Circuit breakers and fallback strategies for financial data
//! - **Core D1, KV, R2 Integration**: Persistent storage, caching, and backup strategies

pub mod balance_tracker;
pub mod financial_coordinator;
pub mod fund_analyzer;

pub use balance_tracker::{
    BalanceTracker, BalanceTrackerConfig, BalanceTrackerHealth, BalanceTrackerMetrics,
};
pub use financial_coordinator::{
    FinancialCoordinator, FinancialCoordinatorConfig, FinancialCoordinatorHealth,
    FinancialCoordinatorMetrics,
};
pub use fund_analyzer::{
    FundAnalyzer, FundAnalyzerConfig, FundAnalyzerHealth, FundAnalyzerMetrics,
};

use crate::types::{Balance, Balances};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::Env;

/// Financial Module Configuration for High-Performance Financial Operations
#[derive(Debug, Clone)]
pub struct FinancialModuleConfig {
    // Core financial settings
    pub enable_real_time_monitoring: bool,
    pub enable_portfolio_analytics: bool,
    pub enable_fund_optimization: bool,
    pub enable_risk_assessment: bool,

    // Performance settings optimized for 1000-2500 concurrent users
    pub update_interval_seconds: u64,
    pub cache_ttl_seconds: u64,
    pub batch_processing_size: usize,
    pub max_concurrent_operations: u32,

    // Component configurations
    pub balance_tracker_config: BalanceTrackerConfig,
    pub fund_analyzer_config: FundAnalyzerConfig,
    pub financial_coordinator_config: FinancialCoordinatorConfig,
}

impl Default for FinancialModuleConfig {
    fn default() -> Self {
        Self {
            enable_real_time_monitoring: true,
            enable_portfolio_analytics: true,
            enable_fund_optimization: true,
            enable_risk_assessment: true,
            update_interval_seconds: 30,
            cache_ttl_seconds: 300,
            batch_processing_size: 100,
            max_concurrent_operations: 50,
            balance_tracker_config: BalanceTrackerConfig::default(),
            fund_analyzer_config: FundAnalyzerConfig::default(),
            financial_coordinator_config: FinancialCoordinatorConfig::default(),
        }
    }
}

impl FinancialModuleConfig {
    /// High-performance configuration for 1000-2500 concurrent users
    pub fn high_performance() -> Self {
        Self {
            enable_real_time_monitoring: true,
            enable_portfolio_analytics: true,
            enable_fund_optimization: true,
            enable_risk_assessment: true,
            update_interval_seconds: 15,
            cache_ttl_seconds: 180,
            batch_processing_size: 200,
            max_concurrent_operations: 100,
            balance_tracker_config: BalanceTrackerConfig::high_performance(),
            fund_analyzer_config: FundAnalyzerConfig::high_performance(),
            financial_coordinator_config: FinancialCoordinatorConfig::high_performance(),
        }
    }

    /// High-reliability configuration with enhanced data retention
    pub fn high_reliability() -> Self {
        Self {
            enable_real_time_monitoring: true,
            enable_portfolio_analytics: true,
            enable_fund_optimization: false, // Disable for stability
            enable_risk_assessment: true,
            update_interval_seconds: 60,
            cache_ttl_seconds: 600,
            batch_processing_size: 50,
            max_concurrent_operations: 25,
            balance_tracker_config: BalanceTrackerConfig::high_reliability(),
            fund_analyzer_config: FundAnalyzerConfig::high_reliability(),
            financial_coordinator_config: FinancialCoordinatorConfig::high_reliability(),
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.update_interval_seconds == 0 {
            return Err(ArbitrageError::configuration_error(
                "update_interval_seconds must be greater than 0".to_string(),
            ));
        }
        if self.batch_processing_size == 0 {
            return Err(ArbitrageError::configuration_error(
                "batch_processing_size must be greater than 0".to_string(),
            ));
        }
        if self.max_concurrent_operations == 0 {
            return Err(ArbitrageError::configuration_error(
                "max_concurrent_operations must be greater than 0".to_string(),
            ));
        }

        // Validate component configurations
        self.balance_tracker_config.validate()?;
        self.fund_analyzer_config.validate()?;
        self.financial_coordinator_config.validate()?;

        Ok(())
    }
}

/// Financial Module Health Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialModuleHealth {
    pub overall_health: bool,
    pub health_percentage: f64,
    pub component_health: HashMap<String, bool>,
    pub balance_tracker_health: BalanceTrackerHealth,
    pub fund_analyzer_health: FundAnalyzerHealth,
    pub financial_coordinator_health: FinancialCoordinatorHealth,
    pub last_health_check: u64,
    pub uptime_seconds: u64,
}

/// Financial Module Performance Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialModuleMetrics {
    // Overall metrics
    pub total_balances_tracked: u64,
    pub total_analyses_performed: u64,
    pub total_optimizations_completed: u64,
    pub average_operation_latency_ms: f64,
    pub cache_hit_rate: f64,

    // Component metrics
    pub balance_tracker_metrics: BalanceTrackerMetrics,
    pub fund_analyzer_metrics: FundAnalyzerMetrics,
    pub financial_coordinator_metrics: FinancialCoordinatorMetrics,

    // Performance tracking
    pub operations_per_second: f64,
    pub analyses_per_hour: f64,
    pub optimizations_per_day: f64,
    pub error_rate: f64,
    pub last_updated: u64,
}

/// Exchange balance snapshot for financial tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeBalanceSnapshot {
    pub exchange_id: String,
    pub balances: Balances,
    pub timestamp: u64,
    pub total_usd_value: f64,
    pub last_updated: String,
}

/// Fund allocation recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundAllocation {
    pub exchange_id: String,
    pub asset: String,
    pub current_amount: f64,
    pub optimal_amount: f64,
    pub variance_percentage: f64,
    pub action_needed: String, // "buy", "sell", "hold", "transfer"
    pub priority: String,      // "high", "medium", "low"
    pub estimated_impact: f64,
}

/// Balance history entry for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceHistoryEntry {
    pub id: String,
    pub user_id: String,
    pub exchange_id: String,
    pub asset: String,
    pub balance: Balance,
    pub usd_value: f64,
    pub timestamp: u64,
    pub snapshot_id: String,
}

/// Fund optimization result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundOptimizationResult {
    pub allocations: Vec<FundAllocation>,
    pub total_portfolio_value: f64,
    pub optimization_score: f64,
    pub recommendations: Vec<String>,
    pub risk_assessment: String,
    pub expected_improvement: f64,
    pub implementation_priority: String,
}

/// Portfolio analytics result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioAnalytics {
    pub total_value_usd: f64,
    pub total_value_change_24h: f64,
    pub total_value_change_percentage: f64,
    pub best_performing_asset: String,
    pub worst_performing_asset: String,
    pub portfolio_diversity_score: f64,
    pub risk_score: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub exchange_distribution: HashMap<String, f64>,
    pub asset_distribution: HashMap<String, f64>,
}

/// Main Financial Module orchestrating all financial components
pub struct FinancialModule {
    config: FinancialModuleConfig,
    balance_tracker: BalanceTracker,
    fund_analyzer: FundAnalyzer,
    financial_coordinator: FinancialCoordinator,
    is_initialized: bool,
    startup_time: Option<u64>,
}

impl FinancialModule {
    /// Create new Financial Module with configuration
    pub fn new(config: FinancialModuleConfig) -> ArbitrageResult<Self> {
        // Validate configuration
        config.validate()?;

        // Create components
        let balance_tracker = BalanceTracker::new(config.balance_tracker_config.clone())?;
        let fund_analyzer = FundAnalyzer::new(config.fund_analyzer_config.clone())?;
        let financial_coordinator =
            FinancialCoordinator::new(config.financial_coordinator_config.clone())?;

        Ok(Self {
            config,
            balance_tracker,
            fund_analyzer,
            financial_coordinator,
            is_initialized: false,
            startup_time: None,
        })
    }

    /// Initialize the Financial Module with environment
    pub async fn initialize(&mut self, env: &Env) -> ArbitrageResult<()> {
        let start_time = worker::Date::now().as_millis();

        // Initialize all components
        self.balance_tracker.initialize(env).await?;
        self.fund_analyzer.initialize(env).await?;
        self.financial_coordinator.initialize(env).await?;

        self.is_initialized = true;
        self.startup_time = Some(start_time);

        Ok(())
    }

    /// Get comprehensive health status
    pub async fn health_check(&self) -> ArbitrageResult<FinancialModuleHealth> {
        let balance_tracker_health = self.balance_tracker.health_check().await?;
        let fund_analyzer_health = self.fund_analyzer.health_check().await?;
        let financial_coordinator_health = self.financial_coordinator.health_check().await?;

        let mut component_health = HashMap::new();
        component_health.insert(
            "balance_tracker".to_string(),
            balance_tracker_health.is_healthy,
        );
        component_health.insert("fund_analyzer".to_string(), fund_analyzer_health.is_healthy);
        component_health.insert(
            "financial_coordinator".to_string(),
            financial_coordinator_health.is_healthy,
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

        Ok(FinancialModuleHealth {
            overall_health,
            health_percentage,
            component_health,
            balance_tracker_health,
            fund_analyzer_health,
            financial_coordinator_health,
            last_health_check: worker::Date::now().as_millis(),
            uptime_seconds,
        })
    }

    /// Get comprehensive performance metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<FinancialModuleMetrics> {
        let balance_tracker_metrics = self.balance_tracker.get_metrics().await?;
        let fund_analyzer_metrics = self.fund_analyzer.get_metrics().await?;
        let financial_coordinator_metrics = self.financial_coordinator.get_metrics().await?;

        // Calculate overall metrics
        let total_balances_tracked = balance_tracker_metrics.balances_tracked;
        let total_analyses_performed = fund_analyzer_metrics.analyses_performed;
        let total_optimizations_completed = fund_analyzer_metrics.optimizations_completed;

        let average_operation_latency_ms = (balance_tracker_metrics.average_update_time_ms
            + fund_analyzer_metrics.average_analysis_time_ms
            + financial_coordinator_metrics.average_operation_time_ms)
            / 3.0;

        let cache_hit_rate = (balance_tracker_metrics.cache_hit_rate
            + fund_analyzer_metrics.cache_hit_rate
            + financial_coordinator_metrics.cache_hit_rate)
            / 3.0;

        let operations_per_second = balance_tracker_metrics.updates_per_second;
        let analyses_per_hour = fund_analyzer_metrics.analyses_per_hour;
        let optimizations_per_day = fund_analyzer_metrics.optimizations_per_day;

        let error_rate = (balance_tracker_metrics.error_rate
            + fund_analyzer_metrics.error_rate
            + financial_coordinator_metrics.error_rate)
            / 3.0;

        Ok(FinancialModuleMetrics {
            total_balances_tracked,
            total_analyses_performed,
            total_optimizations_completed,
            average_operation_latency_ms,
            cache_hit_rate,
            balance_tracker_metrics,
            fund_analyzer_metrics,
            financial_coordinator_metrics,
            operations_per_second,
            analyses_per_hour,
            optimizations_per_day,
            error_rate,
            last_updated: worker::Date::now().as_millis(),
        })
    }

    /// Get real-time balances for a user across all exchanges
    pub async fn get_real_time_balances(
        &mut self,
        user_id: &str,
        exchange_ids: &[String],
    ) -> ArbitrageResult<HashMap<String, ExchangeBalanceSnapshot>> {
        self.balance_tracker
            .get_real_time_balances(user_id, exchange_ids)
            .await
    }

    /// Analyze portfolio for a user
    pub async fn analyze_portfolio(
        &mut self,
        user_id: &str,
        exchange_id: Option<&str>,
    ) -> ArbitrageResult<PortfolioAnalytics> {
        // Get balance snapshots for the user
        let exchange_ids = if let Some(id) = exchange_id {
            vec![id.to_string()]
        } else {
            vec![] // Empty means all exchanges
        };

        let balance_snapshots = self.get_real_time_balances(user_id, &exchange_ids).await?;

        self.fund_analyzer
            .analyze_portfolio(user_id, &balance_snapshots)
            .await
    }

    /// Optimize fund allocation for a user
    pub async fn optimize_fund_allocation(
        &mut self,
        user_id: &str,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
        target_allocation: &HashMap<String, f64>,
    ) -> ArbitrageResult<FundOptimizationResult> {
        self.fund_analyzer
            .optimize_fund_allocation(user_id, balance_snapshots, target_allocation)
            .await
    }

    /// Get balance history for a user
    pub async fn get_balance_history(
        &mut self,
        user_id: &str,
        exchange_id: Option<&str>,
        asset: Option<&str>,
        from_timestamp: Option<u64>,
        to_timestamp: Option<u64>,
        limit: Option<u32>,
    ) -> ArbitrageResult<Vec<BalanceHistoryEntry>> {
        self.balance_tracker
            .get_balance_history(
                user_id,
                exchange_id,
                asset,
                from_timestamp,
                to_timestamp,
                limit,
            )
            .await
    }

    /// Access to balance tracker component
    pub fn balance_tracker(&self) -> &BalanceTracker {
        &self.balance_tracker
    }

    /// Access to fund analyzer component
    pub fn fund_analyzer(&self) -> &FundAnalyzer {
        &self.fund_analyzer
    }

    /// Access to financial coordinator component
    pub fn financial_coordinator(&self) -> &FinancialCoordinator {
        &self.financial_coordinator
    }

    /// Check if module is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Get module configuration
    pub fn config(&self) -> &FinancialModuleConfig {
        &self.config
    }

    /// Get startup time
    pub fn startup_time(&self) -> Option<u64> {
        self.startup_time
    }

    /// Optimize portfolio for a user
    pub async fn optimize_portfolio(
        &mut self,
        user_id: &str,
        target_allocation: &HashMap<String, f64>,
    ) -> ArbitrageResult<FundOptimizationResult> {
        // Get current balance snapshots
        let balance_snapshots = self.get_real_time_balances(user_id, &[]).await?;

        self.fund_analyzer
            .optimize_fund_allocation(user_id, &balance_snapshots, target_allocation)
            .await
    }

    /// Get balance tracker (mutable access)
    pub fn get_balance_tracker_mut(&mut self) -> &mut BalanceTracker {
        &mut self.balance_tracker
    }

    /// Get fund analyzer (mutable access)
    pub fn get_fund_analyzer_mut(&mut self) -> &mut FundAnalyzer {
        &mut self.fund_analyzer
    }
}

/// Financial Module utility functions
pub mod utils {
    use super::*;

    /// Create high-performance financial configuration
    pub fn create_high_performance_config() -> FinancialModuleConfig {
        FinancialModuleConfig::high_performance()
    }

    /// Create high-reliability financial configuration
    pub fn create_high_reliability_config() -> FinancialModuleConfig {
        FinancialModuleConfig::high_reliability()
    }

    /// Create development financial configuration
    pub fn create_development_config() -> FinancialModuleConfig {
        FinancialModuleConfig {
            update_interval_seconds: 60,
            batch_processing_size: 25,
            max_concurrent_operations: 10,
            enable_fund_optimization: false,
            ..Default::default()
        }
    }

    /// Calculate portfolio diversity score
    pub fn calculate_diversity_score(asset_distribution: &HashMap<String, f64>) -> f64 {
        if asset_distribution.is_empty() {
            return 0.0;
        }

        // Calculate Herfindahl-Hirschman Index (HHI) for diversity
        let hhi: f64 = asset_distribution
            .values()
            .map(|&percentage| (percentage / 100.0).powi(2))
            .sum();

        // Convert to diversity score (1 - HHI, normalized to 0-100)
        ((1.0 - hhi) * 100.0).clamp(0.0, 100.0)
    }

    /// Calculate risk score based on portfolio composition
    pub fn calculate_risk_score(
        asset_distribution: &HashMap<String, f64>,
        volatility_data: &HashMap<String, f64>,
    ) -> f64 {
        if asset_distribution.is_empty() || volatility_data.is_empty() {
            return 50.0; // Neutral risk score
        }

        let weighted_volatility: f64 = asset_distribution
            .iter()
            .map(|(asset, &percentage)| {
                let volatility = volatility_data.get(asset).unwrap_or(&0.5); // Default 50% volatility
                (percentage / 100.0) * volatility
            })
            .sum();

        // Normalize to 0-100 scale
        (weighted_volatility * 100.0).clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_financial_module_config_default() {
        let config = FinancialModuleConfig::default();
        assert!(config.enable_real_time_monitoring);
        assert!(config.enable_portfolio_analytics);
        assert_eq!(config.update_interval_seconds, 30);
        assert_eq!(config.batch_processing_size, 100);
    }

    #[test]
    fn test_high_performance_config() {
        let config = FinancialModuleConfig::high_performance();
        assert_eq!(config.update_interval_seconds, 15);
        assert_eq!(config.batch_processing_size, 200);
        assert_eq!(config.max_concurrent_operations, 100);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = FinancialModuleConfig::high_reliability();
        assert_eq!(config.update_interval_seconds, 60);
        assert_eq!(config.max_concurrent_operations, 25);
        assert!(!config.enable_fund_optimization);
    }

    #[test]
    fn test_config_validation() {
        let mut config = FinancialModuleConfig::default();
        assert!(config.validate().is_ok());

        config.update_interval_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_diversity_score_calculation() {
        let mut distribution = HashMap::new();
        distribution.insert("BTC".to_string(), 50.0);
        distribution.insert("ETH".to_string(), 30.0);
        distribution.insert("USDT".to_string(), 20.0);

        let score = utils::calculate_diversity_score(&distribution);
        assert!(score > 0.0 && score <= 100.0);
    }

    #[test]
    fn test_risk_score_calculation() {
        let mut distribution = HashMap::new();
        distribution.insert("BTC".to_string(), 60.0);
        distribution.insert("ETH".to_string(), 40.0);

        let mut volatility = HashMap::new();
        volatility.insert("BTC".to_string(), 0.8);
        volatility.insert("ETH".to_string(), 0.9);

        let score = utils::calculate_risk_score(&distribution, &volatility);
        assert!(score > 0.0 && score <= 100.0);
    }
}
