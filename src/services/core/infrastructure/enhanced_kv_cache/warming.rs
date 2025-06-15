//! Cache Warming Service
//!
//! Provides predictive cache warming and preloading strategies

use super::metadata::{AccessPattern, DataType};
use crate::utils::now_system_time;
use std::collections::{HashMap, VecDeque};
use std::time::UNIX_EPOCH;

/// Cache warming service for predictive preloading
pub struct CacheWarmingService {
    /// Configuration
    config: WarmingConfig,
    /// Warming queue with priorities
    warming_queue: VecDeque<WarmingRequest>,
    /// Usage pattern analysis
    pattern_analyzer: PatternAnalyzer,
    /// Warming statistics
    stats: WarmingStats,
}

impl CacheWarmingService {
    /// Create a new cache warming service
    pub fn new(config: WarmingConfig) -> Self {
        Self {
            config,
            warming_queue: VecDeque::new(),
            pattern_analyzer: PatternAnalyzer::new(),
            stats: WarmingStats::default(),
        }
    }

    /// Analyze access patterns and add warming requests
    pub fn analyze_and_queue_warming(
        &mut self,
        key: &str,
        data_type: &DataType,
        access_pattern: &AccessPattern,
    ) {
        if !self.config.enabled {
            return;
        }

        // Update pattern analysis
        self.pattern_analyzer
            .record_access(key, data_type, access_pattern);

        // Generate warming predictions
        if let Some(prediction) = self.pattern_analyzer.predict_warming_need(key, data_type) {
            let request = WarmingRequest {
                key: key.to_string(),
                data_type: data_type.clone(),
                priority: prediction.priority,
                predicted_access_time: prediction.predicted_access_time,
                confidence: prediction.confidence,
                created_at: now_system_time()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            self.add_warming_request(request);
        }
    }

    /// Add a warming request to the queue
    pub fn add_warming_request(&mut self, request: WarmingRequest) {
        // Insert in priority order
        let insert_position = self
            .warming_queue
            .iter()
            .position(|r| r.priority < request.priority)
            .unwrap_or(self.warming_queue.len());

        self.warming_queue.insert(insert_position, request);

        // Limit queue size
        if self.warming_queue.len() > self.config.max_queue_size {
            self.warming_queue.pop_back();
        }
    }

    /// Get next warming requests (up to batch size)
    pub fn get_next_warming_batch(&mut self) -> Vec<WarmingRequest> {
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut batch = Vec::new();

        // Rate limiting check
        if !self.can_perform_warming(now) {
            return batch;
        }

        // Take up to batch_size requests
        for _ in 0..self.config.batch_size {
            if let Some(request) = self.warming_queue.pop_front() {
                // Check if warming is still needed
                if self.should_warm_now(&request, now) {
                    batch.push(request);
                }
            } else {
                break;
            }
        }

        batch
    }

    /// Check if warming can be performed (rate limiting)
    fn can_perform_warming(&self, now: u64) -> bool {
        // Simple rate limiting implementation
        let window_start = now - 60; // 1 minute window
        let recent_warmings = self
            .stats
            .warming_operations
            .iter()
            .filter(|&&time| time >= window_start)
            .count();

        recent_warmings < self.config.max_warming_per_minute
    }

    /// Check if warming should be performed now
    fn should_warm_now(&self, request: &WarmingRequest, now: u64) -> bool {
        // Check if predicted time is approaching
        let time_until_predicted = request.predicted_access_time.saturating_sub(now);
        let warming_window = self.config.warming_window_seconds;

        // Warm if predicted access is within warming window
        time_until_predicted <= warming_window && request.confidence >= self.config.min_confidence
    }

    /// Record successful warming operation
    pub fn record_warming_success(&mut self, key: &str) {
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.stats.successful_warmings += 1;
        self.stats.warming_operations.push(now);

        // Update pattern analyzer
        self.pattern_analyzer.record_warming_success(key);
    }

    /// Record failed warming operation
    pub fn record_warming_failure(&mut self, key: &str, error: &str) {
        self.stats.failed_warmings += 1;
        self.pattern_analyzer.record_warming_failure(key, error);
    }

    /// Record warming hit (cache was accessed after warming)
    pub fn record_warming_hit(&mut self, key: &str) {
        self.stats.warming_hits += 1;
        self.pattern_analyzer.record_warming_hit(key);
    }

    /// Get warming effectiveness ratio
    pub fn warming_effectiveness(&self) -> f64 {
        if self.stats.successful_warmings == 0 {
            0.0
        } else {
            self.stats.warming_hits as f64 / self.stats.successful_warmings as f64
        }
    }

    /// Get warming statistics
    pub fn get_stats(&self) -> &WarmingStats {
        &self.stats
    }

    /// Clear old warming operations from stats
    pub fn cleanup_old_stats(&mut self, max_age_seconds: u64) {
        let cutoff = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(max_age_seconds);

        self.stats.warming_operations.retain(|&time| time >= cutoff);
    }
}

/// Warming request with priority and prediction data
#[derive(Debug, Clone)]
pub struct WarmingRequest {
    /// Cache key to warm
    pub key: String,
    /// Data type
    pub data_type: DataType,
    /// Warming priority (higher = more important)
    pub priority: u8,
    /// Predicted access time (Unix timestamp)
    pub predicted_access_time: u64,
    /// Confidence in prediction (0.0-1.0)
    pub confidence: f64,
    /// Request creation time
    pub created_at: u64,
}

/// Pattern analyzer for predicting cache warming needs
struct PatternAnalyzer {
    /// Access history per key
    access_history: HashMap<String, Vec<AccessRecord>>,
    /// Data type access patterns
    data_type_patterns: HashMap<DataType, DataTypePattern>,
    /// Warming success tracking
    warming_success: HashMap<String, WarmingTracking>,
}

impl PatternAnalyzer {
    fn new() -> Self {
        Self {
            access_history: HashMap::new(),
            data_type_patterns: HashMap::new(),
            warming_success: HashMap::new(),
        }
    }

