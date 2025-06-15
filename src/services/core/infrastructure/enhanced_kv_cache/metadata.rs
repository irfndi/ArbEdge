//! Cache Metadata Tracking
//!
//! Provides comprehensive metadata tracking for cache analytics, cleanup optimization,
//! and performance monitoring

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Cache entry metadata for analytics and optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// Data type categorization
    pub data_type: DataType,
    /// Service that created this cache entry
    pub service_name: String,
    /// Priority level for cleanup decisions
    pub priority: Priority,
    /// Tags for categorization and bulk operations
    pub tags: Vec<String>,
    /// Access pattern information
    pub access_pattern: AccessPattern,
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
    /// Cleanup information
    pub cleanup_info: CleanupInfo,
}

impl CacheMetadata {
    /// Create new metadata with defaults
    pub fn new() -> Self {
        Self {
            data_type: DataType::Generic,
            service_name: "unknown".to_string(),
            priority: Priority::Medium,
            tags: vec![],
            access_pattern: AccessPattern::default(),
            performance_metrics: PerformanceMetrics::default(),
            cleanup_info: CleanupInfo::default(),
        }
    }

    /// Create metadata for a specific service and data type
    pub fn for_service(service_name: String, data_type: DataType) -> Self {
        let priority = data_type.default_priority();
        Self {
            data_type,
            service_name,
            priority,
            tags: vec![],
            access_pattern: AccessPattern::default(),
            performance_metrics: PerformanceMetrics::default(),
            cleanup_info: CleanupInfo::default(),
        }
    }

    /// Add a tag for categorization
    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// Set priority
    pub fn set_priority(&mut self, priority: Priority) {
        self.priority = priority;
    }

    /// Update access pattern with new access
    pub fn record_access(&mut self) {
        self.access_pattern.record_access();
    }

    /// Update performance metrics
    pub fn record_performance(&mut self, response_time_ms: u64, cache_hit: bool) {
        self.performance_metrics
            .record_operation(response_time_ms, cache_hit);
    }

    /// Check if this entry should be cleaned up
    pub fn should_cleanup(&self, max_age: Duration) -> bool {
        self.cleanup_info
            .should_cleanup(max_age, &self.access_pattern, &self.priority)
    }
}

impl Default for CacheMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Data type classification for TTL and priority decisions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataType {
    /// Real-time market data
    MarketData,
    /// Funding rate information
    FundingRate,
    /// User profile information
    UserProfile,
    /// Trading opportunities
    Opportunities,
    /// Session data
    Session,
    /// Configuration data
    Configuration,
    /// Analytics data
    Analytics,
    /// AI responses and embeddings
    AiResponse,
    /// Historical data
    Historical,
    /// Generic data
    Generic,
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataType::MarketData => write!(f, "market_data"),
            DataType::FundingRate => write!(f, "funding_rate"),
            DataType::UserProfile => write!(f, "user_profile"),
            DataType::Opportunities => write!(f, "opportunities"),
            DataType::Session => write!(f, "session"),
            DataType::Configuration => write!(f, "configuration"),
            DataType::Analytics => write!(f, "analytics"),
            DataType::AiResponse => write!(f, "ai_response"),
            DataType::Historical => write!(f, "historical"),
            DataType::Generic => write!(f, "generic"),
        }
    }
}

impl DataType {
    /// Get default priority for this data type
    pub fn default_priority(&self) -> Priority {
        match self {
            DataType::MarketData => Priority::High,
            DataType::FundingRate => Priority::High,
            DataType::UserProfile => Priority::High,
            DataType::Opportunities => Priority::High,
            DataType::Session => Priority::Medium,
            DataType::Configuration => Priority::Medium,
            DataType::Analytics => Priority::Low,
            DataType::AiResponse => Priority::Medium,
            DataType::Historical => Priority::Low,
            DataType::Generic => Priority::Medium,
        }
    }

    /// Get suggested TTL for this data type
    pub fn suggested_ttl(&self) -> Duration {
        match self {
            DataType::MarketData => Duration::from_secs(300), // 5 minutes
            DataType::FundingRate => Duration::from_secs(3600), // 1 hour (funding rates change less frequently)
            DataType::UserProfile => Duration::from_secs(3600), // 1 hour
            DataType::Opportunities => Duration::from_secs(600), // 10 minutes
            DataType::Session => Duration::from_secs(1800),     // 30 minutes
            DataType::Configuration => Duration::from_secs(86400), // 24 hours
            DataType::Analytics => Duration::from_secs(3600),   // 1 hour
            DataType::AiResponse => Duration::from_secs(7200),  // 2 hours
            DataType::Historical => Duration::from_secs(604800), // 7 days
            DataType::Generic => Duration::from_secs(3600),     // 1 hour
        }
    }
}

/// Priority levels for cleanup and tier management
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Priority {
    Critical, // Never cleanup unless absolutely necessary
    High,     // Cleanup only when space is needed
    Medium,   // Normal cleanup policies
    Low,      // Aggressive cleanup
}

impl Priority {
    /// Get cleanup threshold (lower = more aggressive cleanup)
    pub fn cleanup_threshold(&self) -> f64 {
        match self {
            Priority::Critical => 0.95, // Only cleanup if 95% full
            Priority::High => 0.85,     // Cleanup if 85% full
            Priority::Medium => 0.70,   // Cleanup if 70% full
            Priority::Low => 0.50,      // Cleanup if 50% full
        }
    }
}

/// Access pattern tracking for tier management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPattern {
    /// Total number of accesses
    pub access_count: u32,
    /// Last access timestamp
    pub last_access: u64,
    /// Access frequency (accesses per hour)
    pub access_frequency: f64,
    /// Peak access times (hourly buckets)
    pub hourly_access_counts: [u16; 24],
    /// Sequential access indicators
    pub is_sequential: bool,
    /// Burst access pattern
    pub has_burst_pattern: bool,
}

impl AccessPattern {
    /// Record a new access
    pub fn record_access(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.access_count += 1;
        self.last_access = now;

        // Update hourly access count
        let hour = ((now % 86400) / 3600) as usize;
        if hour < 24 {
            self.hourly_access_counts[hour] = self.hourly_access_counts[hour].saturating_add(1);
        }

        // Update access frequency (simple moving average)
        let time_diff = now.saturating_sub(self.last_access).max(1);
        let new_frequency = 3600.0 / time_diff as f64;
        self.access_frequency = (self.access_frequency * 0.9) + (new_frequency * 0.1);

        // Detect burst patterns (rapid consecutive accesses)
        if time_diff < 60 && self.access_count > 5 {
            self.has_burst_pattern = true;
        }
    }

    /// Get time since last access
    pub fn last_access_age(&self) -> Duration {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Duration::from_secs(now.saturating_sub(self.last_access))
    }

