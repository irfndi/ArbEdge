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

        // Use feature flags for dynamic threshold configuration
        let min_threshold = crate::utils::feature_flags::get_numeric_feature_value(
            "opportunity_engine.min_rate_threshold",
            self.config.min_rate_difference,
        );

        // Validate rate difference meets minimum threshold
        let rate_difference_percent = rate_difference * 100.0;

        // Enhanced logging for production debugging
        if crate::utils::feature_flags::is_feature_enabled("opportunity_engine.enhanced_logging")
            .unwrap_or(false)
        {
            #[cfg(target_arch = "wasm32")]
            worker::console_log!(
                "📊 FUNDING_ARBITRAGE - Pair: {}, Long: {} ({:.6}%), Short: {} ({:.6}%), Diff: {:.6}%, Threshold: {:.6}%",
                pair,
                long_exchange.as_str(),
                long_rate * 100.0,
                short_exchange.as_str(),
                short_rate * 100.0,
                rate_difference_percent,
                min_threshold
            );

            #[cfg(not(target_arch = "wasm32"))]
            println!(
                "📊 FUNDING_ARBITRAGE - Pair: {}, Long: {} ({:.6}%), Short: {} ({:.6}%), Diff: {:.6}%, Threshold: {:.6}%",
                pair,
                long_exchange.as_str(),
                long_rate * 100.0,
                short_exchange.as_str(),
                short_rate * 100.0,
                rate_difference_percent,
                min_threshold
            );
        }

        if rate_difference_percent < min_threshold {
            return Err(ArbitrageError::validation_error(format!(
                "Rate difference {:.6}% below minimum threshold {:.6}%",
                rate_difference_percent, min_threshold
            )));
        }

        // Calculate potential profit value
        let potential_profit_value =
            self.calculate_arbitrage_profit_value(rate_difference, context);

        let confidence_score =
            self.calculate_funding_rate_confidence(rate_difference, long_rate, short_rate);

        let opportunity = ArbitrageOpportunity {
            id: Uuid::new_v4().to_string(),
            trading_pair: pair.clone(),
            exchanges: vec![long_exchange.to_string(), short_exchange.to_string()],
            profit_percentage: rate_difference,
            confidence_score,
            risk_level: "medium".to_string(),
            buy_exchange: long_exchange.to_string(),
            sell_exchange: short_exchange.to_string(),
            buy_price: 0.0,
            sell_price: 0.0,
            volume: 1000.0, // Default volume
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000)), // 15 minutes
            pair: pair.clone(),
            long_exchange,
            short_exchange,
            long_rate: Some(long_rate),
            short_rate: Some(short_rate),
            rate_difference,
            net_rate_difference: Some(rate_difference),
            potential_profit_value: Some(potential_profit_value),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            detected_at: chrono::Utc::now().timestamp_millis() as u64,
            r#type: ArbitrageType::FundingRate,
            details: Some(format!(
                "Funding rate arbitrage: Long {} at {:.6}%, Short {} at {:.6}%, Diff: {:.6}%",
                long_exchange.as_str(),
                long_rate * 100.0,
                short_exchange.as_str(),
                short_rate * 100.0,
                rate_difference_percent
            )),
            min_exchanges_required: 2,
        };

        log_info!(
            "Built funding rate arbitrage opportunity",
            serde_json::json!({
                "pair": opportunity.pair,
                "rate_difference": rate_difference,
                "rate_difference_percent": rate_difference_percent,
                "potential_profit": potential_profit_value,
                "confidence_score": confidence_score,
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

        // Use feature flags for dynamic threshold configuration
        let min_threshold = crate::utils::feature_flags::get_numeric_feature_value(
            "opportunity_engine.min_rate_threshold",
            self.config.min_rate_difference,
        );

        // Validate price difference meets minimum threshold
        let price_difference_percent = price_difference * 100.0;

        // Enhanced logging for production debugging
        if crate::utils::feature_flags::is_feature_enabled("opportunity_engine.enhanced_logging")
            .unwrap_or(false)
        {
            #[cfg(target_arch = "wasm32")]
            worker::console_log!(
                "💰 PRICE_ARBITRAGE - Pair: {}, Long: {} (${:.4}), Short: {} (${:.4}), Diff: {:.6}%, Threshold: {:.6}%",
                pair,
                long_exchange.as_str(),
                long_price,
                short_exchange.as_str(),
                short_price,
                price_difference_percent,
                min_threshold
            );

            #[cfg(not(target_arch = "wasm32"))]
            println!(
                "💰 PRICE_ARBITRAGE - Pair: {}, Long: {} (${:.4}), Short: {} (${:.4}), Diff: {:.6}%, Threshold: {:.6}%",
                pair,
                long_exchange.as_str(),
                long_price,
                short_exchange.as_str(),
                short_price,
                price_difference_percent,
                min_threshold
            );
        }

        if price_difference_percent < min_threshold {
            return Err(ArbitrageError::validation_error(format!(
                "Price difference {:.6}% below minimum threshold {:.6}%",
                price_difference_percent, min_threshold
            )));
        }

        let potential_profit_value =
            self.calculate_arbitrage_profit_value(price_difference, context);

        let confidence_score =
            self.calculate_price_arbitrage_confidence(price_difference, long_price, short_price);

        let opportunity = ArbitrageOpportunity {
            id: Uuid::new_v4().to_string(),
            trading_pair: pair.clone(),
            exchanges: vec![long_exchange.to_string(), short_exchange.to_string()],
            profit_percentage: price_difference,
            confidence_score,
            risk_level: "medium".to_string(),
            buy_exchange: long_exchange.to_string(),
            sell_exchange: short_exchange.to_string(),
            buy_price: long_price,
            sell_price: short_price,
            volume: 1000.0, // Default volume
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000)), // 15 minutes
            pair: pair.clone(),
            long_exchange,
            short_exchange,
            long_rate: None, // Not applicable for price arbitrage
            short_rate: None,
            rate_difference: price_difference,
            net_rate_difference: Some(price_difference),
            potential_profit_value: Some(potential_profit_value),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            detected_at: chrono::Utc::now().timestamp_millis() as u64,
            r#type: ArbitrageType::Price,
            details: Some(format!(
                "Price arbitrage: Buy {} (${:.4}) vs Sell {} (${:.4}), Diff: {:.4}%",
                long_exchange.as_str(),
                long_price,
                short_exchange.as_str(),
                short_price,
                price_difference_percent
            )),
            min_exchanges_required: 2,
        };

        log_info!(
            "Built price arbitrage opportunity",
            serde_json::json!({
                "pair": opportunity.pair,
                "price_difference": price_difference,
                "price_difference_percent": price_difference_percent,
                "potential_profit": potential_profit_value,
                "confidence_score": confidence_score,
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

        let difference_percent = difference * 100.0;
        if difference_percent < self.config.min_rate_difference {
            return Err(ArbitrageError::validation_error(format!(
                "Cross-exchange difference {:.4}% below minimum threshold {:.4}%",
                difference_percent, self.config.min_rate_difference
            )));
        }

        let potential_profit_value = self.calculate_arbitrage_profit_value(difference, context);

        let confidence_score =
            self.calculate_cross_exchange_confidence(difference, exchanges.len());

        let opportunity = ArbitrageOpportunity {
            id: Uuid::new_v4().to_string(),
            trading_pair: pair.clone(),
            exchanges: vec![min_exchange.to_string(), max_exchange.to_string()],
            profit_percentage: difference,
            confidence_score,
            risk_level: "medium".to_string(),
            buy_exchange: min_exchange.to_string(),
            sell_exchange: max_exchange.to_string(),
            buy_price: 0.0,
            sell_price: 0.0,
            volume: 1000.0, // Default volume
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000)), // 15 minutes
            pair: pair.clone(),
            long_exchange: *min_exchange,
            short_exchange: *max_exchange,
            long_rate: Some(*min_value),
            short_rate: Some(*max_value),
            rate_difference: difference,
            net_rate_difference: Some(difference),
            potential_profit_value: Some(potential_profit_value),
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
        _signal_strength: TechnicalSignalStrength,
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
            exchanges: vec![exchange.to_string()],
            pair: pair.clone(),
            signal_type,
            confidence: confidence_score,
            risk_level: match risk_level {
                TechnicalRiskLevel::VeryLow => "very_low".to_string(),
                TechnicalRiskLevel::Low => "low".to_string(),
                TechnicalRiskLevel::Medium => "medium".to_string(),
                TechnicalRiskLevel::High => "high".to_string(),
                TechnicalRiskLevel::VeryHigh => "very_high".to_string(),
            },
            entry_price,
            target_price: target_price.unwrap_or(entry_price * 1.02),
            stop_loss: stop_loss_price.unwrap_or(entry_price * 0.98),
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
        let now = chrono::Utc::now().timestamp_millis() as u64;

        let global_opportunity = GlobalOpportunity {
            id: format!(
                "global_arb_{}",
                Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("unknown")
            ),
            source: source.clone(),
            opportunity_type: source.clone(),
            target_users: Vec::new(),
            opportunity_data: OpportunityData::Arbitrage(arbitrage_opportunity.clone()), // All specific opportunity data is here
            created_at: now,
            detection_timestamp: now,
            expires_at,
            priority: 1,    // Default priority, can be adjusted based on logic
            priority_score, // Assign calculated priority score
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
            distributed_to: Vec::new(),
            max_participants: Some(max_participants.unwrap_or(100)),
            current_participants: 0,
            distribution_strategy: _distribution_strategy, // Use the passed distribution strategy
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
        let now = chrono::Utc::now().timestamp_millis() as u64;

        let global_opportunity = GlobalOpportunity {
            id: format!(
                "global_tech_{}",
                Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("unknown")
            ),
            source: source.clone(),
            opportunity_type: source.clone(), // Keep source if it's distinct from opportunity_data's source
            target_users: Vec::new(),
            opportunity_data: OpportunityData::Technical(technical_opportunity.clone()),
            created_at: now,
            detection_timestamp: now,
            expires_at,
            priority: 1,    // Default priority, can be adjusted
            priority_score, // Assign calculated priority score
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
            distributed_to: Vec::new(),
            max_participants: Some(max_participants.unwrap_or(100)),
            current_participants: 0,
            distribution_strategy: DistributionStrategy::Broadcast, // Assuming Broadcast is a valid default or passed in
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
        let strength_multiplier = if opportunity.confidence >= 0.8 {
            2.0 // VeryStrong
        } else if opportunity.confidence >= 0.6 {
            1.5 // Strong
        } else if opportunity.confidence >= 0.4 {
            1.2 // Moderate
        } else {
            0.8 // Weak
        };

        return_score * confidence_multiplier * strength_multiplier
    }

    /// Calculate funding rate confidence score
    fn calculate_funding_rate_confidence(
        &self,
        rate_difference: f64,
        long_rate: f64,
        short_rate: f64,
    ) -> f64 {
        // Base confidence from rate difference magnitude
        let base_confidence = (rate_difference * 100.0).min(10.0) / 10.0; // 0-1 scale

        // Rate stability factor - prefer rates closer to typical funding ranges
        let typical_funding_range = 0.01; // 1% is typical max funding rate
        let rate_stability =
            1.0 - ((long_rate.abs() + short_rate.abs()) / (2.0 * typical_funding_range)).min(1.0);

        // Rate spread factor - higher spread = higher confidence
        let spread_factor = (rate_difference / 0.005).min(1.0); // 0.5% is good spread

        // Combine factors with weights
        (base_confidence * 0.4 + rate_stability * 0.3 + spread_factor * 0.3).clamp(0.1, 0.95)
        // 10% to 95% confidence range
    }

    /// Calculate price arbitrage confidence score
    fn calculate_price_arbitrage_confidence(
        &self,
        price_difference: f64,
        long_price: f64,
        short_price: f64,
    ) -> f64 {
        // Base confidence from price difference magnitude
        let base_confidence = (price_difference * 100.0).min(5.0) / 5.0; // 0-1 scale, 5% max

        // Price level factor - prefer higher absolute prices (more liquid)
        let avg_price = (long_price + short_price) / 2.0;
        let price_level_factor = if avg_price > 1000.0 {
            0.9
        } else if avg_price > 100.0 {
            0.8
        } else if avg_price > 10.0 {
            0.7
        } else {
            0.5
        };

        // Spread magnitude factor
        let spread_factor = (price_difference / 0.02).min(1.0); // 2% is good spread

        // Combine factors
        (base_confidence * 0.5 + price_level_factor * 0.3 + spread_factor * 0.2).clamp(0.1, 0.95)
    }

    /// Calculate cross-exchange confidence score
    fn calculate_cross_exchange_confidence(&self, difference: f64, exchanges_count: usize) -> f64 {
        // Base confidence from difference magnitude
        let base_confidence = (difference * 100.0).min(8.0) / 8.0; // 0-1 scale, 8% max

        // Exchange count factor - more exchanges = higher confidence
        let exchange_factor = match exchanges_count {
            2 => 0.6,
            3 => 0.75,
            4 => 0.85,
            5..=10 => 0.9,
            _ => 0.95,
        };

        // Difference magnitude factor
        let magnitude_factor = if difference > 0.05 {
            0.95 // Very high difference
        } else if difference > 0.02 {
            0.85 // Good difference
        } else if difference > 0.01 {
            0.7 // Moderate difference
        } else {
            0.5 // Low difference
        };

        // Combine factors
        (base_confidence * 0.4 + exchange_factor * 0.3 + magnitude_factor * 0.3).clamp(0.1, 0.95)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> OpportunityConfig {
        // Create test config without calling WASM-specific functions
        OpportunityConfig {
            symbols: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            max_opportunities: 100,
            enable_ai: true,
            enable_caching: true,
            cache_ttl_seconds: 300,
            min_confidence_threshold: 0.7,
            max_risk_level: 0.8,
            default_pairs: vec![
                "BTC/USDT".to_string(),
                "ETH/USDT".to_string(),
                "BNB/USDT".to_string(),
                "SOL/USDT".to_string(),
                "ADA/USDT".to_string(),
            ],
            min_rate_difference: 0.05,
            monitored_exchanges: vec![
                ExchangeIdEnum::Coinbase,
                ExchangeIdEnum::OKX,
                ExchangeIdEnum::Binance,
                ExchangeIdEnum::Bybit,
                ExchangeIdEnum::Bitget,
            ],
            opportunity_ttl_minutes: 15,
            max_participants_per_opportunity: 10,
        }
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
        assert_eq!(opportunity.trading_pair, "ETHUSDT");
        assert_eq!(opportunity.exchanges, vec!["binance".to_string()]);
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
        assert_eq!(opportunity.trading_pair, "ADAUSDT");
        assert_eq!(opportunity.exchanges, vec!["bybit".to_string()]);
        assert_eq!(opportunity.signal_type, TechnicalSignalType::Buy);
        assert_eq!(opportunity.confidence, 0.605); // Calculated confidence: (0.75*0.5)+(0.5*0.3)+(0.4*0.2) = 0.605
        assert_eq!(opportunity.entry_price, 0.5);
        assert_eq!(opportunity.target_price, 0.575); // 0.5 * (1.0 + 0.15) = 0.575
        assert_eq!(opportunity.stop_loss, 0.4625); // 0.5 * (1.0 - 0.075) = 0.4625
        assert_eq!(opportunity.timeframe, "24h");
        assert!(opportunity.confidence > 0.0);
    }

    #[test]
    fn test_global_opportunity_from_arbitrage() {
        let config = create_test_config();
        let builder = OpportunityBuilder::new(config);

        let arbitrage_opp = ArbitrageOpportunity {
            id: "test_arb_001".to_string(),
            trading_pair: "BTCUSDT".to_string(),
            exchanges: vec!["binance".to_string(), "bybit".to_string()],
            profit_percentage: 0.015,
            confidence_score: 0.85,
            risk_level: "low".to_string(),
            buy_exchange: "binance".to_string(),
            sell_exchange: "bybit".to_string(),
            buy_price: 50000.0,
            sell_price: 50750.0,
            volume: 1000.0,
            created_at: Utc::now().timestamp_millis() as u64,
            expires_at: Some(Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000)),
            pair: "BTCUSDT".to_string(),
            long_exchange: ExchangeIdEnum::Binance,
            short_exchange: ExchangeIdEnum::Bybit,
            long_rate: Some(0.01),
            short_rate: Some(-0.005),
            rate_difference: 0.015,
            net_rate_difference: Some(0.015),
            potential_profit_value: Some(150.0),
            timestamp: Utc::now().timestamp_millis() as u64,
            detected_at: Utc::now().timestamp_millis() as u64,
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

        // Low risk: moderate values with high confidence (0.045 risk score)
        let low_risk_2 = builder.determine_risk_level(0.1, 0.05, 0.7);
        assert!(matches!(low_risk_2, TechnicalRiskLevel::Low));
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