    fn record_access(&mut self, key: &str, data_type: &DataType, access_pattern: &AccessPattern) {
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Record in access history
        let history = self.access_history.entry(key.to_string()).or_default();
        history.push(AccessRecord {
            timestamp: now,
            access_count: access_pattern.access_count,
            frequency: access_pattern.access_frequency,
        });

        // Limit history size
        if history.len() > 100 {
            history.remove(0);
        }

        // Update data type patterns
        let dt_pattern = self
            .data_type_patterns
            .entry(data_type.clone())
            .or_default();
        dt_pattern.total_accesses += 1;
        dt_pattern.avg_frequency =
            (dt_pattern.avg_frequency + access_pattern.access_frequency) / 2.0;
    }

    fn predict_warming_need(&self, key: &str, data_type: &DataType) -> Option<WarmingPrediction> {
        let history = self.access_history.get(key)?;
        if history.len() < 2 {
            return None;
        }

        // Analyze access intervals
        let mut intervals = Vec::new();
        for window in history.windows(2) {
            let interval = window[1].timestamp - window[0].timestamp;
            intervals.push(interval);
        }

        if intervals.is_empty() {
            return None;
        }

        // Calculate average interval
        let avg_interval = intervals.iter().sum::<u64>() / intervals.len() as u64;

        // Predict next access time
        let last_access = history.last()?.timestamp;
        let predicted_time = last_access + avg_interval;

        // Calculate confidence based on interval consistency
        let interval_variance = self.calculate_variance(&intervals, avg_interval);
        let confidence = (1.0 / (1.0 + interval_variance)).clamp(0.0, 1.0);

        // Determine priority based on data type and frequency
        let dt_pattern = self.data_type_patterns.get(data_type)?;
        let priority = self.calculate_priority(data_type, dt_pattern, confidence);

        Some(WarmingPrediction {
            predicted_access_time: predicted_time,
            confidence,
            priority,
        })
    }