    /// Check if this is a frequently accessed item
    pub fn is_frequently_accessed(&self) -> bool {
        self.access_frequency > 1.0 || // More than 1 access per hour
        self.access_count > 10 ||      // Total accesses > 10
        self.has_burst_pattern // Has burst access pattern
    }

    /// Get peak access hour
    pub fn peak_access_hour(&self) -> Option<u8> {
        self.hourly_access_counts
            .iter()
            .enumerate()
            .max_by_key(|(_, &count)| count)
            .map(|(hour, _)| hour as u8)
    }
}

impl Default for AccessPattern {
    fn default() -> Self {
        Self {
            access_count: 0,
            last_access: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            access_frequency: 0.0,
            hourly_access_counts: [0; 24],
            is_sequential: false,
            has_burst_pattern: false,
        }
    }
}

/// Performance metrics for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Total operations count
    pub total_operations: u64,
    /// Cache hit count
    pub hit_count: u64,
    /// Cache miss count
    pub miss_count: u64,
    /// Compression effectiveness
    pub compression_ratio: Option<f64>,
    /// Network transfer size
    pub network_bytes: u64,
}

impl PerformanceMetrics {
    /// Record a cache operation
    pub fn record_operation(&mut self, response_time_ms: u64, cache_hit: bool) {
        self.total_operations += 1;

        if cache_hit {
            self.hit_count += 1;
        } else {
            self.miss_count += 1;
        }

        // Update average response time
        let new_avg = (self.avg_response_time_ms * (self.total_operations - 1) as f64
            + response_time_ms as f64)
            / self.total_operations as f64;
        self.avg_response_time_ms = new_avg;
    }

    /// Get cache hit ratio
    pub fn hit_ratio(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            self.hit_count as f64 / self.total_operations as f64
        }
    }

    /// Check if performance is degraded
    pub fn is_performance_degraded(&self) -> bool {
        self.avg_response_time_ms > 100.0 || // > 100ms average
        self.hit_ratio() < 0.8 // < 80% hit ratio
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            avg_response_time_ms: 0.0,
            total_operations: 0,
            hit_count: 0,
            miss_count: 0,
            compression_ratio: None,
            network_bytes: 0,
        }
    }
}

/// Cleanup optimization information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupInfo {
    /// Cleanup eligibility score (0.0 = never cleanup, 1.0 = immediate cleanup)
    pub cleanup_score: f64,
    /// Last cleanup check timestamp
    pub last_cleanup_check: u64,
    /// Number of times this entry was saved from cleanup
    pub cleanup_saves: u32,
    /// Estimated cost of regenerating this data
    pub regeneration_cost: RegenerationCost,
}

impl CleanupInfo {
    /// Check if this entry should be cleaned up
    pub fn should_cleanup(
        &self,
        max_age: Duration,
        access_pattern: &AccessPattern,
        priority: &Priority,
    ) -> bool {
        // Never cleanup critical priority items unless absolutely necessary
        if matches!(priority, Priority::Critical) && self.cleanup_score < 0.95 {
            return false;
        }

        // Check age-based cleanup
        let age_based = access_pattern.last_access_age() > max_age;

        // Check score-based cleanup
        let score_based = self.cleanup_score > priority.cleanup_threshold();

        // Don't cleanup frequently accessed items
        let frequency_protection = access_pattern.is_frequently_accessed();

        (age_based || score_based) && !frequency_protection
    }

    /// Update cleanup score based on access patterns and space pressure
    pub fn update_cleanup_score(&mut self, access_pattern: &AccessPattern, space_pressure: f64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.last_cleanup_check = now;

        // Base score on last access age (older = higher score)
        let age_factor = access_pattern.last_access_age().as_secs() as f64 / 86400.0; // Days
        let age_score = (age_factor * 0.4).min(0.5); // Max 0.5 for age

        // Access frequency factor (less frequent = higher score)
        let frequency_score = if access_pattern.access_frequency < 0.1 {
            0.3 // Low frequency
        } else if access_pattern.access_frequency < 1.0 {
            0.2 // Medium frequency
        } else {
            0.0 // High frequency
        };

        // Space pressure factor
        let pressure_score = space_pressure * 0.3;

        // Regeneration cost factor (high cost = lower score)
        let cost_factor = match self.regeneration_cost {
            RegenerationCost::Low => 0.0,
            RegenerationCost::Medium => -0.1,
            RegenerationCost::High => -0.2,
            RegenerationCost::Critical => -0.4,
        };

        self.cleanup_score =
            (age_score + frequency_score + pressure_score + cost_factor).clamp(0.0, 1.0);
    }
}

impl Default for CleanupInfo {
    fn default() -> Self {
        Self {
            cleanup_score: 0.0,
            last_cleanup_check: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            cleanup_saves: 0,
            regeneration_cost: RegenerationCost::Medium,
        }
    }
}

/// Cost estimate for regenerating cached data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegenerationCost {
    /// Low cost - can be regenerated quickly
    Low,
    /// Medium cost - moderate regeneration time
    Medium,
    /// High cost - expensive to regenerate
    High,
    /// Critical - very expensive or impossible to regenerate
    Critical,
}

/// Metadata tracker for analyzing cache usage patterns
pub struct MetadataTracker {
    /// Global cache metadata
    global_stats: HashMap<String, CacheMetadata>,
    /// Service-specific statistics
    service_stats: HashMap<String, ServiceStats>,
    /// Data type statistics
    data_type_stats: HashMap<DataType, DataTypeStats>,
}

impl MetadataTracker {
    /// Create a new metadata tracker
    pub fn new() -> Self {
        Self {
            global_stats: HashMap::new(),
            service_stats: HashMap::new(),
            data_type_stats: HashMap::new(),
        }
    }

    /// Track cache entry metadata
    pub fn track_entry(&mut self, key: &str, metadata: &CacheMetadata) {
        self.global_stats.insert(key.to_string(), metadata.clone());

        // Update service stats
        let service_stats = self
            .service_stats
            .entry(metadata.service_name.clone())
            .or_default();
        service_stats.entry_count += 1;

        // Update data type stats
        let data_type_stats = self
            .data_type_stats
            .entry(metadata.data_type.clone())
            .or_default();
        data_type_stats.entry_count += 1;
    }

    /// Get cleanup candidates
    pub fn get_cleanup_candidates(&self, max_candidates: usize) -> Vec<String> {
        let mut candidates: Vec<_> = self
            .global_stats
            .iter()
            .map(|(key, metadata)| (key.clone(), metadata.cleanup_info.cleanup_score))
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates
            .into_iter()
            .take(max_candidates)
            .map(|(key, _)| key)
            .collect()
    }

