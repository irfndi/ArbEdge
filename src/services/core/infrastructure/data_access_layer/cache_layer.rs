// Cache Layer - Intelligent Caching with Freshness Validation Component
// Provides multi-tier caching with automatic compression and data freshness validation

use crate::utils::{ArbitrageError, ArbitrageResult};
// use crate::services::core::infrastructure::shared_types::{ComponentHealth, CircuitBreaker, CacheStats};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

// Temporary local types until shared_types is working
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub is_healthy: bool,
    pub last_check: u64,
    pub error_count: u32,
    pub warning_count: u32,
    pub uptime_seconds: u64,
    pub performance_score: f32,
    pub resource_usage_percent: f32,
    pub last_error: Option<String>,
    pub last_warning: Option<String>,
    pub component_name: String,
    pub version: String,
}

impl Default for ComponentHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            last_check: chrono::Utc::now().timestamp_millis() as u64,
            error_count: 0,
            warning_count: 0,
            uptime_seconds: 0,
            performance_score: 0.0,
            resource_usage_percent: 0.0,
            last_error: None,
            last_warning: None,
            component_name: "cache_layer".to_string(),
            version: "1.0.0".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub threshold: u32,
    pub timeout_seconds: u64,
    pub last_failure_time: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            threshold: 5,
            timeout_seconds: 60,
            last_failure_time: 0,
        }
    }
}

/// Cache entry types with different TTL requirements
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheEntryType {
    MarketData,      // 5 minutes TTL
    FundingRates,    // 15 minutes TTL
    Analytics,       // 30 minutes TTL
    UserPreferences, // 1 hour TTL
    Opportunities,   // 2 minutes TTL
    AIAnalysis,      // 1 hour TTL
    SystemConfig,    // 24 hours TTL
    TradingPairs,    // 6 hours TTL
}

impl CacheEntryType {
    pub fn as_str(&self) -> &str {
        match self {
            CacheEntryType::MarketData => "market_data",
            CacheEntryType::FundingRates => "funding_rates",
            CacheEntryType::Analytics => "analytics",
            CacheEntryType::UserPreferences => "user_preferences",
            CacheEntryType::Opportunities => "opportunities",
            CacheEntryType::AIAnalysis => "ai_analysis",
            CacheEntryType::SystemConfig => "system_config",
            CacheEntryType::TradingPairs => "trading_pairs",
        }
    }

    pub fn default_ttl_seconds(&self) -> u64 {
        match self {
            CacheEntryType::MarketData => 300,       // 5 minutes
            CacheEntryType::FundingRates => 900,     // 15 minutes
            CacheEntryType::Analytics => 1800,       // 30 minutes
            CacheEntryType::UserPreferences => 3600, // 1 hour
            CacheEntryType::Opportunities => 120,    // 2 minutes
            CacheEntryType::AIAnalysis => 3600,      // 1 hour
            CacheEntryType::SystemConfig => 86400,   // 24 hours
            CacheEntryType::TradingPairs => 21600,   // 6 hours
        }
    }

    pub fn compression_threshold_bytes(&self) -> usize {
        match self {
            CacheEntryType::MarketData => 5120,      // 5KB
            CacheEntryType::FundingRates => 2048,    // 2KB
            CacheEntryType::Analytics => 10240,      // 10KB
            CacheEntryType::UserPreferences => 1024, // 1KB
            CacheEntryType::Opportunities => 2048,   // 2KB
            CacheEntryType::AIAnalysis => 5120,      // 5KB
            CacheEntryType::SystemConfig => 1024,    // 1KB
            CacheEntryType::TradingPairs => 2048,    // 2KB
        }
    }
}

/// Cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub data: String,
    pub entry_type: CacheEntryType,
    pub created_at: u64,
    pub expires_at: u64,
    pub last_accessed: u64,
    pub access_count: u64,
    pub is_compressed: bool,
    pub original_size: usize,
    pub compressed_size: Option<usize>,
    pub freshness_score: f32,
}

impl CacheEntry {
    pub fn new(key: String, data: String, entry_type: CacheEntryType) -> Self {
        let _start_time = chrono::Utc::now().timestamp_millis() as u64;
        let ttl_ms = entry_type.default_ttl_seconds() * 1000;

        Self {
            key,
            data,
            entry_type,
            created_at: _start_time,
            expires_at: _start_time + ttl_ms,
            last_accessed: _start_time,
            access_count: 0,
            is_compressed: false,
            original_size: 0,
            compressed_size: None,
            freshness_score: 1.0,
        }
    }

