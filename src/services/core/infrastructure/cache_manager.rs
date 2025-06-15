// Cache Manager Module - Centralized KV Operations and Caching Strategies
// Consolidates caching logic from multiple services with optimized patterns for high concurrency

use crate::utils::error::{ArbitrageError, ArbitrageResult};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::{console_log, kv::KvStore};

// Helper for sleeping (abstracts worker::Delay)
async fn sleep_ms(ms: u64) {
    worker::Delay::from(std::time::Duration::from_millis(ms)).await;
}

/// Cache configuration for different data types and access patterns
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub default_ttl_seconds: u64,
    pub max_key_size_bytes: usize,
    pub max_value_size_bytes: usize,
    pub compression_enabled: bool,
    pub batch_size: usize,
    pub retry_attempts: u32,
    pub retry_delay_ms: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl_seconds: 3600,        // 1 hour default TTL
            max_key_size_bytes: 512,          // 512 bytes max key size
            max_value_size_bytes: 25_000_000, // 25MB max value size (KV limit)
            compression_enabled: true,        // Enable compression for large values
            batch_size: 50,                   // Batch operations for efficiency
            retry_attempts: 3,                // Retry failed operations
            retry_delay_ms: 100,              // Base delay for exponential backoff
        }
    }
}

/// Cache operation result with metadata
#[derive(Debug, Clone)]
pub struct CacheResult<T> {
    pub data: Option<T>,
    pub hit: bool,
    pub source: CacheSource,
    pub latency_ms: f64,
    pub compression_ratio: Option<f64>,
}

/// Cache data source types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CacheSource {
    Memory,   // KV cache hit
    Pipeline, // Pipeline data hit
    Api,      // Direct API call (uncached subrequest)
}

/// Cache operation types for metrics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CacheOperation {
    Get,
    Set,
    Delete,
    Exists,
    BatchGet,
    BatchSet,
    BatchDelete,
}

/// Cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub data: T,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub access_count: u64,
    pub last_accessed: u64,
    pub metadata: HashMap<String, String>,
}

/// Cache statistics for monitoring
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_operations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub hit_rate: f64,
    pub avg_operation_time_ms: f64,
    pub total_data_size_bytes: u64,
    pub key_count: u64,
    pub last_updated: u64,
}

/// Batch cache operation
#[derive(Debug, Clone)]
pub struct BatchCacheOperation {
    pub key: String,
    pub value: Option<String>,
    pub ttl_seconds: Option<u64>,
    pub operation: CacheOperation,
}

/// Cache health metrics
#[derive(Debug, Clone)]
pub struct CacheHealth {
    pub is_healthy: bool,
    pub response_time_ms: f64,
    pub error_rate: f64,
    pub memory_usage_percent: f64,
    pub last_error: Option<String>,
    pub uptime_seconds: u64,
}

impl Default for CacheHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            response_time_ms: 0.0,
            error_rate: 0.0,
            memory_usage_percent: 0.0,
            last_error: None,
            uptime_seconds: 0,
        }
    }
}

/// Centralized cache manager for all KV operations
pub struct CacheManager {
    kv_store: KvStore,
    config: CacheConfig,
    stats: Arc<std::sync::Mutex<CacheStats>>,
    namespace_prefix: String,
}

impl CacheManager {
    /// Create new CacheManager with default configuration
    pub fn new(kv_store: KvStore) -> Self {
        Self {
            kv_store,
            config: CacheConfig::default(),
            stats: Arc::new(std::sync::Mutex::new(CacheStats::default())),
            namespace_prefix: "arb_edge".to_string(),
        }
    }

    /// Create CacheManager with custom configuration
    pub fn new_with_config(kv_store: KvStore, config: CacheConfig, namespace: &str) -> Self {
        Self {
            kv_store,
            config,
            stats: Arc::new(std::sync::Mutex::new(CacheStats::default())),
            namespace_prefix: namespace.to_string(),
        }
    }

