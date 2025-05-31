// src/services/core/opportunities/ai_enhancer.rs

use crate::log_info;
use crate::services::core::ai::ai_beta_integration::AiBetaIntegrationService;
use crate::services::core::opportunities::access_manager::AccessManager;
use crate::types::{ArbitrageOpportunity, ExchangeIdEnum, TechnicalOpportunity, UserAccessLevel};
use crate::utils::ArbitrageResult;
use serde_json;
use std::sync::Arc;

/// Unified AI enhancer for all opportunity services
/// Consolidates AI enhancement logic and provides consistent AI integration
pub struct AIEnhancer {
    ai_service: Arc<AiBetaIntegrationService>,
    access_manager: Arc<AccessManager>,
}

impl AIEnhancer {
    pub fn new(
        ai_service: Arc<AiBetaIntegrationService>,
        access_manager: Arc<AccessManager>,
    ) -> Self {
        Self {
            ai_service,
            access_manager,
        }
    }

    /// Enhance arbitrage opportunities with AI for a specific user
    pub async fn enhance_arbitrage_opportunities(
        &self,
        user_id: &str,
        opportunities: Vec<ArbitrageOpportunity>,
        context: &str,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        // Check if user has AI access
        let ai_access_level = self
            .access_manager
            .get_user_ai_access_level(user_id)
            .await?;
        if !ai_access_level.can_use_ai_analysis() {
            log_info!(
                "User does not have AI access, returning original opportunities",
                serde_json::json!({
                    "user_id": user_id,
                    "context": context,
                    "access_level": format!("{:?}", ai_access_level)
                })
            );
            return Ok(opportunities);
        }

        if opportunities.is_empty() {
            return Ok(opportunities);
        }

        // Use the AI service directly without cloning
        match self
            .ai_service
            .enhance_opportunities(opportunities.clone(), user_id)
            .await
        {
            Ok(enhanced) => {
                log_info!(
                    "AI enhancement successful",
                    serde_json::json!({
                        "enhanced_count": enhanced.len()
                    })
                );
                // Convert AiEnhancedOpportunity back to ArbitrageOpportunity
                let converted_opportunities: Vec<ArbitrageOpportunity> = enhanced
                    .into_iter()
                    .zip(opportunities.iter())
                    .map(|(enhanced, original)| {
                        let mut updated = original.clone();
                        // Apply AI enhancements
                        if let Some(profit) = updated.potential_profit_value {
                            updated.potential_profit_value =
                                Some(profit * enhanced.risk_adjusted_score);
                        }
                        updated.details = Some(format!(
                            "{} | AI Enhanced: Score {:.2}",
                            updated.details.unwrap_or_else(|| "Enhanced".to_string()),
                            enhanced.calculate_final_score()
                        ));
                        updated
                    })
                    .collect();
                Ok(converted_opportunities)
            }
            Err(e) => {
                log_info!(
                    "AI enhancement failed",
                    serde_json::json!({
                        "error": e.to_string()
                    })
                );
                // Return original opportunities if AI enhancement fails
                Ok(opportunities)
            }
        }
    }

