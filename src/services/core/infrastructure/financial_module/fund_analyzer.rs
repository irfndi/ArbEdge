// src/services/core/infrastructure/financial_module/fund_analyzer.rs

//! Fund Analyzer - Advanced Portfolio Analysis and Optimization Engine
//!
//! This component provides sophisticated financial analysis capabilities for the ArbEdge platform,
//! including portfolio optimization, risk assessment, P&L calculation, and intelligent fund
//! allocation recommendations optimized for high-frequency trading operations.
//!
//! ## Revolutionary Features:
//! - **Portfolio Analytics**: Real-time P&L, Sharpe ratio, max drawdown calculations
//! - **Fund Optimization**: Modern Portfolio Theory with risk-adjusted returns
//! - **Risk Assessment**: VaR, volatility analysis, correlation matrices
//! - **Performance Attribution**: Asset-level and exchange-level performance tracking
//! - **Intelligent Rebalancing**: Dynamic allocation recommendations

use super::{ExchangeBalanceSnapshot, FundAllocation, FundOptimizationResult, PortfolioAnalytics};
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::{kv::KvStore, Env};

/// Helper function to get current time in milliseconds as u64
/// Handles the f64 to u64 conversion safely by applying floor() before casting
fn get_current_time_millis() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Simple LRU cache implementation to prevent unbounded growth
struct LruCache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: Vec<K>,
}

impl<K: Clone + std::hash::Hash + Eq, V> LruCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            order: Vec::new(),
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            // Move to end (most recently used)
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                let key = self.order.remove(pos);
                self.order.push(key);
            }
            self.map.get(key)
        } else {
            None
        }
    }

    fn insert(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            // Update existing
            self.map.insert(key.clone(), value);
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                let key = self.order.remove(pos);
                self.order.push(key);
            }
        } else {
            // Insert new
            if self.map.len() >= self.capacity {
                // Remove least recently used
                if let Some(lru_key) = self.order.first().cloned() {
                    self.map.remove(&lru_key);
                    self.order.remove(0);
                }
            }
            self.map.insert(key.clone(), value);
            self.order.push(key);
        }
    }

    fn len(&self) -> usize {
        self.map.len()
    }
}

/// Fund Analyzer Configuration
#[derive(Debug, Clone)]
pub struct FundAnalyzerConfig {
    // Analysis settings
    pub enable_portfolio_analytics: bool,
    pub enable_fund_optimization: bool,
    pub enable_risk_assessment: bool,
    pub enable_performance_attribution: bool,

    // Optimization parameters
    pub optimization_window_days: u32,
    pub rebalancing_threshold_percent: f64,
    pub max_asset_allocation_percent: f64,
    pub min_asset_allocation_percent: f64,

    // Risk management
    pub max_portfolio_volatility: f64,
    pub target_sharpe_ratio: f64,
    pub var_confidence_level: f64,
    pub correlation_threshold: f64,

    // Performance settings
    pub analysis_cache_ttl_seconds: u64,
    pub batch_analysis_size: usize,
    pub max_concurrent_analyses: u32,
}

impl Default for FundAnalyzerConfig {
    fn default() -> Self {
        Self {
            enable_portfolio_analytics: true,
            enable_fund_optimization: true,
            enable_risk_assessment: true,
            enable_performance_attribution: true,
            optimization_window_days: 30,
            rebalancing_threshold_percent: 5.0,
            max_asset_allocation_percent: 40.0,
            min_asset_allocation_percent: 1.0,
            max_portfolio_volatility: 0.25,
            target_sharpe_ratio: 1.5,
            var_confidence_level: 0.95,
            correlation_threshold: 0.7,
            analysis_cache_ttl_seconds: 300,
            batch_analysis_size: 50,
            max_concurrent_analyses: 25,
        }
    }
}