    /// Get value from cache with automatic deserialization
    pub async fn get<T>(&self, key: &str) -> ArbitrageResult<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let namespaced_key = self.build_key(key);

        let result = self.get_with_retry(&namespaced_key).await;
        let execution_time = chrono::Utc::now().timestamp_millis() as u64 - start_time;

        match result {
            Ok(Some(value_str)) => {
                match serde_json::from_str::<CacheEntry<T>>(&value_str) {
                    Ok(entry) => {
                        // Check if entry has expired
                        if let Some(expires_at) = entry.expires_at {
                            let now = chrono::Utc::now().timestamp_millis() as u64;
                            if now > expires_at {
                                // Entry expired, delete it and return None
                                let _ = self.delete::<T>(key).await;
                                self.update_stats(CacheOperation::Get, execution_time, false, 0);
                                return Ok(None);
                            }
                        }

                        self.update_stats(
                            CacheOperation::Get,
                            execution_time,
                            true,
                            value_str.len(),
                        );
                        Ok(Some(entry.data))
                    }
                    Err(_) => {
                        // Try to deserialize directly (backward compatibility)
                        match serde_json::from_str::<T>(&value_str) {
                            Ok(data) => {
                                self.update_stats(
                                    CacheOperation::Get,
                                    execution_time,
                                    true,
                                    value_str.len(),
                                );
                                Ok(Some(data))
                            }
                            Err(e) => {
                                self.update_stats(CacheOperation::Get, execution_time, false, 0);
                                Err(ArbitrageError::parse_error(format!(
                                    "Failed to deserialize cache value: {}",
                                    e
                                )))
                            }
                        }
                    }
                }
            } // Added missing closing brace for Ok(Some(value_str)) arm
            Ok(None) => {
                self.update_stats(CacheOperation::Get, execution_time, false, 0);
                Ok(None)
            }
            Err(e) => {
                self.update_stats(CacheOperation::Get, execution_time, false, 0);
                Err(e)
            }
        }
    }

    pub async fn set<T>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: Option<u64>,
    ) -> ArbitrageResult<CacheResult<T>>
    where
        T: Serialize + Clone, // Keep Clone bound, it's necessary for value.clone() below
    {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let namespaced_key = self.build_key(key);

        // Create cache entry with metadata
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let expires_at = ttl_seconds.map(|ttl| now + (ttl * 1000));

        let entry = CacheEntry {
            data: value.clone(), // Ensure value is cloned here
            created_at: now,
            expires_at,
            access_count: 0,
            last_accessed: now,
            metadata: HashMap::new(),
        };

        let value_str = serde_json::to_string(&entry).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize cache value: {}", e))
        })?;

        // Check value size
        if value_str.len() > self.config.max_value_size_bytes {
            return Err(ArbitrageError::validation_error(format!(
                "Cache value too large: {} bytes (max: {})",
                value_str.len(),
                self.config.max_value_size_bytes
            )));
        }

        let result = self
            .set_with_retry(&namespaced_key, &value_str, ttl_seconds)
            .await;
        let execution_time = chrono::Utc::now().timestamp_millis() as u64 - start_time;

        match result {
            Ok(_) => {
                self.update_stats(CacheOperation::Set, execution_time, true, value_str.len());
                Ok(CacheResult {
                    data: Some(value.clone()),
                    hit: false,
                    source: CacheSource::Memory,
                    latency_ms: 0.0,
                    compression_ratio: None,
                })
            }
            Err(e) => {
                self.update_stats(CacheOperation::Set, execution_time, false, 0);
                Err(e)
            }
        }
    }

    /// Delete value from cache
    pub async fn delete<T>(&self, key: &str) -> ArbitrageResult<CacheResult<T>> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;
        let namespaced_key = self.build_key(key);

        let result = self.delete_with_retry(&namespaced_key).await;
        let execution_time = chrono::Utc::now().timestamp_millis() as u64 - start_time;

        match result {
            Ok(_) => {
                self.update_stats(CacheOperation::Delete, execution_time, true, 0);
                Ok(CacheResult {
                    data: None,
                    hit: false,
                    source: CacheSource::Memory,
                    latency_ms: 0.0,
                    compression_ratio: None,
                })
            }
            Err(e) => {
                self.update_stats(CacheOperation::Delete, execution_time, false, 0);
                Err(e)
            }
        }
    }

    /// Check if key exists in cache
    pub async fn exists(&self, key: &str) -> ArbitrageResult<bool> {
        let namespaced_key = self.build_key(key);

        match self.get_with_retry(&namespaced_key).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(_) => Ok(false), // Treat errors as non-existence
        }
    }

    /// Batch get operations for high-throughput scenarios
    pub async fn batch_get<T>(&self, keys: &[&str]) -> ArbitrageResult<HashMap<String, T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut results = HashMap::new();
        let chunks = keys.chunks(self.config.batch_size);

        for chunk in chunks {
            for &key in chunk {
                if let Ok(Some(value)) = self.get::<T>(key).await {
                    results.insert(key.to_string(), value);
                }
            }
        }

        Ok(results)
    }

    /// Batch set operations for high-throughput scenarios
    pub async fn batch_set<T>(
        &self,
        operations: &[(&str, &T, Option<u64>)],
    ) -> ArbitrageResult<Vec<CacheResult<T>>>
    where
        T: Serialize + Clone,
    {
        let mut results = Vec::new();
        let chunks = operations.chunks(self.config.batch_size);

        for chunk in chunks {
            for &(key, value, ttl) in chunk {
                match self.set(key, value, ttl).await {
                    Ok(result) => results.push(result),
                    Err(_) => results.push(CacheResult {
                        data: None,
                        hit: false,
                        source: CacheSource::Memory,
                        latency_ms: 0.0,
                        compression_ratio: None,
                    }),
                }
            }
        }

        Ok(results)
    }

    /// Cache health check
    pub async fn health_check(&self) -> ArbitrageResult<CacheHealth> {
        let start_time = chrono::Utc::now().timestamp_millis();
        let test_key = format!("{}:health_check", self.namespace_prefix);
        let test_value = "health_check_value";

        // Test write operation
        let write_result = self
            .kv_store
            .put(&test_key, test_value)
            .map_err(|e| ArbitrageError::cache_error(format!("Health check write failed: {}", e)));

        // Test read operation
        let read_result = if write_result.is_ok() {
            self.kv_store.get(&test_key).text().await.map_err(|e| {
                ArbitrageError::cache_error(format!("Health check read failed: {}", e))
            })
        } else {
            Err(ArbitrageError::cache_error("Write operation failed"))
        };

        // Clean up test key
        let _ = self.kv_store.delete(&test_key).await;

        let response_time = chrono::Utc::now().timestamp_millis() - start_time;
        let is_healthy = write_result.is_ok() && read_result.is_ok();

        Ok(CacheHealth {
            is_healthy,
            response_time_ms: response_time as f64,
            error_rate: if is_healthy { 0.0 } else { 1.0 },
            memory_usage_percent: 0.0, // KV doesn't expose memory usage
            last_error: if is_healthy {
                None
            } else {
                Some("Health check failed".to_string())
            },
            uptime_seconds: 0, // Would be tracked by service health module
        })
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> CacheStats {
        self.stats.lock().unwrap().clone()
    }

    /// Reset cache statistics
    pub fn reset_stats(&self) {
        let mut stats = self.stats.lock().unwrap();
        *stats = CacheStats::default();
    }

    /// Enhanced cache-first data retrieval with pipeline integration
    pub async fn get_market_data_cached<T>(
        &self,
        cache_key: &str,
        exchange: &str,
        symbol: &str,
        fetch_fn: impl std::future::Future<Output = ArbitrageResult<T>>,
    ) -> ArbitrageResult<CacheResult<T>>
    where
        T: for<'de> serde::Deserialize<'de> + serde::Serialize + Clone,
    {
        // 1. Try KV cache first (fastest)
        if let Ok(Some(data)) = self.get::<T>(cache_key).await {
            console_log!("🎯 CACHE HIT - KV cache for key: {}", cache_key);
            return Ok(CacheResult {
                data: Some(data),
                hit: true,
                source: CacheSource::Memory,
                latency_ms: 0.0, // Cache hit is instant
                compression_ratio: None,
            });
        }

        // 2. Try pipeline data (if available)
        if let Some(pipeline_data) = self.get_from_pipeline::<T>(exchange, symbol).await? {
            console_log!("📊 PIPELINE HIT - Found data for {}:{}", exchange, symbol);
            // Cache pipeline data in KV for faster access
            let _ = self.set(cache_key, &pipeline_data, Some(300)).await; // 5 min TTL
            return Ok(CacheResult {
                data: Some(pipeline_data),
                hit: true,
                source: CacheSource::Pipeline,
                latency_ms: 0.0,
                compression_ratio: None,
            });
        }

        // 3. Fallback to direct API call (last resort - causes uncached subrequest)
        console_log!(
            "⚠️ CACHE MISS - Falling back to API for {}:{}",
            exchange,
            symbol
        );

        let start_time = crate::utils::time::now_instant();
        let api_data = fetch_fn.await?;
        let latency_ms = start_time.elapsed().as_millis() as f64;

        // Cache the API result for future requests
        let _ = self.set(cache_key, &api_data, Some(180)).await; // 3 min TTL

        // Record cache miss metrics
        self.record_cache_miss(cache_key).await;

        Ok(CacheResult {
            data: Some(api_data),
            hit: false,
            source: CacheSource::Api,
            latency_ms,
            compression_ratio: None,
        })
    }

    /// Get data from pipeline if available
    async fn get_from_pipeline<T>(&self, exchange: &str, symbol: &str) -> ArbitrageResult<Option<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        // Check if pipeline data is available in R2 or pipeline cache
        let pipeline_key = format!("pipeline:market_data:{}:{}", exchange, symbol);

        // Try to get from pipeline cache first
        match self.kv_store.get(&pipeline_key).text().await {
            Ok(Some(data_str)) => match serde_json::from_str::<T>(&data_str) {
                Ok(data) => return Ok(Some(data)),
                Err(e) => {
                    console_log!("❌ Pipeline data parse error: {}", e);
                }
            },
            Ok(None) => {
                console_log!("📊 No pipeline data for {}:{}", exchange, symbol);
            }
            Err(e) => {
                console_log!("❌ Pipeline cache error: {:?}", e);
            }
        }

        Ok(None)
    }

    /// Record cache miss for analytics
    async fn record_cache_miss(&self, cache_key: &str) {
        let miss_key = format!("cache_miss:{}", cache_key);
        let current_count = self
            .kv_store
            .get(&miss_key)
            .text()
            .await
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let _ = self
            .kv_store
            .put(&miss_key, (current_count + 1).to_string())
            .unwrap_or_else(|_| self.kv_store.put(&miss_key, "1".to_string()).unwrap())
            .expiration_ttl(86400) // 24 hours
            .execute()
            .await;
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

    // ============= SPECIALIZED CACHE OPERATIONS =============

    /// Cache user session data with optimized TTL
    pub async fn cache_user_session<T>(
        &self,
        user_id: &str,
        session_data: &T,
    ) -> ArbitrageResult<CacheResult<T>>
    where
        T: Serialize + Clone,
    {
        let key = format!("user_session:{}", user_id);
        let ttl = 3600; // 1 hour for user sessions
        self.set(&key, session_data, Some(ttl)).await
    }

    /// Cache market data with short TTL for real-time updates
    pub async fn cache_market_data<T>(
        &self,
        exchange: &str,
        symbol: &str,
        data: &T,
    ) -> ArbitrageResult<CacheResult<T>>
    where
        T: Serialize + Clone,
    {
        let key = format!("market_data:{}:{}", exchange, symbol);
        let ttl = 60; // 1 minute for market data
        self.set(&key, data, Some(ttl)).await
    }

    /// Cache opportunity data with medium TTL
    pub async fn cache_opportunity<T>(
        &self,
        opportunity_id: &str,
        data: &T,
    ) -> ArbitrageResult<CacheResult<T>>
    where
        T: Serialize + Clone,
    {
        let key = format!("opportunity:{}", opportunity_id);
        let ttl = 300; // 5 minutes for opportunities
        self.set(&key, data, Some(ttl)).await
    }

    /// Cache AI analysis results with longer TTL
    pub async fn cache_ai_analysis<T>(
        &self,
        analysis_id: &str,
        data: &T,
    ) -> ArbitrageResult<CacheResult<T>>
    where
        T: Serialize + Clone,
    {
        let key = format!("ai_analysis:{}", analysis_id);
        let ttl = 1800; // 30 minutes for AI analysis
        self.set(&key, data, Some(ttl)).await
    }

    /// Cache configuration data with long TTL
    pub async fn cache_config<T>(
        &self,
        config_key: &str,
        data: &T,
    ) -> ArbitrageResult<CacheResult<T>>
    where
        T: Serialize + Clone,
    {
        let key = format!("config:{}", config_key);
        let ttl = 7200; // 2 hours for configuration
        self.set(&key, data, Some(ttl)).await
    }

    // ============= INTERNAL HELPER METHODS =============

    // Utility to convert Duration to milliseconds

    fn build_key(&self, key: &str) -> String {
        // Validate key size
        let full_key = format!("{}:{}", self.namespace_prefix, key);
        if full_key.len() > self.config.max_key_size_bytes {
            // Truncate or hash long keys
            format!(
                "{}:hash:{:x}",
                self.namespace_prefix,
                md5::compute(key.as_bytes())
            )
        } else {
            full_key
        }
    }

    async fn get_with_retry(&self, key: &str) -> ArbitrageResult<Option<String>> {
        for attempt in 0..=self.config.retry_attempts {
            match self.kv_store.get(key).text().await {
                Ok(value) => return Ok(value),
                Err(e) => {
                    if attempt == self.config.retry_attempts {
                        return Err(ArbitrageError::cache_error(format!(
                            "Failed to get from cache: {}\n",
                            e
                        )));
                    }
                    // Exponential backoff
                    let delay_ms = self.config.retry_delay_ms * (2_u64.pow(attempt));
                    sleep_ms(delay_ms).await;
                }
            }
        }

        Err(ArbitrageError::cache_error("Max retries exceeded"))
    }

    async fn set_with_retry(
        &self,
        key: &str,
        value: &String,
        ttl_seconds: Option<u64>,
    ) -> ArbitrageResult<()> {
        for attempt in 0..=self.config.retry_attempts {
            let builder = self.kv_store.put(key, value)?;
            let builder = if let Some(ttl) = ttl_seconds {
                builder.expiration_ttl(ttl)
            } else {
                builder
            };
            let result = builder.execute().await.map_err(|e| {
                ArbitrageError::cache_error(format!("Failed to set in cache after execute: {}", e))
            });

            match result {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt == self.config.retry_attempts {
                        return Err(e);
                    }
                    // Exponential backoff
                    let delay_ms = self.config.retry_delay_ms * (2_u64.pow(attempt));
                    sleep_ms(delay_ms).await;
                }
            }
        }

        Err(ArbitrageError::cache_error("Max retries exceeded"))
    }

    async fn delete_with_retry(&self, key: &str) -> ArbitrageResult<()> {
        for attempt in 0..=self.config.retry_attempts {
            match self.kv_store.delete(key).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt == self.config.retry_attempts {
                        return Err(ArbitrageError::cache_error(format!(
                            "Failed to delete from cache: {}\n",
                            e
                        )));
                    }
                    // Exponential backoff
                    let delay_ms = self.config.retry_delay_ms * (2_u64.pow(attempt));
                    sleep_ms(delay_ms).await;
                }
            }
        }

        Err(ArbitrageError::cache_error("Max retries exceeded"))
    }

    fn update_stats(
        &self,
        operation: CacheOperation,
        execution_time_ms: u64,
        success: bool,
        data_size: usize,
    ) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.total_operations += 1;

            if success && operation == CacheOperation::Get {
                stats.cache_hits += 1;
            } else if !success && operation == CacheOperation::Get {
                stats.cache_misses += 1;
            }

            // Update hit rate
            let denom = stats.cache_hits + stats.cache_misses;
            if denom > 0 {
                stats.hit_rate = stats.cache_hits as f64 / denom as f64;
            }

            // Update average operation time
            let total_time = stats.avg_operation_time_ms * (stats.total_operations - 1) as f64
                + execution_time_ms as f64;
            stats.avg_operation_time_ms = total_time / stats.total_operations as f64;

            if success {
                stats.total_data_size_bytes += data_size as u64;
                if operation == CacheOperation::Set {
                    stats.key_count += 1;
                }
            }

            stats.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }
    }
} // End of impl CacheManager

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            total_operations: 0,
            cache_hits: 0,
            cache_misses: 0,
            hit_rate: 0.0,
            avg_operation_time_ms: 0.0,
            total_data_size_bytes: 0,
            key_count: 0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.default_ttl_seconds, 3600);
        assert_eq!(config.max_key_size_bytes, 512);
        assert_eq!(config.max_value_size_bytes, 25_000_000);
        assert!(config.compression_enabled);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.retry_attempts, 3);
    }

    #[test]
    fn test_cache_entry_creation() {
        let data = "test_data";
        let now = chrono::Utc::now().timestamp_millis() as u64;

        let entry = CacheEntry {
            data,
            created_at: now,
            expires_at: Some(now + 3600000), // 1 hour
            access_count: 0,
            last_accessed: now,
            metadata: HashMap::new(),
        };

        assert_eq!(entry.data, "test_data");
        assert_eq!(entry.created_at, now);
        assert!(entry.expires_at.is_some());
        assert_eq!(entry.access_count, 0);
    }

    #[test]
    fn test_cache_result_creation() {
        let result = CacheResult {
            data: Some("test_data".to_string()),
            hit: true,
            source: CacheSource::Memory,
            latency_ms: 0.0,
            compression_ratio: None,
        };

        assert!(result.hit);
        assert_eq!(result.source, CacheSource::Memory);
        assert_eq!(result.latency_ms, 0.0);
        assert!(result.data.is_some());
    }

    #[test]
    fn test_cache_stats_default() {
        let stats = CacheStats::default();
        assert_eq!(stats.total_operations, 0);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.hit_rate, 0.0);
        assert_eq!(stats.avg_operation_time_ms, 0.0);
        assert_eq!(stats.total_data_size_bytes, 0);
        assert_eq!(stats.key_count, 0);
    }

    #[test]
    fn test_batch_cache_operation_creation() {
        let operation = BatchCacheOperation {
            key: "batch_key".to_string(),
            value: Some("batch_value".to_string()),
            ttl_seconds: Some(3600),
            operation: CacheOperation::Set,
        };

        assert_eq!(operation.key, "batch_key");
        assert_eq!(operation.value, Some("batch_value".to_string()));
        assert_eq!(operation.ttl_seconds, Some(3600));
        assert_eq!(operation.operation, CacheOperation::Set);
    }
}