    pub fn is_expired(&self) -> bool {
        let _now = chrono::Utc::now().timestamp_millis() as u64;
        _now > self.expires_at
    }

    pub fn is_fresh(&self, freshness_threshold: f32) -> bool {
        self.freshness_score >= freshness_threshold && !self.is_expired()
    }

    pub fn update_freshness(&mut self) {
        let _now = chrono::Utc::now().timestamp_millis() as u64;
        let age_ratio =
            (_now - self.created_at) as f32 / (self.expires_at - self.created_at) as f32;
        self.freshness_score = (1.0 - age_ratio).max(0.0);
    }

    pub fn record_access(&mut self) {
        self.last_accessed = chrono::Utc::now().timestamp_millis() as u64;
        self.access_count += 1;
    }
}

/// Cache statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: u64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub total_sets: u64,
    pub total_deletes: u64,
    pub total_size_bytes: u64,
    pub compressed_entries: u64,
    pub expired_entries: u64,
    pub hit_rate_percent: f32,
    pub compression_ratio_percent: f32,
    pub average_freshness_score: f32,
    pub last_updated: u64,
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            total_entries: 0,
            total_hits: 0,
            total_misses: 0,
            total_sets: 0,
            total_deletes: 0,
            total_size_bytes: 0,
            compressed_entries: 0,
            expired_entries: 0,
            hit_rate_percent: 0.0,
            compression_ratio_percent: 0.0,
            average_freshness_score: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Cache health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheHealth {
    pub is_healthy: bool,
    pub kv_store_available: bool,
    pub hit_rate_percent: f32,
    pub freshness_score: f32,
    pub memory_usage_percent: f32,
    pub expired_entries_percent: f32,
    pub last_health_check: u64,
    pub last_error: Option<String>,
}

impl Default for CacheHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            kv_store_available: false,
            hit_rate_percent: 0.0,
            freshness_score: 0.0,
            memory_usage_percent: 0.0,
            expired_entries_percent: 0.0,
            last_health_check: chrono::Utc::now().timestamp_millis() as u64,
            last_error: None,
        }
    }
}

/// Cache performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetrics {
    pub operations_per_second: f64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub batch_operations: u64,
    pub compression_savings_bytes: u64,
    pub freshness_validations: u64,
    pub cache_evictions: u64,
    pub last_updated: u64,
}

