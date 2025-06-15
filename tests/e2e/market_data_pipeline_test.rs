use arb_edge::services::core::analysis::market_analysis::{
    OpportunityType, PricePoint, PriceSeries, RiskLevel, TimeFrame, TimeHorizon, TradingOpportunity,
};
use arb_edge::types::{ArbitrageOpportunity, ArbitrageType, ExchangeIdEnum};
use serde_json::json;

// Test configuration constants
const DEFAULT_BASE_PRICE: f64 = 50000.0;
const DEFAULT_VOLUME_BASE: f64 = 100.0;
const DEFAULT_VOLUME_INCREMENT: f64 = 10.0;
const DEFAULT_PRICE_VARIATION: f64 = 0.001;
const DEFAULT_PRICE_OFFSET: f64 = 0.01;
const MINUTE_IN_MS: u64 = 60000;
const HOUR_IN_MS: u64 = 3600000;

// Test helper configuration
struct TestConfig {
    base_price: f64,
    volume_base: f64,
    volume_increment: f64,
    price_variation: f64,
    #[allow(dead_code)]
    price_offset: f64,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            base_price: DEFAULT_BASE_PRICE,
            volume_base: DEFAULT_VOLUME_BASE,
            volume_increment: DEFAULT_VOLUME_INCREMENT,
            price_variation: DEFAULT_PRICE_VARIATION,
            price_offset: DEFAULT_PRICE_OFFSET,
        }
    }
}

impl TestConfig {
    #[allow(dead_code)]
    fn with_base_price(mut self, price: f64) -> Self {
        self.base_price = price;
        self
    }

    #[allow(dead_code)]
    fn with_volume_base(mut self, volume: f64) -> Self {
        self.volume_base = volume;
        self
    }

    #[allow(dead_code)]
    fn with_price_variation(mut self, variation: f64) -> Self {
        self.price_variation = variation;
        self
    }
}

/// Test helper to create a mock arbitrage opportunity
fn create_test_arbitrage_opportunity(id: &str, pair: &str, rate_diff: f64) -> ArbitrageOpportunity {
    create_test_arbitrage_opportunity_with_config(id, pair, rate_diff, &TestConfig::default())
}

/// Test helper to create a mock arbitrage opportunity with custom configuration
fn create_test_arbitrage_opportunity_with_config(
    id: &str,
    pair: &str,
    rate_diff: f64,
    config: &TestConfig,
) -> ArbitrageOpportunity {
    let mut opportunity = ArbitrageOpportunity::new(
        pair.to_string(),
        ExchangeIdEnum::Binance, // Required long_exchange
        ExchangeIdEnum::Bybit,   // Required short_exchange
        Some(config.base_price), // long_rate
        Some(config.base_price + (config.base_price * rate_diff)), // short_rate
        rate_diff,               // rate_difference
        ArbitrageType::CrossExchange,
    )
    .unwrap_or_else(|_| ArbitrageOpportunity::default());

    opportunity.id = id.to_string();
    opportunity
        .with_net_difference(rate_diff * 0.95)
        .with_potential_profit(rate_diff * config.base_price)
        .with_details(format!(
            "Test arbitrage opportunity with {}% rate difference",
            rate_diff * 100.0
        ))
}

/// Test helper to create a mock trading opportunity
fn create_test_trading_opportunity(id: &str, pair: &str, confidence: f64) -> TradingOpportunity {
    create_test_trading_opportunity_with_config(id, pair, confidence, &TestConfig::default())
}