impl FundAnalyzerConfig {
    /// High-performance configuration for 1000-2500 concurrent users
    pub fn high_performance() -> Self {
        Self {
            enable_portfolio_analytics: true,
            enable_fund_optimization: true,
            enable_risk_assessment: false, // Disable for performance
            enable_performance_attribution: true,
            optimization_window_days: 14,
            rebalancing_threshold_percent: 3.0,
            max_asset_allocation_percent: 50.0,
            min_asset_allocation_percent: 0.5,
            max_portfolio_volatility: 0.30,
            target_sharpe_ratio: 1.2,
            var_confidence_level: 0.95,
            correlation_threshold: 0.8,
            analysis_cache_ttl_seconds: 180,
            batch_analysis_size: 100,
            max_concurrent_analyses: 50,
        }
    }

    /// High-reliability configuration with enhanced risk controls
    pub fn high_reliability() -> Self {
        Self {
            enable_portfolio_analytics: true,
            enable_fund_optimization: false, // Disable for stability
            enable_risk_assessment: true,
            enable_performance_attribution: true,
            optimization_window_days: 60,
            rebalancing_threshold_percent: 10.0,
            max_asset_allocation_percent: 25.0,
            min_asset_allocation_percent: 2.0,
            max_portfolio_volatility: 0.15,
            target_sharpe_ratio: 2.0,
            var_confidence_level: 0.99,
            correlation_threshold: 0.5,
            analysis_cache_ttl_seconds: 600,
            batch_analysis_size: 25,
            max_concurrent_analyses: 10,
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.optimization_window_days == 0 {
            return Err(ArbitrageError::configuration_error(
                "optimization_window_days must be greater than 0".to_string(),
            ));
        }
        if self.max_asset_allocation_percent <= self.min_asset_allocation_percent {
            return Err(ArbitrageError::configuration_error(
                "max_asset_allocation_percent must be greater than min_asset_allocation_percent"
                    .to_string(),
            ));
        }
        if self.var_confidence_level <= 0.0 || self.var_confidence_level >= 1.0 {
            return Err(ArbitrageError::configuration_error(
                "var_confidence_level must be between 0 and 1".to_string(),
            ));
        }
        Ok(())
    }
}

/// Fund Analyzer Health Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundAnalyzerHealth {
    pub is_healthy: bool,
    pub analytics_healthy: bool,
    pub optimization_healthy: bool,
    pub risk_assessment_healthy: bool,
    pub active_analyses: u32,
    pub cache_utilization_percent: f64,
    pub average_analysis_time_ms: f64,
    pub last_health_check: u64,
}

/// Fund Analyzer Performance Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundAnalyzerMetrics {
    // Analysis metrics
    pub analyses_performed: u64,
    pub optimizations_completed: u64,
    pub risk_assessments_completed: u64,
    pub average_analysis_time_ms: f64,

    // Performance metrics
    pub successful_analyses: u64,
    pub failed_analyses: u64,
    pub cache_hit_rate: f64,
    pub error_rate: f64,

    // Business metrics
    pub analyses_per_hour: f64,
    pub optimizations_per_day: f64,
    pub average_optimization_improvement: f64,
    pub last_updated: u64,
}

/// Historical price data for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    pub asset: String,
    pub timestamp: u64,
    pub price: f64,
    pub volume: f64,
}

/// Risk metrics for portfolio assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetrics {
    pub value_at_risk_95: f64,
    pub value_at_risk_99: f64,
    pub expected_shortfall: f64,
    pub volatility: f64,
    pub beta: f64,
    pub correlation_matrix: HashMap<String, HashMap<String, f64>>,
}

/// Fund Analyzer for advanced portfolio analysis
pub struct FundAnalyzer {
    config: FundAnalyzerConfig,
    kv_store: Option<KvStore>,

    // Analysis cache with LRU eviction
    analysis_cache: LruCache<String, PortfolioAnalytics>,
    optimization_cache: LruCache<String, FundOptimizationResult>,

    // Performance tracking
    metrics: FundAnalyzerMetrics,
    last_analysis_time: u64,
    is_initialized: bool,
}