    /// Get service statistics
    pub fn get_service_stats(&self, service_name: &str) -> Option<&ServiceStats> {
        self.service_stats.get(service_name)
    }

    /// Get data type statistics
    pub fn get_data_type_stats(&self, data_type: &DataType) -> Option<&DataTypeStats> {
        self.data_type_stats.get(data_type)
    }

    /// Record cache access for tracking patterns
    pub async fn record_access(
        &mut self,
        key: &str,
        data_type: &DataType,
        _tier: &super::CacheTier,
    ) {
        if let Some(metadata) = self.global_stats.get_mut(key) {
            metadata.record_access();
        } else {
            // Create new metadata if it doesn't exist
            let mut metadata =
                CacheMetadata::for_service("cache_manager".to_string(), data_type.clone());
            metadata.record_access();
            self.global_stats.insert(key.to_string(), metadata);
        }
    }

    /// Record storage operation
    pub async fn record_storage(
        &mut self,
        key: &str,
        data_type: &DataType,
        _tier: &super::CacheTier,
        size_bytes: usize,
    ) {
        if let Some(metadata) = self.global_stats.get_mut(key) {
            // Update existing metadata
            metadata.performance_metrics.network_bytes += size_bytes as u64;
        } else {
            // Create new metadata
            let mut metadata =
                CacheMetadata::for_service("cache_manager".to_string(), data_type.clone());
            metadata.performance_metrics.network_bytes = size_bytes as u64;
            self.global_stats.insert(key.to_string(), metadata);
        }

        // Update service stats
        let service_stats = self
            .service_stats
            .entry("cache_manager".to_string())
            .or_default();
        service_stats.total_size += size_bytes as u64;

        // Update data type stats
        let data_type_stats = self.data_type_stats.entry(data_type.clone()).or_default();
        data_type_stats.total_size += size_bytes as u64;
    }

    /// Get access pattern for a key
    pub async fn get_access_pattern(&self, key: &str) -> Option<AccessPattern> {
        self.global_stats
            .get(key)
            .map(|metadata| metadata.access_pattern.clone())
    }

    /// Get metadata for a key
    pub fn get_metadata(&self, key: &str) -> Result<&CacheMetadata, crate::utils::ArbitrageError> {
        self.global_stats.get(key).ok_or_else(|| {
            crate::utils::ArbitrageError::cache_error(format!("No metadata found for key: {}", key))
        })
    }

    /// Generate comprehensive analytics report
    pub fn generate_analytics_report(&self) -> CacheAnalyticsReport {
        let generated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let health_score = self.calculate_health_score();
        let performance_analysis = self.analyze_performance();
        let cleanup_recommendations = self.generate_cleanup_recommendations();
        let tier_insights = self.analyze_tier_efficiency();
        let top_entries = self.analyze_top_entries();
        let trends = self.analyze_trends();

        CacheAnalyticsReport {
            health_score,
            performance_analysis,
            cleanup_recommendations,
            tier_insights,
            top_entries,
            trends,
            generated_at,
        }
    }

    /// Calculate overall cache health score
    fn calculate_health_score(&self) -> f64 {
        let mut score_components = Vec::new();

        // Hit rate component (30% weight)
        let avg_hit_rate = self.calculate_average_hit_rate();
        score_components.push((avg_hit_rate, 0.3));

        // Performance component (25% weight)
        let performance_score = self.calculate_performance_score();
        score_components.push((performance_score, 0.25));

        // Distribution efficiency (20% weight)
        let distribution_score = self.calculate_distribution_efficiency();
        score_components.push((distribution_score, 0.2));

        // Cleanup health (15% weight)
        let cleanup_score = self.calculate_cleanup_health();
        score_components.push((cleanup_score, 0.15));

        // Resource utilization (10% weight)
        let resource_score = self.calculate_resource_efficiency();
        score_components.push((resource_score, 0.1));

        // Weighted average
        score_components
            .into_iter()
            .map(|(score, weight)| score * weight)
            .sum()
    }

    /// Analyze performance patterns
    fn analyze_performance(&self) -> PerformanceAnalysis {
        let overall_hit_rate = self.calculate_average_hit_rate();

        // Calculate response times by tier (simulated for now)
        let mut response_times_by_tier = HashMap::new();
        response_times_by_tier.insert("hot".to_string(), 2.5);
        response_times_by_tier.insert("warm".to_string(), 8.3);
        response_times_by_tier.insert("cold".to_string(), 25.7);

        // Identify underperforming data types
        let underperforming_types = self.identify_underperforming_types();

        // Calculate compression effectiveness
        let compression_effectiveness = self.calculate_compression_effectiveness();

        // Generate hot path metrics
        let hot_path_metrics = self.generate_hot_path_metrics();

        PerformanceAnalysis {
            overall_hit_rate,
            response_times_by_tier,
            underperforming_types,
            compression_effectiveness,
            hot_path_metrics,
        }
    }

    /// Generate cleanup recommendations
    fn generate_cleanup_recommendations(&self) -> CleanupRecommendations {
        let mut immediate_cleanup = Vec::new();
        let mut conditional_cleanup = Vec::new();
        let mut total_space_savings = 0u64;

        for (key, metadata) in &self.global_stats {
            let cleanup_score = metadata.cleanup_info.cleanup_score;
            let size_estimate = 1024u64; // Placeholder size
            let last_access_hours = self.hours_since_last_access(metadata);

            let candidate = CleanupCandidate {
                key: key.clone(),
                score: cleanup_score,
                size_bytes: size_estimate,
                last_access_hours_ago: last_access_hours,
                regeneration_cost: metadata.cleanup_info.regeneration_cost.clone(),
                reasoning: self.generate_cleanup_reasoning(metadata),
            };

            if cleanup_score > 0.8 {
                immediate_cleanup.push(candidate);
                total_space_savings += size_estimate;
            } else if cleanup_score > 0.5 {
                conditional_cleanup.push(candidate);
            }
        }

        // Generate frequency recommendations
        let cleanup_frequency_recommendations = self.generate_cleanup_frequency_recommendations();

        CleanupRecommendations {
            immediate_cleanup,
            conditional_cleanup,
            potential_space_savings_bytes: total_space_savings,
            cleanup_frequency_recommendations,
        }
    }

    /// Analyze tier efficiency and generate insights
    fn analyze_tier_efficiency(&self) -> TierInsights {
        // Calculate current distribution
        let current_distribution = self.calculate_current_tier_distribution();

        // Generate optimal distribution recommendations
        let recommended_distribution = self.calculate_optimal_tier_distribution();

        // Identify migration candidates
        let tier_migration_candidates = self.identify_tier_migration_candidates();

        // Calculate efficiency scores
        let tier_efficiency = self.calculate_tier_efficiency_scores();

        TierInsights {
            recommended_distribution,
            current_distribution,
            tier_migration_candidates,
            tier_efficiency,
        }
    }