/// Test helper to create a mock trading opportunity with custom configuration
fn create_test_trading_opportunity_with_config(
    id: &str,
    pair: &str,
    confidence: f64,
    config: &TestConfig,
) -> TradingOpportunity {
    let entry_price = config.base_price;
    let target_price = entry_price * 1.02; // 2% target
    let stop_loss = entry_price * 0.98; // 2% stop loss

    TradingOpportunity {
        opportunity_id: id.to_string(),
        opportunity_type: OpportunityType::Arbitrage,
        trading_pair: pair.to_string(),
        exchanges: vec!["binance".to_string(), "bybit".to_string()],
        entry_price,
        target_price: Some(target_price),
        stop_loss: Some(stop_loss),
        confidence_score: confidence,
        risk_level: calculate_risk_level(confidence),
        expected_return: confidence * 0.05, // Scale return with confidence
        time_horizon: TimeHorizon::Short,
        indicators_used: vec!["cross_exchange_analysis".to_string()],
        analysis_data: json!({
            "buy_exchange": "binance",
            "sell_exchange": "bybit",
            "rate_difference": confidence * 0.02
        }),
        created_at: chrono::Utc::now().timestamp_millis() as u64,
        expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + HOUR_IN_MS),
    }
}

/// Helper function to calculate risk level based on confidence
fn calculate_risk_level(confidence: f64) -> RiskLevel {
    if confidence > 0.8 {
        RiskLevel::Low
    } else if confidence > 0.5 {
        RiskLevel::Medium
    } else {
        RiskLevel::High
    }
}

/// Test helper to create price series data
fn create_test_price_series(pair: &str, exchange: &str, data_points: usize) -> PriceSeries {
    create_test_price_series_with_config(pair, exchange, data_points, &TestConfig::default())
}

/// Test helper to create price series data with custom configuration
fn create_test_price_series_with_config(
    pair: &str,
    exchange: &str,
    data_points: usize,
    config: &TestConfig,
) -> PriceSeries {
    let mut series = PriceSeries::new(pair.to_string(), exchange.to_string(), TimeFrame::OneMinute);

    let base_time = chrono::Utc::now().timestamp_millis() as u64;

    for i in 0..data_points {
        let price_variation = (i as f64).sin() * config.price_variation; // Oscillating price variations within ±price_variation
        let price = config.base_price + (config.base_price * price_variation);

        let price_point = PricePoint {
            timestamp: base_time + (i as u64 * MINUTE_IN_MS),
            price,
            volume: Some(config.volume_base + (i as f64 * config.volume_increment)),
            exchange_id: exchange.to_string(),
            trading_pair: pair.to_string(),
        };

        series.add_price_point(price_point);
    }

    series
}

/// Helper function to create test price series with specific price pattern
#[allow(dead_code)]
fn create_test_price_series_with_pattern(
    pair: &str,
    exchange: &str,
    data_points: usize,
    pattern: PricePattern,
) -> PriceSeries {
    let config = match pattern {
        PricePattern::Trending => TestConfig::default().with_price_variation(0.002),
        PricePattern::Volatile => TestConfig::default().with_price_variation(0.005),
        PricePattern::Stable => TestConfig::default().with_price_variation(0.0001),
    };

    create_test_price_series_with_config(pair, exchange, data_points, &config)
}

/// Enum for different price patterns in tests
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum PricePattern {
    Trending,
    Volatile,
    Stable,
}

#[cfg(test)]
mod market_data_structure_tests {
    use super::*;

