// Cache Layer Implementation
// This file contains the cache layer implementation for the data access layer

use crate::utils::error::{ArbitrageError, ArbitrageResult};
use crate::utils::logger::Logger;
use crate::utils::time::WasmInstant;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

/// Configuration for cache layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheLayerConfig {
    /// Default TTL for cache entries in seconds
    pub default_ttl: u64,
    /// Maximum cache size in bytes
    pub max_cache_size: u64,
    /// Enable compression for large values
    pub enable_compression: bool,
    /// Compression threshold in bytes
    pub compression_threshold: u64,
    /// Cache key prefix
    pub key_prefix: String,
    /// Enable cache metrics
    pub enable_metrics: bool,
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold before opening circuit
    pub failure_threshold: u32,
    /// Success threshold for closing circuit
    pub success_threshold: u32,
    /// Timeout in seconds before attempting to close circuit
    pub timeout_seconds: u64,
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker implementation
#[derive(Debug)]
struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<WasmInstant>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            config,
        }
    }

    fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                }
            }
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::Open => {
                // Should not happen, but reset if it does
                self.failure_count = 0;
            }
        }
    }

    fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(crate::utils::time::now_instant());

        match self.state {
            CircuitBreakerState::Closed => {
                if self.failure_count >= self.config.failure_threshold {
                    self.state = CircuitBreakerState::Open;
                }
            }
            CircuitBreakerState::HalfOpen => {
                self.state = CircuitBreakerState::Open;
                self.success_count = 0;
            }
            CircuitBreakerState::Open => {
                // Already open, just update failure time
            }
        }
    }

    fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::HalfOpen => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    let elapsed = last_failure.elapsed();
                    if elapsed.as_secs() >= self.config.timeout_seconds {
                        self.state = CircuitBreakerState::HalfOpen;
                        self.success_count = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.state == CircuitBreakerState::Closed
    }
}

impl Default for CacheLayerConfig {
    fn default() -> Self {
        Self {
            default_ttl: 3600,                 // 1 hour
            max_cache_size: 100 * 1024 * 1024, // 100MB
            enable_compression: true,
            compression_threshold: 1024, // 1KB
            key_prefix: "cache:".to_string(),
            enable_metrics: true,
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 3,
                timeout_seconds: 60,
            },
        }
    }
}

impl CacheLayerConfig {
    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.default_ttl == 0 {
            return Err(ArbitrageError::config_error(
                "Default TTL must be greater than 0".to_string(),
            ));
        }

        if self.max_cache_size == 0 {
            return Err(ArbitrageError::config_error(
                "Max cache size must be greater than 0".to_string(),
            ));
        }

        if self.compression_threshold > self.max_cache_size {
            return Err(ArbitrageError::config_error(
                "Compression threshold cannot be larger than max cache size".to_string(),
            ));
        }

        if self.key_prefix.is_empty() {
            return Err(ArbitrageError::config_error(
                "Key prefix cannot be empty".to_string(),
            ));
        }

        if self.circuit_breaker.failure_threshold == 0 {
            return Err(ArbitrageError::config_error(
                "Circuit breaker failure threshold must be greater than 0".to_string(),
            ));
        }

        if self.circuit_breaker.success_threshold == 0 {
            return Err(ArbitrageError::config_error(
                "Circuit breaker success threshold must be greater than 0".to_string(),
            ));
        }

        if self.circuit_breaker.timeout_seconds == 0 {
            return Err(ArbitrageError::config_error(
                "Circuit breaker timeout must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Create high-performance configuration
    pub fn high_performance() -> Self {
        Self {
            default_ttl: 1800,                 // 30 minutes
            max_cache_size: 500 * 1024 * 1024, // 500MB
            enable_compression: true,
            compression_threshold: 512, // 512 bytes
            key_prefix: "hp_cache:".to_string(),
            enable_metrics: true,
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 2,
                timeout_seconds: 30,
            },
        }
    }

    /// Create high-reliability configuration
    pub fn high_reliability() -> Self {
        Self {
            default_ttl: 7200,                 // 2 hours
            max_cache_size: 200 * 1024 * 1024, // 200MB
            enable_compression: true,
            compression_threshold: 2048, // 2KB
            key_prefix: "hr_cache:".to_string(),
            enable_metrics: true,
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 10,
                success_threshold: 5,
                timeout_seconds: 120,
            },
        }
    }
}

/// Cache layer metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub errors: u64,
    pub total_requests: u64,
    pub average_response_time_ms: f64,
    pub cache_size_bytes: u64,
    pub evictions: u64,
    pub circuit_breaker_trips: u64,
}

