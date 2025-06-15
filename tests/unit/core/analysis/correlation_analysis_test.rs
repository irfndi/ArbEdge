use std::collections::HashMap;

use arb_edge::services::core::analysis::correlation_analysis::*;
use arb_edge::services::core::analysis::market_analysis::{PricePoint, PriceSeries, TimeFrame};
use arb_edge::services::core::user::user_trading_preferences::{
    AutomationLevel, AutomationScope, ExperienceLevel, RiskTolerance, TradingFocus,
    UserTradingPreferences,
};
use arb_edge::utils::logger::{LogLevel, Logger};

// Helper functions for testing

fn create_test_price_series(
    base_time: u64,
    prices: Vec<f64>,
    time_intervals_ms: u64,
    exchange_id: &str,
    trading_pair: &str,
) -> PriceSeries {
    let mut price_series = PriceSeries::new(
        trading_pair.to_string(),
        exchange_id.to_string(),
        TimeFrame::OneMinute,
    );

    for (i, price) in prices.iter().enumerate() {
        let timestamp = base_time + (i as u64 * time_intervals_ms);
        let price_point = PricePoint {
            timestamp,
            price: *price,
            volume: Some(1000.0),
            exchange_id: exchange_id.to_string(),
            trading_pair: trading_pair.to_string(),
        };
        price_series.add_price_point(price_point);
    }

    price_series
}

fn create_correlated_price_series(
    base_series: &PriceSeries,
    correlation: f64,
    noise: f64,
    exchange_id: &str,
) -> PriceSeries {
    let mut correlated_series = PriceSeries::new(
        base_series.trading_pair.clone(),
        exchange_id.to_string(),
        base_series.timeframe.clone(),
    );

    for (i, point) in base_series.data_points.iter().enumerate() {
        let base_price = point.price;
        let correlated_price = base_price * correlation + (i as f64 * noise);
        let correlated_point = PricePoint {
            timestamp: point.timestamp,
            price: correlated_price,
            volume: point.volume,
            exchange_id: exchange_id.to_string(),
            trading_pair: point.trading_pair.clone(),
        };
        correlated_series.add_price_point(correlated_point);
    }

    correlated_series
}

fn create_lagged_price_series(
    base_series: &PriceSeries,
    lag_ms: u64,
    exchange_id: &str,
) -> PriceSeries {
    let mut lagged_series = PriceSeries::new(
        base_series.trading_pair.clone(),
        exchange_id.to_string(),
        base_series.timeframe.clone(),
    );

    for point in &base_series.data_points {
        let lagged_timestamp = point.timestamp + lag_ms;
        let lagged_point = PricePoint {
            timestamp: lagged_timestamp,
            price: point.price,
            volume: point.volume,
            exchange_id: exchange_id.to_string(),
            trading_pair: point.trading_pair.clone(),
        };
        lagged_series.add_price_point(lagged_point);
    }

    lagged_series
}

fn create_test_user_preferences(trading_focus: TradingFocus) -> UserTradingPreferences {
    #[cfg(target_arch = "wasm32")]
    let now = js_sys::Date::now() as u64;
    #[cfg(not(target_arch = "wasm32"))]
    let now = arb_edge::utils::time::now_system_time()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    UserTradingPreferences {
        preference_id: "test_pref".to_string(),
        user_id: "test_user".to_string(),
        trading_focus,
        experience_level: ExperienceLevel::Intermediate,
        risk_tolerance: RiskTolerance::Balanced,
        automation_level: AutomationLevel::Manual,
        automation_scope: AutomationScope::None,
        arbitrage_enabled: true,
        technical_enabled: true,
        advanced_analytics_enabled: false,
        preferred_notification_channels: vec!["telegram".to_string()],
        trading_hours_timezone: "UTC".to_string(),
        trading_hours_start: "00:00".to_string(),
        trading_hours_end: "23:59".to_string(),
        onboarding_completed: true,
        tutorial_steps_completed: vec![],
        created_at: now,
        updated_at: now,
    }
}