impl Default for CacheMetrics {
    fn default() -> Self {
        Self {
            operations_per_second: 0.0,
            average_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            max_latency_ms: 0.0,
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            batch_operations: 0,
            compression_savings_bytes: 0,
            freshness_validations: 0,
            cache_evictions: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for CacheLayer
#[derive(Debug, Clone)]
pub struct CacheLayerConfig {
    pub enable_compression: bool,
    pub compression_threshold_bytes: usize,
    pub enable_freshness_validation: bool,
    pub freshness_threshold: f32,
    pub enable_automatic_expiration: bool,
    pub enable_batch_operations: bool,
    pub batch_size: usize,
    pub enable_cache_warming: bool,
    pub enable_performance_tracking: bool,
    pub enable_health_monitoring: bool,
    pub max_cache_size_mb: usize,
    pub eviction_policy: String,
    pub background_cleanup_interval_seconds: u64,
}

impl Default for CacheLayerConfig {
    fn default() -> Self {
        Self {
            enable_compression: true,
            compression_threshold_bytes: 1024, // 1KB
            enable_freshness_validation: true,
            freshness_threshold: 0.8, // 80% freshness required
            enable_automatic_expiration: true,
            enable_batch_operations: true,
            batch_size: 50,
            enable_cache_warming: true,
            enable_performance_tracking: true,
            enable_health_monitoring: true,
            max_cache_size_mb: 256, // 256MB cache limit
            eviction_policy: "lru".to_string(),
            background_cleanup_interval_seconds: 300, // 5 minutes
        }
    }
}

impl CacheLayerConfig {
    /// Create configuration optimized for high concurrency
    pub fn high_concurrency() -> Self {
        Self {
            batch_size: 100,
            max_cache_size_mb: 512,
            compression_threshold_bytes: 512,
            freshness_threshold: 0.7,
            background_cleanup_interval_seconds: 180, // 3 minutes
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            freshness_threshold: 0.9,
            enable_cache_warming: true,
            background_cleanup_interval_seconds: 600, // 10 minutes
            max_cache_size_mb: 128,
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.compression_threshold_bytes == 0 {
            return Err(ArbitrageError::validation_error(
                "compression_threshold_bytes must be greater than 0",
            ));
        }
        if !(0.0..=1.0).contains(&self.freshness_threshold) {
            return Err(ArbitrageError::validation_error(
                "freshness_threshold must be between 0.0 and 1.0",
            ));
        }
        if self.batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "batch_size must be greater than 0",
            ));
        }
        if self.max_cache_size_mb == 0 {
            return Err(ArbitrageError::validation_error(
                "max_cache_size_mb must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Cache layer for intelligent caching with freshness validation
pub struct CacheLayer {
    config: CacheLayerConfig,
    kv_store: KvStore,
    logger: crate::utils::logger::Logger,

    // Metadata tracking
    entry_metadata: Arc<std::sync::Mutex<HashMap<String, CacheEntry>>>,

    // Performance metrics - use CacheStats instead of CacheMetrics
    cache_metrics: Arc<std::sync::Mutex<CacheStats>>,

    // Health monitoring
    health_status: Arc<std::sync::Mutex<ComponentHealth>>,
    last_health_check: Arc<std::sync::Mutex<Option<u64>>>,

    // Circuit breaker for KV operations
    circuit_breaker: Arc<std::sync::Mutex<CircuitBreaker>>,
}

impl CacheLayer {
    /// Create new CacheLayer instance
    pub fn new(kv_store: KvStore, config: CacheLayerConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        let layer = Self {
            config,
            logger,
            kv_store,
            entry_metadata: Arc::new(std::sync::Mutex::new(HashMap::new())),
            cache_metrics: Arc::new(std::sync::Mutex::new(CacheStats::default())),
            health_status: Arc::new(std::sync::Mutex::new(ComponentHealth::default())),
            last_health_check: Arc::new(std::sync::Mutex::new(None)),
            circuit_breaker: Arc::new(std::sync::Mutex::new(CircuitBreaker::default())),
        };

        layer.logger.info(&format!(
            "CacheLayer initialized: compression={}, freshness_validation={}, batch_size={}",
            layer.config.enable_compression,
            layer.config.enable_freshness_validation,
            layer.config.batch_size
        ));

        Ok(layer)
    }

    /// Get data from cache with freshness validation
    pub async fn get<T>(&self, key: &str, _entry_type: CacheEntryType) -> ArbitrageResult<Option<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let _start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check metadata first
        let should_record_miss = {
            let mut metadata = self.entry_metadata.lock().unwrap();
            if let Some(entry) = metadata.get_mut(key) {
                // Update freshness score
                entry.update_freshness();

                // Check if entry is fresh enough
                if self.config.enable_freshness_validation
                    && !entry.is_fresh(self.config.freshness_threshold)
                {
                    true // Should record miss
                } else {
                    // Record access
                    entry.record_access();
                    false // Should not record miss
                }
            } else {
                false // No entry found, continue with normal flow
            }
        };

        // Record miss if needed (outside of lock scope)
        if should_record_miss {
            self.record_miss(_start_time).await;
            return Ok(None);
        }

        // Attempt uncompressed then compressed key
        let raw = match self.kv_store.get(key).text().await {
            Ok(Some(v)) => Some(v),
            _ => self
                .kv_store
                .get(&format!("compressed:{}", key))
                .text()
                .await
                .ok()
                .flatten(),
        };

        match raw {
            Some(data_str) => {
                // Decompress if needed
                let final_data = if self.is_compressed_key(key) {
                    self.decompress_data(&data_str)?
                } else {
                    data_str.to_string()
                };

                // Deserialize
                match serde_json::from_str::<T>(&final_data) {
                    Ok(data) => {
                        self.update_cache_hit_metrics().await;
                        Ok(Some(data))
                    }
                    Err(e) => {
                        self.logger.error(&format!(
                            "Failed to deserialize cached data for key {}: {}",
                            key, e
                        ));
                        self.update_cache_miss_metrics().await;
                        Ok(None)
                    }
                }
            }
            None => {
                self.update_cache_miss_metrics().await;
                Ok(None)
            }
        }
    }

    /// Set data in cache with compression and TTL
    pub async fn set<T>(
        &self,
        key: &str,
        data: &T,
        entry_type: CacheEntryType,
        custom_ttl: Option<u64>,
    ) -> ArbitrageResult<()>
    where
        T: serde::Serialize,
    {
        let _start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Serialize data
        let data_str = serde_json::to_string(data)
            .map_err(|e| ArbitrageError::parse_error(format!("Failed to serialize data: {}", e)))?;

        let original_size = data_str.len();
        let mut final_data = data_str;
        let mut is_compressed = false;
        let mut compressed_size = None;

        // Compress if enabled and data is large enough
        if self.config.enable_compression
            && original_size > entry_type.compression_threshold_bytes()
        {
            match self.compress_data(&final_data) {
                Ok(compressed) => {
                    if compressed.len() < original_size {
                        final_data = compressed;
                        is_compressed = true;
                        compressed_size = Some(final_data.len());
                    }
                }
                Err(e) => {
                    self.logger
                        .warn(&format!("Compression failed for key {}: {}", key, e));
                }
            }
        }

        // Determine TTL
        let ttl_seconds = custom_ttl.unwrap_or_else(|| entry_type.default_ttl_seconds());

        // Store in KV
        let cache_key = if is_compressed {
            format!("compressed:{}", key)
        } else {
            key.to_string()
        };

        let mut put_request = self.kv_store.put(&cache_key, &final_data)?;
        put_request = put_request.expiration_ttl(ttl_seconds);

        match put_request.execute().await {
            Ok(_) => {
                // Update metadata
                let mut entry = CacheEntry::new(key.to_string(), final_data, entry_type);
                entry.is_compressed = is_compressed;
                entry.original_size = original_size;
                entry.compressed_size = compressed_size;

                if let Ok(mut metadata) = self.entry_metadata.lock() {
                    metadata.insert(key.to_string(), entry);
                }

                self.record_set(_start_time, original_size, compressed_size)
                    .await;
                Ok(())
            }
            Err(e) => {
                self.record_failure(
                    _start_time,
                    &ArbitrageError::cache_error(format!("KV put failed: {}", e)),
                )
                .await;
                Err(ArbitrageError::cache_error(format!(
                    "Cache set failed: {}",
                    e
                )))
            }
        }
    }

    /// Delete data from cache
    pub async fn delete(&self, key: &str) -> ArbitrageResult<()> {
        let _start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Try both compressed and uncompressed keys
        let keys_to_delete = vec![key.to_string(), format!("compressed:{}", key)];

        let mut delete_success = false;
        for delete_key in keys_to_delete {
            match self.kv_store.delete(&delete_key).await {
                Ok(_) => delete_success = true,
                Err(e) => {
                    self.logger
                        .warn(&format!("Failed to delete key {}: {}", delete_key, e));
                }
            }
        }

        // Remove from metadata
        if let Ok(mut metadata) = self.entry_metadata.lock() {
            metadata.remove(key);
        }

        if delete_success {
            self.record_delete(_start_time).await;
            Ok(())
        } else {
            self.record_failure(
                _start_time,
                &ArbitrageError::cache_error("Failed to delete cache entry"),
            )
            .await;
            Err(ArbitrageError::cache_error("Cache delete failed"))
        }
    }

    /// Batch get operations
    pub async fn batch_get<T>(
        &self,
        keys: &[String],
        entry_type: CacheEntryType,
    ) -> ArbitrageResult<HashMap<String, T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let mut results = HashMap::new();

        // Process in batches
        for chunk in keys.chunks(self.config.batch_size) {
            for key in chunk {
                if let Ok(Some(data)) = self.get::<T>(key, entry_type.clone()).await {
                    results.insert(key.clone(), data);
                }
            }
        }

        self.record_batch_operation().await;
        Ok(results)
    }

    /// Batch set operations
    pub async fn batch_set<T>(
        &self,
        data_map: HashMap<String, T>,
        entry_type: CacheEntryType,
        custom_ttl: Option<u64>,
    ) -> ArbitrageResult<()>
    where
        T: serde::Serialize,
    {
        let keys: Vec<String> = data_map.keys().cloned().collect();

        // Process in batches
        for chunk in keys.chunks(self.config.batch_size) {
            for key in chunk {
                if let Some(data) = data_map.get(key) {
                    if let Err(e) = self.set(key, data, entry_type.clone(), custom_ttl).await {
                        self.logger
                            .warn(&format!("Batch set failed for key {}: {}", key, e));
                    }
                }
            }
        }

        self.record_batch_operation().await;
        Ok(())
    }

    /// Clean up expired entries
    pub async fn cleanup_expired(&self) -> ArbitrageResult<u64> {
        let mut expired_count = 0;
        let mut keys_to_remove = Vec::new();

        // Check metadata for expired entries
        if let Ok(metadata) = self.entry_metadata.lock() {
            for (key, entry) in metadata.iter() {
                if entry.is_expired() {
                    keys_to_remove.push(key.clone());
                }
            }
        }

        // Remove expired entries
        for key in keys_to_remove {
            if self.delete(&key).await.is_ok() {
                expired_count += 1;
            }
        }

        if expired_count > 0 {
            self.logger.info(&format!(
                "Cleaned up {} expired cache entries",
                expired_count
            ));
        }

        Ok(expired_count)
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        if let Ok(stats) = self.cache_metrics.lock() {
            stats.clone()
        } else {
            CacheStats::default()
        }
    }

    /// Get cache health
    pub async fn get_health(&self) -> ComponentHealth {
        if let Ok(health) = self.health_status.lock() {
            health.clone()
        } else {
            ComponentHealth::default()
        }
    }

    /// Health check for cache layer
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        let _start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Test KV store availability
        let test_key = "health_check_test";
        let test_data = "test";

        match self.kv_store.put(test_key, test_data)?.execute().await {
            Ok(_) => {
                // Try to read it back
                match self.kv_store.get(test_key).text().await {
                    Ok(Some(_)) => {
                        // Clean up test key
                        let _ = self.kv_store.delete(test_key).await;

                        // Update health status
                        if let Ok(mut health) = self.health_status.lock() {
                            health.is_healthy = true;
                            health.last_check = chrono::Utc::now().timestamp_millis() as u64;
                            health.last_error = None;

                            // Update hit rate from stats
                            if let Ok(stats) = self.cache_metrics.lock() {
                                health.performance_score = stats.hit_rate_percent / 100.0;
                            }
                        }

                        Ok(true)
                    }
                    Ok(None) => {
                        self.record_health_failure("KV store read test failed")
                            .await;
                        Ok(false)
                    }
                    Err(e) => {
                        self.record_health_failure(&format!("KV store read failed: {}", e))
                            .await;
                        Ok(false)
                    }
                }
            }
            Err(e) => {
                self.record_health_failure(&format!("KV store write failed: {}", e))
                    .await;
                Ok(false)
            }
        }
    }

    /// Check if key is compressed
    fn is_compressed_key(&self, key: &str) -> bool {
        key.starts_with("compressed:")
    }

    /// Compress data (placeholder implementation)
    fn compress_data(&self, data: &str) -> ArbitrageResult<String> {
        // In a real implementation, this would use a compression library like flate2
        // For now, we'll just return the original data
        Ok(data.to_string())
    }

    /// Decompress data (placeholder implementation)
    fn decompress_data(&self, data: &str) -> ArbitrageResult<String> {
        // In a real implementation, this would decompress the data
        // For now, we'll just return the original data
        Ok(data.to_string())
    }

    /// Record cache hit
    async fn record_hit(&self, _start_time: u64) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        if let Ok(mut stats) = self.cache_metrics.lock() {
            stats.total_hits += 1;
            let total_requests = stats.total_hits + stats.total_misses;
            if total_requests > 0 {
                stats.hit_rate_percent = stats.total_hits as f32 / total_requests as f32 * 100.0;
            }
            stats.last_updated = end_time;
        }
    }

    /// Record cache miss
    async fn record_miss(&self, _start_time: u64) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        if let Ok(mut stats) = self.cache_metrics.lock() {
            stats.total_misses += 1;
            let total_requests = stats.total_hits + stats.total_misses;
            if total_requests > 0 {
                stats.hit_rate_percent = stats.total_hits as f32 / total_requests as f32 * 100.0;
            }
            stats.last_updated = end_time;
        }
    }

    /// Record cache set operation
    async fn record_set(
        &self,
        _start_time: u64,
        original_size: usize,
        compressed_size: Option<usize>,
    ) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        if let Ok(mut stats) = self.cache_metrics.lock() {
            stats.total_sets += 1;
            stats.total_entries += 1;
            stats.total_size_bytes += original_size as u64;

            if compressed_size.is_some() {
                stats.compressed_entries += 1;
                let savings = original_size - compressed_size.unwrap_or(original_size);
                stats.compression_ratio_percent = savings as f32 / original_size as f32 * 100.0;
            }

            stats.last_updated = end_time;
        }
    }