    /// Analyze top performing and problematic entries
    fn analyze_top_entries(&self) -> TopEntriesAnalysis {
        let mut hot_entries = Vec::new();
        let space_heavy_entries = Vec::new();
        let compression_champions = Vec::new();
        let mut problematic_entries = Vec::new();

        for (key, metadata) in &self.global_stats {
            // Most frequently accessed
            if metadata.access_pattern.access_frequency > 10.0 {
                hot_entries.push(EntryPerformance {
                    key: key.clone(),
                    metric_value: metadata.access_pattern.access_frequency,
                    context: "High access frequency".to_string(),
                    recommendation: Some("Consider promoting to hot tier".to_string()),
                });
            }

            // Check for problematic patterns
            if metadata.performance_metrics.hit_ratio() < 0.3 {
                problematic_entries.push(EntryPerformance {
                    key: key.clone(),
                    metric_value: metadata.performance_metrics.hit_ratio(),
                    context: "Low hit rate".to_string(),
                    recommendation: Some("Consider removing or optimizing".to_string()),
                });
            }
        }

        // Sort by metric value and take top entries
        hot_entries.sort_by(|a, b| b.metric_value.partial_cmp(&a.metric_value).unwrap());
        hot_entries.truncate(10);

        problematic_entries.sort_by(|a, b| a.metric_value.partial_cmp(&b.metric_value).unwrap());
        problematic_entries.truncate(10);

        TopEntriesAnalysis {
            hot_entries,
            space_heavy_entries,
            compression_champions,
            problematic_entries,
        }
    }

    /// Analyze trends and patterns over time
    fn analyze_trends(&self) -> TrendAnalysis {
        let access_trends = self.calculate_access_trends();
        let performance_alerts = self.generate_performance_alerts();
        let seasonal_patterns = self.detect_seasonal_patterns();
        let growth_projections = self.calculate_growth_projections();

        TrendAnalysis {
            access_trends,
            performance_alerts,
            seasonal_patterns,
            growth_projections,
        }
    }

    /// Get advanced cleanup candidates with detailed analysis
    pub fn get_advanced_cleanup_candidates(
        &self,
        space_pressure: f64,
        target_cleanup_bytes: u64,
    ) -> Vec<CleanupCandidate> {
        let mut candidates = Vec::new();
        let mut total_bytes = 0u64;

        for (key, metadata) in &self.global_stats {
            if total_bytes >= target_cleanup_bytes {
                break;
            }

            let cleanup_score = self.calculate_enhanced_cleanup_score(metadata, space_pressure);
            if cleanup_score > 0.4 {
                let size_estimate = 1024u64; // Placeholder
                candidates.push(CleanupCandidate {
                    key: key.clone(),
                    score: cleanup_score,
                    size_bytes: size_estimate,
                    last_access_hours_ago: self.hours_since_last_access(metadata),
                    regeneration_cost: metadata.cleanup_info.regeneration_cost.clone(),
                    reasoning: self.generate_enhanced_cleanup_reasoning(metadata, space_pressure),
                });
                total_bytes += size_estimate;
            }
        }

        // Sort by score (highest first)
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        candidates
    }

    /// Performance monitoring with real-time insights
    pub fn get_performance_insights(&self) -> HashMap<String, serde_json::Value> {
        let mut insights = HashMap::new();

        // Cache hit rate insights
        let hit_rate = self.calculate_average_hit_rate();
        insights.insert("hit_rate".to_string(), serde_json::json!({
            "value": hit_rate,
            "status": if hit_rate > 0.8 { "good" } else if hit_rate > 0.6 { "warning" } else { "critical" },
            "trend": self.calculate_hit_rate_trend()
        }));

        // Response time insights
        insights.insert(
            "response_time".to_string(),
            serde_json::json!({
                "average_ms": self.calculate_average_response_time(),
                "percentiles": self.calculate_response_time_percentiles(),
                "trend": self.calculate_response_time_trend()
            }),
        );

        // Memory utilization insights
        insights.insert(
            "memory_utilization".to_string(),
            serde_json::json!({
                "utilization_percent": self.calculate_memory_utilization(),
                "growth_rate": self.calculate_memory_growth_rate(),
                "projected_full_in_days": self.calculate_projected_full_days()
            }),
        );

        // Hot spot analysis
        insights.insert(
            "hot_spots".to_string(),
            serde_json::json!({
                "identified_count": self.count_hot_spots(),
                "top_hot_spots": self.get_top_hot_spots(),
                "optimization_opportunities": self.identify_optimization_opportunities()
            }),
        );

        insights
    }

    // === Helper Methods for Analytics ===

    fn calculate_average_hit_rate(&self) -> f64 {
        let total_ops = self
            .service_stats
            .values()
            .map(|s| s.hit_ratio * s.entry_count as f64)
            .sum::<f64>();
        let total_entries = self
            .service_stats
            .values()
            .map(|s| s.entry_count)
            .sum::<u64>() as f64;

        if total_entries > 0.0 {
            total_ops / total_entries
        } else {
            0.0
        }
    }

    fn calculate_performance_score(&self) -> f64 {
        // Simplified performance score based on average response time
        let avg_response = self.calculate_average_response_time();
        // Score decreases as response time increases (optimal is under 10ms)
        (20.0 - avg_response.min(20.0)) / 20.0
    }

    fn calculate_distribution_efficiency(&self) -> f64 {
        // Calculate actual distribution efficiency based on service stats
        if self.service_stats.is_empty() {
            return 0.0;
        }

        let total_entries: u64 = self.service_stats.values().map(|s| s.entry_count).sum();
        if total_entries == 0 {
            return 0.0;
        }

        // Calculate variance in distribution
        let avg_entries = total_entries as f64 / self.service_stats.len() as f64;
        let variance: f64 = self
            .service_stats
            .values()
            .map(|s| {
                let diff = s.entry_count as f64 - avg_entries;
                diff * diff
            })
            .sum::<f64>()
            / self.service_stats.len() as f64;

        // Lower variance means better distribution
        let normalized_variance = (variance / (avg_entries * avg_entries)).min(1.0);
        1.0 - normalized_variance
    }

    fn calculate_cleanup_health(&self) -> f64 {
        // Check how many entries are eligible for cleanup vs total
        let cleanup_eligible = self
            .global_stats
            .values()
            .filter(|m| m.cleanup_info.cleanup_score > 0.5)
            .count() as f64;
        let total = self.global_stats.len() as f64;

        if total > 0.0 {
            1.0 - (cleanup_eligible / total).min(1.0)
        } else {
            1.0
        }
    }