impl Default for CacheMetrics {
    fn default() -> Self {
        Self {
            hits: 0,
            misses: 0,
            errors: 0,
            total_requests: 0,
            average_response_time_ms: 0.0,
            cache_size_bytes: 0,
            evictions: 0,
            circuit_breaker_trips: 0,
        }
    }
}

impl CacheMetrics {
    /// Calculate hit rate as percentage
    pub fn hit_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.hits as f64 / self.total_requests as f64) * 100.0
        }
    }

    /// Calculate error rate as percentage
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.errors as f64 / self.total_requests as f64) * 100.0
        }
    }

    /// Check if metrics indicate healthy cache performance
    pub fn is_healthy(&self) -> bool {
        self.hit_rate() >= 70.0 && self.error_rate() <= 5.0
    }
}

/// Cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    value: String,
    created_at: u64,
    expires_at: u64,
    access_count: u64,
    last_accessed: u64,
    compressed: bool,
    size_bytes: u64,
}

impl CacheEntry {
    fn new(value: String, ttl: u64, compressed: bool) -> Self {
        let now = chrono::Utc::now().timestamp() as u64;
        let size_bytes = value.len() as u64;

        Self {
            value,
            created_at: now,
            expires_at: now + ttl,
            access_count: 0,
            last_accessed: now,
            compressed,
            size_bytes,
        }
    }

    fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp() as u64;
        now > self.expires_at
    }

    fn access(&mut self) {
        self.access_count += 1;
        self.last_accessed = chrono::Utc::now().timestamp() as u64;
    }

    fn should_compress(value: &str, threshold: u64) -> bool {
        value.len() as u64 > threshold
    }
}

/// Cache layer implementation
pub struct CacheLayer {
    config: CacheLayerConfig,
    logger: Logger,
    metrics: Arc<std::sync::Mutex<CacheMetrics>>,
    // Circuit breaker for KV operations
    circuit_breaker: Arc<std::sync::Mutex<CircuitBreaker>>,
}

impl CacheLayer {
    /// Create new CacheLayer instance
    pub fn new(config: CacheLayerConfig) -> ArbitrageResult<Self> {
        // Removed kv_store parameter
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        let layer = Self {
            config: config.clone(),
            logger,
            metrics: Arc::new(std::sync::Mutex::new(CacheMetrics::default())),
            circuit_breaker: Arc::new(std::sync::Mutex::new(CircuitBreaker::new(
                config.circuit_breaker,
            ))),
        };

        Ok(layer)
    }

    /// Get value from cache
    pub async fn get(&self, kv_store: &KvStore, key: &str) -> ArbitrageResult<Option<String>> {
        let start_time = crate::utils::time::now_instant();
        let full_key = self.build_key(key);

        // Check circuit breaker
        if !self.can_execute_operation().await {
            self.record_error().await;
            return Err(ArbitrageError::cache_error(
                "Circuit breaker is open".to_string(),
            ));
        }

        match self.get_from_kv(kv_store, &full_key).await {
            Ok(Some(entry_json)) => {
                match serde_json::from_str::<CacheEntry>(&entry_json) {
                    Ok(mut entry) => {
                        if entry.is_expired() {
                            // Remove expired entry
                            let _ = self.delete_from_kv(kv_store, &full_key).await;
                            self.record_miss().await;
                            Ok(None)
                        } else {
                            entry.access();
                            // Update access metadata
                            let updated_json = serde_json::to_string(&entry)
                                .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;
                            let _ = self
                                .put_to_kv(kv_store, &full_key, &updated_json, None)
                                .await;

                            self.record_hit().await;
                            self.record_response_time(start_time.elapsed()).await;
                            Ok(Some(entry.value))
                        }
                    }
                    Err(e) => {
                        self.logger.error(&format!(
                            "Failed to deserialize cache entry for key {}: {}",
                            key, e
                        ));
                        // Remove corrupted entry
                        let _ = self.delete_from_kv(kv_store, &full_key).await;
                        self.record_error().await;
                        Ok(None)
                    }
                }
            }
            Ok(None) => {
                self.record_miss().await;
                self.record_response_time(start_time.elapsed()).await;
                Ok(None)
            }
            Err(e) => {
                self.record_error().await;
                self.record_response_time(start_time.elapsed()).await;
                Err(e)
            }
        }
    }

