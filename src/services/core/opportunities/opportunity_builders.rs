// src/services/core/opportunities/opportunity_builders.rs

use crate::log_info;
use crate::services::core::opportunities::opportunity_core::{
    OpportunityConfig, OpportunityContext,
};
use crate::types::{
    ArbitrageOpportunity, ArbitrageType, DistributionStrategy, ExchangeIdEnum, GlobalOpportunity,
    OpportunityData, OpportunitySource, TechnicalOpportunity, TechnicalRiskLevel,
    TechnicalSignalStrength, TechnicalSignalType,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::Utc;
use serde_json;
use uuid::Uuid;

/// Unified opportunity builder for all opportunity services
/// Consolidates opportunity creation logic and provides consistent building patterns
pub struct OpportunityBuilder {
    config: OpportunityConfig,
}

impl OpportunityBuilder {
    pub fn new(config: OpportunityConfig) -> Self {
        Self { config }
    }

    // Arbitrage Opportunity Builders

    /// Build funding rate arbitrage opportunity
    pub fn build_funding_rate_arbitrage(
        &self,
        pair: String,
        long_exchange: ExchangeIdEnum,
        short_exchange: ExchangeIdEnum,
        long_rate: f64,
        short_rate: f64,
        context: &OpportunityContext,
    ) -> ArbitrageResult<ArbitrageOpportunity> {
        let rate_difference = (short_rate - long_rate).abs();

        // Validate rate difference meets minimum threshold
        if rate_difference < self.config.min_rate_difference {
            return Err(ArbitrageError::validation_error(format!(
                "Rate difference {:.4}% below minimum threshold {:.4}%",
                rate_difference * 100.0,
                self.config.min_rate_difference * 100.0
            )));
        }

        // Calculate potential profit value
        let potential_profit_value =
            self.calculate_arbitrage_profit_value(rate_difference, context);

        let opportunity = ArbitrageOpportunity {
            id: Uuid::new_v4().to_string(),
            trading_pair: pair.clone(),
            exchanges: vec![long_exchange.to_string(), short_exchange.to_string()],
            profit_percentage: rate_difference,
            confidence_score: 0.8, // Default confidence for funding rate arbitrage
            risk_level: "medium".to_string(),
            buy_exchange: long_exchange.to_string(),
            sell_exchange: short_exchange.to_string(),
            buy_price: 0.0,
            sell_price: 0.0,
            volume: 1000.0, // Default volume
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000)), // 15 minutes
            pair: pair.clone(),
            long_exchange: long_exchange.to_string(),
            short_exchange: short_exchange.to_string(),
            long_rate: None,
            short_rate: None,
            rate_difference,
            net_rate_difference: Some(rate_difference),
            potential_profit_value: None,
            confidence: 0.8,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            detected_at: chrono::Utc::now().timestamp_millis() as u64,
            r#type: ArbitrageType::FundingRate,
            details: Some(format!(
                "Funding rate arbitrage: Long {} at {:.4}%, Short {} at {:.4}%",
                long_exchange,
                long_rate * 100.0,
                short_exchange,
                short_rate * 100.0
            )),
            min_exchanges_required: 2,
        };

        log_info!(
            "Built funding rate arbitrage opportunity",
            serde_json::json!({
                "pair": opportunity.pair,
                "rate_difference": rate_difference,
                "potential_profit": potential_profit_value,
                "context": format!("{:?}", context)
            })
        );

        Ok(opportunity)
    }

    /// Build price arbitrage opportunity
    pub fn build_price_arbitrage(
        &self,
        pair: String,
        long_exchange: ExchangeIdEnum,
        short_exchange: ExchangeIdEnum,
        long_price: f64,
        short_price: f64,
        context: &OpportunityContext,
    ) -> ArbitrageResult<ArbitrageOpportunity> {
        let price_difference = ((short_price - long_price) / long_price).abs();

        // Validate price difference meets minimum threshold
        if price_difference < self.config.min_rate_difference {
            return Err(ArbitrageError::validation_error(format!(
                "Price difference {:.4}% below minimum threshold {:.4}%",
                price_difference * 100.0,
                self.config.min_rate_difference * 100.0
            )));
        }

        let potential_profit_value =
            self.calculate_arbitrage_profit_value(price_difference, context);

        let opportunity = ArbitrageOpportunity {
            id: Uuid::new_v4().to_string(),
            trading_pair: pair.clone(),
            exchanges: vec![long_exchange.to_string(), short_exchange.to_string()],
            profit_percentage: price_difference,
            confidence_score: 0.8, // Default confidence score
            risk_level: "medium".to_string(),
            buy_exchange: long_exchange.to_string(),
            sell_exchange: short_exchange.to_string(),
            buy_price: 0.0,
            sell_price: 0.0,
            volume: 1000.0, // Default volume
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000)), // 15 minutes
            pair: pair.clone(),
            long_exchange: long_exchange.to_string(),
            short_exchange: short_exchange.to_string(),
            long_rate: None, // Not applicable for price arbitrage
            short_rate: None,
            rate_difference: price_difference,
            net_rate_difference: Some(price_difference),
            potential_profit_value: None,
            confidence: 0.8,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            detected_at: chrono::Utc::now().timestamp_millis() as u64,
            r#type: ArbitrageType::Price,
            details: Some(format!(
                "Price arbitrage: Buy {} (${:.2}) vs Sell {} (${:.2})",
                long_exchange.as_str(),
                long_price,
                short_exchange.as_str(),
                short_price
            )),
            min_exchanges_required: 2,
        };

        log_info!(
            "Built price arbitrage opportunity",
            serde_json::json!({
                "pair": opportunity.pair,
                "price_difference": price_difference,
                "potential_profit": potential_profit_value,
                "context": format!("{:?}", context)
            })
        );

        Ok(opportunity)
    }

    /// Build cross-exchange arbitrage opportunity
    pub fn build_cross_exchange_arbitrage(
        &self,
        pair: String,
        exchanges: Vec<(ExchangeIdEnum, f64)>, // (exchange, price/rate)
        arbitrage_type: ArbitrageType,
        context: &OpportunityContext,
    ) -> ArbitrageResult<ArbitrageOpportunity> {
        if exchanges.len() < 2 {
            return Err(ArbitrageError::validation_error(
                "At least 2 exchanges required for cross-exchange arbitrage".to_string(),
            ));
        }

        // Find best buy and sell opportunities
        let (min_exchange, min_value) = exchanges
            .iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();
        let (max_exchange, max_value) = exchanges
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();

        let difference = (max_value - min_value) / min_value;

        if difference < self.config.min_rate_difference {
            return Err(ArbitrageError::validation_error(format!(
                "Cross-exchange difference {:.4}% below minimum threshold {:.4}%",
                difference * 100.0,
                self.config.min_rate_difference * 100.0
            )));
        }

        let potential_profit_value = self.calculate_arbitrage_profit_value(difference, context);

        let opportunity = ArbitrageOpportunity {
            id: Uuid::new_v4().to_string(),
            trading_pair: pair.clone(),
            exchanges: vec![min_exchange.to_string(), max_exchange.to_string()],
            profit_percentage: difference,
            confidence_score: 0.8, // Default confidence score
            risk_level: "medium".to_string(),
            buy_exchange: min_exchange.to_string(),
            sell_exchange: max_exchange.to_string(),
            buy_price: 0.0,
            sell_price: 0.0,
            volume: 1000.0, // Default volume
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000)), // 15 minutes
            pair: pair.clone(),
            long_exchange: min_exchange.to_string(),
            short_exchange: max_exchange.to_string(),
            long_rate: Some(*min_value),
            short_rate: Some(*max_value),
            rate_difference: difference,
            net_rate_difference: Some(difference),
            potential_profit_value: None,
            confidence: 0.8,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            detected_at: chrono::Utc::now().timestamp_millis() as u64,
            r#type: arbitrage_type,
            details: Some(format!(
                "Cross-exchange arbitrage: {} exchanges, best spread {:.4}%",
                exchanges.len(),
                difference * 100.0
            )),
            min_exchanges_required: 2,
        };

        log_info!(
            "Built cross-exchange arbitrage opportunity",
            serde_json::json!({
                "pair": opportunity.pair,
                "exchanges_count": exchanges.len(),
                "difference": difference,
                "potential_profit": potential_profit_value
            })
        );

        Ok(opportunity)
    }

    // Technical Opportunity Builders

    /// Build technical analysis opportunity
    #[allow(clippy::too_many_arguments)]
    pub fn build_technical_opportunity(
        &self,
        pair: String,
        exchange: ExchangeIdEnum,
        signal_type: TechnicalSignalType,
        signal_strength: TechnicalSignalStrength,
        confidence_score: f64,
        entry_price: f64,
        target_price: Option<f64>,
        stop_loss_price: Option<f64>,
        technical_indicators: Vec<String>,
        timeframe: String,
        expected_return_percentage: f64,
        _market_conditions: String,
        _context: &OpportunityContext,
    ) -> ArbitrageResult<TechnicalOpportunity> {
        // Validate inputs
        if pair.is_empty() {
            return Err(ArbitrageError::validation_error(
                "Pair cannot be empty".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&confidence_score) {
            return Err(ArbitrageError::validation_error(
                "Confidence score must be between 0.0 and 1.0".to_string(),
            ));
        }
        if entry_price <= 0.0 {
            return Err(ArbitrageError::validation_error(
                "Entry price must be positive".to_string(),
            ));
        }

        // Calculate expected return based on signal type and target price
        let calculated_return = if let Some(target) = target_price {
            match signal_type {
                TechnicalSignalType::Buy => (target - entry_price) / entry_price,
                TechnicalSignalType::Sell => (entry_price - target) / entry_price,
                TechnicalSignalType::Hold => 0.0,
                TechnicalSignalType::MovingAverageCrossover => (target - entry_price) / entry_price,
                TechnicalSignalType::RSIOverBought => (entry_price - target) / entry_price,
                TechnicalSignalType::RSIOverSold => (target - entry_price) / entry_price,
                TechnicalSignalType::MACDSignal => (target - entry_price) / entry_price,
                TechnicalSignalType::BollingerBands => (target - entry_price) / entry_price,
                TechnicalSignalType::SupportResistance => (target - entry_price) / entry_price,
                TechnicalSignalType::VolumeSpike => (target - entry_price) / entry_price,
                TechnicalSignalType::PriceBreakout => (target - entry_price) / entry_price,
                TechnicalSignalType::DivergencePattern => (target - entry_price) / entry_price,
                TechnicalSignalType::CandlestickPattern => (target - entry_price) / entry_price,
            }
        } else {
            expected_return_percentage
        };

        // Calculate stop loss distance for risk assessment
        let stop_loss_distance = if let Some(stop_loss) = stop_loss_price {
            (entry_price - stop_loss).abs() / entry_price
        } else {
            0.02 // Default 2% stop loss distance
        };

        let risk_level =
            self.determine_risk_level(calculated_return, stop_loss_distance, confidence_score);

        // Calculate expiration time (default 4 hours from now)
        let expires_at = Utc::now().timestamp_millis() as u64 + (4 * 60 * 60 * 1000);

        let opportunity = TechnicalOpportunity {
            id: Uuid::new_v4().to_string(),
            trading_pair: pair.clone(),
            symbol: pair.clone(),
            exchange,
            signal_type,
            signal_strength,
            risk_level,
            entry_price,
            target_price: target_price.unwrap_or(entry_price * 1.02),
            stop_loss: stop_loss_price.unwrap_or(entry_price * 0.98),
            confidence: confidence_score,
            timeframe: timeframe.to_string(),
            indicators: serde_json::to_value(&technical_indicators).unwrap_or_default(),
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(expires_at),
            metadata: serde_json::json!({
                "builder_version": "1.0",
                "signal_source": "technical_analysis",
                "market_conditions": _market_conditions,
                "expected_return": expected_return_percentage,
                "stop_loss_distance": stop_loss_distance
            }),
            expected_return_percentage,
            details: None,
            confidence_score,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };

        Ok(opportunity)
    }

    /// Build a momentum-based technical opportunity
    /// This is a specialized version of technical opportunity for momentum signals
    #[allow(clippy::too_many_arguments)]
    pub fn build_momentum_opportunity(
        &self,
        pair: String,
        exchange: ExchangeIdEnum,
        momentum_score: f64,
        price_change_24h: f64,
        volume_change_24h: f64,
        current_price: f64,
        context: &OpportunityContext,
    ) -> ArbitrageResult<TechnicalOpportunity> {
        // Determine signal type based on momentum
        let signal_type = if momentum_score > 0.5 {
            TechnicalSignalType::Buy
        } else if momentum_score < -0.5 {
            TechnicalSignalType::Sell
        } else {
            TechnicalSignalType::Hold
        };

        // Determine signal strength based on momentum score
        let signal_strength = if momentum_score.abs() > 0.8 {
            TechnicalSignalStrength::Strong
        } else if momentum_score.abs() > 0.6 {
            TechnicalSignalStrength::Moderate
        } else {
            TechnicalSignalStrength::Weak
        };

        // Calculate confidence based on momentum and volume
        let confidence =
            self.calculate_momentum_confidence(momentum_score, price_change_24h, volume_change_24h);

        // Calculate target and stop loss prices
        let (target_price, stop_loss_price) =
            self.calculate_momentum_targets(current_price, momentum_score, price_change_24h);

        // Use the main technical opportunity builder
        self.build_technical_opportunity(
            pair,
            exchange,
            signal_type,
            signal_strength,
            confidence,
            current_price,
            Some(target_price),
            Some(stop_loss_price),
            vec!["Momentum".to_string(), "Volume".to_string()],
            "24h".to_string(),
            price_change_24h.abs(),
            format!(
                "Momentum: {:.2}, Volume change: {:.2}%",
                momentum_score,
                volume_change_24h * 100.0
            ),
            context,
        )
    }

    // Global Opportunity Builders

    /// Build global opportunity from arbitrage opportunity
    pub fn build_global_opportunity_from_arbitrage(
        &self,
        arbitrage_opportunity: ArbitrageOpportunity,
        source: OpportunitySource,
        expires_at: u64,
        max_participants: Option<u32>,
        _distribution_strategy: DistributionStrategy,
    ) -> ArbitrageResult<GlobalOpportunity> {
        let priority_score = self.calculate_priority_score(&arbitrage_opportunity);

        let global_opportunity = GlobalOpportunity {
            id: Uuid::new_v4().to_string(),
            opportunity_id: Uuid::new_v4().to_string(),
            opportunity_type: source.as_str().to_string(),
            trading_pair: arbitrage_opportunity.trading_pair.clone(),
            exchanges: arbitrage_opportunity.exchanges.clone(),
            profit_percentage: arbitrage_opportunity.profit_percentage,
            confidence_score: arbitrage_opportunity.confidence_score,
            risk_level: arbitrage_opportunity.risk_level.clone(),
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(expires_at),
            metadata: serde_json::json!({}),
            distributed_to: Vec::new(),
            max_participants: max_participants.unwrap_or(100),
            current_participants: 0,
            distribution_strategy: DistributionStrategy::Broadcast,
            arbitrage_opportunity: arbitrage_opportunity.clone(),
            target_users: Vec::new(),
            opportunity_data: OpportunityData::Arbitrage(arbitrage_opportunity.clone()),
            source,
            detection_timestamp: chrono::Utc::now().timestamp_millis() as u64,
            priority: 1,
            priority_score: 0.0,
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
        };

        // Create analytics metadata
        let analytics_metadata = serde_json::json!({
            "global_id": global_opportunity.id,
            "arbitrage_id": arbitrage_opportunity.id,
            "priority_score": priority_score,
            "expires_at": expires_at
        });

        log_info!(
            "Built global opportunity from arbitrage",
            analytics_metadata
        );

        Ok(global_opportunity)
    }

    /// Build global opportunity from technical opportunity
    pub fn build_global_opportunity_from_technical(
        &self,
        technical_opportunity: TechnicalOpportunity,
        source: OpportunitySource,
        expires_at: u64,
        max_participants: Option<u32>,
        _distribution_strategy: DistributionStrategy,
    ) -> ArbitrageResult<GlobalOpportunity> {
        let priority_score = self.calculate_technical_priority_score(&technical_opportunity);

        let global_opportunity = GlobalOpportunity {
            id: Uuid::new_v4().to_string(),
            opportunity_id: Uuid::new_v4().to_string(),
            opportunity_type: source.as_str().to_string(),
            trading_pair: technical_opportunity.trading_pair.clone(),
            exchanges: vec![technical_opportunity.exchange.to_string()],
            profit_percentage: technical_opportunity.expected_return_percentage,
            confidence_score: technical_opportunity.confidence_score,
            risk_level: technical_opportunity.risk_level.to_string(),
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(expires_at),
            metadata: serde_json::json!({}),
            distributed_to: Vec::new(),
            max_participants: max_participants.unwrap_or(100),
            current_participants: 0,
            distribution_strategy: DistributionStrategy::Broadcast,
            arbitrage_opportunity: ArbitrageOpportunity::default(),
            target_users: Vec::new(),
            opportunity_data: OpportunityData::Technical(technical_opportunity.clone()),
            source,
            detection_timestamp: chrono::Utc::now().timestamp_millis() as u64,
            priority: 1,
            priority_score: 0.0,
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
        };

        log_info!(
            "Built global opportunity from technical",
            serde_json::json!({
                "global_id": global_opportunity.id,
                "technical_id": technical_opportunity.id,
                "priority_score": priority_score,
                "expires_at": expires_at
            })
        );

        Ok(global_opportunity)
    }

    // Helper Methods

    /// Calculate arbitrage profit value based on context
    fn calculate_arbitrage_profit_value(
        &self,
        rate_difference: f64,
        context: &OpportunityContext,
    ) -> f64 {
        let base_value = rate_difference * 1000.0; // Scale up for easier comparison

        match context {
            OpportunityContext::Personal { user_id: _ } => base_value,
            OpportunityContext::Group {
                admin_id: _,
                chat_context: _,
            } => base_value * 2.0, // Group multiplier
            OpportunityContext::Global { system_level: _ } => base_value * 1.5, // Global multiplier
        }
    }

    /// Determine risk level for technical opportunities
    fn determine_risk_level(
        &self,
        expected_return: f64,
        stop_loss_distance: f64,
        confidence_score: f64,
    ) -> TechnicalRiskLevel {
        let risk_score = (expected_return.abs() + stop_loss_distance) * (1.0 - confidence_score);

        if risk_score > 0.15 {
            TechnicalRiskLevel::High
        } else if risk_score > 0.08 {
            TechnicalRiskLevel::Medium
        } else {
            TechnicalRiskLevel::Low
        }
    }

    /// Calculate momentum confidence score
    fn calculate_momentum_confidence(
        &self,
        momentum_score: f64,
        price_change: f64,
        volume_change: f64,
    ) -> f64 {
        let momentum_weight = 0.5;
        let price_weight = 0.3;
        let volume_weight = 0.2;

        let momentum_confidence = momentum_score.abs().min(1.0);
        let price_confidence = (price_change.abs() / 0.1).min(1.0); // Normalize to 10% change
        let volume_confidence = (volume_change.abs() / 0.5).min(1.0); // Normalize to 50% change

        (momentum_confidence * momentum_weight
            + price_confidence * price_weight
            + volume_confidence * volume_weight)
            .min(1.0)
    }

    /// Calculate momentum-based targets
    fn calculate_momentum_targets(
        &self,
        current_price: f64,
        momentum_score: f64,
        price_change: f64,
    ) -> (f64, f64) {
        let momentum_factor = momentum_score.abs().min(0.1); // Cap at 10%
        let price_factor = price_change.abs().min(0.05); // Cap at 5%

        let target_change = (momentum_factor + price_factor) * momentum_score.signum();
        let stop_loss_change = -target_change * 0.5; // 50% of target as stop loss

        let target_price = current_price * (1.0 + target_change);
        let stop_loss = current_price * (1.0 + stop_loss_change);

        (target_price, stop_loss)
    }

    /// Calculate priority score for arbitrage opportunities
    fn calculate_priority_score(&self, opportunity: &ArbitrageOpportunity) -> f64 {
        let base_score = opportunity.rate_difference * 1000.0;
        let profit_multiplier = opportunity.potential_profit_value.unwrap_or(1.0) / 100.0;

        base_score * profit_multiplier.max(1.0)
    }

    /// Calculate priority score for technical opportunities
    fn calculate_technical_priority_score(&self, opportunity: &TechnicalOpportunity) -> f64 {
        // Extract calculated_return from metadata
        let return_score = opportunity
            .metadata
            .get("calculated_return")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .abs()
            * 100.0;
        let confidence_multiplier = opportunity.confidence;
        let strength_multiplier = match opportunity.signal_strength {
            TechnicalSignalStrength::Strong => 1.5,
            TechnicalSignalStrength::Moderate => 1.2,
            TechnicalSignalStrength::Weak => 0.8,
            TechnicalSignalStrength::VeryStrong => 2.0,
        };

        return_score * confidence_multiplier * strength_multiplier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> OpportunityConfig {
        OpportunityConfig::default()
    }

    #[test]
    fn test_funding_rate_arbitrage_builder() {
        let config = create_test_config();
        let builder = OpportunityBuilder::new(config);

        let result = builder.build_funding_rate_arbitrage(
            "BTCUSDT".to_string(),
            ExchangeIdEnum::Binance,
            ExchangeIdEnum::Bybit,
            0.0005,
            0.0015,
            &OpportunityContext::Personal {
                user_id: "test_user".to_string(),
            },
        );

        assert!(result.is_ok());
        let opportunity = result.unwrap();
        assert_eq!(opportunity.pair, "BTCUSDT");
        assert_eq!(opportunity.long_exchange, ExchangeIdEnum::Binance);
        assert_eq!(opportunity.short_exchange, ExchangeIdEnum::Bybit);
        assert_eq!(opportunity.rate_difference, 0.001);
        assert!(matches!(opportunity.r#type, ArbitrageType::FundingRate));
    }

    #[test]
    fn test_technical_opportunity_builder() {
        let config = create_test_config();
        let builder = OpportunityBuilder::new(config);
        let context = OpportunityContext::Personal {
            user_id: "test_user".to_string(),
        };

        let result = builder.build_technical_opportunity(
            "ETHUSDT".to_string(),
            ExchangeIdEnum::Binance,
            TechnicalSignalType::Buy,
            TechnicalSignalStrength::Strong,
            0.85,
            3000.0,                                      // entry_price
            Some(3150.0),                                // target_price
            Some(2950.0),                                // stop_loss_price
            vec!["RSI".to_string(), "MACD".to_string()], // technical_indicators
            "1h".to_string(),                            // timeframe
            0.05,                                        // expected_return_percentage
            "Bullish momentum".to_string(),
            &context,
        );

        assert!(result.is_ok());
        let opportunity = result.unwrap();
        assert_eq!(opportunity.symbol, "ETHUSDT");
        assert_eq!(opportunity.exchange, ExchangeIdEnum::Binance);
        assert_eq!(opportunity.signal_type, TechnicalSignalType::Buy);
        assert_eq!(opportunity.confidence, 0.85);
        assert_eq!(opportunity.entry_price, 3000.0);
        assert_eq!(opportunity.target_price, 3150.0);
        assert_eq!(opportunity.stop_loss, 2950.0);
        assert_eq!(opportunity.timeframe, "1h");
        assert!(opportunity.expected_return_percentage > 0.0);
    }

    #[test]
    fn test_momentum_opportunity_builder() {
        let config = create_test_config();
        let builder = OpportunityBuilder::new(config);
        let context = OpportunityContext::Personal {
            user_id: "test_user".to_string(),
        };

        let result = builder.build_momentum_opportunity(
            "ADAUSDT".to_string(),
            ExchangeIdEnum::Bybit,
            0.75, // momentum_score
            0.05, // price_change_24h (5%)
            0.20, // volume_change_24h (20%)
            0.5,  // current_price
            &context,
        );

        assert!(result.is_ok());
        let opportunity = result.unwrap();
        assert_eq!(opportunity.symbol, "ADAUSDT");
        assert_eq!(opportunity.exchange, ExchangeIdEnum::Bybit);
        assert_eq!(opportunity.signal_type, TechnicalSignalType::Buy);
        assert_eq!(opportunity.confidence, 0.75);
        assert_eq!(opportunity.entry_price, 0.5);
        assert_eq!(opportunity.target_price, 0.51);
        assert_eq!(opportunity.stop_loss, 0.49);
        assert_eq!(opportunity.timeframe, "24h");
        assert!(opportunity.confidence > 0.0);
    }

    #[test]
    fn test_global_opportunity_from_arbitrage() {
        let config = create_test_config();
        let builder = OpportunityBuilder::new(config);

        let arbitrage_opp = ArbitrageOpportunity {
            id: "test_arb_001".to_string(),
            pair: "BTCUSDT".to_string(),
            long_exchange: ExchangeIdEnum::Binance,
            short_exchange: ExchangeIdEnum::Bybit,
            long_rate: Some(0.01),
            short_rate: Some(-0.005),
            rate_difference: 0.015,
            net_rate_difference: Some(0.015),
            potential_profit_value: Some(150.0),
            confidence: 0.85,
            volume: 1000.0,
            timestamp: Utc::now().timestamp_millis() as u64,
            created_at: Utc::now().timestamp_millis() as u64,
            detected_at: Utc::now().timestamp_millis() as u64,
            expires_at: Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000),
            r#type: ArbitrageType::FundingRate,
            details: Some("Test arbitrage".to_string()),
            min_exchanges_required: 2,
        };

        let result = builder.build_global_opportunity_from_arbitrage(
            arbitrage_opp,
            OpportunitySource::SystemGenerated,
            Utc::now().timestamp_millis() as u64 + (10 * 60 * 1000),
            Some(100),
            DistributionStrategy::FirstComeFirstServe,
        );

        assert!(result.is_ok());
        let global_opp = result.unwrap();
        assert!(global_opp.id.starts_with("global_arb_"));
        assert_eq!(global_opp.source, OpportunitySource::SystemGenerated);
        assert_eq!(global_opp.max_participants, Some(100));
        assert_eq!(
            global_opp.distribution_strategy,
            DistributionStrategy::FirstComeFirstServe
        );
    }

    #[test]
    fn test_risk_level_determination() {
        let config = create_test_config();
        let builder = OpportunityBuilder::new(config);

        // High risk: high expected return, low confidence
        let high_risk = builder.determine_risk_level(0.2, 0.1, 0.3);
        assert!(matches!(high_risk, TechnicalRiskLevel::High));

        // Low risk: moderate return, high confidence
        let low_risk = builder.determine_risk_level(0.05, 0.02, 0.9);
        assert!(matches!(low_risk, TechnicalRiskLevel::Low));

        // Medium risk: moderate values
        let medium_risk = builder.determine_risk_level(0.1, 0.05, 0.7);
        assert!(matches!(medium_risk, TechnicalRiskLevel::Medium));
    }

    #[test]
    fn test_validation_errors() {
        let config = create_test_config();
        let builder = OpportunityBuilder::new(config);

        // Test rate difference below threshold
        let result = builder.build_funding_rate_arbitrage(
            "BTCUSDT".to_string(),
            ExchangeIdEnum::Binance,
            ExchangeIdEnum::Bybit,
            0.0001,
            0.0002,
            &OpportunityContext::Personal {
                user_id: "test_user".to_string(),
            },
        );
        assert!(result.is_err());

        // Test invalid confidence score
        let result = builder.build_technical_opportunity(
            "ETHUSDT".to_string(),
            ExchangeIdEnum::Binance,
            TechnicalSignalType::Buy,
            TechnicalSignalStrength::Strong,
            1.5, // Invalid: > 1.0
            3500.0,
            Some(3200.0),
            Some(3300.0),
            vec!["Test".to_string()],
            "1h".to_string(),
            0.05,
            "Test market conditions".to_string(),
            &OpportunityContext::Personal {
                user_id: "test_user".to_string(),
            },
        );
        assert!(result.is_err());
    }
}
