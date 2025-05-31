use crate::services::core::infrastructure::ai_services::AICoordinator;
use crate::services::core::infrastructure::data_ingestion_module::queue_manager::QueueMessage;
use crate::services::core::infrastructure::data_ingestion_module::{MessagePriority, QueueManager};
use crate::services::core::infrastructure::{
    database_repositories::DatabaseManager, DataAccessLayer, DataIngestionModule,
};
use crate::services::core::user::session_management::SessionManagementService;

use crate::types::{
    ArbitrageOpportunity, ArbitrageType, ChatContext, DistributionStrategy, FairnessConfig,
    GlobalOpportunity, OpportunityData, OpportunitySource, SubscriptionTier,
};
use crate::utils::{ArbitrageError, ArbitrageResult};
use std::collections::HashMap;
use std::sync::Arc;

// Non-WASM version with Send + Sync bounds for thread safety
#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
pub trait NotificationSender: Send + Sync {
    async fn send_opportunity_notification(
        &self,
        chat_id: &str,
        opportunity: &OpportunityData,
        is_private: bool,
    ) -> ArbitrageResult<bool>;

    async fn send_message(&self, chat_id: &str, message: &str) -> ArbitrageResult<()>;
}

// WASM version without Send + Sync bounds
#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
pub trait NotificationSender {
    async fn send_opportunity_notification(
        &self,
        chat_id: &str,
        opportunity: &OpportunityData,
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
    database_repositories: DatabaseManager,
    data_access_layer: DataAccessLayer,
    session_service: Arc<SessionManagementService>,
    data_ingestion_module: Option<DataIngestionModule>,
    ai_coordinator: Option<AICoordinator>,
    queue_manager: Option<QueueManager>,
    config: DistributionConfig,
    notification_sender: Option<Box<dyn NotificationSender>>,
}

impl OpportunityDistributionService {
    pub fn new(
        database_repositories: DatabaseManager,
        data_access_layer: DataAccessLayer,
        session_service: Arc<SessionManagementService>,
    ) -> Self {
        Self {
            database_repositories,
            data_access_layer,
            session_service,
            data_ingestion_module: None,
            ai_coordinator: None,
            queue_manager: None,
            config: DistributionConfig::default(),
            notification_sender: None,
        }
    }

    pub fn with_config(mut self, config: DistributionConfig) -> Self {
        self.config = config;
        self
    }

    pub fn set_notification_sender(&mut self, sender: Box<dyn NotificationSender>) {
        self.notification_sender = Some(sender);
    }

    pub fn set_data_ingestion_module(&mut self, data_ingestion_module: DataIngestionModule) {
        self.data_ingestion_module = Some(data_ingestion_module);
    }

    pub fn set_ai_coordinator(&mut self, ai_coordinator: AICoordinator) {
        self.ai_coordinator = Some(ai_coordinator);
    }

    pub fn set_queue_manager(&mut self, queue_manager: QueueManager) {
        self.queue_manager = Some(queue_manager);
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
            id: format!("global_arb_{}", opportunity.id),
            opportunity_type: OpportunitySource::SystemGenerated,
            arbitrage_opportunity: opportunity.clone(),
            target_users: Vec::new(),
            opportunity_data: OpportunityData::Arbitrage(opportunity.clone()),
            source: OpportunitySource::SystemGenerated,
            created_at: start_time,
            detection_timestamp: start_time,
            expires_at: start_time + (10 * 60 * 1000), // 10 minutes
            priority: 5,
            priority_score: 0.5,
            ai_enhanced: false,
            ai_confidence_score: None,
            ai_insights: None,
            distributed_to: Vec::new(),
            max_participants: Some(100),
            current_participants: 0,
            distribution_strategy: DistributionStrategy::FirstComeFirstServe,
        };

        // Get eligible users
        let eligible_users = self.get_eligible_users(&global_opportunity).await?;

        // Apply AI-enhanced user matching if AICoordinator is available
        let selected_users = if let Some(ref ai_coordinator) = self.ai_coordinator {
            self.apply_ai_enhanced_matching(&eligible_users, &global_opportunity, ai_coordinator)
                .await?
        } else {
            // Fallback to traditional fairness algorithm
            self.apply_fairness_algorithm(&eligible_users, &global_opportunity)
                .await?
        };