    /// Record cache delete operation
    async fn record_delete(&self, _start_time: u64) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        if let Ok(mut stats) = self.cache_metrics.lock() {
            stats.total_deletes += 1;
            if stats.total_entries > 0 {
                stats.total_entries -= 1;
            }
            stats.last_updated = end_time;
        }
    }

    /// Record batch operation
    async fn record_batch_operation(&self) {
        if let Ok(mut stats) = self.cache_metrics.lock() {
            stats.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Record operation failure
    async fn record_failure(&self, _start_time: u64, error: &ArbitrageError) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        if let Ok(mut health) = self.health_status.lock() {
            health.last_error = Some(error.to_string());
            health.last_check = end_time;
        }
    }

    /// Record health check failure
    async fn record_health_failure(&self, error: &str) {
        if let Ok(mut health) = self.health_status.lock() {
            health.is_healthy = false;
            health.last_error = Some(error.to_string());
            health.last_check = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Update cache hit metrics
    async fn update_cache_hit_metrics(&self) {
        if let Ok(mut stats) = self.cache_metrics.lock() {
            stats.total_hits += 1;
            let total_requests = stats.total_hits + stats.total_misses;
            if total_requests > 0 {
                stats.hit_rate_percent = stats.total_hits as f32 / total_requests as f32 * 100.0;
            }
            stats.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }

    /// Update cache miss metrics
    async fn update_cache_miss_metrics(&self) {
        if let Ok(mut stats) = self.cache_metrics.lock() {
            stats.total_misses += 1;
            let total_requests = stats.total_hits + stats.total_misses;
            if total_requests > 0 {
                stats.hit_rate_percent = stats.total_hits as f32 / total_requests as f32 * 100.0;
            }
            stats.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_entry_type_ttl() {
        assert_eq!(CacheEntryType::MarketData.default_ttl_seconds(), 300);
        assert_eq!(CacheEntryType::FundingRates.default_ttl_seconds(), 900);
        assert_eq!(CacheEntryType::Analytics.default_ttl_seconds(), 1800);
        assert_eq!(CacheEntryType::UserPreferences.default_ttl_seconds(), 3600);
    }

    #[test]
    fn test_cache_entry_freshness() {
        let mut entry = CacheEntry::new(
            "test_key".to_string(),
            "test_data".to_string(),
            CacheEntryType::MarketData,
        );

        // Fresh entry should be fresh
        assert!(entry.is_fresh(0.8));

        // Simulate aging
        entry.created_at = chrono::Utc::now().timestamp_millis() as u64 - 240000; // 4 minutes ago
        entry.update_freshness();

        // Should still be fresh (TTL is 5 minutes)
        assert!(entry.is_fresh(0.8));
    }

    #[test]
    fn test_cache_layer_config_validation() {
        let mut config = CacheLayerConfig::default();
        assert!(config.validate().is_ok());

        config.freshness_threshold = 1.5; // Invalid
        assert!(config.validate().is_err());

        config.freshness_threshold = 0.8; // Valid
        config.batch_size = 0; // Invalid
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_cache_stats_default() {
        let stats = CacheStats::default();
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.total_hits, 0);
        assert_eq!(stats.hit_rate_percent, 0.0);
    }

    #[test]
    fn test_cache_health_default() {
        let health = CacheHealth::default();
        assert!(!health.is_healthy);
        assert!(!health.kv_store_available);
        assert_eq!(health.hit_rate_percent, 0.0);
    }
}