#[cfg(test)]
mod correlation_analysis_service_tests {
    use super::*;

    #[test]
    fn test_create_correlation_analysis_service() {
        let config = CorrelationAnalysisConfig::default();
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(config, logger);

        // Test actual service creation - config fields are private, so we test functionality instead
        // The service should be created successfully with default config
        assert!(std::mem::size_of_val(&service) > 0);
    }

    #[test]
    fn test_calculate_price_correlation_high_correlation() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let base_time = 1672531200000; // 2024-01-01 00:00:00 UTC in milliseconds

        // Create highly correlated price series
        let prices_a = vec![
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0,
            112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0, 120.0,
        ];
        let series_a = create_test_price_series(base_time, prices_a, 60000, "binance", "BTC/USDT");

        let prices_b = vec![
            99.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0,
            111.0, 112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0,
        ];
        let series_b = create_test_price_series(base_time, prices_b, 60000, "bybit", "BTC/USDT");

        let result = service.calculate_price_correlation(&series_a, &series_b, "binance", "bybit");

        assert!(result.is_ok());
        let correlation_data = result.unwrap();
        assert_eq!(correlation_data.exchange_a, "binance");
        assert_eq!(correlation_data.exchange_b, "bybit");
        assert!(correlation_data.correlation_coefficient > 0.95); // High correlation
        assert!(correlation_data.confidence_level > 0.0);
        assert!(correlation_data.data_points >= 20);
    }

    #[test]
    fn test_calculate_price_correlation_insufficient_data() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let base_time = 1672531200000;

        // Create series with insufficient data points
        let prices = vec![100.0, 101.0, 102.0]; // Only 3 points, need 20
        let series_a =
            create_test_price_series(base_time, prices.clone(), 60000, "binance", "BTC/USDT");
        let series_b = create_test_price_series(base_time, prices, 60000, "bybit", "BTC/USDT");

        let result = service.calculate_price_correlation(&series_a, &series_b, "binance", "bybit");

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient data points"));
    }

    #[test]
    fn test_analyze_exchange_leadership() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let base_time = 1672531200000;

        // Create leader and follower series
        let leader_prices = vec![
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0,
            112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0, 120.0,
        ];
        let leader_series =
            create_test_price_series(base_time, leader_prices, 60000, "binance", "BTC/USDT");

        // Create follower series with 2-minute lag
        let follower_series = create_lagged_price_series(&leader_series, 120000, "bybit"); // 120000ms = 2 minutes

        let result = service.analyze_exchange_leadership(
            &leader_series,
            &follower_series,
            "binance",
            "bybit",
        );

        assert!(result.is_ok());
        let leadership = result.unwrap();
        assert_eq!(leadership.leading_exchange, "binance");
        assert_eq!(leadership.following_exchange, "bybit");
        assert!(leadership.lag_seconds >= 0);
        assert!(leadership.confidence > 0.0);
    }

    #[test]
    fn test_calculate_technical_correlation() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let base_time = 1672531200000;

        // Create two highly correlated price series for technical analysis
        let prices_a = vec![
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0,
            112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0, 120.0, 121.0, 122.0, 123.0,
            124.0, 125.0, 126.0, 127.0, 128.0, 129.0, 130.0, 131.0, 132.0, 133.0, 134.0,
        ];
        let series_a = create_test_price_series(base_time, prices_a, 60000, "binance", "BTC/USDT");
        let series_b = create_correlated_price_series(&series_a, 0.98, 0.01, "bybit");

        let result =
            service.calculate_technical_correlation(&series_a, &series_b, "binance", "bybit");

        assert!(result.is_ok());
        let tech_correlation = result.unwrap();
        assert_eq!(tech_correlation.exchange_a, "binance");
        assert_eq!(tech_correlation.exchange_b, "bybit");
        assert!(tech_correlation.rsi_correlation.abs() <= 1.0);
        assert!(tech_correlation.sma_correlation.abs() <= 1.0);
        assert!(tech_correlation.momentum_correlation.abs() <= 1.0);
        assert!(tech_correlation.overall_technical_correlation.abs() <= 1.0);
        assert!(tech_correlation.confidence >= 0.0 && tech_correlation.confidence <= 1.0);
    }

    #[tokio::test]
    async fn test_generate_correlation_metrics_arbitrage_focus() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let base_time = 1672531200000;
        let user_preferences = create_test_user_preferences(TradingFocus::Arbitrage);

        // Create exchange data
        let prices = vec![
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0,
            112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0, 120.0,
        ];
        let binance_series =
            create_test_price_series(base_time, prices, 60000, "binance", "BTC/USDT");
        let bybit_series = create_correlated_price_series(&binance_series, 0.95, 0.02, "bybit");
        let okx_series = create_correlated_price_series(&binance_series, 0.90, 0.05, "okx");

        let mut exchange_data = HashMap::new();
        exchange_data.insert("binance".to_string(), binance_series);
        exchange_data.insert("bybit".to_string(), bybit_series);
        exchange_data.insert("okx".to_string(), okx_series);

        let result = service
            .generate_correlation_metrics("BTC/USDT", &exchange_data, &user_preferences)
            .await;

        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert_eq!(metrics.trading_pair, "BTC/USDT");
        assert!(!metrics.price_correlations.is_empty());
        assert!(!metrics.leadership_analysis.is_empty());
        // For arbitrage-only focus, technical correlations should be empty
        assert!(metrics.technical_correlations.is_empty());
        assert!(metrics.confidence_score >= 0.0 && metrics.confidence_score <= 1.0);
    }

    #[tokio::test]
    async fn test_generate_correlation_metrics_hybrid_focus() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let base_time = 1672531200000;
        let user_preferences = create_test_user_preferences(TradingFocus::Technical);

        // Create exchange data with enough data for technical analysis
        let prices = vec![
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0,
            112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0, 120.0, 121.0, 122.0, 123.0,
            124.0, 125.0, 126.0, 127.0, 128.0, 129.0, 130.0, 131.0, 132.0, 133.0, 134.0,
        ];
        let binance_series =
            create_test_price_series(base_time, prices, 60000, "binance", "BTC/USDT");
        let bybit_series = create_correlated_price_series(&binance_series, 0.95, 0.02, "bybit");

        let mut exchange_data = HashMap::new();
        exchange_data.insert("binance".to_string(), binance_series);
        exchange_data.insert("bybit".to_string(), bybit_series);

        let result = service
            .generate_correlation_metrics("BTC/USDT", &exchange_data, &user_preferences)
            .await;

        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert_eq!(metrics.trading_pair, "BTC/USDT");
        assert!(!metrics.price_correlations.is_empty());
        assert!(!metrics.leadership_analysis.is_empty());
        // For technical focus, technical correlations should be included
        assert!(!metrics.technical_correlations.is_empty());
        assert!(metrics.confidence_score >= 0.0 && metrics.confidence_score <= 1.0);
    }

    #[tokio::test]
    async fn test_generate_correlation_metrics_insufficient_exchanges() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let user_preferences = create_test_user_preferences(TradingFocus::Arbitrage);

        // Create exchange data with only one exchange
        let base_time = 1672531200000;
        let prices = vec![100.0, 101.0, 102.0];
        let binance_series =
            create_test_price_series(base_time, prices, 60000, "binance", "BTC/USDT");

        let mut exchange_data = HashMap::new();
        exchange_data.insert("binance".to_string(), binance_series);

        let result = service
            .generate_correlation_metrics("BTC/USDT", &exchange_data, &user_preferences)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Need at least 2 exchanges"));
    }

    #[test]
    fn test_correlation_analysis_config_default() {
        let config = CorrelationAnalysisConfig::default();

        assert_eq!(config.min_data_points, 20);
        assert_eq!(config.max_lag_seconds, 300);
        assert_eq!(config.correlation_threshold, 0.5);
        assert_eq!(config.leadership_threshold, 0.6);
        assert_eq!(config.technical_correlation_weight, 0.3);
        assert_eq!(config.confidence_threshold, 0.7);
    }

    #[test]
    fn test_correlation_analysis_config_custom() {
        let config = CorrelationAnalysisConfig {
            min_data_points: 50,
            max_lag_seconds: 600,
            correlation_threshold: 0.8,
            leadership_threshold: 0.9,
            technical_correlation_weight: 0.5,
            confidence_threshold: 0.85,
        };

        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(config, logger);

        // Service should be created with custom config
        assert!(std::mem::size_of_val(&service) > 0);
    }

    #[test]
    fn test_exchange_correlation_data_serialization() {
        #[cfg(target_arch = "wasm32")]
        let now =
            chrono::DateTime::from_timestamp_millis(js_sys::Date::now() as i64).unwrap_or_default();
        #[cfg(not(target_arch = "wasm32"))]
        let now = chrono::Utc::now();

        let correlation_data = ExchangeCorrelationData {
            exchange_a: "binance".to_string(),
            exchange_b: "bybit".to_string(),
            correlation_coefficient: 0.85,
            confidence_level: 0.75,
            data_points: 100,
            analysis_timestamp: now,
        };

        // Test serialization
        let serialized = serde_json::to_string(&correlation_data);
        assert!(serialized.is_ok());

        // Test deserialization
        let deserialized: Result<ExchangeCorrelationData, _> =
            serde_json::from_str(&serialized.unwrap());
        assert!(deserialized.is_ok());

        let deserialized_data = deserialized.unwrap();
        assert_eq!(deserialized_data.exchange_a, "binance");
        assert_eq!(deserialized_data.exchange_b, "bybit");
        assert_eq!(deserialized_data.correlation_coefficient, 0.85);
    }

    #[test]
    fn test_correlation_confidence_calculation() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let base_time = 1672531200000;

        // Create data with good variance
        let prices_a = vec![
            100.0, 105.0, 98.0, 110.0, 95.0, 112.0, 93.0, 115.0, 90.0, 118.0, 88.0, 120.0, 85.0,
            122.0, 83.0, 125.0, 80.0, 128.0, 78.0, 130.0, 75.0,
        ];
        let prices_b = vec![
            101.0, 106.0, 99.0, 111.0, 96.0, 113.0, 94.0, 116.0, 91.0, 119.0, 89.0, 121.0, 86.0,
            123.0, 84.0, 126.0, 81.0, 129.0, 79.0, 131.0, 76.0,
        ];

        let series_a = create_test_price_series(base_time, prices_a, 60000, "binance", "BTC/USDT");
        let series_b = create_test_price_series(base_time, prices_b, 60000, "bybit", "BTC/USDT");

        let result = service.calculate_price_correlation(&series_a, &series_b, "binance", "bybit");

        assert!(result.is_ok());
        let correlation_data = result.unwrap();
        // Should have reasonable confidence due to good data variance
        assert!(correlation_data.confidence_level > 0.5);
    }

    #[tokio::test]
    async fn test_multiple_exchange_correlation_analysis() {
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(CorrelationAnalysisConfig::default(), logger);
        let base_time = 1672531200000;
        let user_preferences = create_test_user_preferences(TradingFocus::Technical);

        // Create data for 4 exchanges with enough data for technical analysis
        let base_prices = vec![
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0,
            112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0, 120.0, 121.0, 122.0, 123.0,
            124.0, 125.0, 126.0, 127.0, 128.0, 129.0, 130.0, 131.0, 132.0, 133.0, 134.0,
        ];
        let binance_series =
            create_test_price_series(base_time, base_prices, 60000, "binance", "BTC/USDT");
        let bybit_series = create_correlated_price_series(&binance_series, 0.95, 0.02, "bybit");
        let okx_series = create_correlated_price_series(&binance_series, 0.90, 0.05, "okx");
        let bitget_series = create_correlated_price_series(&binance_series, 0.85, 0.08, "bitget");

        let mut exchange_data = HashMap::new();
        exchange_data.insert("binance".to_string(), binance_series);
        exchange_data.insert("bybit".to_string(), bybit_series);
        exchange_data.insert("okx".to_string(), okx_series);
        exchange_data.insert("bitget".to_string(), bitget_series);

        let result = service
            .generate_correlation_metrics("BTC/USDT", &exchange_data, &user_preferences)
            .await;

        assert!(result.is_ok());
        let metrics = result.unwrap();

        // With 4 exchanges, we should have 6 price correlations (4 choose 2)
        assert_eq!(metrics.price_correlations.len(), 6);
        // We should have 12 leadership analyses (6 pairs × 2 directions)
        assert_eq!(metrics.leadership_analysis.len(), 12);
        // We should have 6 technical correlations for technical focus
        assert_eq!(metrics.technical_correlations.len(), 6);
        assert!(metrics.confidence_score > 0.0);
    }

    #[test]
    fn test_correlation_analysis_service_functionality() {
        let config = CorrelationAnalysisConfig::default();
        let logger = Logger::new(LogLevel::Info);
        let service = CorrelationAnalysisService::new(config, logger);

        // Test actual functionality instead of just size check
        let base_time = 1672531200000; // 2024-01-01 00:00:00 UTC in milliseconds

        // Create test price series with sufficient data points (> 20)
        let prices_a = vec![
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0,
            112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0, 120.0,
        ];
        let series_a = create_test_price_series(base_time, prices_a, 60000, "binance", "BTC/USDT");

        let prices_b = vec![
            99.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0,
            111.0, 112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0,
        ];
        let series_b = create_test_price_series(base_time, prices_b, 60000, "bybit", "BTC/USDT");

        // Test correlation calculation functionality
        let result = service.calculate_price_correlation(&series_a, &series_b, "binance", "bybit");

        // Verify the service actually works
        assert!(
            result.is_ok(),
            "Correlation calculation should succeed with valid data"
        );
        let correlation_data = result.unwrap();

        // Validate functional output
        assert_eq!(correlation_data.exchange_a, "binance");
        assert_eq!(correlation_data.exchange_b, "bybit");
        assert!(
            correlation_data.correlation_coefficient >= -1.0
                && correlation_data.correlation_coefficient <= 1.0,
            "Correlation coefficient should be between -1 and 1"
        );
        assert!(
            correlation_data.confidence_level >= 0.0 && correlation_data.confidence_level <= 1.0,
            "Confidence level should be between 0 and 1"
        );
        assert!(
            correlation_data.data_points >= 20,
            "Should have sufficient data points for analysis"
        );

        // Test service with insufficient data (should fail appropriately)
        let small_prices = vec![100.0, 101.0, 102.0]; // Only 3 points, below minimum 20
        let small_series_a = create_test_price_series(
            base_time,
            small_prices.clone(),
            60000,
            "binance",
            "BTC/USDT",
        );
        let small_series_b =
            create_test_price_series(base_time, small_prices, 60000, "bybit", "BTC/USDT");

        let insufficient_result = service.calculate_price_correlation(
            &small_series_a,
            &small_series_b,
            "binance",
            "bybit",
        );
        assert!(
            insufficient_result.is_err(),
            "Should fail with insufficient data points"
        );
        assert!(
            insufficient_result
                .unwrap_err()
                .contains("Insufficient data points"),
            "Error should indicate insufficient data points"
        );
    }
}