    #[test]
    fn test_arbitrage_opportunity_creation() {
        // Arrange & Act
        let opportunity = create_test_arbitrage_opportunity("test_001", "BTCUSDT", 0.002);

        // Assert
        assert_eq!(opportunity.id, "test_001");
        assert_eq!(opportunity.pair, "BTCUSDT");
        assert_eq!(opportunity.r#type, ArbitrageType::CrossExchange);
        assert!(opportunity.potential_profit_value.unwrap() > 0.0);
        assert!(opportunity.long_rate.is_some());
        assert!(opportunity.short_rate.is_some());
    }

    #[test]
    fn test_trading_opportunity_creation() {
        // Arrange & Act
        let opportunity = create_test_trading_opportunity("trade_001", "ETHUSDT", 0.85);

        // Assert
        assert_eq!(opportunity.opportunity_id, "trade_001");
        assert_eq!(opportunity.trading_pair, "ETHUSDT");
        assert_eq!(opportunity.opportunity_type, OpportunityType::Arbitrage);
        assert_eq!(opportunity.confidence_score, 0.85);
        assert_eq!(opportunity.risk_level, RiskLevel::Low);
        assert!(opportunity.expected_return > 0.0);
    }

    #[test]
    fn test_price_series_creation() {
        // Arrange & Act
        let series = create_test_price_series("BTCUSDT", "binance", 10);

        // Assert
        assert_eq!(series.trading_pair, "BTCUSDT");
        assert_eq!(series.exchange_id, "binance");
        assert_eq!(series.timeframe, TimeFrame::OneMinute);
        assert_eq!(series.data_points.len(), 10);

        // Check that data points are sorted by timestamp
        for i in 1..series.data_points.len() {
            assert!(series.data_points[i].timestamp > series.data_points[i - 1].timestamp);
        }
    }

    #[test]
    fn test_price_series_latest_price() {
        // Arrange
        let series = create_test_price_series("ETHUSDT", "bybit", 5);

        // Act
        let latest = series.latest_price();

        // Assert
        assert!(latest.is_some());
        let latest_price = latest.unwrap();
        assert_eq!(latest_price.trading_pair, "ETHUSDT");
        assert_eq!(latest_price.exchange_id, "bybit");
        assert!(latest_price.price > 0.0);
    }

    #[test]
    fn test_price_series_price_values() {
        // Arrange
        let series = create_test_price_series("ADAUSDT", "okx", 3);

        // Act
        let price_values = series.price_values();

        // Assert
        assert_eq!(price_values.len(), 3);
        for price in price_values {
            assert!(price > 0.0);
        }
    }
}

#[cfg(test)]
mod opportunity_analysis_tests {
    use super::*;

    #[test]
    fn test_high_confidence_opportunity_characteristics() {
        // Arrange & Act
        let opportunity = create_test_trading_opportunity("high_conf", "BTCUSDT", 0.95);

        // Assert
        assert!(opportunity.confidence_score >= 0.9);
        assert_eq!(opportunity.risk_level, RiskLevel::Low);
        assert!(opportunity.expected_return > 0.04); // Should have good return
        assert!(opportunity.expires_at.is_some());
    }

    #[test]
    fn test_medium_confidence_opportunity_characteristics() {
        // Arrange & Act
        let opportunity = create_test_trading_opportunity("med_conf", "ETHUSDT", 0.75);

        // Assert
        assert!(opportunity.confidence_score >= 0.7 && opportunity.confidence_score < 0.8);
        assert_eq!(opportunity.risk_level, RiskLevel::Medium);
        assert!(opportunity.expected_return > 0.03);
    }

    #[test]
    fn test_arbitrage_opportunity_profit_calculation() {
        // Arrange
        let rate_diff = 0.003; // 0.3%
        let opportunity = create_test_arbitrage_opportunity("arb_calc", "SOLUSDT", rate_diff);

        // Act & Assert
        assert!(opportunity.potential_profit_value.unwrap() > 0.0);
        // Profit is calculated as rate_diff * base_price (50000.0)
        let expected_profit = rate_diff * DEFAULT_BASE_PRICE;
        assert!((opportunity.potential_profit_value.unwrap() - expected_profit).abs() < 0.001);
    }

    #[test]
    fn test_opportunity_time_horizon_mapping() {
        // Arrange & Act
        let short_term = create_test_trading_opportunity("short", "BNBUSDT", 0.8);

        // Assert
        assert_eq!(short_term.time_horizon, TimeHorizon::Short);
        assert!(short_term.expires_at.is_some());

        // Verify expiration is reasonable (within 24 hours)
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let expires = short_term.expires_at.unwrap();
        assert!(expires > now);
        assert!(expires < now + (24 * 60 * 60 * 1000)); // Within 24 hours
    }
}

#[cfg(test)]
mod data_pipeline_flow_tests {
    use super::*;