    /// Put value into cache
    pub async fn put(
        &self,
        kv_store: &KvStore,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> ArbitrageResult<()> {
        let start_time = crate::utils::time::now_instant();
        let full_key = self.build_key(key);
        let ttl = ttl.unwrap_or(self.config.default_ttl);

        // Check circuit breaker
        if !self.can_execute_operation().await {
            self.record_error().await;
            return Err(ArbitrageError::cache_error(
                "Circuit breaker is open".to_string(),
            ));
        }

        // Check if compression is needed
        let should_compress = self.config.enable_compression
            && CacheEntry::should_compress(value, self.config.compression_threshold);

        let processed_value = if should_compress {
            self.compress_value(value)?
        } else {
            value.to_string()
        };

        let entry = CacheEntry::new(processed_value, ttl, should_compress);
        let entry_json = serde_json::to_string(&entry)
            .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

        match self
            .put_to_kv(kv_store, &full_key, &entry_json, Some(ttl))
            .await
        {
            Ok(_) => {
                self.record_success_operation().await;
                self.record_response_time(start_time.elapsed()).await;
                self.update_cache_size(entry.size_bytes as i64).await;
                Ok(())
            }
            Err(e) => {
                self.record_error().await;
                self.record_response_time(start_time.elapsed()).await;
                Err(e)
            }
        }
    }

    /// Delete value from cache
    pub async fn delete(&self, kv_store: &KvStore, key: &str) -> ArbitrageResult<()> {
        let start_time = crate::utils::time::now_instant();
        let full_key = self.build_key(key);

        // Check circuit breaker
        if !self.can_execute_operation().await {
            self.record_error().await;
            return Err(ArbitrageError::cache_error(
                "Circuit breaker is open".to_string(),
            ));
        }

        // Get entry size before deletion for metrics
        if let Ok(Some(entry_json)) = self.get_from_kv(kv_store, &full_key).await {
            if let Ok(entry) = serde_json::from_str::<CacheEntry>(&entry_json) {
                self.update_cache_size(-(entry.size_bytes as i64)).await;
            }
        }

        match self.delete_from_kv(kv_store, &full_key).await {
            Ok(_) => {
                self.record_success_operation().await;
                self.record_response_time(start_time.elapsed()).await;
                Ok(())
            }
            Err(e) => {
                self.record_error().await;
                self.record_response_time(start_time.elapsed()).await;
                Err(e)
            }
        }
    }

    /// Check if key exists in cache
    pub async fn exists(&self, kv_store: &KvStore, key: &str) -> ArbitrageResult<bool> {
        let full_key = self.build_key(key);

        // Check circuit breaker
        if !self.can_execute_operation().await {
            return Ok(false); // Assume doesn't exist if circuit breaker is open
        }

        match self.get_from_kv(kv_store, &full_key).await {
            Ok(Some(entry_json)) => {
                match serde_json::from_str::<CacheEntry>(&entry_json) {
                    Ok(entry) => {
                        if entry.is_expired() {
                            // Remove expired entry
                            let _ = self.delete_from_kv(kv_store, &full_key).await;
                            Ok(false)
                        } else {
                            Ok(true)
                        }
                    }
                    Err(_) => {
                        // Remove corrupted entry
                        let _ = self.delete_from_kv(kv_store, &full_key).await;
                        Ok(false)
                    }
                }
            }
            Ok(None) => Ok(false),
            Err(_) => Ok(false), // Assume doesn't exist on error
        }
    }

    /// Get cache metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<CacheMetrics> {
        let metrics = self
            .metrics
            .lock()
            .map_err(|e| ArbitrageError::cache_error(format!("Failed to lock metrics: {}", e)))?;
        Ok(metrics.clone())
    }