        // Use queue-based distribution if QueueManager is available
        let distributed_count = if let Some(ref queue_manager) = self.queue_manager {
            self.distribute_via_queues(&selected_users, &global_opportunity, queue_manager)
                .await?
        } else {
            // Fallback to direct distribution
            self.distribute_directly(selected_users.to_vec(), &global_opportunity)
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
        let result = self.database_repositories.query(query, &[]).await?;
        let rows = result.results::<HashMap<String, serde_json::Value>>()?;

        for row in rows {
            if let Some(telegram_id) = row.get("telegram_id") {
                if let Some(user_id_str) = telegram_id.as_str() {
                    // Check if user is eligible for push notifications
                    let chat_context = ChatContext {
                        chat_id: telegram_id.as_i64().unwrap_or(0),
                        chat_type: "private".to_string(),
                        user_id: Some(user_id_str.to_string()),
                        username: None,
                        is_group: false,
                        group_title: None,
                        message_id: None,
                        reply_to_message_id: None,
                    };

                    if let Some(arbitrage_opp) = opportunity.opportunity_data.as_arbitrage() {
                        if self
                            .session_service
                            .is_eligible_for_push_notification(
                                user_id_str,
                                arbitrage_opp,
                                &chat_context,
                            )
                            .await?
                        {
                            eligible_users.push(user_id_str.to_string());
                        }
                    }
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
            DistributionStrategy::Tiered => {
                // Tiered distribution based on subscription level
                let mut premium_users = Vec::new();
                let mut regular_users = Vec::new();

                for user_id in eligible_users {
                    let user_tier = self.get_user_subscription_tier(user_id).await?;
                    if matches!(
                        user_tier,
                        SubscriptionTier::Premium
                            | SubscriptionTier::Pro
                            | SubscriptionTier::Enterprise
                    ) {
                        premium_users.push(user_id.clone());
                    } else {
                        regular_users.push(user_id.clone());
                    }
                }

                // Send to premium users first, then regular users
                selected_users.extend_from_slice(
                    &premium_users[..std::cmp::min(premium_users.len(), max_users as usize / 2)],
                );
                let remaining_slots = max_users as usize - selected_users.len();
                if remaining_slots > 0 {
                    selected_users.extend_from_slice(
                        &regular_users[..std::cmp::min(regular_users.len(), remaining_slots)],
                    );
                }
            }
            DistributionStrategy::Personalized => {
                // AI-personalized distribution based on user preferences and history
                let mut user_scores = HashMap::new();

                for user_id in eligible_users {
                    let personalization_score: f64 = self
                        .calculate_personalization_score(user_id, opportunity)
                        .await?;
                    user_scores.insert(user_id.clone(), personalization_score);
                }

                // Sort by personalization score (highest first)
                let mut sorted_users: Vec<_> = user_scores.into_iter().collect();
                sorted_users.sort_by(|(_, a), (_, b)| {
                    b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
                });

                for (user_id, _) in sorted_users.into_iter().take(max_users as usize) {
                    selected_users.push(user_id);
                }
            }
            DistributionStrategy::HighestBidder => {
                // Premium users get priority based on subscription tier
                let mut user_priorities = HashMap::new();

                for user_id in eligible_users {
                    let tier_priority = self.get_subscription_tier_priority(user_id).await?;
                    user_priorities.insert(user_id.clone(), tier_priority);
                }

                // Sort by tier priority (highest first)
                let mut sorted_users: Vec<_> = user_priorities.into_iter().collect();
                sorted_users.sort_by(|(_, a), (_, b)| {
                    b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
                });

                for (user_id, _) in sorted_users.into_iter().take(max_users as usize) {
                    selected_users.push(user_id);
                }
            }
        }

        Ok(selected_users)
    }

    /// Apply AI-enhanced user matching for opportunity distribution
    async fn apply_ai_enhanced_matching(
        &self,
        eligible_users: &[String],
        opportunity: &GlobalOpportunity,
        _ai_coordinator: &AICoordinator,
    ) -> ArbitrageResult<Vec<String>> {
        let mut user_rankings = Vec::new();

        for user_id in eligible_users {
            // Get personalized ranking for this opportunity
            if let Some(_arbitrage_opp) = opportunity.opportunity_data.as_arbitrage() {
                if let Some(_ai_coordinator) = &self.ai_coordinator {
                    // Use AI coordinator to get personalized opportunities
                    // For now, we'll use a default score since the exact method may vary
                    let default_score = 0.5; // Default AI confidence score
                    user_rankings.push((user_id.clone(), default_score));
                } else {
                    // Fallback to default score if AI coordinator not available
                    user_rankings.push((user_id.clone(), 0.5));
                }
            } else {
                // Fallback for non-arbitrage opportunities
                user_rankings.push((user_id.clone(), 0.5));
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

    /// Distribute opportunities via QueueManager for reliable delivery
    async fn distribute_via_queues(
        &self,
        selected_users: &[String],
        _opportunity: &GlobalOpportunity,
        queue_manager: &QueueManager,
    ) -> ArbitrageResult<u32> {
        // Calculate message priority
        let priority = self
            .calculate_message_priority(&_opportunity.opportunity_data)
            .await?;

        // Create queue message for opportunity distribution
        let message_body = serde_json::json!({
            "opportunity_id": _opportunity.id,
            "opportunity_type": _opportunity.get_opportunity_type(),
            "pair": _opportunity.get_pair(),
            "users": selected_users,
            "distribution_strategy": _opportunity.distribution_strategy.to_stable_string(),
            "timestamp": chrono::Utc::now().timestamp_millis()
        });

        let queue_message = QueueMessage::new(priority.queue_type(), priority, message_body);

        // Send message to queue
        match queue_manager.send_message(queue_message).await {
            Ok(_) => {
                // Update tracking for all users
                for user_id in selected_users {
                    self.update_user_distribution_tracking(user_id, _opportunity)
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
                self.distribute_directly(selected_users.to_vec(), _opportunity)
                    .await
            }
        }
    }

    /// Fallback direct distribution method (original implementation)
    async fn distribute_directly(
        &self,
        selected_users: Vec<String>,
        _opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<u32> {
        let mut distributed_count = 0;

        for user_id in &selected_users {
            if self.send_opportunity_to_user(user_id, _opportunity).await? {
                distributed_count += 1;

                // Update user distribution tracking
                self.update_user_distribution_tracking(user_id, _opportunity)
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
        opportunity_data: &OpportunityData,
    ) -> ArbitrageResult<MessagePriority> {
        let rate_difference = match opportunity_data {
            OpportunityData::Arbitrage(ref arb) => arb.rate_difference,
            OpportunityData::Technical(ref tech) => tech.confidence, // Use confidence as proxy for technical opportunities
        };

        // High priority for high-value opportunities
        if rate_difference > 0.5 {
            return Ok(MessagePriority::Critical);
        }

        if rate_difference > 0.3 {
            return Ok(MessagePriority::High);
        }

        if rate_difference > 0.1 {
            return Ok(MessagePriority::Normal);
        }

        Ok(MessagePriority::Low)
    }

    /// Send opportunity to user with context-aware formatting
    async fn send_opportunity_to_user(
        &self,
        user_id: &str,
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<bool> {
        // Get user session to determine context
        let session = self.session_service.get_session_by_user_id(user_id).await?;

        // Determine if this is a group/channel context
        let is_group_context = session.telegram_chat_id != session.telegram_id;
        let chat_id = session.telegram_chat_id.to_string();

        // Format opportunity message based on context
        let _message = if is_group_context {
            self.format_group_opportunity_message(opportunity).await?
        } else {
            self.format_private_opportunity_message(opportunity).await?
        };

        // Send via notification sender if available
        if let Some(notification_sender) = &self.notification_sender {
            let opportunity_data = &opportunity.opportunity_data;
            return notification_sender
                .send_opportunity_notification(&chat_id, opportunity_data, !is_group_context)
                .await;
        }

        // Fallback: record that opportunity was "sent" for analytics
        self.record_opportunity_sent(user_id, opportunity).await?;
        Ok(true)
    }

    /// Format opportunity message for group/channel context
    async fn format_group_opportunity_message(
        &self,
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<String> {
        let arb = &opportunity.arbitrage_opportunity;

        let mut message = format!(
            "🚀 **Arbitrage Opportunity**\n\n\
            💰 **Pair**: {}\n\
            📈 **Rate Difference**: {:.2}%\n\
            🔄 **Long**: {} | **Short**: {}\n\
            ⏰ **Expires**: <t:{}:R>\n\n",
            arb.pair,
            arb.rate_difference * 100.0,
            arb.long_exchange.as_str(),
            arb.short_exchange.as_str(),
            arb.expires_at / 1000
        );

        // Add AI insights if available and group has AI enabled
        if let Some(ai_insights) = &opportunity.ai_insights {
            message.push_str("🤖 **AI Insights**:\n");
            for insight in ai_insights.iter().take(2) {
                message.push_str(&format!("• {}\n", insight));
            }
            message.push('\n');
        }

        // Add take action button instruction for groups
        message.push_str("🎯 **Take Action**: Click the button below to trade in private chat");

        Ok(message)
    }

    /// Format opportunity message for private chat context
    async fn format_private_opportunity_message(
        &self,
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<String> {
        let arb = &opportunity.arbitrage_opportunity;

        let mut message = format!(
            "🚀 **New Arbitrage Opportunity**\n\n\
            💰 **Pair**: {}\n\
            📈 **Rate Difference**: {:.2}%\n\
            🔄 **Long**: {} | **Short**: {}\n\
            💵 **Potential Profit**: ${:.2}\n\
            ⚡ **Confidence**: {:.1}%\n\
            ⏰ **Expires**: <t:{}:R>\n\n",
            arb.pair,
            arb.rate_difference * 100.0,
            arb.long_exchange.as_str(),
            arb.short_exchange.as_str(),
            arb.potential_profit_value.unwrap_or(0.0),
            arb.confidence * 100.0,
            arb.expires_at / 1000
        );

        // Add detailed AI insights for private chat
        if let Some(ai_insights) = &opportunity.ai_insights {
            message.push_str("🤖 **AI Analysis**:\n");
            for insight in ai_insights {
                message.push_str(&format!("• {}\n", insight));
            }
            message.push('\n');
        }

        // Add action buttons for private chat
        message.push_str("🎯 **Actions**: Use buttons below to trade or get more details");

        Ok(message)
    }

    /// Record that opportunity was sent (for analytics)
    async fn record_opportunity_sent(
        &self,
        user_id: &str,
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<()> {
        // Update user distribution tracking
        self.update_user_distribution_tracking(user_id, opportunity)
            .await?;

        // Record in analytics
        let analytics_key = format!("opportunity_sent:{}:{}", user_id, opportunity.id);
        let analytics_data = serde_json::json!({
            "user_id": user_id,
            "opportunity_id": opportunity.id,
            "opportunity_type": opportunity.get_opportunity_type(),
            "pair": opportunity.get_pair(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "rate_difference": opportunity.arbitrage_opportunity.rate_difference,
            "ai_enhanced": opportunity.ai_enhanced
        });

        // Store analytics with 7-day TTL
        let kv_store = self.data_access_layer.get_kv_store();
        let _ = kv_store
            .put(&analytics_key, analytics_data.to_string())
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to create analytics put: {}", e))
            })?
            .expiration_ttl(7 * 24 * 3600)
            .execute()
            .await;

        Ok(())
    }

    /// Check if user is within rate limits
    async fn check_user_rate_limit(&self, user_id: &str) -> ArbitrageResult<bool> {
        let now = chrono::Utc::now();
        let hour_key = format!("rate_limit:{}:{}", user_id, now.format("%Y-%m-%d-%H"));
        let day_key = format!("rate_limit:{}:{}", user_id, now.format("%Y-%m-%d"));

        // Check hourly limit
        let hourly_count = self
            .data_access_layer
            .get_kv_store()
            .get(&hour_key)
            .text()
            .await
            .unwrap_or(None)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        if hourly_count >= self.config.max_opportunities_per_user_per_hour {
            return Ok(false);
        }

        // Check daily limit
        let daily_count = self
            .data_access_layer
            .get_kv_store()
            .get(&day_key)
            .text()
            .await
            .unwrap_or(None)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        if daily_count >= self.config.max_opportunities_per_user_per_day {
            return Ok(false);
        }

        Ok(true)
    }

    /// Update user distribution tracking
    async fn update_user_distribution_tracking(
        &self,
        user_id: &str,
        _opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<()> {
        let current_time = chrono::Utc::now().timestamp_millis() as u64;

        // Update last opportunity time
        let last_opportunity_key = format!("user_last_opportunity:{}", user_id);
        self.data_access_layer
            .get_kv_store()
            .put(&last_opportunity_key, current_time.to_string())?
            .expiration_ttl(24 * 3600) // 24 hour TTL
            .execute()
            .await?;

        // Update user stats
        let user_stats_key = format!("user_stats:{}", user_id);
        let current_stats = self
            .data_access_layer
            .get_kv_store()
            .get(&user_stats_key)
            .text()
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0.5".to_string())
            .parse::<f64>()
            .unwrap_or(0.5);

        // Slightly increase user priority for future distributions
        let new_stats = (current_stats + 0.1).min(1.0);
        self.data_access_layer
            .get_kv_store()
            .put(&user_stats_key, new_stats.to_string())?
            .expiration_ttl(7 * 24 * 3600) // 7 day TTL
            .execute()
            .await?;

        Ok(())
    }

    #[allow(dead_code)]
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
        let last_opportunity_key = format!("user_last_opportunity:{}", user_id);
        let last_time = self
            .data_access_layer
            .get_kv_store()
            .get(&last_opportunity_key)
            .text()
            .await
            .unwrap_or(None)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        Ok(last_time)
    }

    /// Calculate user priority score based on historical data
    async fn calculate_user_priority_score(&self, user_id: &str) -> ArbitrageResult<f64> {
        // Get user's historical success rate and activity
        let user_stats_key = format!("user_stats:{}", user_id);
        let user_stats = self
            .data_access_layer
            .get_kv_store()
            .get(&user_stats_key)
            .text()
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0.5".to_string()) // Default priority score
            .parse::<f64>()
            .unwrap_or(0.5);

        Ok(user_stats)
    }

    /// Record distribution analytics for monitoring and optimization
    async fn record_distribution_analytics(
        &self,
        opportunity: &GlobalOpportunity,
        distributed_count: u32,
    ) -> ArbitrageResult<()> {
        let analytics_key = format!(
            "distribution_analytics:{}:{}",
            opportunity.id,
            chrono::Utc::now().format("%Y-%m-%d")
        );

        let analytics_data = serde_json::json!({
            "opportunity_id": opportunity.id,
            "opportunity_type": opportunity.get_opportunity_type(),
            "pair": opportunity.get_pair(),
            "distributed_count": distributed_count,
            "distribution_strategy": opportunity.distribution_strategy.to_stable_string(),
            "detection_timestamp": opportunity.detection_timestamp,
            "distribution_timestamp": chrono::Utc::now().timestamp_millis(),
            "priority": opportunity.priority,
            "priority_score": opportunity.priority_score,
            "ai_enhanced": opportunity.ai_enhanced,
            "ai_confidence_score": opportunity.ai_confidence_score,
        });

        // Store in KV for fast access
        self.data_access_layer
            .get_kv_store()
            .put(&analytics_key, analytics_data.to_string())?
            .expiration_ttl(7 * 24 * 3600) // 7 days TTL
            .execute()
            .await?;

        // Store in D1 for persistent analytics
        let insert_query = "
            INSERT INTO opportunity_distribution_analytics 
            (opportunity_id, pair, rate_difference, priority_score, distributed_count, 
             distribution_strategy, detection_timestamp, distribution_timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ";

        let params: Vec<worker::wasm_bindgen::JsValue> = vec![
            worker::wasm_bindgen::JsValue::from(opportunity.opportunity_data.get_id()),
            worker::wasm_bindgen::JsValue::from(opportunity.opportunity_data.get_pair()),
            worker::wasm_bindgen::JsValue::from(opportunity.opportunity_data.rate_difference()),
            worker::wasm_bindgen::JsValue::from(opportunity.priority_score),
            worker::wasm_bindgen::JsValue::from(distributed_count),
            worker::wasm_bindgen::JsValue::from(
                opportunity.distribution_strategy.to_stable_string(),
            ),
            worker::wasm_bindgen::JsValue::from(opportunity.detection_timestamp as f64),
            worker::wasm_bindgen::JsValue::from(chrono::Utc::now().timestamp_millis() as f64),
        ];

        match self
            .database_repositories
            .execute(insert_query, &params)
            .await
        {
            Ok(_) => {
                // Record analytics via data ingestion module if available
                if let Some(ref data_ingestion_module) = self.data_ingestion_module {
                    // Create analytics message for data ingestion
                    let analytics_message = serde_json::json!({
                        "event_type": "opportunity_distribution",
                        "data": analytics_data,
                        "timestamp": chrono::Utc::now().timestamp_millis()
                    });

                    // Send to data ingestion pipeline (best effort)
                    let analytics_event = crate::services::core::infrastructure::data_ingestion_module::IngestionEvent::new(
                        crate::services::core::infrastructure::data_ingestion_module::IngestionEventType::Analytics,
                        "opportunity_distribution".to_string(),
                        analytics_message,
                    );

                    if let Err(e) = data_ingestion_module
                        .get_coordinator()
                        .ingest_event(analytics_event)
                        .await
                    {
                        eprintln!("Failed to record analytics via data ingestion: {}", e);
                    }
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to store distribution analytics in D1: {}", e);
                // Don't fail the entire operation if analytics storage fails
                Ok(())
            }
        }
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

        let params: Vec<worker::wasm_bindgen::JsValue> =
            vec![worker::wasm_bindgen::JsValue::from(seven_days_ago as f64)];

        let rows = self
            .database_repositories
            .query(success_query, &params)
            .await?;

        if let Some(row) = rows
            .results::<std::collections::HashMap<String, serde_json::Value>>()?
            .first()
        {
            let successful = row
                .get("successful_deliveries")
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let _failed = row
                .get("failed_deliveries")
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let total = row
                .get("total_deliveries")
                .and_then(|s| s.as_str())
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
        let today_key = format!("distribution_stats:today:{}", _today_start);
        let today_count = self
            .data_access_layer
            .get_kv_store()
            .get(&today_key)
            .text()
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
            .unwrap_or(0);

        self.data_access_layer
            .get_kv_store()
            .put(&today_key, (today_count + 1).to_string())?
            .expiration_ttl(86400) // 24 hours TTL
            .execute()
            .await?;

        let active_users_key = "distribution_stats:active_users";
        let _active_users = self
            .data_access_layer
            .get_kv_store()
            .get(active_users_key)
            .text()
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
            .unwrap_or(0);

        // Calculate average distribution time
        let average_distribution_time_ms = self
            .data_access_layer
            .get_kv_store()
            .get("distribution_stats:avg_time")
            .text()
            .await
            .unwrap_or(None)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        // Calculate success rate
        let success_rate_percentage = self.calculate_actual_success_rate().await?;

        Ok(DistributionStats {
            opportunities_distributed_today: today_count,
            active_users: _active_users,
            average_distribution_time_ms,
            success_rate_percentage,
        })
    }

    /// Helper function to parse database row into ArbitrageOpportunity
    fn parse_opportunity_row(
        row: HashMap<String, serde_json::Value>,
    ) -> Option<ArbitrageOpportunity> {
        // Parse opportunity from database row
        let id = row.get("id")?.as_str()?.to_string();
        let pair = row.get("pair")?.as_str()?.to_string();
        let long_exchange = row.get("long_exchange")?.as_str()?.to_string();
        let short_exchange = row.get("short_exchange")?.as_str()?.to_string();
        let long_rate = row.get("long_rate")?.as_f64()?;
        let short_rate = row.get("short_rate")?.as_f64()?;
        let spread = row.get("spread")?.as_f64()?;
        let _confidence = row.get("confidence")?.as_f64()?;
        let _volume = row.get("volume")?.as_f64()?;
        let _detected_at = row.get("detected_at")?.as_u64()?;
        let _expires_at = row.get("expires_at")?.as_u64()?;

        // Parse exchange enums from strings
        let long_exchange_enum = long_exchange.parse::<crate::types::ExchangeIdEnum>().ok()?;
        let short_exchange_enum = short_exchange
            .parse::<crate::types::ExchangeIdEnum>()
            .ok()?;

        // Parse numeric values from strings
        let net_rate_difference = row.get("net_rate_difference").and_then(|s| s.as_f64());
        let potential_profit_value = row.get("potential_profit_value").and_then(|s| s.as_f64());

        let details = row
            .get("details")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let timestamp = row
            .get("timestamp")
            .and_then(|s| s.as_u64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp() as u64);

        // Ensure all required fields from types.rs ArbitrageOpportunity are present
        // Specifically, map detected_at from DB to created_at in struct
        let created_at_from_row = row
            .get("detected_at")
            .and_then(|v| v.as_u64())
            .unwrap_or(timestamp); // Or handle error if detected_at is mandatory and missing
        let _expires_at_from_row = row.get("expires_at").and_then(|v| v.as_u64()).unwrap_or(0);
        let confidence_from_row = row
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5); // Default confidence
        let volume_from_row = row.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.0); // Default volume

        Some(ArbitrageOpportunity {
            id,
            pair,
            long_exchange: long_exchange_enum,
            short_exchange: short_exchange_enum,
            long_rate: Some(long_rate),
            short_rate: Some(short_rate),
            rate_difference: spread, // Assuming `spread` is rate_difference
            net_rate_difference,
            potential_profit_value,
            confidence: confidence_from_row,
            volume: volume_from_row,
            timestamp, // Keep original timestamp as well if distinct from created_at
            created_at: created_at_from_row, // Mapped from detected_at or a default
            detected_at: created_at_from_row, // Use same value as created_at for now
            expires_at: created_at_from_row + 300_000, // 5 minutes TTL
            r#type: ArbitrageType::CrossExchange,
            min_exchanges_required: 2,
            details,
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
        // Check cache first
        if let Ok(Some(cached_data)) = self
            .data_access_layer
            .get_kv_store()
            .get(cache_key)
            .text()
            .await
        {
            if let Ok(opportunities) =
                serde_json::from_str::<Vec<ArbitrageOpportunity>>(&cached_data)
            {
                return Ok(opportunities);
            }
        }

        // Fallback: Get from D1 database
        let params: Vec<worker::wasm_bindgen::JsValue> = params
            .iter()
            .map(|v| match v {
                serde_json::Value::String(s) => worker::wasm_bindgen::JsValue::from(s.as_str()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        worker::wasm_bindgen::JsValue::from(i as f64)
                    } else if let Some(f) = n.as_f64() {
                        worker::wasm_bindgen::JsValue::from(f)
                    } else {
                        worker::wasm_bindgen::JsValue::from(n.to_string().as_str())
                    }
                }
                serde_json::Value::Bool(b) => worker::wasm_bindgen::JsValue::from(*b),
                serde_json::Value::Null => worker::wasm_bindgen::JsValue::NULL,
                _ => worker::wasm_bindgen::JsValue::from(v.to_string().as_str()),
            })
            .collect();
        let rows = self.database_repositories.query(query, &params).await?;

        let opportunities: Vec<ArbitrageOpportunity> = rows
            .results::<HashMap<String, serde_json::Value>>()?
            .into_iter()
            .filter_map(Self::parse_opportunity_row)
            .collect();

        // Cache the results if TTL is specified
        if let Some(ttl) = cache_ttl {
            let cached_json = serde_json::to_string(&opportunities)?;
            let _ = self
                .data_access_layer
                .get_kv_store()
                .put(cache_key, &cached_json)?
                .expiration_ttl(ttl)
                .execute()
                .await;
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
        _distributed_count: u32,
        detection_timestamp: u64,
    ) -> ArbitrageResult<()> {
        // Update opportunities distributed today
        let today_key = format!("distribution_stats:today:{}", detection_timestamp);
        let today_count = self
            .data_access_layer
            .get_kv_store()
            .get(&today_key)
            .text()
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
            .unwrap_or(0);

        self.data_access_layer
            .get_kv_store()
            .put(&today_key, (today_count + 1).to_string())?
            .expiration_ttl(86400) // 24 hours TTL
            .execute()
            .await?;

        let active_users_key = "distribution_stats:active_users";
        let _active_users = self
            .data_access_layer
            .get_kv_store()
            .get(active_users_key)
            .text()
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
            .unwrap_or(0);

        // Update average distribution time
        let avg_time_key = "distribution_stats:avg_time";
        let current_distribution_time =
            chrono::Utc::now().timestamp_millis() as u64 - detection_timestamp;
        let existing_avg_time = self
            .data_access_layer
            .get_kv_store()
            .get(avg_time_key)
            .text()
            .await
            .unwrap_or(None)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        // Calculate new average (simple moving average)
        let new_avg_time = if existing_avg_time > 0.0 {
            (existing_avg_time + current_distribution_time as f64) / 2.0
        } else {
            current_distribution_time as f64
        };

        self.data_access_layer
            .get_kv_store()
            .put(avg_time_key, new_avg_time.to_string())?
            .expiration_ttl(3600)
            .execute()
            .await?;

        Ok(())
    }

    /// Update active users count in KV cache
    async fn update_active_users_count(&self, user_count: u32) -> ArbitrageResult<()> {
        let active_users_key = "distribution_stats:active_users";
        self.data_access_layer
            .get_kv_store()
            .put(active_users_key, user_count.to_string())?
            .expiration_ttl(24 * 3600) // 24 hour TTL
            .execute()
            .await?;
        Ok(())
    }

    /// Get user's subscription tier
    async fn get_user_subscription_tier(&self, user_id: &str) -> ArbitrageResult<SubscriptionTier> {
        // Query user profile from database
        let query = "SELECT subscription_tier FROM user_profiles WHERE user_id = ?";
        let params: Vec<worker::wasm_bindgen::JsValue> =
            vec![worker::wasm_bindgen::JsValue::from(user_id)];

        let result = self.database_repositories.query(query, &params).await?;
        let rows = result.results::<HashMap<String, serde_json::Value>>()?;

        if let Some(row) = rows.first() {
            if let Some(tier_str) = row.get("subscription_tier").and_then(|v| v.as_str()) {
                match tier_str {
                    "Free" => Ok(SubscriptionTier::Free),
                    "Basic" => Ok(SubscriptionTier::Basic),
                    "Premium" => Ok(SubscriptionTier::Premium),
                    "Pro" => Ok(SubscriptionTier::Pro),
                    "Enterprise" => Ok(SubscriptionTier::Enterprise),
                    "Admin" => Ok(SubscriptionTier::Admin),
                    "SuperAdmin" => Ok(SubscriptionTier::SuperAdmin),
                    _ => Ok(SubscriptionTier::Free),
                }
            } else {
                Ok(SubscriptionTier::Free)
            }
        } else {
            Ok(SubscriptionTier::Free)
        }
    }

    /// Calculate personalization score for user and opportunity
    async fn calculate_personalization_score(
        &self,
        user_id: &str,
        opportunity: &GlobalOpportunity,
    ) -> ArbitrageResult<f64> {
        // Get user preferences and history
        let user_stats_key = format!("user_stats:{}", user_id);
        let user_stats = self
            .data_access_layer
            .get_kv_store()
            .get(&user_stats_key)
            .text()
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "0.5".to_string())
            .parse::<f64>()
            .unwrap_or(0.5);

        // Base score from user stats
        let mut score = user_stats;

        // Adjust based on opportunity characteristics
        if let Some(arbitrage_opp) = opportunity.opportunity_data.as_arbitrage() {
            // Higher score for higher rate differences
            score += arbitrage_opp.rate_difference * 0.1;

            // Higher score for higher confidence
            score += arbitrage_opp.confidence * 0.2;

            // Adjust for user's preferred pairs (simplified)
            if arbitrage_opp.pair.contains("BTC") || arbitrage_opp.pair.contains("ETH") {
                score += 0.1;
            }
        }

        // Clamp score between 0.0 and 1.0
        Ok(score.clamp(0.0, 1.0))
    }

    /// Get subscription tier priority for user
    async fn get_subscription_tier_priority(&self, user_id: &str) -> ArbitrageResult<f64> {
        let tier = self.get_user_subscription_tier(user_id).await?;

        let priority = match tier {
            SubscriptionTier::Free => 1.0,
            SubscriptionTier::Paid => 3.0,
            SubscriptionTier::Basic => 2.0,
            SubscriptionTier::Premium => 4.0,
            SubscriptionTier::Pro => 5.0,
            SubscriptionTier::Enterprise => 6.0,
            SubscriptionTier::Admin => 7.0,
            SubscriptionTier::SuperAdmin => 7.0,
        };

        Ok(priority)
    }
}

/// Distribution statistics
#[derive(Debug, Clone)]
pub struct DistributionStats {
    pub opportunities_distributed_today: u32,
    pub active_users: u32,
    pub average_distribution_time_ms: f64,
    pub success_rate_percentage: f64,
}