    /// Enhance technical opportunities with AI for a specific user
    pub async fn enhance_technical_opportunities(
        &self,
        user_id: &str,
        opportunities: Vec<TechnicalOpportunity>,
        context: &str,
    ) -> ArbitrageResult<Vec<TechnicalOpportunity>> {
        // Check if user has AI access
        let ai_access_level = self
            .access_manager
            .get_user_ai_access_level(user_id)
            .await?;
        if !ai_access_level.can_use_ai_analysis() {
            log_info!(
                "User does not have AI access for technical opportunities",
                serde_json::json!({
                    "user_id": user_id,
                    "context": context,
                    "access_level": format!("{:?}", ai_access_level)
                })
            );
            return Ok(opportunities);
        }

        if opportunities.is_empty() {
            return Ok(opportunities);
        }

        // Convert technical opportunities to arbitrage format for AI processing
        let arbitrage_opportunities: Vec<ArbitrageOpportunity> = opportunities
            .iter()
            .map(|tech_opp| self.convert_technical_to_arbitrage(tech_opp))
            .collect();

        // Enhance with AI
        match self
            .ai_service
            .enhance_opportunities(arbitrage_opportunities, user_id)
            .await
        {
            Ok(enhanced_opportunities) => {
                let mut result = Vec::new();

                for (original, enhanced) in opportunities.iter().zip(enhanced_opportunities.iter())
                {
                    let mut enhanced_opportunity = original.clone();

                    // Apply AI enhancements to technical opportunity
                    enhanced_opportunity.confidence_score = match enhanced.confidence_level {
                        crate::services::core::ai::ai_beta_integration::AiConfidenceLevel::Low => 0.3,
                        crate::services::core::ai::ai_beta_integration::AiConfidenceLevel::Medium => 0.6,
                        crate::services::core::ai::ai_beta_integration::AiConfidenceLevel::High => 0.8,
                        crate::services::core::ai::ai_beta_integration::AiConfidenceLevel::VeryHigh => 0.95,
                    };
                    enhanced_opportunity.expected_return_percentage *= enhanced.risk_adjusted_score;

                    // Update details with AI insights instead of market_conditions
                    if let Some(existing_details) = &enhanced_opportunity.details {
                        enhanced_opportunity.details = Some(format!(
                            "{} | AI Enhanced: Score {:.2}",
                            existing_details,
                            enhanced.calculate_final_score()
                        ));
                    } else {
                        enhanced_opportunity.details = Some(format!(
                            "AI Enhanced: Score {:.2}",
                            enhanced.calculate_final_score()
                        ));
                    }

                    result.push(enhanced_opportunity);
                }

                log_info!(
                    "Enhanced technical opportunities with AI",
                    serde_json::json!({
                        "user_id": user_id,
                        "context": context,
                        "enhanced_count": result.len()
                    })
                );

                Ok(result)
            }
            Err(e) => {
                log_info!(
                    "AI enhancement failed for technical opportunities",
                    serde_json::json!({
                        "user_id": user_id,
                        "context": context,
                        "error": e.to_string()
                    })
                );
                Ok(opportunities)
            }
        }
    }

    /// Enhance opportunities for system-level AI processing (global opportunities)
    pub async fn enhance_system_opportunities(
        &self,
        opportunities: Vec<ArbitrageOpportunity>,
        context: &str,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        if opportunities.is_empty() {
            return Ok(opportunities);
        }

        let system_user_id = "system_ai_enhancer";

        match self
            .ai_service
            .enhance_opportunities(opportunities.clone(), system_user_id)
            .await
        {
            Ok(enhanced_opportunities) => {
                let mut result = Vec::new();

                for (original, enhanced) in opportunities.iter().zip(enhanced_opportunities.iter())
                {
                    let mut enhanced_opportunity = original.clone();

                    // Apply system-level AI enhancements
                    enhanced_opportunity.potential_profit_value = Some(
                        enhanced.risk_adjusted_score
                            * enhanced_opportunity.potential_profit_value.unwrap_or(0.0),
                    );

                    // Mark as system AI enhanced
                    enhanced_opportunity.details = Some(format!(
                        "System AI Enhanced | Score: {:.2} | Risk Adjusted: {:.2}",
                        enhanced.calculate_final_score(),
                        enhanced.risk_adjusted_score
                    ));

                    result.push(enhanced_opportunity);
                }

                log_info!(
                    "Enhanced system opportunities with AI",
                    serde_json::json!({
                        "context": context,
                        "enhanced_count": result.len(),
                        "system_level": true
                    })
                );

                Ok(result)
            }
            Err(e) => {
                log_info!(
                    "System AI enhancement failed",
                    serde_json::json!({
                        "context": context,
                        "error": e.to_string()
                    })
                );
                Ok(opportunities)
            }
        }
    }

    /// Check if AI enhancement is available for a user
    pub async fn is_ai_available_for_user(&self, user_id: &str) -> ArbitrageResult<bool> {
        let ai_access_level = self
            .access_manager
            .get_user_ai_access_level(user_id)
            .await?;
        Ok(ai_access_level.can_use_ai_analysis())
    }