    fn calculate_resource_efficiency(&self) -> f64 {
        // Calculate actual resource efficiency based on hit rates and compression
        if self.service_stats.is_empty() {
            return 0.0;
        }

        let avg_hit_rate = self.calculate_average_hit_rate();
        let avg_compression = self
            .data_type_stats
            .values()
            .map(|s| s.compression_ratio)
            .sum::<f64>()
            / self.data_type_stats.len().max(1) as f64;

        // Combine hit rate and compression efficiency
        (avg_hit_rate * 0.7 + avg_compression * 0.3).min(1.0)
    }

    fn identify_underperforming_types(&self) -> Vec<String> {
        self.data_type_stats
            .iter()
            .filter(|(_, stats)| stats.avg_ttl.as_secs() < 300) // Less than 5 minutes
            .map(|(data_type, _)| format!("{:?}", data_type))
            .collect()
    }

    fn calculate_compression_effectiveness(&self) -> HashMap<String, f64> {
        self.data_type_stats
            .iter()
            .map(|(data_type, stats)| (format!("{:?}", data_type), stats.compression_ratio))
            .collect()
    }

    fn generate_hot_path_metrics(&self) -> HotPathMetrics {
        // Calculate actual hot path metrics from service stats
        let avg_response_time = self.calculate_average_response_time();

        let mut bottlenecks = Vec::new();
        let mut optimizations = Vec::new();

        // Identify bottlenecks based on actual data
        if avg_response_time > 20.0 {
            bottlenecks.push("High average response time".to_string());
            optimizations.push("Optimize cache tier allocation".to_string());
        }

        let avg_hit_rate = self.calculate_average_hit_rate();
        if avg_hit_rate < 0.8 {
            bottlenecks.push("Low cache hit rate".to_string());
            optimizations.push("Improve cache warming strategies".to_string());
        }

        HotPathMetrics {
            critical_path_response_ms: avg_response_time,
            bottlenecks,
            optimization_opportunities: optimizations,
        }
    }

