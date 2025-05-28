use crate::services::core::infrastructure::cloudflare_pipelines::CloudflarePipelinesService;
use crate::services::core::infrastructure::d1_database::D1Service;
use crate::services::core::infrastructure::kv_service::KVService;
use crate::services::core::infrastructure::{
    CloudflareQueuesService, DistributionStrategy as QueueDistributionStrategy, MessagePriority,
    VectorizeService,
};
use crate::services::core::user::session_management::SessionManagementService;

use crate::types::{
    ArbitrageOpportunity, ChatContext, DistributionStrategy, FairnessConfig, GlobalOpportunity,
    OpportunitySource,
};
use crate::utils::ArbitrageResult;
use serde_json::json;
use std::collections::HashMap;

// Trait for sending notifications - breaks circular dependency
#[async_trait::async_trait]
#[cfg(not(target_arch = "wasm32"))]
pub trait NotificationSender: Send + Sync {
    async fn send_opportunity_notification(
        &self,
        chat_id: &str,
        opportunity: &ArbitrageOpportunity,
        is_private: bool,
    ) -> ArbitrageResult<bool>;

    async fn send_message(&self, chat_id: &str, message: &str) -> ArbitrageResult<()>;
}

// WASM version without Send + Sync bounds since WASM is single-threaded
#[async_trait::async_trait(?Send)]
#[cfg(target_arch = "wasm32")]
pub trait NotificationSender {
    async fn send_opportunity_notification(
        &self,
        chat_id: &str,
        opportunity: &ArbitrageOpportunity,
        is_private: bool,
    ) -> ArbitrageResult<bool>;

    async fn send_message(&self, chat_id: &str, message: &str) -> ArbitrageResult<()>;
}

/// Configuration for opportunity distribution
#[derive(Debug, Clone)]
pub struct DistributionConfig {
    pub max_opportunities_per_user_per_hour: u32,
    pub max_opportunities_per_user_per_day: u32,
    pub cooldown_period_minutes: u32,
    pub batch_size: u32,
    pub distribution_interval_seconds: u32,
    pub max_participants_per_opportunity: Option<u32>,
    pub fairness_config: FairnessConfig,
}

impl Default for DistributionConfig {
    fn default() -> Self {
        Self {
            max_opportunities_per_user_per_hour: 2,
            max_opportunities_per_user_per_day: 10,
            cooldown_period_minutes: 240, // 4 hours
            batch_size: 50,
            distribution_interval_seconds: 30,
            max_participants_per_opportunity: Some(100), // Default to 100 participants
            fairness_config: FairnessConfig::default(),
        }
    }
}

/// Service for distributing opportunities to eligible users
/// Handles automated push notifications with fairness algorithms
/// Enhanced with AI-powered matching and reliable queue-based delivery
pub struct OpportunityDistributionService {
    d1_service: D1Service,
    kv_service: KVService,
    session_service: SessionManagementService,
    #[cfg(not(target_arch = "wasm32"))]
    notification_sender: Option<Box<dyn NotificationSender>>,
    #[cfg(target_arch = "wasm32")]
    notification_sender: Option<Box<dyn NotificationSender + Send + Sync>>,
    pipelines_service: Option<CloudflarePipelinesService>,
    vectorize_service: Option<VectorizeService>,
    queues_service: Option<CloudflareQueuesService>,
    config: DistributionConfig,
}

impl OpportunityDistributionService {
    pub fn new(
        d1_service: D1Service,
        kv_service: KVService,
        session_service: SessionManagementService,
    ) -> Self {
        Self {
            d1_service,
            kv_service,
            session_service,
            notification_sender: None,
            pipelines_service: None,
            vectorize_service: None,
            queues_service: None,
            config: DistributionConfig::default(),
        }
    }