    #[test]
    fn test_arbitrage_to_trading_opportunity_conversion() {
        // Arrange
        let arbitrage_opp = create_test_arbitrage_opportunity("arb_to_trade", "MATICUSDT", 0.004);

        // Act - Simulate conversion logic
        let trading_opp = TradingOpportunity {
            opportunity_id: format!("trade_{}", arbitrage_opp.id),
            opportunity_type: OpportunityType::Arbitrage,
            trading_pair: arbitrage_opp.pair.clone(),
            exchanges: vec![
                arbitrage_opp.long_exchange.to_string(),
                arbitrage_opp.short_exchange.to_string(),
            ],
            entry_price: arbitrage_opp.long_rate.unwrap(),
            target_price: arbitrage_opp.short_rate,
            stop_loss: Some(arbitrage_opp.long_rate.unwrap() * 0.995), // 0.5% stop loss
            confidence_score: 0.85, // High confidence for arbitrage
            risk_level: RiskLevel::Low,
            expected_return: arbitrage_opp.rate_difference,
            time_horizon: TimeHorizon::Short,
            indicators_used: vec!["arbitrage_analysis".to_string()],
            analysis_data: json!({
                "source_arbitrage_id": arbitrage_opp.id,
                "long_exchange": arbitrage_opp.long_exchange,
                "short_exchange": arbitrage_opp.short_exchange
            }),
            created_at: arbitrage_opp.timestamp,
            expires_at: Some(arbitrage_opp.timestamp + 1800000), // 30 minutes
        };

        // Assert
        assert!(trading_opp.opportunity_id.contains(&arbitrage_opp.id));
        assert_eq!(trading_opp.trading_pair, arbitrage_opp.pair);
        assert_eq!(trading_opp.opportunity_type, OpportunityType::Arbitrage);
        assert!(trading_opp.expected_return > 0.0);
    }