    fn hours_since_last_access(&self, metadata: &CacheMetadata) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        ((now - metadata.access_pattern.last_access) / 3600).min(999)
    }

    fn generate_cleanup_reasoning(&self, metadata: &CacheMetadata) -> String {
        let hours_ago = self.hours_since_last_access(metadata);
        format!(
            "Last accessed {} hours ago, cleanup score: {:.2}",
            hours_ago, metadata.cleanup_info.cleanup_score
        )
    }

    fn generate_cleanup_frequency_recommendations(&self) -> HashMap<String, Duration> {
        let mut recommendations = HashMap::new();
        recommendations.insert("MarketData".to_string(), Duration::from_secs(3600));
        recommendations.insert("UserProfile".to_string(), Duration::from_secs(86400));
        recommendations.insert("Analytics".to_string(), Duration::from_secs(21600));
        recommendations
    }

    fn calculate_current_tier_distribution(&self) -> HashMap<String, f64> {
        let mut distribution = HashMap::new();
        let total_entries = self
            .service_stats
            .values()
            .map(|s| s.entry_count)
            .sum::<u64>() as f64;

        if total_entries == 0.0 {
            return distribution;
        }

        // Calculate distribution based on service statistics
        for (service_name, stats) in &self.service_stats {
            let percentage = (stats.entry_count as f64 / total_entries) * 100.0;
            distribution.insert(service_name.clone(), percentage);
        }

        // Add data type distribution
        let total_by_type = self
            .data_type_stats
            .values()
            .map(|s| s.entry_count)
            .sum::<u64>() as f64;
        if total_by_type > 0.0 {
            for (data_type, stats) in &self.data_type_stats {
                let type_name = format!("{:?}", data_type);
                let percentage = (stats.entry_count as f64 / total_by_type) * 100.0;
                distribution.insert(format!("type_{}", type_name), percentage);
            }
        }

        distribution
    }

    fn calculate_optimal_tier_distribution(&self) -> HashMap<String, f64> {
        let mut optimal_distribution = HashMap::new();
        let total_access_frequency = self
            .service_stats
            .values()
            .map(|s| s.avg_access_frequency)
            .sum::<f64>();

        if total_access_frequency == 0.0 {
            return optimal_distribution;
        }

        // Calculate optimal distribution based on access patterns and hit ratios
        for (service_name, stats) in &self.service_stats {
            // Weight by access frequency and hit ratio
            let weight = (stats.avg_access_frequency * stats.hit_ratio) / total_access_frequency;
            let optimal_percentage = weight * 100.0;
            optimal_distribution.insert(service_name.clone(), optimal_percentage);
        }

        // Normalize to ensure total is 100%
        let total_percentage: f64 = optimal_distribution.values().sum();
        if total_percentage > 0.0 {
            for percentage in optimal_distribution.values_mut() {
                *percentage = (*percentage / total_percentage) * 100.0;
            }
        }

        optimal_distribution
    }

    fn identify_tier_migration_candidates(&self) -> Vec<TierMigrationCandidate> {
        let mut candidates = Vec::new();

        // Identify entries that should be promoted or demoted based on access patterns
        for (key, metadata) in &self.global_stats {
            let current_tier = if metadata.access_pattern.access_frequency > 10.0 {
                "hot"
            } else if metadata.access_pattern.access_frequency > 1.0 {
                "warm"
            } else {
                "cold"
            };

            let recommended_tier = if metadata.access_pattern.access_frequency > 20.0 {
                "hot"
            } else if metadata.access_pattern.access_frequency > 5.0 {
                "warm"
            } else {
                "cold"
            };

            if current_tier != recommended_tier {
                let confidence_score = if metadata.access_pattern.access_count > 10 {
                    0.9
                } else if metadata.access_pattern.access_count > 5 {
                    0.7
                } else {
                    0.5
                };

                candidates.push(TierMigrationCandidate {
                    key: key.clone(),
                    current_tier: current_tier.to_string(),
                    recommended_tier: recommended_tier.to_string(),
                    confidence_score,
                    reasoning: format!(
                        "Access frequency: {:.1}, Access count: {}, Hit ratio: {:.2}",
                        metadata.access_pattern.access_frequency,
                        metadata.access_pattern.access_count,
                        metadata.performance_metrics.hit_ratio()
                    ),
                });
            }
        }

        // Sort by confidence score descending
        candidates.sort_by(|a, b| b.confidence_score.partial_cmp(&a.confidence_score).unwrap());
        candidates.truncate(20); // Limit to top 20 candidates
        candidates
    }

    fn calculate_tier_efficiency_scores(&self) -> HashMap<String, f64> {
        let mut efficiency_scores = HashMap::new();

        // Calculate efficiency for each service based on hit ratio and response time
        for (service_name, stats) in &self.service_stats {
            let hit_ratio_score = stats.hit_ratio;
            let frequency_score = if stats.avg_access_frequency > 0.0 {
                (stats.avg_access_frequency / 100.0).min(1.0)
            } else {
                0.0
            };

            // Combine hit ratio and frequency for overall efficiency
            let efficiency = (hit_ratio_score * 0.7) + (frequency_score * 0.3);
            efficiency_scores.insert(service_name.clone(), efficiency);
        }

        // Add data type efficiency scores
        for (data_type, stats) in &self.data_type_stats {
            let type_name = format!("{:?}", data_type);
            let compression_efficiency = stats.compression_ratio;
            let size_efficiency = if stats.total_size > 0 {
                1.0 - ((stats.total_size as f64 / (1024.0 * 1024.0)).min(1.0))
            } else {
                1.0
            };

            let efficiency = (compression_efficiency * 0.6) + (size_efficiency * 0.4);
            efficiency_scores.insert(format!("type_{}", type_name), efficiency);
        }

        efficiency_scores
    }

    fn calculate_access_trends(&self) -> Vec<TrendDataPoint> {
        let mut trends = Vec::new();
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate trend data points based on current statistics
        for (service_name, stats) in &self.service_stats {
            trends.push(TrendDataPoint {
                timestamp: current_timestamp,
                metric_name: format!("{}_hit_ratio", service_name),
                value: stats.hit_ratio,
            });

            trends.push(TrendDataPoint {
                timestamp: current_timestamp,
                metric_name: format!("{}_access_frequency", service_name),
                value: stats.avg_access_frequency,
            });

            trends.push(TrendDataPoint {
                timestamp: current_timestamp,
                metric_name: format!("{}_entry_count", service_name),
                value: stats.entry_count as f64,
            });
        }

        // Add overall system trends
        let total_entries = self
            .service_stats
            .values()
            .map(|s| s.entry_count)
            .sum::<u64>();
        let avg_hit_ratio = self.calculate_average_hit_rate();

        trends.push(TrendDataPoint {
            timestamp: current_timestamp,
            metric_name: "system_total_entries".to_string(),
            value: total_entries as f64,
        });

        trends.push(TrendDataPoint {
            timestamp: current_timestamp,
            metric_name: "system_avg_hit_ratio".to_string(),
            value: avg_hit_ratio,
        });

        trends
    }

    fn generate_performance_alerts(&self) -> Vec<PerformanceAlert> {
        let mut alerts = Vec::new();

        // Check for low hit ratios
        for (service_name, stats) in &self.service_stats {
            if stats.hit_ratio < 0.5 {
                alerts.push(PerformanceAlert {
                    alert_type: "Low Hit Ratio".to_string(),
                    severity: if stats.hit_ratio < 0.3 {
                        AlertSeverity::High
                    } else {
                        AlertSeverity::Medium
                    },
                    message: format!(
                        "Service {} has low hit ratio: {:.1}%",
                        service_name,
                        stats.hit_ratio * 100.0
                    ),
                    affected_keys: vec![service_name.clone()],
                    recommended_action: "Consider increasing cache TTL or pre-warming cache"
                        .to_string(),
                });
            }

            // Check for high access frequency without proportional hit ratio
            if stats.avg_access_frequency > 10.0 && stats.hit_ratio < 0.7 {
                alerts.push(PerformanceAlert {
                    alert_type: "Inefficient High-Frequency Access".to_string(),
                    severity: AlertSeverity::Medium,
                    message: format!(
                        "Service {} has high access frequency ({:.1}) but low hit ratio ({:.1}%)",
                        service_name,
                        stats.avg_access_frequency,
                        stats.hit_ratio * 100.0
                    ),
                    affected_keys: vec![service_name.clone()],
                    recommended_action: "Optimize caching strategy or increase cache size"
                        .to_string(),
                });
            }
        }

        // Check for memory pressure
        let memory_utilization = self.calculate_memory_utilization();
        if memory_utilization > 0.8 {
            alerts.push(PerformanceAlert {
                alert_type: "High Memory Utilization".to_string(),
                severity: if memory_utilization > 0.9 {
                    AlertSeverity::Critical
                } else {
                    AlertSeverity::High
                },
                message: format!("Memory utilization is {:.1}%", memory_utilization * 100.0),
                affected_keys: vec!["system".to_string()],
                recommended_action: "Consider cleanup or increasing cache capacity".to_string(),
            });
        }

        // Check for entries with poor performance
        for (key, metadata) in &self.global_stats {
            if metadata.performance_metrics.hit_ratio() < 0.3
                && metadata.access_pattern.access_count > 5
            {
                alerts.push(PerformanceAlert {
                    alert_type: "Poor Entry Performance".to_string(),
                    severity: AlertSeverity::Low,
                    message: format!(
                        "Entry {} has poor hit ratio: {:.1}%",
                        key,
                        metadata.performance_metrics.hit_ratio() * 100.0
                    ),
                    affected_keys: vec![key.clone()],
                    recommended_action: "Review caching strategy for this entry".to_string(),
                });
            }
        }

        alerts
    }

    fn detect_seasonal_patterns(&self) -> Vec<SeasonalPattern> {
        let mut patterns = Vec::new();

        // Analyze hourly access patterns across all entries
        let mut hourly_totals = [0u64; 24];
        let mut total_accesses = 0u64;

        for metadata in self.global_stats.values() {
            for (hour, count) in metadata
                .access_pattern
                .hourly_access_counts
                .iter()
                .enumerate()
            {
                hourly_totals[hour] += *count as u64;
                total_accesses += *count as u64;
            }
        }

        if total_accesses > 0 {
            // Find peak hours (above average + 1 standard deviation)
            let average = total_accesses as f64 / 24.0;
            let variance = hourly_totals
                .iter()
                .map(|&count| {
                    let diff = count as f64 - average;
                    diff * diff
                })
                .sum::<f64>()
                / 24.0;
            let std_dev = variance.sqrt();
            let threshold = average + std_dev;

            let peak_hours: Vec<usize> = hourly_totals
                .iter()
                .enumerate()
                .filter(|(_, &count)| count as f64 > threshold)
                .map(|(hour, _)| hour)
                .collect();

            if !peak_hours.is_empty() {
                patterns.push(SeasonalPattern {
                    pattern_type: "Daily Peak Hours".to_string(),
                    confidence: if peak_hours.len() <= 3 { 0.8 } else { 0.6 },
                    description: format!(
                        "Peak access hours detected: {}",
                        peak_hours
                            .iter()
                            .map(|h| format!("{}:00", h))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    affected_data_types: self
                        .data_type_stats
                        .keys()
                        .map(|dt| format!("{:?}", dt))
                        .collect(),
                });
            }

            // Detect burst patterns
            let burst_services: Vec<String> = self
                .global_stats
                .iter()
                .filter(|(_, metadata)| metadata.access_pattern.has_burst_pattern)
                .map(|(key, _)| key.clone())
                .take(5)
                .collect();

            if !burst_services.is_empty() {
                patterns.push(SeasonalPattern {
                    pattern_type: "Burst Access Pattern".to_string(),
                    confidence: 0.7,
                    description: format!(
                        "Burst access patterns detected in {} services",
                        burst_services.len()
                    ),
                    affected_data_types: vec!["Various".to_string()],
                });
            }

            // Detect sequential access patterns
            let sequential_count = self
                .global_stats
                .values()
                .filter(|metadata| metadata.access_pattern.is_sequential)
                .count();

            if sequential_count > 0 {
                patterns.push(SeasonalPattern {
                    pattern_type: "Sequential Access Pattern".to_string(),
                    confidence: 0.6,
                    description: format!(
                        "Sequential access patterns detected in {} entries",
                        sequential_count
                    ),
                    affected_data_types: vec!["MarketData".to_string(), "Historical".to_string()],
                });
            }
        }

        patterns
    }

    fn calculate_growth_projections(&self) -> GrowthProjection {
        GrowthProjection {
            projected_growth_rate: 0.15,
            projected_size_in_30_days: 1024 * 1024 * 100, // 100MB
            resource_recommendations: vec![
                "Consider increasing cache tier sizes".to_string(),
                "Monitor compression ratios".to_string(),
            ],
        }
    }

    fn calculate_enhanced_cleanup_score(
        &self,
        metadata: &CacheMetadata,
        space_pressure: f64,
    ) -> f64 {
        let base_score = metadata.cleanup_info.cleanup_score;
        let access_factor = if metadata.access_pattern.access_frequency < 1.0 {
            0.3
        } else {
            0.0
        };
        let pressure_factor = space_pressure * 0.2;

        (base_score + access_factor + pressure_factor).min(1.0)
    }

    fn generate_enhanced_cleanup_reasoning(
        &self,
        metadata: &CacheMetadata,
        space_pressure: f64,
    ) -> String {
        format!(
            "Enhanced cleanup: base_score={:.2}, space_pressure={:.2}, frequency={:.1}",
            metadata.cleanup_info.cleanup_score,
            space_pressure,
            metadata.access_pattern.access_frequency
        )
    }

    fn calculate_average_response_time(&self) -> f64 {
        let total_time = self
            .service_stats
            .values()
            .map(|s| s.avg_access_frequency * s.entry_count as f64)
            .sum::<f64>();
        let total_entries = self
            .service_stats
            .values()
            .map(|s| s.entry_count)
            .sum::<u64>() as f64;

        if total_entries > 0.0 {
            total_time / total_entries
        } else {
            10.0
        }
    }

    fn calculate_hit_rate_trend(&self) -> String {
        let avg_hit_rate = self.calculate_average_hit_rate();

        // Analyze hit rate distribution to determine trend
        let hit_rates: Vec<f64> = self.service_stats.values().map(|s| s.hit_ratio).collect();

        if hit_rates.is_empty() {
            return "stable".to_string();
        }

        let variance = hit_rates
            .iter()
            .map(|&rate| {
                let diff = rate - avg_hit_rate;
                diff * diff
            })
            .sum::<f64>()
            / hit_rates.len() as f64;

        let std_dev = variance.sqrt();

        // Determine trend based on variance and average
        if avg_hit_rate > 0.8 && std_dev < 0.1 {
            "excellent".to_string()
        } else if avg_hit_rate > 0.6 && std_dev < 0.2 {
            "improving".to_string()
        } else if avg_hit_rate < 0.4 || std_dev > 0.3 {
            "declining".to_string()
        } else {
            "stable".to_string()
        }
    }

    fn calculate_response_time_percentiles(&self) -> HashMap<String, f64> {
        let mut percentiles = HashMap::new();

        // Collect all response times from performance metrics
        let mut response_times: Vec<f64> = Vec::new();
        for metadata in self.global_stats.values() {
            if metadata.performance_metrics.total_operations > 0 {
                response_times.push(metadata.performance_metrics.avg_response_time_ms);
            }
        }

        if response_times.is_empty() {
            return percentiles;
        }

        // Sort response times for percentile calculation
        response_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let len = response_times.len();

        // Calculate common percentiles
        if len > 0 {
            percentiles.insert("p50".to_string(), response_times[len * 50 / 100]);
            percentiles.insert("p75".to_string(), response_times[len * 75 / 100]);
            percentiles.insert("p90".to_string(), response_times[len * 90 / 100]);
            percentiles.insert("p95".to_string(), response_times[len * 95 / 100]);
            percentiles.insert("p99".to_string(), response_times[len * 99 / 100]);
            percentiles.insert("min".to_string(), response_times[0]);
            percentiles.insert("max".to_string(), response_times[len - 1]);
        }

        percentiles
    }

    fn calculate_response_time_trend(&self) -> String {
        let avg_response_time = self.calculate_average_response_time();

        // Collect response times for trend analysis
        let response_times: Vec<f64> = self
            .global_stats
            .values()
            .filter(|metadata| metadata.performance_metrics.total_operations > 0)
            .map(|metadata| metadata.performance_metrics.avg_response_time_ms)
            .collect();

        if response_times.is_empty() {
            return "stable".to_string();
        }

        let variance = response_times
            .iter()
            .map(|&time| {
                let diff = time - avg_response_time;
                diff * diff
            })
            .sum::<f64>()
            / response_times.len() as f64;

        let std_dev = variance.sqrt();
        let coefficient_of_variation = if avg_response_time > 0.0 {
            std_dev / avg_response_time
        } else {
            0.0
        };

        // Determine trend based on average response time and variability
        if avg_response_time < 50.0 && coefficient_of_variation < 0.2 {
            "excellent".to_string()
        } else if avg_response_time < 100.0 && coefficient_of_variation < 0.3 {
            "good".to_string()
        } else if avg_response_time > 200.0 || coefficient_of_variation > 0.5 {
            "degrading".to_string()
        } else {
            "stable".to_string()
        }
    }

    fn calculate_memory_utilization(&self) -> f64 {
        // Calculate actual memory utilization based on total size
        let total_size: u64 = self.service_stats.values().map(|s| s.total_size).sum();

        // Assume 1GB total capacity for calculation (should be configurable)
        let total_capacity = 1024 * 1024 * 1024; // 1GB

        if total_capacity > 0 {
            (total_size as f64 / total_capacity as f64).min(1.0)
        } else {
            0.0
        }
    }

    fn calculate_memory_growth_rate(&self) -> f64 {
        // Calculate growth rate based on current statistics
        let _total_size: u64 = self.service_stats.values().map(|s| s.total_size).sum();
        let total_entries: u64 = self.service_stats.values().map(|s| s.entry_count).sum();

        if total_entries == 0 {
            return 0.0;
        }

        // Estimate growth rate based on access patterns and entry creation
        let high_frequency_entries = self
            .global_stats
            .values()
            .filter(|metadata| metadata.access_pattern.access_frequency > 5.0)
            .count() as f64;

        let total_metadata_entries = self.global_stats.len() as f64;

        if total_metadata_entries == 0.0 {
            return 0.0;
        }

        // Growth rate estimation: higher frequency entries suggest more active usage
        let activity_factor = high_frequency_entries / total_metadata_entries;

        // Base growth rate of 5% with activity multiplier
        let base_growth_rate = 0.05;
        let growth_rate = base_growth_rate * (1.0 + activity_factor);

        // Cap growth rate at 50% to be realistic
        growth_rate.min(0.5)
    }

    fn calculate_projected_full_days(&self) -> u64 {
        let current_utilization = self.calculate_memory_utilization();
        let growth_rate = self.calculate_memory_growth_rate();

        if growth_rate <= 0.0 || current_utilization >= 1.0 {
            return 0; // Already full or no growth
        }

        // Calculate days until full capacity (assuming 1.0 = 100% utilization)
        let remaining_capacity = 1.0 - current_utilization;

        // Using compound growth formula: remaining_capacity = current * (1 + growth_rate)^days
        // Solving for days: days = ln(remaining_capacity / current) / ln(1 + growth_rate)
        if current_utilization > 0.0 {
            let days_to_full =
                (remaining_capacity / current_utilization).ln() / (1.0 + growth_rate).ln();

            // Return reasonable projection (max 365 days)
            if days_to_full > 0.0 && days_to_full.is_finite() {
                (days_to_full as u64).min(365)
            } else {
                365 // Default to 1 year if calculation is invalid
            }
        } else {
            365 // Default if no current utilization data
        }
    }

    fn count_hot_spots(&self) -> usize {
        self.global_stats
            .values()
            .filter(|m| m.access_pattern.access_frequency > 20.0)
            .count()
    }

    fn get_top_hot_spots(&self) -> Vec<String> {
        self.global_stats
            .iter()
            .filter(|(_, m)| m.access_pattern.access_frequency > 15.0)
            .map(|(key, _)| key.clone())
            .take(5)
            .collect()
    }

    fn identify_optimization_opportunities(&self) -> Vec<String> {
        vec![
            "Pre-warm frequently accessed data".to_string(),
            "Optimize compression for large entries".to_string(),
        ]
    }
}