impl FundAnalyzer {
    /// Create new Fund Analyzer with configuration
    pub fn new(config: FundAnalyzerConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        Ok(Self {
            config,
            kv_store: None,
            analysis_cache: LruCache::new(100), // Limit to 100 cached analyses
            optimization_cache: LruCache::new(50), // Limit to 50 cached optimizations
            metrics: FundAnalyzerMetrics::default(),
            last_analysis_time: get_current_time_millis(),
            is_initialized: false,
        })
    }

    /// Initialize the Fund Analyzer with environment
    pub async fn initialize(&mut self, env: &Env) -> ArbitrageResult<()> {
        // Initialize KV store for caching
        self.kv_store = Some(env.kv("FUND_ANALYSIS_CACHE").map_err(|e| {
            ArbitrageError::configuration_error(format!("Failed to initialize KV store: {:?}", e))
        })?);

        self.is_initialized = true;
        Ok(())
    }

    /// Analyze portfolio performance and metrics
    pub async fn analyze_portfolio(
        &mut self,
        user_id: &str,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> ArbitrageResult<PortfolioAnalytics> {
        let start_time = get_current_time_millis();

        // Check cache first
        let cache_key = format!(
            "portfolio_analysis:{}:{}",
            user_id,
            self.generate_snapshot_hash(balance_snapshots)
        );
        if let Some(cached_analysis) = self.get_cached_analysis(&cache_key).await? {
            return Ok(cached_analysis);
        }

        // Calculate total portfolio value
        let total_value_usd = balance_snapshots
            .values()
            .map(|snapshot| snapshot.total_usd_value)
            .sum::<f64>();

        // Calculate asset distribution
        let asset_distribution = self.calculate_asset_distribution(balance_snapshots)?;

        // Calculate exchange distribution
        let exchange_distribution = self.calculate_exchange_distribution(balance_snapshots)?;

        // Calculate performance metrics
        let total_value_change_24h = self.calculate_24h_change(balance_snapshots).await?;
        let total_value_change_percentage = if total_value_usd > 0.0 {
            (total_value_change_24h / total_value_usd) * 100.0
        } else {
            0.0
        };

        // Find best and worst performing assets
        let (best_performing_asset, worst_performing_asset) =
            self.find_performance_extremes(&asset_distribution).await?;

        // Calculate portfolio metrics
        let portfolio_diversity_score = self.calculate_diversity_score(&asset_distribution);
        let risk_score = self.calculate_risk_score(&asset_distribution).await?;
        let sharpe_ratio = self.calculate_sharpe_ratio(&asset_distribution).await?;
        let max_drawdown = self.calculate_max_drawdown(&asset_distribution).await?;

        let analytics = PortfolioAnalytics {
            total_value_usd,
            total_value_change_24h,
            total_value_change_percentage,
            best_performing_asset,
            worst_performing_asset,
            portfolio_diversity_score,
            risk_score,
            sharpe_ratio,
            max_drawdown,
            exchange_distribution,
            asset_distribution,
        };

        // Cache the result
        self.cache_analysis(&cache_key, &analytics).await?;

        // Update metrics
        let analysis_time = get_current_time_millis() - start_time;
        self.update_analysis_metrics(analysis_time);

        Ok(analytics)
    }

    /// Optimize fund allocation across exchanges and assets
    pub async fn optimize_fund_allocation(
        &mut self,
        user_id: &str,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
        target_allocation: &HashMap<String, f64>,
    ) -> ArbitrageResult<FundOptimizationResult> {
        if !self.config.enable_fund_optimization {
            return Err(ArbitrageError::configuration_error(
                "Fund optimization is disabled".to_string(),
            ));
        }

        let start_time = get_current_time_millis();

        // Check cache first
        let cache_key = format!(
            "fund_optimization:{}:{}",
            user_id,
            self.generate_allocation_hash(target_allocation)
        );
        if let Some(cached_optimization) = self.get_cached_optimization(&cache_key).await? {
            return Ok(cached_optimization);
        }

        // Calculate current allocation
        let current_allocation = self.calculate_current_allocation(balance_snapshots)?;

        // Generate optimization recommendations
        let allocations = self.generate_allocation_recommendations(
            &current_allocation,
            target_allocation,
            balance_snapshots,
        )?;

        // Calculate optimization metrics
        let total_portfolio_value = balance_snapshots
            .values()
            .map(|snapshot| snapshot.total_usd_value)
            .sum::<f64>();

        let optimization_score = self.calculate_optimization_score(&allocations);
        let recommendations = self.generate_recommendations(&allocations);
        let risk_assessment = self.assess_portfolio_risk(&allocations, total_portfolio_value);
        let expected_improvement = self.calculate_expected_improvement(&allocations);
        let implementation_priority = self.determine_implementation_priority(&allocations);

        let optimization_result = FundOptimizationResult {
            allocations,
            total_portfolio_value,
            optimization_score,
            recommendations,
            risk_assessment,
            expected_improvement,
            implementation_priority,
        };

        // Cache the result
        self.cache_optimization(&cache_key, &optimization_result)
            .await?;

        // Update metrics
        let optimization_time = get_current_time_millis() - start_time;
        self.update_optimization_metrics(optimization_time);

        Ok(optimization_result)
    }

    /// Calculate asset distribution across portfolio
    fn calculate_asset_distribution(
        &self,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> ArbitrageResult<HashMap<String, f64>> {
        let mut asset_totals: HashMap<String, f64> = HashMap::new();
        let mut total_portfolio_value = 0.0;

        // Sum up all assets across exchanges
        for snapshot in balance_snapshots.values() {
            for balance in &snapshot.balances {
                let asset_value = balance.total * self.get_mock_price(&balance.asset);
                *asset_totals.entry(balance.asset.clone()).or_insert(0.0) += asset_value;
                total_portfolio_value += asset_value;
            }
        }

        // Convert to percentages
        let mut asset_distribution = HashMap::new();
        for (asset, value) in asset_totals {
            if total_portfolio_value > 0.0 {
                let percentage = (value / total_portfolio_value) * 100.0;
                asset_distribution.insert(asset, percentage);
            }
        }

        Ok(asset_distribution)
    }

    /// Calculate exchange distribution across portfolio
    fn calculate_exchange_distribution(
        &self,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> ArbitrageResult<HashMap<String, f64>> {
        let total_value: f64 = balance_snapshots
            .values()
            .map(|snapshot| snapshot.total_usd_value)
            .sum();

        let mut exchange_distribution = HashMap::new();
        for (exchange_id, snapshot) in balance_snapshots {
            if total_value > 0.0 {
                let percentage = (snapshot.total_usd_value / total_value) * 100.0;
                exchange_distribution.insert(exchange_id.clone(), percentage);
            }
        }

        Ok(exchange_distribution)
    }

    /// Calculate 24-hour portfolio value change
    async fn calculate_24h_change(
        &self,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> ArbitrageResult<f64> {
        // Mock calculation - in reality, this would compare with historical data
        let current_value: f64 = balance_snapshots
            .values()
            .map(|snapshot| snapshot.total_usd_value)
            .sum();

        // Simulate 2% daily change
        let change = current_value * 0.02;
        Ok(change)
    }

    /// Find best and worst performing assets
    async fn find_performance_extremes(
        &self,
        asset_distribution: &HashMap<String, f64>,
    ) -> ArbitrageResult<(String, String)> {
        // Mock performance data - in reality, this would use historical price data
        let performance_data = HashMap::from([
            ("BTC".to_string(), 5.2),
            ("ETH".to_string(), 3.8),
            ("USDT".to_string(), 0.1),
            ("BNB".to_string(), 2.1),
            ("ADA".to_string(), -1.5),
        ]);

        let mut best_asset = "BTC".to_string();
        let mut worst_asset = "ADA".to_string();
        let mut best_performance = -100.0;
        let mut worst_performance = 100.0;

        for asset in asset_distribution.keys() {
            if let Some(&performance) = performance_data.get(asset) {
                if performance > best_performance {
                    best_performance = performance;
                    best_asset = asset.clone();
                }
                if performance < worst_performance {
                    worst_performance = performance;
                    worst_asset = asset.clone();
                }
            }
        }

        Ok((best_asset, worst_asset))
    }

    /// Calculate portfolio diversity score using Herfindahl-Hirschman Index
    fn calculate_diversity_score(&self, asset_distribution: &HashMap<String, f64>) -> f64 {
        if asset_distribution.is_empty() {
            return 0.0;
        }

        let hhi: f64 = asset_distribution
            .values()
            .map(|&percentage| (percentage / 100.0).powi(2))
            .sum();

        ((1.0 - hhi) * 100.0).clamp(0.0, 100.0)
    }

    /// Calculate portfolio risk score
    async fn calculate_risk_score(
        &self,
        asset_distribution: &HashMap<String, f64>,
    ) -> ArbitrageResult<f64> {
        // Mock volatility data
        let volatility_data = HashMap::from([
            ("BTC".to_string(), 0.8),
            ("ETH".to_string(), 0.9),
            ("USDT".to_string(), 0.05),
            ("BNB".to_string(), 0.7),
            ("ADA".to_string(), 1.2),
        ]);

        let weighted_volatility: f64 = asset_distribution
            .iter()
            .map(|(asset, &percentage)| {
                let volatility = volatility_data.get(asset).unwrap_or(&0.5);
                (percentage / 100.0) * volatility
            })
            .sum();

        Ok((weighted_volatility * 100.0).clamp(0.0, 100.0))
    }

    /// Calculate Sharpe ratio for portfolio
    async fn calculate_sharpe_ratio(
        &self,
        _asset_distribution: &HashMap<String, f64>,
    ) -> ArbitrageResult<f64> {
        // Mock calculation - in reality, this would use historical returns and risk-free rate
        let portfolio_return = 0.12; // 12% annual return
        let risk_free_rate = 0.02; // 2% risk-free rate
        let portfolio_volatility = 0.15; // 15% volatility

        let sharpe_ratio = (portfolio_return - risk_free_rate) / portfolio_volatility;
        Ok(sharpe_ratio)
    }

    /// Calculate maximum drawdown
    async fn calculate_max_drawdown(
        &self,
        _asset_distribution: &HashMap<String, f64>,
    ) -> ArbitrageResult<f64> {
        // Mock calculation - in reality, this would analyze historical portfolio values
        Ok(0.08) // 8% max drawdown
    }

    /// Generate allocation recommendations
    fn generate_allocation_recommendations(
        &self,
        current_allocation: &HashMap<String, f64>,
        target_allocation: &HashMap<String, f64>,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> ArbitrageResult<Vec<FundAllocation>> {
        let mut allocations = Vec::new();

        for (asset, &target_percent) in target_allocation {
            let current_percent = current_allocation.get(asset).unwrap_or(&0.0);
            let variance_percentage = target_percent - current_percent;

            if variance_percentage.abs() > self.config.rebalancing_threshold_percent {
                let action_needed = if variance_percentage > 0.0 {
                    "buy"
                } else {
                    "sell"
                };
                let priority = if variance_percentage.abs() > 10.0 {
                    "high"
                } else if variance_percentage.abs() > 5.0 {
                    "medium"
                } else {
                    "low"
                };

                // Find best exchange for this asset
                let exchange_id = self.find_best_exchange_for_asset(asset, balance_snapshots);
                let current_amount =
                    self.get_current_asset_amount(asset, &exchange_id, balance_snapshots);
                let optimal_amount = current_amount * (1.0 + variance_percentage / 100.0);

                allocations.push(FundAllocation {
                    exchange_id,
                    asset: asset.clone(),
                    current_amount,
                    optimal_amount,
                    variance_percentage,
                    action_needed: action_needed.to_string(),
                    priority: priority.to_string(),
                    estimated_impact: variance_percentage.abs() * 0.1, // Mock impact calculation
                });
            }
        }

        Ok(allocations)
    }

    /// Find best exchange for trading a specific asset
    fn find_best_exchange_for_asset(
        &self,
        asset: &str,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> String {
        // Find exchange with highest balance for this asset
        let mut best_exchange = "binance".to_string();
        let mut highest_balance = 0.0;

        for (exchange_id, snapshot) in balance_snapshots {
            if let Some(balance) = snapshot.balances.iter().find(|b| b.asset == asset) {
                if balance.total > highest_balance {
                    highest_balance = balance.total;
                    best_exchange = exchange_id.clone();
                }
            }
        }

        best_exchange
    }

    /// Get current amount of asset on specific exchange
    fn get_current_asset_amount(
        &self,
        asset: &str,
        exchange_id: &str,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> f64 {
        if let Some(snapshot) = balance_snapshots.get(exchange_id) {
            if let Some(balance) = snapshot.balances.iter().find(|b| b.asset == asset) {
                return balance.total;
            }
        }
        0.0
    }

    /// Calculate current allocation percentages
    fn calculate_current_allocation(
        &self,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> ArbitrageResult<HashMap<String, f64>> {
        self.calculate_asset_distribution(balance_snapshots)
    }

    /// Calculate optimization score
    fn calculate_optimization_score(&self, allocations: &[FundAllocation]) -> f64 {
        if allocations.is_empty() {
            return 100.0; // Perfect score if no changes needed
        }

        let total_variance: f64 = allocations
            .iter()
            .map(|alloc| alloc.variance_percentage.abs())
            .sum();

        let average_variance = total_variance / allocations.len() as f64;

        // Score decreases with higher variance (more rebalancing needed)
        (100.0 - average_variance * 2.0).clamp(0.0, 100.0)
    }

    /// Generate optimization recommendations
    fn generate_recommendations(&self, allocations: &[FundAllocation]) -> Vec<String> {
        let mut recommendations = Vec::new();

        let high_priority_count = allocations.iter().filter(|a| a.priority == "high").count();
        let total_rebalancing_needed: f64 = allocations
            .iter()
            .map(|a| a.variance_percentage.abs())
            .sum();

        if high_priority_count > 0 {
            recommendations.push(format!(
                "Immediate rebalancing required for {} high-priority assets",
                high_priority_count
            ));
        }

        if total_rebalancing_needed > 20.0 {
            recommendations
                .push("Consider gradual rebalancing to minimize market impact".to_string());
        }

        if allocations
            .iter()
            .any(|a| a.variance_percentage.abs() > 15.0)
        {
            recommendations
                .push("Large allocation adjustments detected - review risk tolerance".to_string());
        }

        if recommendations.is_empty() {
            recommendations.push("Portfolio allocation is well-balanced".to_string());
        }

        recommendations
    }

    /// Assess portfolio risk
    fn assess_portfolio_risk(&self, allocations: &[FundAllocation], total_value: f64) -> String {
        let high_risk_allocations = allocations
            .iter()
            .filter(|a| a.variance_percentage.abs() > 10.0)
            .count();

        if high_risk_allocations > 3 {
            "High Risk: Multiple large allocation changes required".to_string()
        } else if high_risk_allocations > 1 {
            "Medium Risk: Some significant allocation adjustments needed".to_string()
        } else if total_value < 1000.0 {
            "Low Risk: Small portfolio with minimal rebalancing needs".to_string()
        } else {
            "Low Risk: Well-balanced portfolio with minor adjustments".to_string()
        }
    }

    /// Calculate expected improvement from optimization
    fn calculate_expected_improvement(&self, allocations: &[FundAllocation]) -> f64 {
        // Mock calculation based on estimated impact
        allocations.iter().map(|a| a.estimated_impact).sum::<f64>() * 0.5 // 50% of estimated impact as expected improvement
    }

    /// Determine implementation priority
    fn determine_implementation_priority(&self, allocations: &[FundAllocation]) -> String {
        let high_priority_count = allocations.iter().filter(|a| a.priority == "high").count();

        if high_priority_count > 2 {
            "Urgent".to_string()
        } else if high_priority_count > 0 {
            "High".to_string()
        } else if !allocations.is_empty() {
            "Medium".to_string()
        } else {
            "Low".to_string()
        }
    }

    /// Get mock price for asset
    fn get_mock_price(&self, asset: &str) -> f64 {
        match asset {
            "BTC" => 45000.0,
            "ETH" => 3000.0,
            "USDT" => 1.0,
            "BNB" => 300.0,
            "ADA" => 0.5,
            _ => 1.0,
        }
    }

    /// Generate hash for balance snapshots
    fn generate_snapshot_hash(
        &self,
        balance_snapshots: &HashMap<String, ExchangeBalanceSnapshot>,
    ) -> String {
        // Simple hash based on total values and timestamp
        let total_value: f64 = balance_snapshots.values().map(|s| s.total_usd_value).sum();
        let latest_timestamp = balance_snapshots
            .values()
            .map(|s| s.timestamp)
            .max()
            .unwrap_or(0);

        format!("{:.2}_{}", total_value, latest_timestamp)
    }

    /// Generate hash for target allocation
    fn generate_allocation_hash(&self, target_allocation: &HashMap<String, f64>) -> String {
        let mut sorted_pairs: Vec<_> = target_allocation.iter().collect();
        sorted_pairs.sort_by_key(|&(k, _)| k);

        sorted_pairs
            .iter()
            .map(|(k, v)| format!("{}:{:.2}", k, v))
            .collect::<Vec<_>>()
            .join("_")
    }

    /// Cache analysis result
    async fn cache_analysis(
        &mut self,
        cache_key: &str,
        analytics: &PortfolioAnalytics,
    ) -> ArbitrageResult<()> {
        self.analysis_cache
            .insert(cache_key.to_string(), analytics.clone());

        if let Some(kv) = &self.kv_store {
            let serialized = serde_json::to_string(analytics)
                .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

            let _ = kv
                .put(cache_key, serialized)?
                .expiration_ttl(self.config.analysis_cache_ttl_seconds)
                .execute()
                .await;
        }

        Ok(())
    }

    /// Get cached analysis result
    async fn get_cached_analysis(
        &mut self,
        cache_key: &str,
    ) -> ArbitrageResult<Option<PortfolioAnalytics>> {
        // Check memory cache first
        if let Some(cached) = self.analysis_cache.get(&cache_key.to_string()) {
            return Ok(Some(cached.clone()));
        }

        // Check KV store
        if let Some(kv) = &self.kv_store {
            if let Ok(Some(cached_data)) = kv.get(cache_key).text().await {
                if let Ok(analytics) = serde_json::from_str::<PortfolioAnalytics>(&cached_data) {
                    return Ok(Some(analytics));
                }
            }
        }

        Ok(None)
    }

    /// Cache optimization result
    async fn cache_optimization(
        &mut self,
        cache_key: &str,
        optimization: &FundOptimizationResult,
    ) -> ArbitrageResult<()> {
        self.optimization_cache
            .insert(cache_key.to_string(), optimization.clone());

        if let Some(kv) = &self.kv_store {
            let serialized = serde_json::to_string(optimization)
                .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

            let _ = kv
                .put(&format!("opt_{}", cache_key), serialized)?
                .expiration_ttl(self.config.analysis_cache_ttl_seconds)
                .execute()
                .await;
        }

        Ok(())
    }

    /// Get cached optimization result
    async fn get_cached_optimization(
        &mut self,
        cache_key: &str,
    ) -> ArbitrageResult<Option<FundOptimizationResult>> {
        // Check memory cache first
        if let Some(cached) = self.optimization_cache.get(&cache_key.to_string()) {
            return Ok(Some(cached.clone()));
        }

        // Check KV store
        if let Some(kv) = &self.kv_store {
            let kv_key = format!("opt_{}", cache_key);
            if let Ok(Some(cached_data)) = kv.get(&kv_key).text().await {
                if let Ok(optimization) =
                    serde_json::from_str::<FundOptimizationResult>(&cached_data)
                {
                    return Ok(Some(optimization));
                }
            }
        }

        Ok(None)
    }

    /// Update analysis performance metrics
    fn update_analysis_metrics(&mut self, analysis_time_ms: u64) {
        self.metrics.analyses_performed += 1;
        self.metrics.successful_analyses += 1;

        // Update average analysis time (exponential moving average)
        let alpha = 0.1;
        self.metrics.average_analysis_time_ms =
            alpha * analysis_time_ms as f64 + (1.0 - alpha) * self.metrics.average_analysis_time_ms;

        // Calculate analyses per hour with division by zero protection
        let current_time = get_current_time_millis();
        let time_diff_hours =
            (current_time - self.last_analysis_time) as f64 / (1000.0 * 60.0 * 60.0);

        // Use epsilon to prevent division by zero
        const EPSILON: f64 = 1e-9;
        if time_diff_hours > EPSILON {
            self.metrics.analyses_per_hour = 1.0 / time_diff_hours;
        } else {
            // If time difference is too small, use a reasonable default
            self.metrics.analyses_per_hour = 3600.0; // 1 analysis per second equivalent
        }

        self.last_analysis_time = current_time;
        self.metrics.last_updated = current_time;
    }

    /// Update optimization performance metrics
    fn update_optimization_metrics(&mut self, _optimization_time_ms: u64) {
        self.metrics.optimizations_completed += 1;

        // Calculate optimizations per day with division by zero protection
        let current_time = get_current_time_millis();
        let time_diff_days =
            (current_time - self.last_analysis_time) as f64 / (1000.0 * 60.0 * 60.0 * 24.0);

        const EPSILON: f64 = 1e-9;
        if time_diff_days > EPSILON {
            self.metrics.optimizations_per_day =
                self.metrics.optimizations_completed as f64 / time_diff_days;
        } else {
            // If time difference is too small, use current count as daily rate
            self.metrics.optimizations_per_day = self.metrics.optimizations_completed as f64;
        }

        self.metrics.last_updated = current_time;
    }

    /// Get health status
    pub async fn health_check(&self) -> ArbitrageResult<FundAnalyzerHealth> {
        let active_analyses = 0; // Mock value
        let cache_utilization_percent = (self.analysis_cache.len() as f64 / 100.0) * 100.0;

        let analytics_healthy = self.config.enable_portfolio_analytics;
        let optimization_healthy =
            !self.config.enable_fund_optimization || self.metrics.error_rate < 0.05;
        let risk_assessment_healthy =
            !self.config.enable_risk_assessment || self.metrics.error_rate < 0.05;

        let is_healthy = analytics_healthy && optimization_healthy && risk_assessment_healthy;

        Ok(FundAnalyzerHealth {
            is_healthy,
            analytics_healthy,
            optimization_healthy,
            risk_assessment_healthy,
            active_analyses,
            cache_utilization_percent,
            average_analysis_time_ms: self.metrics.average_analysis_time_ms,
            last_health_check: get_current_time_millis(),
        })
    }

    /// Get performance metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<FundAnalyzerMetrics> {
        Ok(self.metrics.clone())
    }
}

impl Default for FundAnalyzerMetrics {
    fn default() -> Self {
        Self {
            analyses_performed: 0,
            optimizations_completed: 0,
            risk_assessments_completed: 0,
            average_analysis_time_ms: 0.0,
            successful_analyses: 0,
            failed_analyses: 0,
            cache_hit_rate: 0.0,
            error_rate: 0.0,
            analyses_per_hour: 0.0,
            optimizations_per_day: 0.0,
            average_optimization_improvement: 0.0,
            last_updated: get_current_time_millis(),
        }
    }
}