    fn calculate_variance(&self, intervals: &[u64], avg: u64) -> f64 {
        if intervals.len() <= 1 {
            return 0.0;
        }

        let variance: f64 = intervals
            .iter()
            .map(|&x| {
                let diff = x as f64 - avg as f64;
                diff * diff
            })
            .sum::<f64>()
            / intervals.len() as f64;

        variance.sqrt() / avg as f64 // Normalize by average
    }

    fn calculate_priority(
        &self,
        data_type: &DataType,
        pattern: &DataTypePattern,
        confidence: f64,
    ) -> u8 {
        let base_priority = match data_type {
            DataType::MarketData => 10, // Highest priority for real-time market data
            DataType::FundingRate => 8, // High priority for funding rates
            DataType::Opportunities => 9, // Very high priority for trading opportunities
            DataType::UserProfile => 7, // High priority for user data
            DataType::Session => 6,     // Medium-high priority for session data
            DataType::Configuration => 4, // Medium priority for configuration
            DataType::AiResponse => 5,  // Medium priority for AI responses
            DataType::Analytics => 3,   // Lower priority for analytics
            DataType::Historical => 2,  // Low priority for historical data
            DataType::Generic => 1,
        };

        // Adjust based on frequency and confidence
        let frequency_bonus = if pattern.avg_frequency > 2.0 { 1 } else { 0 };
        let confidence_bonus = if confidence > 0.8 { 1 } else { 0 };

        (base_priority + frequency_bonus + confidence_bonus).min(10)
    }

    fn record_warming_success(&mut self, key: &str) {
        let tracking = self.warming_success.entry(key.to_string()).or_default();
        tracking.successful_warmings += 1;
    }

    fn record_warming_failure(&mut self, key: &str, _error: &str) {
        let tracking = self.warming_success.entry(key.to_string()).or_default();
        tracking.failed_warmings += 1;
    }

    fn record_warming_hit(&mut self, key: &str) {
        let tracking = self.warming_success.entry(key.to_string()).or_default();
        tracking.warming_hits += 1;
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct AccessRecord {
    timestamp: u64,
    access_count: u32,
    frequency: f64,
}

#[derive(Debug, Clone, Default)]
struct DataTypePattern {
    total_accesses: u64,
    avg_frequency: f64,
}

#[derive(Debug, Clone, Default)]
struct WarmingTracking {
    successful_warmings: u32,
    failed_warmings: u32,
    warming_hits: u32,
}

#[derive(Debug, Clone)]
struct WarmingPrediction {
    predicted_access_time: u64,
    confidence: f64,
    priority: u8,
}

/// Cache warming configuration
#[derive(Debug, Clone)]
pub struct WarmingConfig {
    /// Enable warming
    pub enabled: bool,
    /// Maximum items in warming queue
    pub max_queue_size: usize,
    /// Batch size for warming operations
    pub batch_size: usize,
    /// Maximum warming operations per minute
    pub max_warming_per_minute: usize,
    /// Warming window in seconds (how far ahead to warm)
    pub warming_window_seconds: u64,
    /// Minimum confidence required for warming
    pub min_confidence: f64,
}

impl Default for WarmingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_queue_size: 1000,
            batch_size: 10,
            max_warming_per_minute: 100,
            warming_window_seconds: 300, // 5 minutes
            min_confidence: 0.5,
        }
    }
}

/// Warming statistics
#[derive(Debug, Clone, Default)]
pub struct WarmingStats {
    /// Total successful warming operations
    pub successful_warmings: u32,
    /// Total failed warming operations
    pub failed_warmings: u32,
    /// Warming hits (warmed data was accessed)
    pub warming_hits: u32,
    /// Timestamps of recent warming operations
    pub warming_operations: Vec<u64>,
}

impl WarmingStats {
    /// Get warming success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_warmings + self.failed_warmings;
        if total == 0 {
            0.0
        } else {
            self.successful_warmings as f64 / total as f64
        }
    }

    /// Get warming hit rate
    pub fn hit_rate(&self) -> f64 {
        if self.successful_warmings == 0 {
            0.0
        } else {
            self.warming_hits as f64 / self.successful_warmings as f64
        }
    }
}