impl Default for MetadataTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Service-specific cache statistics
#[derive(Debug, Clone, Default)]
pub struct ServiceStats {
    pub entry_count: u64,
    pub total_size: u64,
    pub avg_access_frequency: f64,
    pub hit_ratio: f64,
}

/// Data type-specific cache statistics
#[derive(Debug, Clone, Default)]
pub struct DataTypeStats {
    pub entry_count: u64,
    pub total_size: u64,
    pub avg_ttl: Duration,
    pub compression_ratio: f64,
}

/// Enhanced analytics and reporting for metadata tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheAnalyticsReport {
    /// Overall cache health score (0.0 to 1.0)
    pub health_score: f64,
    /// Performance analysis
    pub performance_analysis: PerformanceAnalysis,
    /// Cleanup recommendations
    pub cleanup_recommendations: CleanupRecommendations,
    /// Tier utilization insights
    pub tier_insights: TierInsights,
    /// Top performing and problematic entries
    pub top_entries: TopEntriesAnalysis,
    /// Trending patterns
    pub trends: TrendAnalysis,
    /// Generated timestamp
    pub generated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    /// Overall hit rate across all data types
    pub overall_hit_rate: f64,
    /// Average response time breakdown by tier
    pub response_times_by_tier: HashMap<String, f64>,
    /// Data types with performance issues
    pub underperforming_types: Vec<String>,
    /// Compression effectiveness by data type
    pub compression_effectiveness: HashMap<String, f64>,
    /// Hot path performance metrics
    pub hot_path_metrics: HotPathMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupRecommendations {
    /// Immediate cleanup candidates
    pub immediate_cleanup: Vec<CleanupCandidate>,
    /// Entries that could be cleaned if space needed
    pub conditional_cleanup: Vec<CleanupCandidate>,
    /// Total potential space savings
    pub potential_space_savings_bytes: u64,
    /// Recommended cleanup frequency by data type
    pub cleanup_frequency_recommendations: HashMap<String, Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupCandidate {
    pub key: String,
    pub score: f64,
    pub size_bytes: u64,
    pub last_access_hours_ago: u64,
    pub regeneration_cost: RegenerationCost,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierInsights {
    /// Optimal tier distribution recommendation
    pub recommended_distribution: HashMap<String, f64>,
    /// Current distribution
    pub current_distribution: HashMap<String, f64>,
    /// Promotion/demotion recommendations
    pub tier_migration_candidates: Vec<TierMigrationCandidate>,
    /// Tier efficiency scores
    pub tier_efficiency: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierMigrationCandidate {
    pub key: String,
    pub current_tier: String,
    pub recommended_tier: String,
    pub confidence_score: f64,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopEntriesAnalysis {
    /// Most frequently accessed entries
    pub hot_entries: Vec<EntryPerformance>,
    /// Entries consuming most space
    pub space_heavy_entries: Vec<EntryPerformance>,
    /// Entries with best compression ratios
    pub compression_champions: Vec<EntryPerformance>,
    /// Problematic entries (low hit rate, high cost)
    pub problematic_entries: Vec<EntryPerformance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPerformance {
    pub key: String,
    pub metric_value: f64,
    pub context: String,
    pub recommendation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    /// Access pattern trends over time
    pub access_trends: Vec<TrendDataPoint>,
    /// Performance degradation alerts
    pub performance_alerts: Vec<PerformanceAlert>,
    /// Seasonal patterns detected
    pub seasonal_patterns: Vec<SeasonalPattern>,
    /// Growth projections
    pub growth_projections: GrowthProjection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendDataPoint {
    pub timestamp: u64,
    pub metric_name: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAlert {
    pub alert_type: String,
    pub severity: AlertSeverity,
    pub message: String,
    pub affected_keys: Vec<String>,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonalPattern {
    pub pattern_type: String,
    pub confidence: f64,
    pub description: String,
    pub affected_data_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthProjection {
    pub projected_growth_rate: f64,
    pub projected_size_in_30_days: u64,
    pub resource_recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotPathMetrics {
    /// Critical path response times
    pub critical_path_response_ms: f64,
    /// Bottleneck identification
    pub bottlenecks: Vec<String>,
    /// Optimization opportunities
    pub optimization_opportunities: Vec<String>,
}