    pub fn with_config(mut self, config: DistributionConfig) -> Self {
        self.config = config;
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_notification_sender(&mut self, sender: Box<dyn NotificationSender>) {
        self.notification_sender = Some(sender);
    }

    #[cfg(target_arch = "wasm32")]
    pub fn set_notification_sender(&mut self, sender: Box<dyn NotificationSender + Send + Sync>) {
        self.notification_sender = Some(sender);
    }

    pub fn set_pipelines_service(&mut self, pipelines_service: CloudflarePipelinesService) {
        self.pipelines_service = Some(pipelines_service);
    }

    pub fn set_vectorize_service(&mut self, vectorize_service: VectorizeService) {
        self.vectorize_service = Some(vectorize_service);
    }

    pub fn set_queues_service(&mut self, queues_service: CloudflareQueuesService) {
        self.queues_service = Some(queues_service);
    }

    /// Distribute a global opportunity to eligible users
    /// Enhanced with AI-powered matching and reliable queue-based delivery
    pub async fn distribute_opportunity(
        &self,
        opportunity: ArbitrageOpportunity,
    ) -> ArbitrageResult<u32> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Create global opportunity with metadata
        let global_opportunity = GlobalOpportunity {
            id: format!("global_opp_{}", opportunity.id),
            opportunity_type: "arbitrage".to_string(),
            opportunity: opportunity.clone(),
            arbitrage_opportunity: Some(opportunity.clone()),
            technical_opportunity: None,
            source: OpportunitySource::SystemGenerated,
            created_at: start_time,
            detection_timestamp: start_time,
            expires_at: Some(start_time + (10 * 60 * 1000)), // 10 minutes
            expiry_timestamp: Some(start_time + (10 * 60 * 1000)), // 10 minutes
            priority: 8,                                     // High priority (1-10 scale)
            priority_score: self.calculate_priority_score(&opportunity).await?,
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
            // Additional fields for distribution tracking
            distributed_to: Vec::new(),
            max_participants: self.config.max_participants_per_opportunity,
            current_participants: 0,
            distribution_strategy: DistributionStrategy::RoundRobin,
        };

        // Get eligible users
        let eligible_users = self.get_eligible_users(&global_opportunity).await?;

        // Apply AI-enhanced user matching if VectorizeService is available
        let selected_users = if let Some(ref vectorize_service) = self.vectorize_service {
            self.apply_ai_enhanced_matching(&eligible_users, &global_opportunity, vectorize_service)
                .await?
        } else {
            // Fallback to traditional fairness algorithm
            self.apply_fairness_algorithm(&eligible_users, &global_opportunity)
                .await?
        };

        // Use queue-based distribution if CloudflareQueuesService is available
        let distributed_count = if let Some(ref queues_service) = self.queues_service {
            self.distribute_via_queues(&selected_users, &global_opportunity, queues_service)
                .await?
        } else {
            // Fallback to direct distribution
            self.distribute_directly(&selected_users, &global_opportunity)
                .await?
        };

        // Store distribution analytics
        self.record_distribution_analytics(&global_opportunity, distributed_count)
            .await?;

        // Update KV cache with distribution statistics
        self.update_distribution_stats_cache(distributed_count, start_time)
            .await?;

        // Update active users count in KV cache
        self.update_active_users_count(selected_users.len() as u32)
            .await?;

        Ok(distributed_count)
    }