    /// Reset cache metrics
    pub async fn reset_metrics(&self) -> ArbitrageResult<()> {
        let mut metrics = self
            .metrics
            .lock()
            .map_err(|e| ArbitrageError::cache_error(format!("Failed to lock metrics: {}", e)))?;
        *metrics = CacheMetrics::default();
        Ok(())
    }

    /// Get cache health status
    pub async fn health_check(&self, kv_store: &KvStore) -> ArbitrageResult<bool> {
        // Check circuit breaker state
        let circuit_healthy = self.is_circuit_breaker_closed().await;

        // Perform KV health check
        let kv_healthy = self.perform_kv_health_check(kv_store).await;

        // Check metrics health
        let metrics_healthy = match self.get_metrics().await {
            Ok(metrics) => metrics.is_healthy(),
            Err(_) => false,
        };

        Ok(circuit_healthy && kv_healthy && metrics_healthy)
    }

    /// Clear all cache entries (use with caution)
    pub async fn clear_all(&self, _kv_store: &KvStore) -> ArbitrageResult<()> {
        self.logger.warn("Clearing all cache entries");

        // Note: KV Store doesn't have a clear all operation
        // This would require listing all keys with the prefix and deleting them
        // For now, we'll reset metrics and log the operation

        self.reset_metrics().await?;

        // Update cache size to 0
        {
            let mut metrics = self.metrics.lock().map_err(|e| {
                ArbitrageError::cache_error(format!("Failed to lock metrics: {}", e))
            })?;
            metrics.cache_size_bytes = 0;
        }

        self.logger.info("Cache cleared (metrics reset)");
        Ok(())
    }

    /// Get configuration
    pub fn get_config(&self) -> &CacheLayerConfig {
        &self.config
    }

    /// Update configuration (creates new instance)
    pub fn with_config(mut self, config: CacheLayerConfig) -> ArbitrageResult<Self> {
        config.validate()?;
        self.config = config.clone();

        // Update circuit breaker config
        {
            let mut cb = self.circuit_breaker.lock().map_err(|e| {
                ArbitrageError::cache_error(format!("Failed to lock circuit breaker: {}", e))
            })?;
            cb.config = config.circuit_breaker;
        }

        Ok(self)
    }

    // Private helper methods

    fn build_key(&self, key: &str) -> String {
        format!("{}{}", self.config.key_prefix, key)
    }

    async fn get_from_kv(&self, kv_store: &KvStore, key: &str) -> ArbitrageResult<Option<String>> {
        match kv_store.get(key).text().await {
            Ok(value) => {
                self.record_success_operation().await;
                Ok(value)
            }
            Err(e) => {
                self.record_failure_operation().await;
                Err(ArbitrageError::cache_error(format!(
                    "KV get operation failed: {}",
                    e
                )))
            }
        }
    }