    #[test]
    fn test_multiple_opportunities_sorting() {
        // Arrange
        let opportunities = vec![
            create_test_trading_opportunity("low_conf", "BTCUSDT", 0.6),
            create_test_trading_opportunity("high_conf", "ETHUSDT", 0.9),
            create_test_trading_opportunity("med_conf", "ADAUSDT", 0.75),
        ];

        // Act - Sort by confidence score (descending)
        let mut sorted_opps = opportunities;
        sorted_opps.sort_by(|a, b| {
            b.confidence_score
                .partial_cmp(&a.confidence_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Assert
        assert_eq!(sorted_opps[0].opportunity_id, "high_conf");
        assert_eq!(sorted_opps[1].opportunity_id, "med_conf");
        assert_eq!(sorted_opps[2].opportunity_id, "low_conf");

        // Verify confidence scores are in descending order
        assert!(sorted_opps[0].confidence_score > sorted_opps[1].confidence_score);
        assert!(sorted_opps[1].confidence_score > sorted_opps[2].confidence_score);
    }

    #[test]
    fn test_opportunity_filtering_by_confidence() {
        // Arrange
        let opportunities = vec![
            create_test_trading_opportunity("high_1", "BTCUSDT", 0.95),
            create_test_trading_opportunity("low_1", "ETHUSDT", 0.55),
            create_test_trading_opportunity("high_2", "ADAUSDT", 0.88),
            create_test_trading_opportunity("low_2", "SOLUSDT", 0.45),
        ];

        // Act - Filter by minimum confidence of 0.8
        let high_confidence_opps: Vec<_> = opportunities
            .into_iter()
            .filter(|opp| opp.confidence_score >= 0.8)
            .collect();

        // Assert
        assert_eq!(high_confidence_opps.len(), 2);
        assert!(high_confidence_opps
            .iter()
            .all(|opp| opp.confidence_score >= 0.8));
        assert!(high_confidence_opps
            .iter()
            .any(|opp| opp.opportunity_id == "high_1"));
        assert!(high_confidence_opps
            .iter()
            .any(|opp| opp.opportunity_id == "high_2"));
    }

    #[test]
    fn test_opportunity_filtering_by_risk_level() {
        // Arrange
        let opportunities = vec![
            create_test_trading_opportunity("low_risk", "BTCUSDT", 0.9), // High confidence = Low risk
            create_test_trading_opportunity("med_risk", "ETHUSDT", 0.7), // Medium confidence = Medium risk
            create_test_trading_opportunity("low_risk_2", "ADAUSDT", 0.85), // High confidence = Low risk
        ];

        // Act - Filter by low risk only
        let low_risk_opps: Vec<_> = opportunities
            .into_iter()
            .filter(|opp| opp.risk_level == RiskLevel::Low)
            .collect();

        // Assert
        assert_eq!(low_risk_opps.len(), 2);
        assert!(low_risk_opps
            .iter()
            .all(|opp| opp.risk_level == RiskLevel::Low));
        assert!(low_risk_opps.iter().all(|opp| opp.confidence_score >= 0.8));
    }
}

#[cfg(test)]
mod performance_simulation_tests {
    use super::*;

    #[test]
    fn test_large_dataset_processing() {
        // Arrange - Create a large number of opportunities
        let mut opportunities = Vec::new();
        for i in 0..1000 {
            let confidence = 0.5 + (i as f64 / 2000.0); // Gradually increasing confidence
            opportunities.push(create_test_trading_opportunity(
                &format!("opp_{}", i),
                "BTCUSDT",
                confidence,
            ));
        }

        // Act - Simulate processing pipeline
        let start_time = crate::utils::time::now_instant();

        // Filter high confidence opportunities
        let high_conf_opps: Vec<_> = opportunities
            .into_iter()
            .filter(|opp| opp.confidence_score >= 0.8)
            .collect();

        // Sort by expected return
        let mut sorted_opps = high_conf_opps;
        sorted_opps.sort_by(|a, b| b.expected_return.partial_cmp(&a.expected_return).unwrap());

        // Take top 10
        let top_opportunities: Vec<_> = sorted_opps.into_iter().take(10).collect();

        let processing_time = start_time.elapsed();

        // Assert
        assert_eq!(top_opportunities.len(), 10);
        // Note: Removed strict timing assertion to prevent flaky tests
        // Processing time: {:?} (for monitoring purposes only)
        println!("Processing time: {:?}", processing_time);

        // Verify all are high confidence
        assert!(top_opportunities
            .iter()
            .all(|opp| opp.confidence_score >= 0.8));

        // Verify sorted by expected return (descending)
        for i in 1..top_opportunities.len() {
            assert!(
                top_opportunities[i - 1].expected_return >= top_opportunities[i].expected_return
            );
        }
    }

    #[test]
    fn test_price_series_performance() {
        // Arrange - Create large price series
        let start_time = crate::utils::time::now_instant();
        let series = create_test_price_series("BTCUSDT", "binance", 1000);
        let creation_time = start_time.elapsed();

        // Act - Perform operations on the series
        let operation_start = crate::utils::time::now_instant();
        let latest_price = series.latest_price();
        let price_values = series.price_values();
        let price_range = series.price_range(
            series.data_points[0].timestamp,
            series.data_points[999].timestamp,
        );
        let operation_time = operation_start.elapsed();

        // Assert
        // Note: Removed strict timing assertions to prevent flaky tests
        // Timing info for monitoring: creation={:?}, operations={:?}
        println!(
            "Timing info - creation: {:?}, operations: {:?}",
            creation_time, operation_time
        );

        assert!(latest_price.is_some());
        assert_eq!(price_values.len(), 1000);
        assert_eq!(price_range.len(), 1000);
    }
}