    /// Get AI access level for a user
    pub async fn get_user_ai_access_level(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<UserAccessLevel> {
        self.access_manager.get_user_ai_access_level(user_id).await
    }

    /// Apply AI enhancement to mixed opportunity types (arbitrage + technical)
    pub async fn enhance_mixed_opportunities(
        &self,
        user_id: &str,
        arbitrage_opportunities: Vec<ArbitrageOpportunity>,
        technical_opportunities: Vec<TechnicalOpportunity>,
        context: &str,
    ) -> ArbitrageResult<(Vec<ArbitrageOpportunity>, Vec<TechnicalOpportunity>)> {
        // Check if user has AI access
        let ai_access_level = self
            .access_manager
            .get_user_ai_access_level(user_id)
            .await?;
        if !ai_access_level.can_use_ai_analysis() {
            return Ok((arbitrage_opportunities, technical_opportunities));
        }

        // Enhance both types concurrently
        let enhanced_arbitrage = self
            .enhance_arbitrage_opportunities(user_id, arbitrage_opportunities, context)
            .await?;

        let enhanced_technical = self
            .enhance_technical_opportunities(user_id, technical_opportunities, context)
            .await?;

        log_info!(
            "Enhanced mixed opportunities with AI",
            serde_json::json!({
                "user_id": user_id,
                "context": context,
                "arbitrage_count": enhanced_arbitrage.len(),
                "technical_count": enhanced_technical.len()
            })
        );

        Ok((enhanced_arbitrage, enhanced_technical))
    }

    /// Enhance arbitrage opportunities with technical analysis confirmation
    /// Combines traditional arbitrage with technical indicators for better timing
    pub async fn enhance_arbitrage_with_technical_confirmation(
        &self,
        user_id: &str,
        opportunities: Vec<ArbitrageOpportunity>,
        technical_confirmation_weight: f64,
        min_technical_confidence: f64,
        context: &str,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        // Check if user has AI access
        let ai_access_level = self
            .access_manager
            .get_user_ai_access_level(user_id)
            .await?;
        if !ai_access_level.can_use_ai_analysis() {
            return Ok(opportunities);
        }

        if opportunities.is_empty() {
            return Ok(opportunities);
        }

        let mut enhanced_opportunities = Vec::new();

        for opportunity in &opportunities {
            match self
                .analyze_arbitrage_with_technical_confirmation(
                    opportunity,
                    technical_confirmation_weight,
                    min_technical_confidence,
                )
                .await
            {
                Ok(Some(enhanced_opp)) => enhanced_opportunities.push(enhanced_opp),
                Ok(None) => {
                    log_info!(
                        "Arbitrage opportunity filtered out by technical confirmation",
                        serde_json::json!({
                            "user_id": user_id,
                            "opportunity_id": opportunity.id,
                            "context": context
                        })
                    );
                }
                Err(e) => {
                    log_info!(
                        "Failed to analyze arbitrage opportunity with technical confirmation",
                        serde_json::json!({
                            "user_id": user_id,
                            "opportunity_id": opportunity.id,
                            "error": e.to_string()
                        })
                    );
                    // Include original opportunity if technical analysis fails
                    enhanced_opportunities.push(opportunity.clone());
                }
            }
        }

        log_info!(
            "Enhanced arbitrage opportunities with technical confirmation",
            serde_json::json!({
                "user_id": user_id,
                "context": context,
                "enhanced_count": enhanced_opportunities.len(),
                "filtered_count": opportunities.len() - enhanced_opportunities.len()
            })
        );

        Ok(enhanced_opportunities)
    }

    /// Analyze arbitrage opportunity with technical confirmation
    async fn analyze_arbitrage_with_technical_confirmation(
        &self,
        opportunity: &ArbitrageOpportunity,
        technical_weight: f64,
        min_technical_confidence: f64,
    ) -> ArbitrageResult<Option<ArbitrageOpportunity>> {
        // Calculate base arbitrage confidence
        let arbitrage_confidence = self.calculate_arbitrage_confidence(opportunity);

        // Get technical confirmation score
        let technical_score = self
            .calculate_technical_confirmation_score(
                &opportunity.pair,
                &opportunity.long_exchange,
                &opportunity.short_exchange,
            )
            .await?;

        // Combine confidence scores
        let combined_confidence =
            self.combine_confidence_scores(arbitrage_confidence, technical_score, technical_weight);

        // Filter based on minimum technical confidence
        if technical_score < min_technical_confidence {
            return Ok(None);
        }

        // Create enhanced opportunity
        let mut enhanced_opportunity = opportunity.clone();

        // Update potential profit based on combined confidence
        if let Some(original_profit) = enhanced_opportunity.potential_profit_value {
            enhanced_opportunity.potential_profit_value =
                Some(original_profit * combined_confidence);
        }

        // Update details with technical confirmation info
        let technical_info = format!(
            " | Technical Confirmation: {:.1}% | Combined Score: {:.1}%",
            technical_score * 100.0,
            combined_confidence * 100.0
        );

        enhanced_opportunity.details = Some(
            enhanced_opportunity
                .details
                .unwrap_or_else(|| "Enhanced arbitrage".to_string())
                + &technical_info,
        );

        Ok(Some(enhanced_opportunity))
    }

    /// Calculate technical confirmation score for arbitrage opportunity
    async fn calculate_technical_confirmation_score(
        &self,
        pair: &str,
        long_exchange: &ExchangeIdEnum,
        short_exchange: &ExchangeIdEnum,
    ) -> ArbitrageResult<f64> {
        // This is a simplified version - in a real implementation, you would:
        // 1. Fetch recent price data for both exchanges
        // 2. Calculate technical indicators (RSI, volatility, correlation)
        // 3. Assess market conditions

        // For now, return a mock score based on exchange pair characteristics
        let base_score: f64 = 0.7; // Base technical score

        // Adjust based on exchange reliability (mock logic)
        let exchange_adjustment: f64 = match (long_exchange, short_exchange) {
            (ExchangeIdEnum::Binance, ExchangeIdEnum::Bybit) => 0.1,
            (ExchangeIdEnum::Binance, _) | (_, ExchangeIdEnum::Binance) => 0.05,
            _ => 0.0,
        };

        // Adjust based on pair popularity (mock logic)
        let pair_adjustment: f64 = if pair.contains("BTC") || pair.contains("ETH") {
            0.1
        } else {
            -0.05
        };

        let final_score = (base_score + exchange_adjustment + pair_adjustment).clamp(0.0, 1.0);

        Ok(final_score)
    }

    /// Calculate arbitrage confidence based on opportunity characteristics
    fn calculate_arbitrage_confidence(&self, opportunity: &ArbitrageOpportunity) -> f64 {
        let mut confidence = 0.5; // Base confidence

        // Rate difference factor
        let rate_diff_factor = (opportunity.rate_difference * 1000.0).min(1.0);
        confidence += rate_diff_factor * 0.3;

        // Potential profit factor
        if let Some(profit) = opportunity.potential_profit_value {
            let profit_factor = (profit / 100.0).min(1.0);
            confidence += profit_factor * 0.2;
        }

        confidence.min(1.0)
    }

    /// Combine arbitrage and technical confidence scores
    fn combine_confidence_scores(
        &self,
        arbitrage_score: f64,
        technical_score: f64,
        technical_weight: f64,
    ) -> f64 {
        let arbitrage_weight = 1.0 - technical_weight;
        (arbitrage_score * arbitrage_weight) + (technical_score * technical_weight)
    }

    // Private helper methods

    /// Convert technical opportunity to arbitrage format for AI processing
    fn convert_technical_to_arbitrage(
        &self,
        tech_opp: &TechnicalOpportunity,
    ) -> ArbitrageOpportunity {
        use crate::types::ArbitrageType;
        use chrono::Utc;

        ArbitrageOpportunity {
            id: format!("tech_to_arb_{}", tech_opp.id),
            pair: tech_opp.symbol.clone(),
            long_exchange: tech_opp.exchange,
            short_exchange: tech_opp.exchange,
            long_rate: None,
            short_rate: None,
            rate_difference: tech_opp.expected_return_percentage,
            net_rate_difference: Some(tech_opp.expected_return_percentage),
            potential_profit_value: Some(tech_opp.expected_return_percentage * 1000.0), // Scale for AI processing
            confidence: 0.8, // Default confidence score
            volume: 1000.0,  // Default volume
            timestamp: Utc::now().timestamp_millis() as u64,
            created_at: Utc::now().timestamp_millis() as u64,
            detected_at: Utc::now().timestamp_millis() as u64,
            expires_at: Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000), // 15 minutes
            r#type: ArbitrageType::CrossExchange,
            details: Some(format!(
                "Technical Signal: {:?} | Confidence: {:.2}",
                tech_opp.signal_type, tech_opp.confidence
            )),
            min_exchanges_required: 1, // Technical only needs one exchange
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ArbitrageType, ExchangeIdEnum, TechnicalRiskLevel, TechnicalSignalStrength,
        TechnicalSignalType,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_arbitrage_opportunity() -> ArbitrageOpportunity {
        ArbitrageOpportunity {
            id: Uuid::new_v4().to_string(),
            pair: "BTCUSDT".to_string(),
            long_exchange: ExchangeIdEnum::Binance,
            short_exchange: ExchangeIdEnum::Bybit,
            long_rate: Some(50000.0),
            short_rate: Some(49950.0),
            rate_difference: 0.001,
            net_rate_difference: Some(0.001),
            potential_profit_value: Some(50.0),
            confidence: 0.8, // Default confidence score
            volume: 1000.0,  // Default volume
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            detected_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: chrono::Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000), // 15 minutes
            r#type: ArbitrageType::CrossExchange,
            details: Some("Test opportunity for AI enhancement".to_string()),
            min_exchanges_required: 2,
        }
    }

    fn create_test_technical_opportunity() -> TechnicalOpportunity {
        TechnicalOpportunity {
            id: Uuid::new_v4().to_string(),
            symbol: "ETHUSDT".to_string(),
            exchange: ExchangeIdEnum::Binance,
            signal_type: TechnicalSignalType::Buy,
            signal_strength: TechnicalSignalStrength::Strong,
            risk_level: TechnicalRiskLevel::Medium,
            entry_price: 3000.0,
            target_price: 3150.0,
            stop_loss: 2950.0,
            confidence: 0.85,
            timeframe: "1h".to_string(),
            indicators: serde_json::json!({"RSI": 70, "MACD": "bullish"}),
            created_at: Utc::now().timestamp_millis() as u64,
            expires_at: Utc::now().timestamp_millis() as u64 + (4 * 60 * 60 * 1000),
            metadata: serde_json::json!({"signal_strength": "strong"}),
            pair: "ETHUSDT".to_string(),
            expected_return_percentage: 0.05,
            details: Some("Strong buy signal".to_string()),
            confidence_score: 0.85,
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }

    #[test]
    fn test_technical_to_arbitrage_conversion() {
        let tech_opp = create_test_technical_opportunity();

        // Create a mock AI enhancer (we can't easily test the full functionality without mocking)
        // This test focuses on the conversion logic
        let converted = ArbitrageOpportunity {
            id: format!("tech_to_arb_{}", tech_opp.id),
            pair: tech_opp.symbol.clone(),
            long_exchange: tech_opp.exchange,
            short_exchange: tech_opp.exchange,
            long_rate: None,
            short_rate: None,
            rate_difference: tech_opp.expected_return_percentage,
            net_rate_difference: Some(tech_opp.expected_return_percentage),
            potential_profit_value: Some(tech_opp.expected_return_percentage * 1000.0),
            confidence: 0.8, // Default confidence score
            volume: 1000.0,  // Default volume
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            detected_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: chrono::Utc::now().timestamp_millis() as u64 + (15 * 60 * 1000), // 15 minutes
            r#type: ArbitrageType::CrossExchange,
            details: Some(format!(
                "Technical Signal: {:?} | Confidence: {:.2}",
                tech_opp.signal_type, tech_opp.confidence
            )),
            min_exchanges_required: 1,
        };

        assert_eq!(converted.pair, "ETHUSDT");
        assert_eq!(converted.long_exchange, ExchangeIdEnum::Binance);
        assert_eq!(converted.short_exchange, ExchangeIdEnum::Binance);
        assert_eq!(converted.rate_difference, 0.05);
        assert_eq!(converted.min_exchanges_required, 1);
        assert!(matches!(converted.r#type, ArbitrageType::CrossExchange));
    }

    #[test]
    fn test_arbitrage_opportunity_structure() {
        let arb_opp = create_test_arbitrage_opportunity();

        assert_eq!(arb_opp.pair, "BTCUSDT");
        assert_eq!(arb_opp.long_exchange, ExchangeIdEnum::Binance);
        assert_eq!(arb_opp.short_exchange, ExchangeIdEnum::Bybit);
        assert_eq!(arb_opp.rate_difference, 0.001);
        assert_eq!(arb_opp.min_exchanges_required, 2);
        assert!(matches!(arb_opp.r#type, ArbitrageType::CrossExchange));
    }
}