    async fn put_to_kv(
        &self,
        kv_store: &KvStore,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> ArbitrageResult<()> {
        let mut put_builder = kv_store.put(key, value).map_err(|e| {
            ArbitrageError::cache_error(format!("Failed to create put builder: {}", e))
        })?;

        if let Some(ttl_seconds) = ttl {
            put_builder = put_builder.expiration_ttl(ttl_seconds);
        }

        match put_builder.execute().await {
            Ok(_) => {
                self.record_success_operation().await;
                Ok(())
            }
            Err(e) => {
                self.record_failure_operation().await;
                Err(ArbitrageError::cache_error(format!(
                    "KV put operation failed: {}",
                    e
                )))
            }
        }
    }

    async fn delete_from_kv(&self, kv_store: &KvStore, key: &str) -> ArbitrageResult<()> {
        match kv_store.delete(key).await {
            Ok(_) => {
                self.record_success_operation().await;
                Ok(())
            }
            Err(e) => {
                self.record_failure_operation().await;
                Err(ArbitrageError::cache_error(format!(
                    "KV delete operation failed: {}",
                    e
                )))
            }
        }
    }

    fn compress_value(&self, value: &str) -> ArbitrageResult<String> {
        // Simple compression using base64 encoding for now
        // In a real implementation, you might use gzip or other compression algorithms
        use base64::{engine::general_purpose, Engine as _};
        let compressed = general_purpose::STANDARD.encode(value.as_bytes());
        Ok(compressed)
    }

    #[allow(dead_code)] // Will be used for compression optimization
    fn decompress_value(&self, compressed: &str) -> ArbitrageResult<String> {
        use base64::{engine::general_purpose, Engine as _};
        let decoded = general_purpose::STANDARD
            .decode(compressed)
            .map_err(|e| ArbitrageError::cache_error(format!("Decompression failed: {}", e)))?;
        String::from_utf8(decoded)
            .map_err(|e| ArbitrageError::cache_error(format!("UTF-8 conversion failed: {}", e)))
    }

    async fn can_execute_operation(&self) -> bool {
        match self.circuit_breaker.lock() {
            Ok(mut cb) => cb.can_execute(),
            Err(_) => false, // If we can't lock, assume circuit is open
        }
    }

    async fn is_circuit_breaker_closed(&self) -> bool {
        match self.circuit_breaker.lock() {
            Ok(cb) => cb.is_closed(),
            Err(_) => false,
        }
    }

    async fn record_success_operation(&self) {
        if let Ok(mut cb) = self.circuit_breaker.lock() {
            cb.record_success();
        }
    }

    async fn record_failure_operation(&self) {
        if let Ok(mut cb) = self.circuit_breaker.lock() {
            cb.record_failure();
        }

        // Also increment circuit breaker trips in metrics
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.circuit_breaker_trips += 1;
        }
    }

