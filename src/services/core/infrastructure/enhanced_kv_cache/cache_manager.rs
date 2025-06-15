// Core KV Cache Manager - Main orchestrator for enhanced KV cache operations
// Compatible with existing DataCoordinator and cache_layer.rs architecture

use crate::utils::error::{ArbitrageError, ArbitrageResult};
use crate::utils::time::WasmInstant;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

use super::{
    compression::CompressionEngine,
    config::CacheConfig as EnhancedCacheConfig,
    metadata::{DataType, MetadataTracker},
    warming::CacheWarmingService,
    CacheEntry, CacheOperation, CacheTier, EnhancedCacheStats,
};

// Optional metrics collector integration
use crate::services::core::infrastructure::monitoring_module::metrics_collector::{
    CacheOperation as MonitoringCacheOperation, MetricsCollector,
};

/// Enhanced cache metrics compatible with DataAccessLayer health monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManagerMetrics {
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub tier_statistics: HashMap<CacheTier, TierStats>,
    pub compression_stats: CompressionStats,
    pub warming_stats: WarmingStats,
    pub overall_hit_rate_percent: f32,
    pub average_latency_ms: f64,
    pub memory_usage_bytes: u64,
    pub last_updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierStats {
    pub hits: u64,
    pub misses: u64,
    pub promotions: u64,
    pub demotions: u64,
    pub evictions: u64,
    pub size_bytes: u64,
    pub entry_count: u64,
    pub average_access_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStats {
    pub compressed_entries: u64,
    pub total_original_bytes: u64,
    pub total_compressed_bytes: u64,
    pub compression_ratio: f32,
    pub compression_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmingStats {
    pub warming_operations: u64,
    pub successful_warming: u64,
    pub prediction_accuracy_percent: f32,
    pub average_warm_time_ms: f64,
}

impl Default for CacheManagerMetrics {
    fn default() -> Self {
        Self {
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            tier_statistics: HashMap::new(),
            compression_stats: CompressionStats {
                compressed_entries: 0,
                total_original_bytes: 0,
                total_compressed_bytes: 0,
                compression_ratio: 1.0,
                compression_time_ms: 0.0,
            },
            warming_stats: WarmingStats {
                warming_operations: 0,
                successful_warming: 0,
                prediction_accuracy_percent: 0.0,
                average_warm_time_ms: 0.0,
            },
            overall_hit_rate_percent: 0.0,
            average_latency_ms: 0.0,
            memory_usage_bytes: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Batch operation for efficient multi-key operations
#[derive(Debug, Clone)]
pub struct BatchOperation {
    pub operation_type: CacheOperation,
    pub key: String,
    pub value: Option<String>,
    pub data_type: DataType,
    pub tier: Option<CacheTier>,
    pub ttl_seconds: Option<u64>,
}

/// Result of a batch operation
#[derive(Debug, Clone)]
pub struct BatchResult {
    pub key: String,
    pub success: bool,
    pub value: Option<String>,
    pub error: Option<String>,
    pub tier_used: Option<CacheTier>,
    pub was_compressed: bool,
}

/// Cache source for tracking where data came from
#[derive(Debug, Clone, PartialEq)]
pub enum CacheSource {
    Memory,   // KV Cache
    Pipeline, // Pipeline/R2 data
    Api,      // Direct API call
}

/// Enhanced cache result with source tracking
#[derive(Debug, Clone)]
pub struct CacheResult<T> {
    pub data: Option<T>,
    pub hit: bool,
    pub source: CacheSource,
    pub latency_ms: f64,
    pub compression_ratio: Option<f64>,
}

/// Core KV Cache Manager - orchestrates all enhanced cache operations
pub struct KvCacheManager {
    config: EnhancedCacheConfig,
    logger: crate::utils::logger::Logger,

    // Component instances
    metadata_tracker: Arc<std::sync::Mutex<MetadataTracker>>,
    compression_engine: Arc<CompressionEngine>,
    warming_service: Arc<std::sync::Mutex<CacheWarmingService>>,

    // Performance and health tracking
    metrics: Arc<std::sync::Mutex<CacheManagerMetrics>>,
    #[allow(dead_code)]
    cache_stats: Arc<std::sync::Mutex<EnhancedCacheStats>>,

    // Optional monitoring integration
    metrics_collector: Option<Arc<MetricsCollector>>,

    // Startup time for metrics
    startup_time: u64,
}

impl KvCacheManager {
    /// Create new KvCacheManager instance
    pub fn new(config: EnhancedCacheConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        if let Err(e) = config.validate() {
            return Err(ArbitrageError::config_error(e));
        }

        // Initialize components
        let metadata_tracker = Arc::new(std::sync::Mutex::new(MetadataTracker::new()));
        let compression_engine = Arc::new(CompressionEngine::new(config.compression.clone()));
        // Convert config warming to warming service config
        let warming_config = super::warming::WarmingConfig {
            enabled: config.warming.enabled,
            max_queue_size: config.warming.priority_queue_size,
            batch_size: config.warming.batch_size,
            max_warming_per_minute: config.warming.max_warming_rate as usize,
            warming_window_seconds: (config.warming.prediction_history_hours * 3600) as u64,
            min_confidence: config.warming.min_access_frequency,
        };
        let warming_service = Arc::new(std::sync::Mutex::new(CacheWarmingService::new(
            warming_config,
        )));

        // Initialize metrics with tier statistics
        let mut tier_statistics = HashMap::new();
        for tier in [CacheTier::Hot, CacheTier::Warm, CacheTier::Cold] {
            tier_statistics.insert(
                tier,
                TierStats {
                    hits: 0,
                    misses: 0,
                    promotions: 0,
                    demotions: 0,
                    evictions: 0,
                    size_bytes: 0,
                    entry_count: 0,
                    average_access_time_ms: 0.0,
                },
            );
        }

        let initial_metrics = CacheManagerMetrics {
            tier_statistics,
            ..Default::default()
        };

        logger.info(&format!(
            "KvCacheManager initialized: tiers={}, compression={}, warming={}",
            3, // Hot, Warm, Cold tiers
            config.compression.enabled,
            config.warming.enabled
        ));

        Ok(Self {
            config,
            logger,
            metadata_tracker,
            compression_engine,
            warming_service,
            metrics: Arc::new(std::sync::Mutex::new(initial_metrics)),
            cache_stats: Arc::new(std::sync::Mutex::new(EnhancedCacheStats::default())),
            metrics_collector: None,
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Set optional metrics collector for monitoring integration
    pub fn with_metrics_collector(mut self, metrics_collector: Arc<MetricsCollector>) -> Self {
        self.metrics_collector = Some(metrics_collector);
        self
    }

    /// Record cache operation metrics if collector is available
    async fn record_cache_metrics(
        &self,
        operation: MonitoringCacheOperation,
        cache_type: &str,
        endpoint: &str,
        was_hit: bool,
        response_time_ms: u64,
        cache_size_bytes: Option<u64>,
    ) {
        if let Some(ref collector) = self.metrics_collector {
            if let Err(e) = collector
                .collect_cache_metric(
                    operation,
                    cache_type,
                    endpoint,
                    was_hit,
                    response_time_ms,
                    cache_size_bytes,
                )
                .await
            {
                self.logger
                    .warn(&format!("Failed to record cache metrics: {:?}", e));
            }
        }
    }

    /// Get value from cache with automatic tier management
    /// Compatible with DataCoordinator's cache operations
    pub async fn get(&self, kv_store: &KvStore, key: &str) -> ArbitrageResult<Option<String>> {
        let start_time = crate::utils::time::now_instant();

        // Try to determine data type from key pattern
        let data_type = self.infer_data_type(key);

        // Record access for warming analysis
        if self.config.warming.enabled {
            self.record_access_for_warming(key, &data_type).await;
        }

        // Get from cache with tier routing
        match self.get_with_tier(kv_store, key, None, &data_type).await {
            Ok(Some(cache_entry)) => {
                // Record warming hit if this was a warmed entry
                if self.config.warming.enabled {
                    self.check_and_record_warming_hit(key).await;
                }

                self.record_hit(&data_type, cache_entry.tier, start_time)
                    .await;

                // Record cache metrics
                let response_time_ms = start_time.elapsed().as_millis() as u64;
                self.record_cache_metrics(
                    MonitoringCacheOperation::Get,
                    &data_type.to_string(),
                    key,
                    true,
                    response_time_ms,
                    Some(cache_entry.value.len() as u64),
                )
                .await;

                Ok(Some(cache_entry.value))
            }
            Ok(None) => {
                // Trigger predictive warming for cache misses
                if self.config.warming.enabled && self.config.warming.predictive_warming {
                    self.trigger_predictive_warming(key, &data_type).await;
                }

                self.record_miss(&data_type, start_time).await;

                // Record cache miss metrics
                let response_time_ms = start_time.elapsed().as_millis() as u64;
                self.record_cache_metrics(
                    MonitoringCacheOperation::Get,
                    &data_type.to_string(),
                    key,
                    false,
                    response_time_ms,
                    None,
                )
                .await;

                Ok(None)
            }
            Err(e) => {
                self.record_error(&data_type, &e, start_time).await;
                Err(e)
            }
        }
    }

    /// Put value into cache with automatic tier assignment
    /// Compatible with DataCoordinator's cache operations
    pub async fn put(
        &self,
        kv_store: &KvStore,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> ArbitrageResult<()> {
        let start_time = crate::utils::time::now_instant();

        // Determine data type and optimal tier
        let data_type = self.infer_data_type(key);
        let tier = self.determine_optimal_tier(key, &data_type).await;

        match self
            .put_with_tier(kv_store, key, value, tier, &data_type, ttl)
            .await
        {
            Ok(()) => {
                self.record_successful_put(&data_type, tier, start_time)
                    .await;

                // Record cache set metrics
                let response_time_ms = start_time.elapsed().as_millis() as u64;
                self.record_cache_metrics(
                    MonitoringCacheOperation::Set,
                    &data_type.to_string(),
                    key,
                    true, // Put operations are always successful when they reach here
                    response_time_ms,
                    Some(value.len() as u64),
                )
                .await;

                Ok(())
            }
            Err(e) => {
                self.record_error(&data_type, &e, start_time).await;
                Err(e)
            }
        }
    }

    /// Get value with explicit tier preference and enhanced metadata
    #[allow(clippy::await_holding_lock)]
    pub async fn get_with_tier(
        &self,
        kv_store: &KvStore,
        key: &str,
        preferred_tier: Option<CacheTier>,
        data_type: &DataType,
    ) -> ArbitrageResult<Option<CacheEntry>> {
        // Determine search order
        let search_tiers = if let Some(tier) = preferred_tier {
            vec![tier]
        } else {
            // Search hot first, then warm, then cold
            vec![CacheTier::Hot, CacheTier::Warm, CacheTier::Cold]
        };

        for tier in search_tiers {
            let tier_key = self.build_tier_key(key, tier);

            match kv_store.get(&tier_key).text().await {
                Ok(Some(data)) => {
                    match self.deserialize_cache_entry(&data) {
                        Ok(mut entry) => {
                            // Check if entry is expired
                            if entry.is_expired() {
                                // Clean up expired entry
                                let _ = kv_store.delete(&tier_key).await;
                                continue;
                            }

                            // Record access and update metadata
                            // Record access and update metadata
                            if let Ok(mut tracker) = self.metadata_tracker.lock() {
                                tracker.record_access(key, data_type, &tier).await;
                            }
                            entry.access_count += 1;
                            entry.last_accessed = chrono::Utc::now().timestamp_millis() as u64;

                            // Update entry with new access data
                            if let Ok(updated_data) = self.serialize_cache_entry(&entry) {
                                let _ = self
                                    .put_raw_to_tier(
                                        kv_store,
                                        key,
                                        &updated_data,
                                        tier,
                                        entry.ttl_seconds,
                                    )
                                    .await;
                            }

                            // Check for tier promotion opportunity
                            if self.should_promote(&entry, tier).await {
                                let _ = self.promote_entry(kv_store, key, &entry, tier).await;
                            }

                            return Ok(Some(entry));
                        }
                        Err(_) => {
                            // Corrupted entry, clean up
                            let _ = kv_store.delete(&tier_key).await;
                            continue;
                        }
                    }
                }
                Ok(None) => continue,
                Err(_) => continue,
            }
        }

        Ok(None)
    }

    /// Put value with explicit tier assignment
    #[allow(clippy::await_holding_lock)]
    pub async fn put_with_tier(
        &self,
        kv_store: &KvStore,
        key: &str,
        value: &str,
        tier: CacheTier,
        data_type: &DataType,
        ttl: Option<u64>,
    ) -> ArbitrageResult<()> {
        // Get tier configuration based on tier
        let tier_config = self.config.get_tier_settings(tier);

        let ttl_seconds = ttl.unwrap_or(tier_config.default_ttl.as_secs());

        // Check if compression is needed
        let (final_value, was_compressed) = if self.config.compression.enabled
            && value.len() >= self.config.compression.size_threshold_bytes
        {
            match self
                .compression_engine
                .compress_with_metadata(value.as_bytes())
                .await
            {
                Ok(compressed_result) => {
                    self.logger.debug(&format!(
                        "Compressed cache entry for key '{}': {} -> {} bytes ({}% saving)",
                        key,
                        compressed_result.original_size,
                        compressed_result.compressed_size,
                        compressed_result.space_saved_percent()
                    ));
                    (
                        String::from_utf8_lossy(&compressed_result.data).to_string(),
                        true,
                    )
                }
                Err(e) => {
                    self.logger
                        .warn(&format!("Compression failed for key '{}': {}", key, e));
                    (value.to_string(), false)
                }
            }
        } else {
            (value.to_string(), false)
        };

        // Create cache entry
        let cache_entry = CacheEntry {
            key: key.to_string(),
            value: final_value,
            tier,
            data_type: data_type.clone(),
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            expires_at: chrono::Utc::now().timestamp_millis() as u64 + (ttl_seconds * 1000),
            last_accessed: chrono::Utc::now().timestamp_millis() as u64,
            access_count: 0,
            size_bytes: value.len() as u64,
            compressed: was_compressed,
            ttl_seconds,
        };

        // Serialize and store
        let serialized_entry = self.serialize_cache_entry(&cache_entry)?;
        self.put_raw_to_tier(kv_store, key, &serialized_entry, tier, ttl_seconds)
            .await?;

        // Update metadata
        if let Ok(mut tracker) = self.metadata_tracker.lock() {
            tracker
                .record_storage(key, data_type, &tier, cache_entry.size_bytes as usize)
                .await;
        }

        self.logger.debug(&format!(
            "Stored cache entry: key='{}', tier={:?}, size={} bytes, compressed={}, ttl={}s",
            key, tier, cache_entry.size_bytes, was_compressed, ttl_seconds
        ));

        Ok(())
    }

    /// Delete key from all tiers
    pub async fn delete(&self, kv_store: &KvStore, key: &str) -> ArbitrageResult<()> {
        let mut deleted_count = 0;

        for tier in [CacheTier::Hot, CacheTier::Warm, CacheTier::Cold] {
            let tier_key = self.build_tier_key(key, tier);
            match kv_store.delete(&tier_key).await {
                Ok(_) => deleted_count += 1,
                Err(e) => {
                    self.logger.warn(&format!(
                        "Failed to delete key '{}' from tier {:?}: {:?}",
                        key, tier, e
                    ));
                }
            }
        }

        if deleted_count > 0 {
            self.logger.debug(&format!(
                "Deleted key '{}' from {} tiers",
                key, deleted_count
            ));
        }

        Ok(())
    }

    /// Check if key exists in any tier
    pub async fn exists(&self, kv_store: &KvStore, key: &str) -> ArbitrageResult<bool> {
        for tier in [CacheTier::Hot, CacheTier::Warm, CacheTier::Cold] {
            let tier_key = self.build_tier_key(key, tier);
            match kv_store.get(&tier_key).text().await {
                Ok(Some(_)) => return Ok(true),
                Ok(None) => continue,
                Err(_) => continue,
            }
        }
        Ok(false)
    }

    /// Batch get operations for efficiency
    pub async fn batch_get(
        &self,
        kv_store: &KvStore,
        keys: &[&str],
    ) -> ArbitrageResult<HashMap<String, Option<String>>> {
        let mut results = HashMap::new();

        // Process in chunks to avoid overwhelming the KV store
        let chunk_size = 50;
        for chunk in keys.chunks(chunk_size) {
            for key in chunk {
                let result = self.get(kv_store, key).await?;
                results.insert(key.to_string(), result);
            }
        }

        Ok(results)
    }

    /// Batch put operations for efficiency
    pub async fn batch_put(
        &self,
        kv_store: &KvStore,
        operations: &[BatchOperation],
    ) -> ArbitrageResult<Vec<BatchResult>> {
        let mut results = Vec::new();

        for operation in operations {
            let result = match &operation.value {
                Some(value) => {
                    let tier = operation.tier.unwrap_or_else(|| {
                        // Use blocking call since we're in async context
                        futures::executor::block_on(async {
                            self.determine_optimal_tier(&operation.key, &operation.data_type)
                                .await
                        })
                    });

                    match self
                        .put_with_tier(
                            kv_store,
                            &operation.key,
                            value,
                            tier,
                            &operation.data_type,
                            operation.ttl_seconds,
                        )
                        .await
                    {
                        Ok(()) => BatchResult {
                            key: operation.key.clone(),
                            success: true,
                            value: Some(value.clone()),
                            error: None,
                            tier_used: Some(tier),
                            was_compressed: value.len()
                                >= self.config.compression.size_threshold_bytes,
                        },
                        Err(e) => BatchResult {
                            key: operation.key.clone(),
                            success: false,
                            value: None,
                            error: Some(e.to_string()),
                            tier_used: None,
                            was_compressed: false,
                        },
                    }
                }
                None => {
                    // Delete operation
                    match self.delete(kv_store, &operation.key).await {
                        Ok(()) => BatchResult {
                            key: operation.key.clone(),
                            success: true,
                            value: None,
                            error: None,
                            tier_used: None,
                            was_compressed: false,
                        },
                        Err(e) => BatchResult {
                            key: operation.key.clone(),
                            success: false,
                            value: None,
                            error: Some(e.to_string()),
                            tier_used: None,
                            was_compressed: false,
                        },
                    }
                }
            };

            results.push(result);
        }

        Ok(results)
    }

    /// Promote entry to a higher performance tier
    pub async fn promote_entry(
        &self,
        kv_store: &KvStore,
        key: &str,
        entry: &CacheEntry,
        current_tier: CacheTier,
    ) -> ArbitrageResult<()> {
        let target_tier = match current_tier {
            CacheTier::Cold => CacheTier::Warm,
            CacheTier::Warm => CacheTier::Hot,
            CacheTier::Hot => return Ok(()), // Already at highest tier
        };

        // Copy to target tier
        let mut promoted_entry = entry.clone();
        promoted_entry.tier = target_tier;

        let serialized = self.serialize_cache_entry(&promoted_entry)?;
        self.put_raw_to_tier(kv_store, key, &serialized, target_tier, entry.ttl_seconds)
            .await?;

        // Remove from current tier
        let current_tier_key = self.build_tier_key(key, current_tier);
        let _ = kv_store.delete(&current_tier_key).await;

        // Update metrics
        if let Ok(mut metrics) = self.metrics.lock() {
            if let Some(tier_stats) = metrics.tier_statistics.get_mut(&target_tier) {
                tier_stats.promotions += 1;
            }
        }

        self.logger.debug(&format!(
            "Promoted cache entry '{}' from {:?} to {:?}",
            key, current_tier, target_tier
        ));

        Ok(())
    }

    /// Get comprehensive cache metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<CacheManagerMetrics> {
        let metrics = self
            .metrics
            .lock()
            .map_err(|e| ArbitrageError::cache_error(format!("Failed to lock metrics: {}", e)))?;
        Ok(metrics.clone())
    }

    /// Health check compatible with DataAccessLayer
    pub async fn health_check(&self, kv_store: &KvStore) -> ArbitrageResult<bool> {
        // Perform basic health checks
        let basic_health = self.perform_basic_health_check(kv_store).await?;
        let metrics_health = self.check_metrics_health().await;
        let component_health = self.check_component_health().await;

        Ok(basic_health && metrics_health && component_health)
    }

    /// Get detailed health summary compatible with DataAccessLayer
    pub async fn get_health_summary(
        &self,
        kv_store: &KvStore,
    ) -> ArbitrageResult<serde_json::Value> {
        let metrics = self.get_metrics().await?;
        let basic_health = self.perform_basic_health_check(kv_store).await?;

        let overall_health = basic_health
            && metrics.overall_hit_rate_percent >= 60.0
            && metrics.average_latency_ms <= 50.0;

        Ok(serde_json::json!({
            "is_healthy": overall_health,
            "hit_rate_percent": metrics.overall_hit_rate_percent,
            "average_latency_ms": metrics.average_latency_ms,
            "total_operations": metrics.total_operations,
            "memory_usage_mb": metrics.memory_usage_bytes as f64 / (1024.0 * 1024.0),
            "tier_statistics": metrics.tier_statistics,
            "compression_ratio": metrics.compression_stats.compression_ratio,
            "warming_accuracy": metrics.warming_stats.prediction_accuracy_percent,
            "last_updated": metrics.last_updated,
            "uptime_seconds": (chrono::Utc::now().timestamp_millis() as u64 - self.startup_time) / 1000
        }))
    }

    // Private helper methods

    /// Infer data type from key pattern
    fn infer_data_type(&self, key: &str) -> DataType {
        match key {
            k if k.contains("market_data") || k.contains("price") || k.contains("ticker") => {
                DataType::MarketData
            }
            k if k.contains("user") || k.contains("profile") => DataType::UserProfile,
            k if k.contains("opportunity") || k.contains("arbitrage") => DataType::Opportunities,
            k if k.contains("analytics") || k.contains("stats") => DataType::Analytics,
            k if k.contains("config") || k.contains("settings") => DataType::Configuration,
            k if k.contains("session") || k.contains("auth") => DataType::Session,
            k if k.contains("ai") || k.contains("response") => DataType::AiResponse,
            k if k.contains("historical") || k.contains("history") => DataType::Historical,
            _ => DataType::Generic,
        }
    }

    /// Determine optimal tier for new cache entry
    #[allow(clippy::await_holding_lock)]
    async fn determine_optimal_tier(&self, key: &str, data_type: &DataType) -> CacheTier {
        // Check if we have access patterns for this key
        #[allow(clippy::await_holding_lock)]
        let access_pattern = if let Ok(tracker) = self.metadata_tracker.lock() {
            tracker.get_access_pattern(key).await
        } else {
            None
        };

        if let Some(pattern) = access_pattern {
            if pattern.access_frequency > 10.0 && pattern.access_count > 50 {
                return CacheTier::Hot;
            } else if pattern.access_frequency > 2.0 {
                return CacheTier::Warm;
            }
        }

        // Default tier assignment based on data type
        match data_type {
            DataType::MarketData => CacheTier::Hot, // Real-time data needs hot tier
            DataType::FundingRate => CacheTier::Warm, // Funding rates change less frequently
            DataType::UserProfile => CacheTier::Warm, // User data accessed regularly
            DataType::Opportunities => CacheTier::Hot, // Trading opportunities are time-sensitive
            DataType::Session => CacheTier::Hot,    // Session data needs fast access
            DataType::Configuration => CacheTier::Cold, // Config rarely changes
            DataType::Analytics => CacheTier::Cold, // Analytics can be slower
            DataType::AiResponse => CacheTier::Warm, // AI responses moderately accessed
            DataType::Historical => CacheTier::Cold, // Historical data less frequently accessed
            DataType::Generic => CacheTier::Warm,
        }
    }

    /// Check if entry should be promoted to higher tier
    async fn should_promote(&self, entry: &CacheEntry, current_tier: CacheTier) -> bool {
        // Don't promote if already at highest tier
        if current_tier == CacheTier::Hot {
            return false;
        }

        // Promote based on access patterns
        let recent_threshold = chrono::Utc::now().timestamp_millis() as u64 - 300000; // 5 minutes
        let high_access_threshold = match current_tier {
            CacheTier::Cold => 5,  // Promote to warm if accessed 5+ times
            CacheTier::Warm => 15, // Promote to hot if accessed 15+ times
            CacheTier::Hot => return false,
        };

        entry.access_count >= high_access_threshold && entry.last_accessed > recent_threshold
    }

    /// Build tier-specific key
    fn build_tier_key(&self, key: &str, tier: CacheTier) -> String {
        format!(
            "{}:{}:{}",
            self.config.general.namespace,
            tier.as_str(),
            key
        )
    }

    /// Serialize cache entry to JSON
    fn serialize_cache_entry(&self, entry: &CacheEntry) -> ArbitrageResult<String> {
        serde_json::to_string(entry).map_err(|e| {
            ArbitrageError::serialization_error(format!("Failed to serialize cache entry: {}", e))
        })
    }

    /// Deserialize cache entry from JSON
    fn deserialize_cache_entry(&self, data: &str) -> ArbitrageResult<CacheEntry> {
        serde_json::from_str(data).map_err(|e| {
            ArbitrageError::serialization_error(format!("Failed to deserialize cache entry: {}", e))
        })
    }

    /// Put raw data to specific tier
    async fn put_raw_to_tier(
        &self,
        kv_store: &KvStore,
        key: &str,
        data: &str,
        tier: CacheTier,
        ttl_seconds: u64,
    ) -> ArbitrageResult<()> {
        let tier_key = self.build_tier_key(key, tier);

        let mut put_request = kv_store.put(&tier_key, data).map_err(|e| {
            ArbitrageError::cache_error(format!("Failed to create put request: {:?}", e))
        })?;

        put_request = put_request.expiration_ttl(ttl_seconds);

        put_request.execute().await.map_err(|e| {
            ArbitrageError::cache_error(format!("Failed to execute put request: {:?}", e))
        })?;

        Ok(())
    }

    /// Record cache hit in metrics
    async fn record_hit(&self, data_type: &DataType, tier: CacheTier, start_time: WasmInstant) {
        let latency_ms = start_time.elapsed().as_millis() as f64;

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.successful_operations += 1;
            metrics.total_operations += 1;

            // Update tier statistics
            if let Some(tier_stats) = metrics.tier_statistics.get_mut(&tier) {
                tier_stats.hits += 1;
                tier_stats.average_access_time_ms =
                    (tier_stats.average_access_time_ms + latency_ms) / 2.0;
            }

            // Update overall metrics
            metrics.overall_hit_rate_percent =
                (metrics.successful_operations as f32 / metrics.total_operations as f32) * 100.0;
            metrics.average_latency_ms = (metrics.average_latency_ms + latency_ms) / 2.0;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.debug(&format!(
            "Cache hit: data_type={:?}, tier={:?}, latency={}ms",
            data_type, tier, latency_ms
        ));
    }

    /// Record cache miss in metrics
    async fn record_miss(&self, data_type: &DataType, start_time: WasmInstant) {
        let latency_ms = start_time.elapsed().as_millis() as f64;

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_operations += 1;

            // Update hit rate
            metrics.overall_hit_rate_percent =
                (metrics.successful_operations as f32 / metrics.total_operations as f32) * 100.0;
            metrics.average_latency_ms = (metrics.average_latency_ms + latency_ms) / 2.0;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.debug(&format!(
            "Cache miss: data_type={:?}, latency={}ms",
            data_type, latency_ms
        ));
    }

    /// Record successful put operation
    async fn record_successful_put(
        &self,
        data_type: &DataType,
        tier: CacheTier,
        start_time: WasmInstant,
    ) {
        let latency_ms = start_time.elapsed().as_millis() as f64;

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.successful_operations += 1;
            metrics.total_operations += 1;

            if let Some(tier_stats) = metrics.tier_statistics.get_mut(&tier) {
                tier_stats.entry_count += 1;
            }

            metrics.average_latency_ms = (metrics.average_latency_ms + latency_ms) / 2.0;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.debug(&format!(
            "Cache put: data_type={:?}, tier={:?}, latency={}ms",
            data_type, tier, latency_ms
        ));
    }

    /// Record operation error
    async fn record_error(
        &self,
        data_type: &DataType,
        error: &ArbitrageError,
        start_time: WasmInstant,
    ) {
        let latency_ms = start_time.elapsed().as_millis() as f64;

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.failed_operations += 1;
            metrics.total_operations += 1;
            metrics.average_latency_ms = (metrics.average_latency_ms + latency_ms) / 2.0;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.warn(&format!(
            "Cache error: data_type={:?}, error={}, latency={}ms",
            data_type, error, latency_ms
        ));
    }

    /// Perform basic health check
    async fn perform_basic_health_check(&self, kv_store: &KvStore) -> ArbitrageResult<bool> {
        let test_key = "health_check_test";
        let test_value = "health_check_ok";

        // Test write
        match self.put(kv_store, test_key, test_value, Some(60)).await {
            Ok(()) => {
                // Test read
                match self.get(kv_store, test_key).await {
                    Ok(Some(value)) if value == test_value => {
                        // Cleanup
                        let _ = self.delete(kv_store, test_key).await;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            Err(_) => Ok(false),
        }
    }

    /// Check metrics health
    async fn check_metrics_health(&self) -> bool {
        if let Ok(metrics) = self.metrics.lock() {
            // Consider healthy if hit rate is reasonable and latency is acceptable
            metrics.overall_hit_rate_percent >= 50.0 && metrics.average_latency_ms <= 100.0
        } else {
            false
        }
    }

    /// Check component health
    async fn check_component_health(&self) -> bool {
        // All components are stateless and should be healthy
        true
    }

    // === Cache Warming Integration Methods ===

    /// Execute cache warming for pending requests
    pub async fn execute_warming_batch(&self, kv_store: &KvStore) -> ArbitrageResult<usize> {
        if !self.config.warming.enabled {
            return Ok(0);
        }

        let warming_requests = {
            let mut service = self.warming_service.lock().map_err(|_| {
                ArbitrageError::cache_error("Failed to acquire warming service lock")
            })?;
            service.get_next_warming_batch()
        };

        if warming_requests.is_empty() {
            return Ok(0);
        }

        let start_time = crate::utils::time::now_instant();
        let mut successful_warmings = 0;

        let warming_count = warming_requests.len();
        for request in warming_requests {
            match self.execute_single_warming(kv_store, &request).await {
                Ok(()) => {
                    successful_warmings += 1;
                    let mut service = self.warming_service.lock().map_err(|_| {
                        ArbitrageError::cache_error("Failed to acquire warming service lock")
                    })?;
                    service.record_warming_success(&request.key);
                }
                Err(e) => {
                    self.logger
                        .warn(&format!("Warming failed for key {}: {}", request.key, e));
                    let mut service = self.warming_service.lock().map_err(|_| {
                        ArbitrageError::cache_error("Failed to acquire warming service lock")
                    })?;
                    service.record_warming_failure(&request.key, &e.to_string());
                }
            }
        }

        // Update warming metrics
        {
            let mut metrics = self
                .metrics
                .lock()
                .map_err(|_| ArbitrageError::cache_error("Failed to acquire metrics lock"))?;

            metrics.warming_stats.warming_operations += successful_warmings as u64;
            metrics.warming_stats.successful_warming += successful_warmings as u64;

            let elapsed_ms = start_time.elapsed().as_millis() as f64;
            if successful_warmings > 0 {
                metrics.warming_stats.average_warm_time_ms =
                    (metrics.warming_stats.average_warm_time_ms + elapsed_ms) / 2.0;
            }
        }

        self.logger.info(&format!(
            "Warming batch completed: {} successful out of {} requests",
            successful_warmings, warming_count
        ));

        Ok(successful_warmings)
    }

    /// Get warming status and metrics
    pub async fn get_warming_stats(&self) -> ArbitrageResult<super::warming::WarmingStats> {
        if !self.config.warming.enabled {
            return Ok(super::warming::WarmingStats::default());
        }

        let service = self
            .warming_service
            .lock()
            .map_err(|_| ArbitrageError::cache_error("Failed to acquire warming service lock"))?;

        Ok(service.get_stats().clone())
    }

    /// Start background warming process
    pub async fn start_warming_process(&self, kv_store: &KvStore) -> ArbitrageResult<()> {
        if !self.config.warming.enabled {
            return Ok(());
        }

        self.logger.info("Starting cache warming process");

        // Execute initial warming batch
        self.execute_warming_batch(kv_store).await?;

        Ok(())
    }

    /// Clean up old warming statistics
    pub async fn cleanup_warming_stats(&self, max_age_seconds: u64) -> ArbitrageResult<()> {
        if !self.config.warming.enabled {
            return Ok(());
        }

        let mut service = self
            .warming_service
            .lock()
            .map_err(|_| ArbitrageError::cache_error("Failed to acquire warming service lock"))?;

        service.cleanup_old_stats(max_age_seconds);
        Ok(())
    }

    /// Record access pattern for warming analysis
    async fn record_access_for_warming(&self, key: &str, data_type: &DataType) {
        if let Ok(tracker) = self.metadata_tracker.lock() {
            if let Ok(metadata) = tracker.get_metadata(key) {
                let access_pattern = &metadata.access_pattern;

                if let Ok(mut service) = self.warming_service.lock() {
                    service.analyze_and_queue_warming(key, data_type, access_pattern);
                }
            }
        }
    }

    /// Check if this access was for a warmed entry and record hit
    async fn check_and_record_warming_hit(&self, key: &str) {
        // This is a simplified check - in production you might want to track
        // which entries were warmed more explicitly
        if let Ok(mut service) = self.warming_service.lock() {
            service.record_warming_hit(key);
        }
    }

    /// Trigger predictive warming for related keys
    async fn trigger_predictive_warming(&self, missed_key: &str, data_type: &DataType) {
        // Generate related keys that might be accessed soon
        let related_keys = self.generate_related_keys(missed_key, data_type);

        if let Ok(mut service) = self.warming_service.lock() {
            for related_key in related_keys {
                let request = super::warming::WarmingRequest {
                    key: related_key,
                    data_type: data_type.clone(),
                    priority: 3, // Medium priority for predictive warming
                    predicted_access_time: chrono::Utc::now().timestamp() as u64 + 300, // 5 minutes
                    confidence: 0.7, // Medium confidence for predictions
                    created_at: chrono::Utc::now().timestamp() as u64,
                };
                service.add_warming_request(request);
            }
        }
    }

    /// Execute warming for a single cache key
    async fn execute_single_warming(
        &self,
        kv_store: &KvStore,
        request: &super::warming::WarmingRequest,
    ) -> ArbitrageResult<()> {
        self.logger.debug(&format!(
            "Executing warming for key: {} (priority: {}, confidence: {})",
            request.key, request.priority, request.confidence
        ));

        // Check if the key is already cached
        if self.exists(kv_store, &request.key).await? {
            return Ok(()); // Already cached, nothing to warm
        }

        // Simulate data fetching and caching
        // In a real implementation, this would fetch from the data source
        match self
            .warm_from_data_source(&request.key, &request.data_type)
            .await
        {
            Ok(Some(value)) => {
                // Cache the warmed data
                self.put(kv_store, &request.key, &value, None).await?;

                self.logger
                    .debug(&format!("Successfully warmed cache key: {}", request.key));
                Ok(())
            }
            Ok(None) => {
                // Data not available from source
                Err(ArbitrageError::cache_error(format!(
                    "Data not available for warming key: {}",
                    request.key
                )))
            }
            Err(e) => Err(e),
        }
    }

    /// Generate related keys for predictive warming
    fn generate_related_keys(&self, base_key: &str, data_type: &DataType) -> Vec<String> {
        let mut related_keys = Vec::new();

        match data_type {
            DataType::MarketData => {
                // For market data, warm related symbols
                if base_key.contains("BTCUSDT") {
                    related_keys.extend(vec![
                        base_key.replace("BTCUSDT", "ETHUSDT"),
                        base_key.replace("BTCUSDT", "ADAUSDT"),
                    ]);
                }
            }
            DataType::UserProfile => {
                // For user data, warm user preferences and profile data
                if base_key.contains("profile") {
                    let user_id = base_key.split(':').nth(1).unwrap_or("");
                    related_keys.extend(vec![
                        format!("user:{}:preferences", user_id),
                        format!("user:{}:settings", user_id),
                    ]);
                }
            }
            DataType::Opportunities => {
                // For opportunities, warm related market data
                related_keys.push(format!("{}_market_data", base_key));
            }
            _ => {
                // For other data types, generate generic related keys
                related_keys.push(format!("{}_metadata", base_key));
            }
        }

        // Filter out the original key and invalid keys
        related_keys
            .into_iter()
            .filter(|k| k != base_key && !k.is_empty())
            .take(3) // Limit to 3 related keys to avoid overwhelming
            .collect()
    }

    /// Simulate warming from data source
    /// In production, this would integrate with actual data sources
    async fn warm_from_data_source(
        &self,
        key: &str,
        data_type: &DataType,
    ) -> ArbitrageResult<Option<String>> {
        // This is a placeholder implementation
        // In production, you would integrate with:
        // - Database queries
        // - API calls
        // - File systems
        // - Other data sources

        match data_type {
            DataType::MarketData => {
                // Simulate market data fetching
                Ok(Some(format!(
                    r#"{{"symbol":"{}","price":42000.0,"timestamp":{}}}"#,
                    key,
                    chrono::Utc::now().timestamp()
                )))
            }
            DataType::UserProfile => {
                // Simulate user data fetching
                Ok(Some(format!(
                    r#"{{"user_id":"{}","cached_at":{}}}"#,
                    key,
                    chrono::Utc::now().timestamp()
                )))
            }
            DataType::Opportunities => {
                // Simulate opportunity data fetching
                Ok(Some(format!(
                    r#"{{"opportunity_id":"{}","profit":1.5,"cached_at":{}}}"#,
                    key,
                    chrono::Utc::now().timestamp()
                )))
            }
            _ => {
                // For other types, return None (not available)
                Ok(None)
            }
        }
    }

    // === Enhanced Analytics and Monitoring Integration ===

    /// Generate comprehensive analytics report using metadata tracking
    pub async fn generate_analytics_report(&self) -> ArbitrageResult<super::CacheAnalyticsReport> {
        if let Ok(tracker) = self.metadata_tracker.lock() {
            Ok(tracker.generate_analytics_report())
        } else {
            Err(ArbitrageError::cache_error(
                "Failed to acquire metadata tracker lock",
            ))
        }
    }

    /// Get advanced cleanup candidates with space pressure analysis
    pub async fn get_advanced_cleanup_candidates(
        &self,
        space_pressure: f64,
        target_cleanup_bytes: u64,
    ) -> ArbitrageResult<Vec<super::CleanupCandidate>> {
        if let Ok(tracker) = self.metadata_tracker.lock() {
            Ok(tracker.get_advanced_cleanup_candidates(space_pressure, target_cleanup_bytes))
        } else {
            Err(ArbitrageError::cache_error(
                "Failed to acquire metadata tracker lock",
            ))
        }
    }

    /// Get real-time performance insights for monitoring
    pub async fn get_performance_insights(
        &self,
    ) -> ArbitrageResult<std::collections::HashMap<String, serde_json::Value>> {
        if let Ok(tracker) = self.metadata_tracker.lock() {
            Ok(tracker.get_performance_insights())
        } else {
            Err(ArbitrageError::cache_error(
                "Failed to acquire metadata tracker lock",
            ))
        }
    }

    /// Execute intelligent cleanup based on metadata analysis
    pub async fn execute_intelligent_cleanup(
        &self,
        kv_store: &KvStore,
        space_pressure: f64,
        target_space_mb: u64,
    ) -> ArbitrageResult<super::CleanupRecommendations> {
        let target_bytes = target_space_mb * 1024 * 1024;

        // Get cleanup candidates using enhanced metadata analysis
        let candidates = self
            .get_advanced_cleanup_candidates(space_pressure, target_bytes)
            .await?;

        let mut cleaned_count = 0;
        let mut cleaned_bytes = 0u64;
        let mut immediate_cleanup = Vec::new();
        let mut conditional_cleanup = Vec::new();

        for candidate in candidates {
            if candidate.score > 0.8 && cleaned_bytes < target_bytes {
                // Execute immediate cleanup
                match self.delete(kv_store, &candidate.key).await {
                    Ok(()) => {
                        cleaned_count += 1;
                        cleaned_bytes += candidate.size_bytes;
                        immediate_cleanup.push(candidate);

                        self.logger.info(&format!(
                            "Cleaned up key: {} (score: {:.2}, saved: {} bytes)",
                            immediate_cleanup.last().unwrap().key,
                            immediate_cleanup.last().unwrap().score,
                            immediate_cleanup.last().unwrap().size_bytes
                        ));
                    }
                    Err(e) => {
                        self.logger
                            .warn(&format!("Failed to cleanup key {}: {}", candidate.key, e));
                    }
                }
            } else if candidate.score > 0.5 {
                conditional_cleanup.push(candidate);
            }
        }

        // Generate cleanup frequency recommendations
        let cleanup_frequency_recommendations = std::collections::HashMap::new();

        let recommendations = super::CleanupRecommendations {
            immediate_cleanup,
            conditional_cleanup,
            potential_space_savings_bytes: cleaned_bytes,
            cleanup_frequency_recommendations,
        };

        self.logger.info(&format!(
            "Intelligent cleanup completed: {} entries cleaned, {} bytes freed",
            cleaned_count, cleaned_bytes
        ));

        Ok(recommendations)
    }

    /// Monitor cache health and generate alerts
    pub async fn monitor_cache_health(&self) -> ArbitrageResult<Vec<super::PerformanceAlert>> {
        let insights = self.get_performance_insights().await?;
        let mut alerts = Vec::new();

        // Check hit rate
        if let Some(hit_rate_value) = insights.get("hit_rate") {
            if let Some(hit_rate) = hit_rate_value.get("value").and_then(|v| v.as_f64()) {
                if hit_rate < 0.5 {
                    alerts.push(super::PerformanceAlert {
                        alert_type: "Low Hit Rate".to_string(),
                        severity: if hit_rate < 0.3 { super::AlertSeverity::Critical } else { super::AlertSeverity::High },
                        message: format!("Cache hit rate is {:.1}%, below healthy threshold", hit_rate * 100.0),
                        affected_keys: vec![], // Would be populated with actual problematic keys
                        recommended_action: "Consider pre-warming frequently accessed data or reviewing cache sizing".to_string(),
                    });
                }
            }
        }

        // Check memory utilization
        if let Some(memory_value) = insights.get("memory_utilization") {
            if let Some(utilization) = memory_value
                .get("utilization_percent")
                .and_then(|v| v.as_f64())
            {
                if utilization > 85.0 {
                    alerts.push(super::PerformanceAlert {
                        alert_type: "High Memory Usage".to_string(),
                        severity: if utilization > 95.0 {
                            super::AlertSeverity::Critical
                        } else {
                            super::AlertSeverity::High
                        },
                        message: format!("Memory utilization at {:.1}%", utilization),
                        affected_keys: vec![],
                        recommended_action: "Execute cleanup or increase cache size".to_string(),
                    });
                }
            }
        }

        // Check for hot spots
        if let Some(hot_spots_value) = insights.get("hot_spots") {
            if let Some(count) = hot_spots_value
                .get("identified_count")
                .and_then(|v| v.as_u64())
            {
                if count > 10 {
                    alerts.push(super::PerformanceAlert {
                        alert_type: "Performance Hot Spots".to_string(),
                        severity: super::AlertSeverity::Medium,
                        message: format!("{} performance hot spots detected", count),
                        affected_keys: vec![],
                        recommended_action:
                            "Review tier assignments and consider data distribution optimization"
                                .to_string(),
                    });
                }
            }
        }

        Ok(alerts)
    }

    /// Get cache health score with detailed breakdown
    pub async fn get_health_score_detailed(&self) -> ArbitrageResult<serde_json::Value> {
        let report = self.generate_analytics_report().await?;

        Ok(serde_json::json!({
            "overall_health_score": report.health_score,
            "health_grade": if report.health_score > 0.9 { "A" }
                           else if report.health_score > 0.8 { "B" }
                           else if report.health_score > 0.7 { "C" }
                           else if report.health_score > 0.6 { "D" }
                           else { "F" },
            "performance_analysis": {
                "hit_rate": report.performance_analysis.overall_hit_rate,
                "response_times": report.performance_analysis.response_times_by_tier,
                "critical_path_ms": report.performance_analysis.hot_path_metrics.critical_path_response_ms
            },
            "tier_efficiency": {
                "current_distribution": report.tier_insights.current_distribution,
                "recommended_distribution": report.tier_insights.recommended_distribution,
                "efficiency_scores": report.tier_insights.tier_efficiency
            },
            "cleanup_recommendations": {
                "immediate_candidates": report.cleanup_recommendations.immediate_cleanup.len(),
                "potential_savings_mb": report.cleanup_recommendations.potential_space_savings_bytes / (1024 * 1024)
            },
            "growth_projections": {
                "growth_rate": report.trends.growth_projections.projected_growth_rate,
                "projected_size_mb": report.trends.growth_projections.projected_size_in_30_days / (1024 * 1024)
            },
            "generated_at": report.generated_at
        }))
    }

    /// Add proper cache headers to response
    pub fn add_cache_headers(
        &self,
        response: &mut worker::Response,
        cache_type: &str,
        ttl_seconds: u64,
    ) -> ArbitrageResult<()> {
        let headers = response.headers_mut();

        match cache_type {
            "market_data" => {
                headers.set(
                    "Cache-Control",
                    &format!("public, max-age={}, s-maxage={}", ttl_seconds, ttl_seconds),
                )?;
                headers.set("Vary", "Accept-Encoding")?;
                headers.set("X-Cache-Type", "market-data")?;
            }
            "opportunities" => {
                headers.set(
                    "Cache-Control",
                    &format!("public, max-age={}, s-maxage={}", ttl_seconds, ttl_seconds),
                )?;
                headers.set("Vary", "User-Agent, Accept-Encoding")?;
                headers.set("X-Cache-Type", "opportunities")?;
            }
            "static" => {
                headers.set(
                    "Cache-Control",
                    &format!("public, max-age={}, immutable", ttl_seconds),
                )?;
                headers.set("X-Cache-Type", "static")?;
            }
            _ => {
                headers.set("Cache-Control", &format!("public, max-age={}", ttl_seconds))?;
            }
        }

        // Add ETag for conditional requests
        let etag = format!("\"{}\"", chrono::Utc::now().timestamp());
        headers.set("ETag", &etag)?;

        Ok(())
    }
}

impl Default for TierStats {
    fn default() -> Self {
        Self {
            hits: 0,
            misses: 0,
            promotions: 0,
            demotions: 0,
            evictions: 0,
            size_bytes: 0,
            entry_count: 0,
            average_access_time_ms: 0.0,
        }
    }
}