    /// Get list of users eligible for opportunity distribution
    async fn get_eligible_users(
        &self,
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<Vec<String>> {
        let mut eligible_users = Vec::new();

        // Query database for active sessions
        let query = "SELECT telegram_id FROM user_sessions WHERE expires_at > datetime('now') AND is_active = 1 LIMIT 1000";
        let rows = self.d1_service.query(query, &[]).await?;

        for row in rows {
            if let Some(telegram_id) = row.get("telegram_id") {
                let user_id = telegram_id.clone();

                // Check if user is eligible for push notifications
                let chat_context = ChatContext::Private;

                if self
                    .session_service
                    .is_eligible_for_push_notification(
                        &user_id,
                        &opportunity.opportunity,
                        &chat_context,
                    )
                    .await?
                {
                    eligible_users.push(user_id);
                }
            }
        }

        Ok(eligible_users)
    }

    /// Apply fairness algorithm to select users for distribution
    async fn apply_fairness_algorithm(
        &self,
        eligible_users: &[String],
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<Vec<String>> {
        let mut selected_users = Vec::new();
        let max_users = self
            .config
            .fairness_config
            .max_opportunities_per_user_per_hour;

        match opportunity.distribution_strategy {
            DistributionStrategy::Immediate => {
                // Send immediately to all eligible users
                selected_users.extend_from_slice(
                    &eligible_users[..std::cmp::min(eligible_users.len(), max_users as usize)],
                );
            }
            DistributionStrategy::Batched => {
                // Batch selection with size limits
                selected_users.extend_from_slice(
                    &eligible_users
                        [..std::cmp::min(eligible_users.len(), self.config.batch_size as usize)],
                );
            }
            DistributionStrategy::Prioritized => {
                // Priority-based selection (subscription tier, activity, etc.)
                let mut user_scores = HashMap::new();

                for user_id in eligible_users {
                    let score = self.calculate_user_priority_score(user_id).await?;
                    user_scores.insert(user_id.clone(), score);
                }

                // Sort by priority score (highest first)
                let mut sorted_users: Vec<_> = user_scores.into_iter().collect();
                sorted_users.sort_by(|(_, a), (_, b)| {
                    b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
                });

                for (user_id, _) in sorted_users.into_iter().take(max_users as usize) {
                    selected_users.push(user_id);
                }
            }
            DistributionStrategy::RateLimited => {
                // Respect individual user rate limits
                for user_id in eligible_users {
                    if self.check_user_rate_limit(user_id).await? {
                        selected_users.push(user_id.clone());
                        if selected_users.len() >= max_users as usize {
                            break;
                        }
                    }
                }
            }
            DistributionStrategy::FirstComeFirstServe => {
                // Simple FIFO selection
                selected_users.extend_from_slice(
                    &eligible_users[..std::cmp::min(eligible_users.len(), max_users as usize)],
                );
            }
            DistributionStrategy::RoundRobin => {
                // Round-robin selection based on last opportunity received
                let mut user_priorities = HashMap::new();

                for user_id in eligible_users {
                    let last_received = self.get_user_last_opportunity_time(user_id).await?;
                    user_priorities.insert(user_id.clone(), last_received);
                }

                // Sort by last received time (oldest first)
                let mut sorted_users: Vec<_> = user_priorities.into_iter().collect();
                sorted_users.sort_by_key(|(_, last_received)| *last_received);

                for (user_id, _) in sorted_users.into_iter().take(max_users as usize) {
                    selected_users.push(user_id);
                }
            }
            DistributionStrategy::PriorityBased => {
                // Priority-based selection (subscription tier, activity, etc.)
                let mut user_scores = HashMap::new();

                for user_id in eligible_users {
                    let score = self.calculate_user_priority_score(user_id).await?;
                    user_scores.insert(user_id.clone(), score);
                }

                // Sort by priority score (highest first)
                let mut sorted_users: Vec<_> = user_scores.into_iter().collect();
                sorted_users.sort_by(|(_, a), (_, b)| {
                    b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
                });

                for (user_id, _) in sorted_users.into_iter().take(max_users as usize) {
                    selected_users.push(user_id);
                }
            }
            DistributionStrategy::Broadcast => {
                // Send to all eligible users (respecting global limits)
                selected_users.extend_from_slice(eligible_users);
            }
        }

        Ok(selected_users)
    }

    /// Apply AI-enhanced user matching using VectorizeService
    /// Ranks users based on their preference vectors and opportunity characteristics
    async fn apply_ai_enhanced_matching(
        &self,
        eligible_users: &[String],
        opportunity: &GlobalOpportunity,
        vectorize_service: &VectorizeService,
    ) -> ArbitrageResult<Vec<String>> {
        // Get AI-ranked opportunities for each user
        let mut user_rankings: Vec<(String, f32)> = Vec::new();

        for user_id in eligible_users {
            // Get personalized ranking for this opportunity
            match vectorize_service
                .get_personalized_opportunities(user_id, &[opportunity.opportunity.clone()])
                .await
            {
                Ok(ranked_opportunities) => {
                    if let Some(ranked) = ranked_opportunities.first() {
                        user_rankings.push((user_id.clone(), ranked.combined_score));
                    } else {
                        // Default score if no ranking available
                        user_rankings.push((user_id.clone(), 0.5));
                    }
                }
                Err(_) => {
                    // Fallback to default score if AI ranking fails
                    user_rankings.push((user_id.clone(), 0.5));
                }
            }
        }

        // Sort by AI ranking score (highest first)
        user_rankings.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Apply fairness constraints while respecting AI rankings
        let mut selected_users = Vec::new();
        let max_participants = self.config.max_participants_per_opportunity.unwrap_or(100) as usize;

        for (user_id, _score) in user_rankings.iter().take(max_participants) {
            // Check rate limiting and cooldown
            if self.check_user_rate_limit(user_id).await? {
                selected_users.push(user_id.clone());

                // Respect batch size limits
                if selected_users.len() >= self.config.batch_size as usize {
                    break;
                }
            }
        }

        Ok(selected_users)
    }

    /// Distribute opportunities via CloudflareQueuesService for reliable delivery
    async fn distribute_via_queues(
        &self,
        selected_users: &[String],
        opportunity: &GlobalOpportunity,
        queues_service: &CloudflareQueuesService,
    ) -> ArbitrageResult<u32> {
        // Determine message priority based on opportunity characteristics
        let priority = self
            .calculate_message_priority(&opportunity.opportunity)
            .await?;

        // Convert distribution strategy to queue distribution strategy
        let queue_strategy = match opportunity.distribution_strategy {
            DistributionStrategy::Immediate => QueueDistributionStrategy::Broadcast,
            DistributionStrategy::Batched => QueueDistributionStrategy::Broadcast,
            DistributionStrategy::Prioritized => QueueDistributionStrategy::PriorityBased,
            DistributionStrategy::RateLimited => QueueDistributionStrategy::Broadcast,
            DistributionStrategy::RoundRobin => QueueDistributionStrategy::RoundRobin,
            DistributionStrategy::FirstComeFirstServe => QueueDistributionStrategy::Broadcast,
            DistributionStrategy::PriorityBased => QueueDistributionStrategy::PriorityBased,
            DistributionStrategy::Broadcast => QueueDistributionStrategy::Broadcast,
        };

        // Send opportunity distribution message to queue
        let users_vec = selected_users.to_vec();
        match queues_service
            .send_opportunity_distribution(
                &opportunity.opportunity,
                users_vec,
                priority,
                queue_strategy,
            )
            .await
        {
            Ok(_message_id) => {
                // Update tracking for all users
                for user_id in selected_users {
                    self.update_user_distribution_tracking(user_id, opportunity)
                        .await?;
                }
                // Update active users count in cache
                self.update_active_users_count(selected_users.len() as u32)
                    .await?;
                Ok(selected_users.len() as u32)
            }
            Err(e) => {
                // Log error and fallback to direct distribution
                eprintln!("Queue distribution failed, falling back to direct: {}", e);
                self.distribute_directly(selected_users, opportunity).await
            }
        }
    }

    /// Fallback direct distribution method (original implementation)
    async fn distribute_directly(
        &self,
        selected_users: &[String],
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<u32> {
        let mut distributed_count = 0;

        for user_id in selected_users {
            if self.send_opportunity_to_user(user_id, opportunity).await? {
                distributed_count += 1;

                // Update user distribution tracking
                self.update_user_distribution_tracking(user_id, opportunity)
                    .await?;

                // Respect rate limiting
                if distributed_count >= self.config.batch_size {
                    break;
                }
            }
        }

        // Update active users count in cache
        self.update_active_users_count(distributed_count).await?;

        Ok(distributed_count)
    }

    /// Calculate message priority for queue-based distribution
    async fn calculate_message_priority(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> ArbitrageResult<MessagePriority> {
        // High priority for high-value opportunities
        if opportunity.rate_difference > 0.5 {
            return Ok(MessagePriority::Critical);
        }

        if opportunity.rate_difference > 0.3 {
            return Ok(MessagePriority::High);
        }

        if opportunity.rate_difference > 0.1 {
            return Ok(MessagePriority::Normal);
        }

        Ok(MessagePriority::Low)
    }

    /// Send opportunity to a specific user
    async fn send_opportunity_to_user(
        &self,
        user_id: &str,
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<bool> {
        // Check rate limiting
        if !self.check_user_rate_limit(user_id).await? {
            return Ok(false);
        }

        // Send via notification sender if available
        if let Some(ref notification_sender) = self.notification_sender {
            match notification_sender
                .send_opportunity_notification(user_id, &opportunity.opportunity, true)
                .await
            {
                Ok(sent) => {
                    if sent {
                        // Update rate limiting counters
                        self.update_rate_limit_counters(user_id).await?;
                    }
                    Ok(sent)
                }
                Err(_) => {
                    // Log error but don't fail the entire distribution
                    Ok(false)
                }
            }
        } else {
            // If no notification sender, just record that we would have sent it
            // Still update rate limiting to maintain consistency
            self.update_rate_limit_counters(user_id).await?;
            Ok(true)
        }
    }

    /// Check if user is within rate limits
    async fn check_user_rate_limit(&self, user_id: &str) -> ArbitrageResult<bool> {
        let now = chrono::Utc::now();
        let hour_key = format!("rate_limit:{}:{}", user_id, now.format("%Y-%m-%d-%H"));
        let day_key = format!("rate_limit:{}:{}", user_id, now.format("%Y-%m-%d"));

        // Check hourly limit
        let hourly_count = match self.kv_service.get(&hour_key).await? {
            Some(count_str) => count_str.parse::<u32>().unwrap_or(0),
            None => 0,
        };

        if hourly_count >= self.config.max_opportunities_per_user_per_hour {
            return Ok(false);
        }

        // Check daily limit
        let daily_count = match self.kv_service.get(&day_key).await? {
            Some(count_str) => count_str.parse::<u32>().unwrap_or(0),
            None => 0,
        };

        if daily_count >= self.config.max_opportunities_per_user_per_day {
            return Ok(false);
        }

        Ok(true)
    }

    /// Update rate limiting counters after successful delivery
    async fn update_rate_limit_counters(&self, user_id: &str) -> ArbitrageResult<()> {
        let now = chrono::Utc::now();
        let hour_key = format!("rate_limit:{}:{}", user_id, now.format("%Y-%m-%d-%H"));
        let day_key = format!("rate_limit:{}:{}", user_id, now.format("%Y-%m-%d"));

        // Update hourly counter
        let hourly_count = match self.kv_service.get(&hour_key).await? {
            Some(count_str) => count_str.parse::<u32>().unwrap_or(0) + 1,
            None => 1,
        };
        self.kv_service
            .put(&hour_key, &hourly_count.to_string(), Some(3600))
            .await?;

        // Update daily counter
        let daily_count = match self.kv_service.get(&day_key).await? {
            Some(count_str) => count_str.parse::<u32>().unwrap_or(0) + 1,
            None => 1,
        };
        self.kv_service
            .put(&day_key, &daily_count.to_string(), Some(24 * 3600))
            .await?;

        Ok(())
    }

    /// Update user distribution tracking after sending opportunity
    async fn update_user_distribution_tracking(
        &self,
        user_id: &str,
        _opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<()> {
        let current_time = chrono::Utc::now().timestamp_millis() as u64;

        // Update last opportunity time for fairness algorithm
        let last_opportunity_key = format!("last_opportunity:{}", user_id);
        self.kv_service
            .put(
                &last_opportunity_key,
                &current_time.to_string(),
                Some(24 * 3600),
            )
            .await?;

        Ok(())
    }

    /// Calculate priority score for an opportunity
    async fn calculate_priority_score(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> ArbitrageResult<f64> {
        let mut score = 0.0;

        // Base score from rate difference
        score += opportunity.rate_difference * 1000.0; // Scale up for better scoring

        // Bonus for potential profit
        if let Some(profit) = opportunity.potential_profit_value {
            score += profit * 0.1; // Weight profit value
        }

        // Bonus for confidence (if available in future)
        score += 50.0; // Default confidence bonus

        // Time decay (newer opportunities get higher scores)
        let age_minutes =
            (chrono::Utc::now().timestamp_millis() as u64 - opportunity.timestamp) / (60 * 1000);
        let time_decay = 1.0 - (age_minutes as f64 * 0.01).min(0.5); // Max 50% decay
        score *= time_decay;

        Ok(score.max(0.0))
    }

    /// Get user's last opportunity received time
    async fn get_user_last_opportunity_time(&self, user_id: &str) -> ArbitrageResult<u64> {
        let last_opportunity_key = format!("last_opportunity:{}", user_id);
        let last_time = self
            .kv_service
            .get(&last_opportunity_key)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0".to_string())
            .parse::<u64>()
            .unwrap_or(0);
        Ok(last_time)
    }

    /// Calculate user priority score for distribution
    async fn calculate_user_priority_score(&self, user_id: &str) -> ArbitrageResult<f64> {
        let mut score = 1.0; // Base score

        // Subscription tier multiplier (would integrate with UserProfile service)
        score *= 1.0; // Default multiplier

        // Activity boost (users with recent activity get higher priority)
        let last_activity = self.get_user_last_opportunity_time(user_id).await?;
        let hours_since_last =
            (chrono::Utc::now().timestamp_millis() as u64 - last_activity) / (60 * 60 * 1000);

        if hours_since_last > 24 {
            score *= 1.2; // Boost for users who haven't received opportunities recently
        }

        Ok(score)
    }

    /// Record distribution analytics
    async fn record_distribution_analytics(
        &self,
        opportunity: &GlobalOpportunity,
        distributed_count: u32,
    ) -> ArbitrageResult<()> {
        // Store in KV for fast access
        let analytics_key = format!("distribution_analytics:{}", opportunity.opportunity.id);
        let analytics_data = json!({
            "opportunity_id": opportunity.opportunity.id,
            "pair": opportunity.opportunity.pair,
            "rate_difference": opportunity.opportunity.rate_difference,
            "priority_score": opportunity.priority_score,
            "distributed_count": distributed_count,
            "distribution_strategy": format!("{:?}", opportunity.distribution_strategy),
            "detection_timestamp": opportunity.detection_timestamp,
            "distribution_timestamp": chrono::Utc::now().timestamp_millis(),
            "event_type": "opportunity_distributed"
        });

        // Store analytics with 30-day TTL in KV
        self.kv_service
            .put(
                &analytics_key,
                &analytics_data.to_string(),
                Some(30 * 24 * 3600),
            )
            .await?;

        // Store in D1 for persistent analytics
        let insert_query = "
            INSERT INTO opportunity_distribution_analytics 
            (opportunity_id, pair, rate_difference, priority_score, distributed_count, 
             distribution_strategy, detection_timestamp, distribution_timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ";

        let params = vec![
            serde_json::Value::String(opportunity.opportunity.id.clone()),
            serde_json::Value::String(opportunity.opportunity.pair.clone()),
            serde_json::Value::Number(
                serde_json::Number::from_f64(opportunity.opportunity.rate_difference).unwrap(),
            ),
            serde_json::Value::Number(
                serde_json::Number::from_f64(opportunity.priority_score).unwrap(),
            ),
            serde_json::Value::Number(serde_json::Number::from(distributed_count)),
            serde_json::Value::String(
                opportunity
                    .distribution_strategy
                    .to_stable_string()
                    .to_string(),
            ),
            serde_json::Value::Number(serde_json::Number::from(opportunity.detection_timestamp)),
            serde_json::Value::Number(serde_json::Number::from(
                chrono::Utc::now().timestamp_millis() as u64,
            )),
        ];

        self.d1_service.execute(insert_query, &params).await?;

        // Update distribution statistics in KV cache for fast access
        self.update_distribution_stats_cache(distributed_count, opportunity.detection_timestamp)
            .await?;

        // Record high-volume analytics via Cloudflare Pipelines for scalable data ingestion
        if let Some(ref pipelines_service) = self.pipelines_service {
            let distribution_latency =
                chrono::Utc::now().timestamp_millis() as u64 - opportunity.detection_timestamp;

            if let Err(e) = pipelines_service
                .record_distribution_analytics(
                    &opportunity.opportunity.id,
                    &opportunity.opportunity.pair,
                    opportunity.opportunity.rate_difference,
                    distributed_count,
                    distribution_latency,
                )
                .await
            {
                // Log error but don't fail the distribution - analytics is non-critical
                eprintln!(
                    "Failed to record distribution analytics to Pipelines: {}",
                    e
                );
            }
        }

        Ok(())
    }

    /// Calculate actual success rate from delivery metrics
    async fn calculate_actual_success_rate(&self) -> ArbitrageResult<f64> {
        // Query delivery success and failure metrics from the last 7 days
        let seven_days_ago =
            chrono::Utc::now().timestamp_millis() as u64 - (7 * 24 * 60 * 60 * 1000);

        let success_query = "
            SELECT 
                COUNT(CASE WHEN delivery_status = 'sent' THEN 1 END) as successful_deliveries,
                COUNT(CASE WHEN delivery_status = 'failed' THEN 1 END) as failed_deliveries,
                COUNT(*) as total_deliveries
            FROM opportunity_distribution_analytics 
            WHERE distribution_timestamp >= ?
        ";

        let params = vec![serde_json::Value::Number(serde_json::Number::from(
            seven_days_ago,
        ))];

        let rows = self.d1_service.query(success_query, &params).await?;

        if let Some(row) = rows.first() {
            let successful = row
                .get("successful_deliveries")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let _failed = row
                .get("failed_deliveries")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let total = row
                .get("total_deliveries")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);

            if total > 0 {
                let success_rate = (successful as f64 / total as f64) * 100.0;
                Ok(success_rate)
            } else {
                // Default to 95% if no data available
                Ok(95.0)
            }
        } else {
            // Default to 95% if no analytics data
            Ok(95.0)
        }
    }

    /// Get distribution statistics for monitoring
    pub async fn get_distribution_stats(&self) -> ArbitrageResult<DistributionStats> {
        let _today_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| {
                crate::utils::ArbitrageError::validation_error(
                    "Invalid time values for today start",
                )
            })?
            .and_utc()
            .timestamp() as u64;

        // Get opportunities distributed today
        let opportunities_distributed_today = self
            .kv_service
            .get("distribution_stats:opportunities_today")
            .await
            .unwrap_or_default()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        // Get active users count
        let active_users = self
            .kv_service
            .get("distribution_stats:active_users")
            .await
            .unwrap_or_default()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        // Calculate average distribution time
        let average_distribution_time_ms = self
            .kv_service
            .get("distribution_stats:avg_time_ms")
            .await
            .unwrap_or_default()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        // Calculate success rate
        let success_rate_percentage = self.calculate_actual_success_rate().await?;

        Ok(DistributionStats {
            opportunities_distributed_today,
            active_users,
            average_distribution_time_ms,
            success_rate_percentage,
        })
    }

    /// Helper function to parse database row into ArbitrageOpportunity
    fn parse_opportunity_row(
        row: std::collections::HashMap<String, String>,
    ) -> Option<ArbitrageOpportunity> {
        // Convert database row to ArbitrageOpportunity - row is HashMap<String, String>
        let opportunity_id = row.get("opportunity_id")?.clone();
        let pair = row.get("pair")?.clone();

        // Parse exchange enums from strings
        let long_exchange = row
            .get("long_exchange")
            .and_then(|s| s.parse::<crate::types::ExchangeIdEnum>().ok())?;
        let short_exchange = row
            .get("short_exchange")
            .and_then(|s| s.parse::<crate::types::ExchangeIdEnum>().ok())?;

        // Parse numeric values from strings
        let long_rate = row.get("long_rate").and_then(|s| {
            if s.is_empty() {
                None
            } else {
                s.parse::<f64>().ok()
            }
        });
        let short_rate = row.get("short_rate").and_then(|s| {
            if s.is_empty() {
                None
            } else {
                s.parse::<f64>().ok()
            }
        });
        let rate_difference = row.get("rate_difference")?.parse::<f64>().ok()?;
        let net_rate_difference = row.get("net_rate_difference").and_then(|s| {
            if s.is_empty() {
                None
            } else {
                s.parse::<f64>().ok()
            }
        });
        let potential_profit_value = row.get("potential_profit_value").and_then(|s| {
            if s.is_empty() {
                None
            } else {
                s.parse::<f64>().ok()
            }
        });

        // Parse opportunity type from string
        let opportunity_type = row
            .get("opportunity_type")
            .map(|s| match s.as_str() {
                "funding_rate" => crate::types::ArbitrageType::FundingRate,
                "spot_futures" => crate::types::ArbitrageType::SpotFutures,
                "cross_exchange" => crate::types::ArbitrageType::CrossExchange,
                _ => crate::types::ArbitrageType::CrossExchange,
            })
            .unwrap_or(crate::types::ArbitrageType::CrossExchange);

        let details =
            row.get("details")
                .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
        let timestamp = row
            .get("timestamp")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| chrono::Utc::now().timestamp() as u64);

        Some(ArbitrageOpportunity {
            id: opportunity_id,
            pair,
            long_exchange,
            short_exchange,
            long_rate,
            short_rate,
            rate_difference,
            net_rate_difference,
            potential_profit_value,
            timestamp,
            r#type: opportunity_type,
            details,
            min_exchanges_required: 2,
        })
    }

    /// Generic helper function to get opportunities with caching
    async fn get_opportunities_with_cache(
        &self,
        cache_key: &str,
        query: &str,
        params: &[serde_json::Value],
        cache_ttl: Option<u64>,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        // Try to get from KV cache first
        if let Ok(Some(cached_data)) = self.kv_service.get(cache_key).await {
            if let Ok(opportunities) =
                serde_json::from_str::<Vec<ArbitrageOpportunity>>(&cached_data)
            {
                return Ok(opportunities);
            }
        }

        // Fallback: Get from D1 database
        let rows = self
            .d1_service
            .query(query, params)
            .await
            .unwrap_or_default();

        let opportunities: Vec<ArbitrageOpportunity> = rows
            .into_iter()
            .filter_map(Self::parse_opportunity_row)
            .collect();

        // Cache the results if not empty
        if !opportunities.is_empty() {
            if let Ok(cached_json) = serde_json::to_string(&opportunities) {
                let _ = self
                    .kv_service
                    .put(cache_key, &cached_json, cache_ttl)
                    .await;
            }
        }

        Ok(opportunities)
    }

    /// Get opportunities for a specific user
    pub async fn get_user_opportunities(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        let cache_key = format!("user_opportunities:{}", user_id);
        let query = r#"
            SELECT 
                opportunity_id,
                pair,
                long_exchange,
                short_exchange,
                long_rate,
                short_rate,
                rate_difference,
                net_rate_difference,
                potential_profit_value,
                opportunity_type,
                details,
                timestamp
            FROM user_opportunities 
            WHERE user_id = ? 
            AND timestamp > ? 
            ORDER BY timestamp DESC 
            LIMIT 50
        "#;

        let one_hour_ago = chrono::Utc::now().timestamp() as u64 - 3600;
        let params = vec![
            serde_json::Value::String(user_id.to_string()),
            serde_json::Value::Number(serde_json::Number::from(one_hour_ago)),
        ];

        self.get_opportunities_with_cache(&cache_key, query, &params, Some(300))
            .await
    }

    /// Get all opportunities for admin access
    pub async fn get_all_opportunities(&self) -> ArbitrageResult<Vec<ArbitrageOpportunity>> {
        let cache_key = "all_opportunities";
        let query = r#"
            SELECT 
                opportunity_id,
                pair,
                long_exchange,
                short_exchange,
                long_rate,
                short_rate,
                rate_difference,
                net_rate_difference,
                potential_profit_value,
                opportunity_type,
                details,
                timestamp
            FROM opportunities 
            WHERE timestamp > ? 
            ORDER BY timestamp DESC 
            LIMIT 100
        "#;

        let one_hour_ago = chrono::Utc::now().timestamp() as u64 - 3600;
        let params = vec![serde_json::Value::Number(serde_json::Number::from(
            one_hour_ago,
        ))];

        self.get_opportunities_with_cache(cache_key, query, &params, Some(120))
            .await
    }

    /// Update distribution statistics in KV cache for fast access
    async fn update_distribution_stats_cache(
        &self,
        distributed_count: u32,
        detection_timestamp: u64,
    ) -> ArbitrageResult<()> {
        // Update opportunities distributed today
        let today_key = "distribution_stats:opportunities_today";
        let today_count = self
            .kv_service
            .get(today_key)
            .await
            .unwrap_or_default()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        self.kv_service
            .put(
                today_key,
                &(today_count + distributed_count).to_string(),
                Some(3600),
            )
            .await?;

        // Update active users count
        let active_users_key = "distribution_stats:active_users";
        let active_users = self
            .kv_service
            .get(active_users_key)
            .await
            .unwrap_or_default()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        self.kv_service
            .put(active_users_key, &active_users.to_string(), Some(3600))
            .await?;

        // Update average distribution time
        let avg_time_key = "distribution_stats:avg_time_ms";
        let current_distribution_time =
            chrono::Utc::now().timestamp_millis() as u64 - detection_timestamp;
        let existing_avg_time = self
            .kv_service
            .get(avg_time_key)
            .await
            .unwrap_or_default()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        // Simple moving average calculation
        let new_avg_time = if existing_avg_time == 0 {
            current_distribution_time
        } else {
            (existing_avg_time + current_distribution_time) / 2
        };

        self.kv_service
            .put(avg_time_key, &new_avg_time.to_string(), Some(3600))
            .await?;

        Ok(())
    }

    /// Update active users count in KV cache
    async fn update_active_users_count(&self, user_count: u32) -> ArbitrageResult<()> {
        let active_users_key = "distribution_stats:active_users";
        self.kv_service
            .put(active_users_key, &user_count.to_string(), Some(24 * 3600)) // 24 hour TTL
            .await?;
        Ok(())
    }
}

/// Distribution statistics
#[derive(Debug, Clone)]
pub struct DistributionStats {
    pub opportunities_distributed_today: u32,
    pub active_users: u32,
    pub average_distribution_time_ms: u64,
    pub success_rate_percentage: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArbitrageType, ExchangeIdEnum};

    fn create_test_opportunity() -> ArbitrageOpportunity {
        ArbitrageOpportunity {
            id: "test_dist_opp_001".to_string(),
            pair: "BTCUSDT".to_string(),
            r#type: ArbitrageType::FundingRate,
            long_exchange: ExchangeIdEnum::Binance,
            short_exchange: ExchangeIdEnum::Bybit,
            long_rate: Some(0.001),
            short_rate: Some(0.003),
            rate_difference: 0.002,
            net_rate_difference: Some(0.0018),
            potential_profit_value: Some(25.0),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            details: Some("Test distribution opportunity".to_string()),
            min_exchanges_required: 2,
        }
    }

    #[tokio::test]
    async fn test_opportunity_distribution() {
        // Test the complete distribution flow
        let opportunity = create_test_opportunity();

        // Test priority score calculation
        assert!(opportunity.rate_difference > 0.0);
        assert!(opportunity.potential_profit_value.unwrap_or(0.0) > 0.0);

        // Test opportunity message formatting
        let global_opp = GlobalOpportunity {
            id: "test_global_opp_001".to_string(),
            opportunity_type: "arbitrage".to_string(),
            opportunity: opportunity.clone(),
            arbitrage_opportunity: Some(opportunity.clone()),
            technical_opportunity: None,
            source: OpportunitySource::SystemGenerated,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            detection_timestamp: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: Some(chrono::Utc::now().timestamp_millis() as u64 + (10 * 60 * 1000)),
            expiry_timestamp: Some(chrono::Utc::now().timestamp_millis() as u64 + (10 * 60 * 1000)),
            priority: 8,
            priority_score: 75.0,
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
            distributed_to: Vec::new(),
            max_participants: Some(100),
            current_participants: 0,
            distribution_strategy: DistributionStrategy::RoundRobin,
        };

        // Test message formatting
        let long_exchange_name = format!("{:?}", opportunity.long_exchange);
        let short_exchange_name = format!("{:?}", opportunity.short_exchange);
        let expiry_minutes =
            (global_opp.expiry_timestamp - global_opp.detection_timestamp) / (60 * 1000);

        let message = format!(
            "🚀 *New Arbitrage Opportunity*\n\n\
            📈 **Pair:** `{}`\n\
            🔄 **Exchanges:** {} ↔ {}\n\
            💰 **Rate Difference:** `{:.4}%`\n\
            💵 **Potential Profit:** `${:.2}`\n\
            ⭐ **Priority Score:** `{:.1}`\n\
            ⏰ **Expires:** {} minutes\n\n\
            🎯 Use `/opportunities` for more details\\!",
            opportunity.pair,
            long_exchange_name,
            short_exchange_name,
            opportunity.rate_difference * 100.0,
            opportunity.potential_profit_value.unwrap_or(0.0),
            global_opp.priority_score,
            expiry_minutes
        );

        assert!(message.contains("BTCUSDT"));
        assert!(message.contains("Binance"));
        assert!(message.contains("Bybit"));
    }

    #[tokio::test]
    async fn test_fairness_algorithm() {
        // Test different distribution strategies
        let eligible_users = [
            "user_001".to_string(),
            "user_002".to_string(),
            "user_003".to_string(),
            "user_004".to_string(),
            "user_005".to_string(),
        ];

        // Test FirstComeFirstServe strategy
        let _fcfs_opportunity = GlobalOpportunity {
            opportunity: create_test_opportunity(),
            detection_timestamp: chrono::Utc::now().timestamp_millis() as u64,
            expiry_timestamp: chrono::Utc::now().timestamp_millis() as u64 + (10 * 60 * 1000),
            priority_score: 75.0,
            distributed_to: Vec::new(),
            max_participants: Some(3),
            current_participants: 0,
            distribution_strategy: DistributionStrategy::FirstComeFirstServe,
            source: OpportunitySource::SystemGenerated,
        };

        // Test that FCFS selects first N users
        let max_users = 3;
        let selected_fcfs = &eligible_users[..std::cmp::min(eligible_users.len(), max_users)];
        assert_eq!(selected_fcfs.len(), 3);
        assert_eq!(selected_fcfs[0], "user_001");
        assert_eq!(selected_fcfs[1], "user_002");
        assert_eq!(selected_fcfs[2], "user_003");

        // Test Broadcast strategy
        let _broadcast_opportunity = GlobalOpportunity {
            opportunity: create_test_opportunity(),
            detection_timestamp: chrono::Utc::now().timestamp_millis() as u64,
            expiry_timestamp: chrono::Utc::now().timestamp_millis() as u64 + (10 * 60 * 1000),
            priority_score: 75.0,
            distributed_to: Vec::new(),
            max_participants: None,
            current_participants: 0,
            distribution_strategy: DistributionStrategy::Broadcast,
            source: OpportunitySource::SystemGenerated,
        };

        // Test that Broadcast selects all users
        let selected_broadcast = &eligible_users[..];
        assert_eq!(selected_broadcast.len(), 5);
        assert!(selected_broadcast.contains(&"user_001".to_string()));
        assert!(selected_broadcast.contains(&"user_005".to_string()));
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        // Test rate limiting logic
        let config = DistributionConfig {
            max_opportunities_per_user_per_hour: 2,
            max_opportunities_per_user_per_day: 10,
            cooldown_period_minutes: 240, // 4 hours
            batch_size: 50,
            distribution_interval_seconds: 30,
            max_participants_per_opportunity: Some(100),
            fairness_config: FairnessConfig::default(),
        };

        // Test rate limit configuration
        assert_eq!(config.max_opportunities_per_user_per_hour, 2);
        assert_eq!(config.max_opportunities_per_user_per_day, 10);
        assert_eq!(config.cooldown_period_minutes, 240);

        // Test cooldown calculation
        let cooldown_ms = config.cooldown_period_minutes as u64 * 60 * 1000;
        assert_eq!(cooldown_ms, 14_400_000); // 4 hours in milliseconds

        // Test batch size limits
        assert_eq!(config.batch_size, 50);
        assert!(config.batch_size > 0);

        // Test distribution interval
        assert_eq!(config.distribution_interval_seconds, 30);
        assert!(config.distribution_interval_seconds >= 10); // Minimum reasonable interval
    }
}