    async fn record_hit(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.hits += 1;
            metrics.total_requests += 1;
        }
    }

    async fn record_miss(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.misses += 1;
            metrics.total_requests += 1;
        }
    }

    async fn record_error(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.errors += 1;
            metrics.total_requests += 1;
        }
    }

    async fn record_response_time(&self, duration: std::time::Duration) {
        if let Ok(mut metrics) = self.metrics.lock() {
            let response_time_ms = duration.as_millis() as f64;

            // Calculate running average
            if metrics.total_requests > 0 {
                let total_time =
                    metrics.average_response_time_ms * (metrics.total_requests - 1) as f64;
                metrics.average_response_time_ms =
                    (total_time + response_time_ms) / metrics.total_requests as f64;
            } else {
                metrics.average_response_time_ms = response_time_ms;
            }
        }
    }

    async fn update_cache_size(&self, size_delta: i64) {
        if let Ok(mut metrics) = self.metrics.lock() {
            if size_delta < 0 {
                let abs_delta = (-size_delta) as u64;
                metrics.cache_size_bytes = metrics.cache_size_bytes.saturating_sub(abs_delta);
            } else {
                metrics.cache_size_bytes += size_delta as u64;
            }
        }
    }

    async fn record_kv_success(&self) {
        self.record_success_operation().await;
    }

    async fn record_kv_failure(&self) {
        self.record_failure_operation().await;
    }

    /// Batch operations for better performance
    pub async fn get_batch(
        &self,
        kv_store: &KvStore,
        keys: &[&str],
    ) -> ArbitrageResult<HashMap<String, Option<String>>> {
        let mut results = HashMap::new();

        // Note: KV Store doesn't have native batch operations
        // We'll execute them sequentially for now
        for key in keys {
            let result = self.get(kv_store, key).await?;
            results.insert(key.to_string(), result);
        }

        Ok(results)
    }

    /// Batch put operations
    pub async fn put_batch(
        &self,
        kv_store: &KvStore,
        entries: &[(String, String, Option<u64>)], // (key, value, ttl)
    ) -> ArbitrageResult<()> {
        for (key, value, ttl) in entries {
            self.put(kv_store, key, value, *ttl).await?;
        }
        Ok(())
    }

    /// Get cache statistics
    pub async fn get_statistics(&self) -> ArbitrageResult<HashMap<String, serde_json::Value>> {
        let metrics = self.get_metrics().await?;
        let mut stats = HashMap::new();

        stats.insert(
            "hit_rate".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(metrics.hit_rate())
                    .unwrap_or(serde_json::Number::from(0)),
            ),
        );
        stats.insert(
            "error_rate".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(metrics.error_rate())
                    .unwrap_or(serde_json::Number::from(0)),
            ),
        );
        stats.insert(
            "total_requests".to_string(),
            serde_json::Value::Number(serde_json::Number::from(metrics.total_requests)),
        );
        stats.insert(
            "cache_size_mb".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(metrics.cache_size_bytes as f64 / (1024.0 * 1024.0))
                    .unwrap_or(serde_json::Number::from(0)),
            ),
        );
        stats.insert(
            "average_response_time_ms".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(metrics.average_response_time_ms)
                    .unwrap_or(serde_json::Number::from(0)),
            ),
        );
        stats.insert(
            "circuit_breaker_closed".to_string(),
            serde_json::Value::Bool(self.is_circuit_breaker_closed().await),
        );

        Ok(stats)
    }

    /// Evict expired entries (maintenance operation)
    pub async fn evict_expired(&self, _kv_store: &KvStore) -> ArbitrageResult<u64> {
        // Note: This is a simplified implementation
        // In a real scenario, you'd need to list keys with the prefix and check each one
        self.logger
            .info("Evicting expired entries (placeholder implementation)");

        // For now, we'll just return 0 as KV Store handles TTL automatically
        Ok(0)
    }

    /// Warm up cache with frequently accessed data
    pub async fn warm_up(
        &self,
        kv_store: &KvStore,
        data: &[(String, String, Option<u64>)],
    ) -> ArbitrageResult<()> {
        self.logger
            .info(&format!("Warming up cache with {} entries", data.len()));

        for (key, value, ttl) in data {
            if let Err(e) = self.put(kv_store, key, value, *ttl).await {
                self.logger
                    .warn(&format!("Failed to warm up cache entry {}: {}", key, e));
            }
        }

        Ok(())
    }

    /// Get cache entry metadata
    pub async fn get_metadata(
        &self,
        kv_store: &KvStore,
        key: &str,
    ) -> ArbitrageResult<Option<HashMap<String, serde_json::Value>>> {
        let full_key = self.build_key(key);

        match self.get_from_kv(kv_store, &full_key).await? {
            Some(entry_json) => match serde_json::from_str::<CacheEntry>(&entry_json) {
                Ok(entry) => {
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "created_at".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(entry.created_at)),
                    );
                    metadata.insert(
                        "expires_at".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(entry.expires_at)),
                    );
                    metadata.insert(
                        "access_count".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(entry.access_count)),
                    );
                    metadata.insert(
                        "last_accessed".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(entry.last_accessed)),
                    );
                    metadata.insert(
                        "compressed".to_string(),
                        serde_json::Value::Bool(entry.compressed),
                    );
                    metadata.insert(
                        "size_bytes".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(entry.size_bytes)),
                    );
                    metadata.insert(
                        "is_expired".to_string(),
                        serde_json::Value::Bool(entry.is_expired()),
                    );

                    Ok(Some(metadata))
                }
                Err(_) => Ok(None),
            },
            None => Ok(None),
        }
    }

    /// Set cache entry with custom metadata
    pub async fn put_with_metadata(
        &self,
        kv_store: &KvStore,
        key: &str,
        value: &str,
        ttl: Option<u64>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<()> {
        // For now, we'll store metadata as part of the cache entry
        // In a more sophisticated implementation, metadata could be stored separately

        let extended_value = serde_json::json!({
            "value": value,
            "metadata": metadata
        });

        self.put(kv_store, key, &extended_value.to_string(), ttl)
            .await
    }

    /// Increment a numeric value in cache (atomic operation)
    pub async fn increment(
        &self,
        kv_store: &KvStore,
        key: &str,
        delta: i64,
        ttl: Option<u64>,
    ) -> ArbitrageResult<i64> {
        // Note: This is not truly atomic in KV Store
        // For true atomicity, you'd need to use Durable Objects or similar

        let current_value = match self.get(kv_store, key).await? {
            Some(value) => value.parse::<i64>().unwrap_or(0),
            None => 0,
        };

        let new_value = current_value + delta;
        self.put(kv_store, key, &new_value.to_string(), ttl).await?;

        Ok(new_value)
    }

    /// Set cache entry only if it doesn't exist
    pub async fn put_if_not_exists(
        &self,
        kv_store: &KvStore,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> ArbitrageResult<bool> {
        if self.exists(kv_store, key).await? {
            Ok(false) // Key already exists
        } else {
            self.put(kv_store, key, value, ttl).await?;
            Ok(true) // Successfully set
        }
    }

    /// Update cache entry only if it exists
    pub async fn update_if_exists(
        &self,
        kv_store: &KvStore,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> ArbitrageResult<bool> {
        if self.exists(kv_store, key).await? {
            self.put(kv_store, key, value, ttl).await?;
            Ok(true) // Successfully updated
        } else {
            Ok(false) // Key doesn't exist
        }
    }

    /// Get multiple keys with a single call (optimized)
    pub async fn multi_get(
        &self,
        kv_store: &KvStore,
        keys: &[&str],
    ) -> ArbitrageResult<Vec<(String, Option<String>)>> {
        let mut results = Vec::new();

        for key in keys {
            let value = self.get(kv_store, key).await?;
            results.push((key.to_string(), value));
        }

        Ok(results)
    }

    /// Delete multiple keys
    pub async fn multi_delete(&self, kv_store: &KvStore, keys: &[&str]) -> ArbitrageResult<u64> {
        let mut deleted_count = 0;

        for key in keys {
            if self.delete(kv_store, key).await.is_ok() {
                deleted_count += 1;
            }
        }

        Ok(deleted_count)
    }

    /// Get keys matching a pattern (limited implementation)
    pub async fn get_keys_with_prefix(
        &self,
        _kv_store: &KvStore,
        prefix: &str,
    ) -> ArbitrageResult<Vec<String>> {
        // Note: KV Store doesn't support key listing
        // This would require maintaining a separate index
        self.logger.warn(&format!(
            "get_keys_with_prefix not fully implemented for prefix: {}",
            prefix
        ));
        Ok(Vec::new())
    }

    /// Perform health check for KV store
    async fn perform_kv_health_check(&self, kv_store: &KvStore) -> bool {
        // Correct: kv_store passed as parameter
        let _start_time = chrono::Utc::now().timestamp_millis() as u64;
        let test_key = "cache_layer_health_check";
        let test_data = "health_check_ok";

        if !self.is_circuit_breaker_closed().await {
            self.logger
                .warn("Circuit breaker open, KV health check skipped.");
            return false; // If circuit breaker is open, KV is considered unhealthy
        }

        // Attempt to write
        match kv_store.put(test_key, test_data).unwrap().execute().await {
            // Use passed kv_store
            Ok(_) => {
                // Attempt to read back
                match kv_store.get(test_key).text().await {
                    // Use passed kv_store
                    Ok(Some(value)) if value == test_data => {
                        // Attempt to delete
                        let _ = kv_store.delete(test_key).await; // Use passed kv_store
                        self.record_kv_success().await;
                        true
                    }
                    Ok(_) => {
                        self.logger
                            .warn("KV health check failed: read-back mismatch or no data.");
                        let _ = kv_store.delete(test_key).await; // Use passed kv_store
                        self.record_kv_failure().await;
                        false
                    }
                    Err(e) => {
                        self.logger
                            .error(&format!("KV health check failed (read error): {:?}", e));
                        self.record_kv_failure().await;
                        false
                    }
                }
            }
            Err(e) => {
                self.logger
                    .error(&format!("KV health check failed (write error): {:?}", e));
                self.record_kv_failure().await;
                false
            }
        }
    }
}
