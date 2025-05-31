// AI Cache - Intelligent Caching for AI Responses and Embeddings
// Provides optimized caching for AI services with TTL management and cache statistics

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use worker::kv::KvStore;

/// Configuration for AICache
#[derive(Debug, Clone)]
pub struct AICacheConfig {
    pub enable_caching: bool,
    pub default_ttl_seconds: u64,
    pub embedding_ttl_seconds: u64,
    pub model_response_ttl_seconds: u64,
    pub personalization_ttl_seconds: u64,
    pub max_cache_size_mb: u64,
    pub compression_enabled: bool,
    pub compression_threshold_bytes: usize,
    pub cache_key_prefix: String,
    pub enable_cache_warming: bool,
    pub enable_cache_analytics: bool,
    pub batch_size: usize,
    pub connection_pool_size: u32,
}

impl Default for AICacheConfig {
    fn default() -> Self {
        Self {
            enable_caching: true,
            default_ttl_seconds: 3600,        // 1 hour
            embedding_ttl_seconds: 7200,      // 2 hours (embeddings are more stable)
            model_response_ttl_seconds: 1800, // 30 minutes
            personalization_ttl_seconds: 900, // 15 minutes (user preferences change)
            max_cache_size_mb: 256,           // 256MB cache limit
            compression_enabled: true,
            compression_threshold_bytes: 1024, // Compress entries > 1KB
            cache_key_prefix: "ai_cache".to_string(),
            enable_cache_warming: true,
            enable_cache_analytics: true,
            batch_size: 50,
            connection_pool_size: 15,
        }
    }
}

impl AICacheConfig {
    /// Create configuration optimized for high concurrency
    pub fn high_concurrency() -> Self {
        Self {
            connection_pool_size: 25,
            batch_size: 100,
            max_cache_size_mb: 512,    // Larger cache for high concurrency
            default_ttl_seconds: 1800, // Shorter TTL for fresher data
            ..Default::default()
        }
    }

    /// Create configuration optimized for memory usage
    pub fn memory_optimized() -> Self {
        Self {
            connection_pool_size: 8,
            batch_size: 25,
            max_cache_size_mb: 128, // Smaller cache
            compression_enabled: true,
            compression_threshold_bytes: 512, // Compress smaller entries
            enable_cache_analytics: false,    // Disable to save memory
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.batch_size == 0 {
            return Err(ArbitrageError::validation_error(
                "batch_size must be greater than 0",
            ));
        }
        if self.connection_pool_size == 0 {
            return Err(ArbitrageError::validation_error(
                "connection_pool_size must be greater than 0",
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

/// Cache entry types for different AI services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheEntryType {
    Embedding,
    ModelResponse,
    PersonalizationScore,
    SimilarityResult,
    RoutingDecision,
    UserPreferences,
    FeatureVector,
}

impl std::fmt::Display for CacheEntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheEntryType::Embedding => write!(f, "embedding"),
            CacheEntryType::ModelResponse => write!(f, "model_response"),
            CacheEntryType::PersonalizationScore => write!(f, "personalization"),
            CacheEntryType::SimilarityResult => write!(f, "similarity"),
            CacheEntryType::RoutingDecision => write!(f, "routing"),
            CacheEntryType::UserPreferences => write!(f, "user_prefs"),
            CacheEntryType::FeatureVector => write!(f, "features"),
        }
    }
}

/// Cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub entry_type: CacheEntryType,
    pub data: String, // JSON-serialized data
    pub created_at: u64,
    pub expires_at: u64,
    pub access_count: u64,
    pub last_accessed: u64,
    pub size_bytes: usize,
    pub is_compressed: bool,
    pub metadata: HashMap<String, String>,
}

impl CacheEntry {
    /// Create new cache entry
    pub fn new(key: String, entry_type: CacheEntryType, data: String, ttl_seconds: u64) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let size_bytes = data.len();

        Self {
            key,
            entry_type,
            data,
            created_at: now,
            expires_at: now + (ttl_seconds * 1000),
            access_count: 0,
            last_accessed: now,
            size_bytes,
            is_compressed: false,
            metadata: HashMap::new(),
        }
    }

    /// Check if entry is expired
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        now > self.expires_at
    }

    /// Update access statistics
    pub fn mark_accessed(&mut self) {
        self.access_count += 1;
        self.last_accessed = chrono::Utc::now().timestamp_millis() as u64;
    }

    /// Get age in seconds
    pub fn age_seconds(&self) -> u64 {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        (now - self.created_at) / 1000
    }

    /// Get time until expiry in seconds
    pub fn ttl_seconds(&self) -> u64 {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        if now >= self.expires_at {
            0
        } else {
            (self.expires_at - now) / 1000
        }
    }
}

/// Cache statistics and analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: u64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub hit_rate_percent: f32,
    pub total_size_bytes: u64,
    pub avg_entry_size_bytes: f32,
    pub entries_by_type: HashMap<String, u64>,
    pub expired_entries_cleaned: u64,
    pub compression_ratio: f32,
    pub cache_efficiency_score: f32,
    pub last_updated: u64,
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            total_entries: 0,
            total_hits: 0,
            total_misses: 0,
            hit_rate_percent: 0.0,
            total_size_bytes: 0,
            avg_entry_size_bytes: 0.0,
            entries_by_type: HashMap::new(),
            expired_entries_cleaned: 0,
            compression_ratio: 1.0,
            cache_efficiency_score: 0.0,
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// AI Cache for intelligent caching of AI responses and embeddings
pub struct AICache {
    config: AICacheConfig,
    logger: crate::utils::logger::Logger,
    cache: Option<KvStore>,
    stats: Arc<std::sync::Mutex<CacheStats>>,
    #[allow(dead_code)] // TODO: Will be used for cache warming functionality
    cache_warming_enabled: Arc<std::sync::Mutex<bool>>,
    popular_keys: Arc<std::sync::Mutex<HashMap<String, u64>>>, // Key -> access count
}

impl AICache {
    /// Create new AICache instance
    pub fn new(config: AICacheConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        let cache = Self {
            config,
            logger,
            cache: None,
            stats: Arc::new(std::sync::Mutex::new(CacheStats::default())),
            cache_warming_enabled: Arc::new(std::sync::Mutex::new(false)),
            popular_keys: Arc::new(std::sync::Mutex::new(HashMap::new())),
        };

        cache.logger.info(&format!(
            "AICache initialized: caching_enabled={}, default_ttl={}s, max_size={}MB",
            cache.config.enable_caching,
            cache.config.default_ttl_seconds,
            cache.config.max_cache_size_mb
        ));

        Ok(cache)
    }

    /// Set cache store for caching operations
    pub fn with_cache(mut self, cache: KvStore) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Get cached data
    pub async fn get<T>(&self, key: &str, entry_type: CacheEntryType) -> ArbitrageResult<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        if !self.config.enable_caching {
            return Ok(None);
        }

        let cache_key = self.build_cache_key(key, &entry_type);

        match self.get_cache_entry(&cache_key).await {
            Ok(Some(mut entry)) => {
                // Check if expired
                if entry.is_expired() {
                    self.delete_cache_entry(&cache_key).await?;
                    self.update_stats_miss().await;
                    return Ok(None);
                }

                // Update access statistics
                entry.mark_accessed();
                self.update_cache_entry(&entry).await?;

                // Deserialize data
                match serde_json::from_str::<T>(&entry.data) {
                    Ok(data) => {
                        self.update_stats_hit().await;
                        self.track_popular_key(&cache_key).await;
                        Ok(Some(data))
                    }
                    Err(e) => {
                        self.logger
                            .warn(&format!("Failed to deserialize cached data: {}", e));
                        self.delete_cache_entry(&cache_key).await?;
                        self.update_stats_miss().await;
                        Ok(None)
                    }
                }
            }
            Ok(None) => {
                self.update_stats_miss().await;
                Ok(None)
            }
            Err(e) => {
                self.logger.warn(&format!("Cache get error: {}", e));
                self.update_stats_miss().await;
                Ok(None)
            }
        }
    }

    /// Set cached data
    pub async fn set<T>(
        &self,
        key: &str,
        entry_type: CacheEntryType,
        data: &T,
        custom_ttl: Option<u64>,
    ) -> ArbitrageResult<()>
    where
        T: Serialize,
    {
        if !self.config.enable_caching {
            return Ok(());
        }

        let cache_key = self.build_cache_key(key, &entry_type);
        let data_json = serde_json::to_string(data)?;

        // Determine TTL based on entry type
        let ttl = custom_ttl.unwrap_or_else(|| self.get_ttl_for_type(&entry_type));

        // Create cache entry
        let mut entry = CacheEntry::new(cache_key.clone(), entry_type.clone(), data_json, ttl);

        // Apply compression if enabled and data is large enough
        if self.config.compression_enabled
            && entry.size_bytes > self.config.compression_threshold_bytes
        {
            entry = self.compress_entry(entry)?;
        }

        // Store cache entry
        self.store_cache_entry(&entry).await?;

        // Update statistics
        self.update_stats_set(&entry).await;

        Ok(())
    }

    /// Delete cached data
    pub async fn delete(&self, key: &str, entry_type: CacheEntryType) -> ArbitrageResult<()> {
        if !self.config.enable_caching {
            return Ok(());
        }

        let cache_key = self.build_cache_key(key, &entry_type);
        self.delete_cache_entry(&cache_key).await
    }

    /// Check if key exists in cache
    pub async fn exists(&self, key: &str, entry_type: CacheEntryType) -> ArbitrageResult<bool> {
        if !self.config.enable_caching {
            return Ok(false);
        }

        let cache_key = self.build_cache_key(key, &entry_type);

        match self.get_cache_entry(&cache_key).await {
            Ok(Some(entry)) => Ok(!entry.is_expired()),
            Ok(None) => Ok(false),
            Err(_) => Ok(false),
        }
    }

    /// Get multiple cached entries in batch
    pub async fn get_batch<T>(
        &self,
        keys: Vec<(&str, CacheEntryType)>,
    ) -> ArbitrageResult<HashMap<String, T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut results = HashMap::new();

        if !self.config.enable_caching {
            return Ok(results);
        }

        // Process in batches
        for chunk in keys.chunks(self.config.batch_size) {
            for (key, entry_type) in chunk {
                if let Ok(Some(data)) = self.get::<T>(key, entry_type.clone()).await {
                    results.insert(key.to_string(), data);
                }
            }
        }

        Ok(results)
    }

    /// Set multiple cached entries in batch
    pub async fn set_batch<T>(
        &self,
        entries: Vec<(&str, CacheEntryType, &T, Option<u64>)>,
    ) -> ArbitrageResult<()>
    where
        T: Serialize,
    {
        if !self.config.enable_caching {
            return Ok(());
        }

        // Process in batches
        for chunk in entries.chunks(self.config.batch_size) {
            for (key, entry_type, data, ttl) in chunk {
                let _ = self.set(key, entry_type.clone(), data, *ttl).await;
            }
        }

        Ok(())
    }

    /// Clear expired entries
    pub async fn cleanup_expired(&self) -> ArbitrageResult<u64> {
        if !self.config.enable_caching {
            return Ok(0);
        }

        // This is a simplified implementation
        // In a full implementation, we would scan all cache keys and remove expired ones
        let cleaned_count = 0u64;

        // Update statistics
        {
            let mut stats = self.stats.lock().unwrap();
            stats.expired_entries_cleaned += cleaned_count;
            stats.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        Ok(cleaned_count)
    }

    /// Warm cache with popular entries
    pub async fn warm_cache(&self, popular_keys: Vec<String>) -> ArbitrageResult<()> {
        if !self.config.enable_cache_warming {
            return Ok(());
        }

        // This is a placeholder for cache warming logic
        // In a full implementation, this would pre-load popular entries
        self.logger.info(&format!(
            "Cache warming initiated for {} keys",
            popular_keys.len()
        ));

        Ok(())
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        self.stats.lock().unwrap().clone()
    }

    /// Reset cache statistics
    pub async fn reset_stats(&self) -> ArbitrageResult<()> {
        *self.stats.lock().unwrap() = CacheStats::default();
        Ok(())
    }

    /// Build cache key with prefix and type
    fn build_cache_key(&self, key: &str, entry_type: &CacheEntryType) -> String {
        format!("{}:{}:{}", self.config.cache_key_prefix, entry_type, key)
    }

    /// Get TTL for specific entry type
    fn get_ttl_for_type(&self, entry_type: &CacheEntryType) -> u64 {
        match entry_type {
            CacheEntryType::Embedding => self.config.embedding_ttl_seconds,
            CacheEntryType::ModelResponse => self.config.model_response_ttl_seconds,
            CacheEntryType::PersonalizationScore => self.config.personalization_ttl_seconds,
            CacheEntryType::UserPreferences => self.config.personalization_ttl_seconds,
            _ => self.config.default_ttl_seconds,
        }
    }

    /// Compress cache entry (simplified implementation)
    fn compress_entry(&self, mut entry: CacheEntry) -> ArbitrageResult<CacheEntry> {
        // In a real implementation, this would use actual compression
        // For now, we just mark it as compressed
        entry.is_compressed = true;
        Ok(entry)
    }

    /// Store cache entry in KV store
    async fn store_cache_entry(&self, entry: &CacheEntry) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            let entry_json = serde_json::to_string(entry)?;

            cache
                .put(&entry.key, &entry_json)?
                .expiration_ttl(entry.ttl_seconds())
                .execute()
                .await?;
        }
        Ok(())
    }

    /// Get cache entry from KV store
    async fn get_cache_entry(&self, key: &str) -> ArbitrageResult<Option<CacheEntry>> {
        if let Some(ref cache) = self.cache {
            match cache.get(key).text().await {
                Ok(Some(entry_json)) => match serde_json::from_str::<CacheEntry>(&entry_json) {
                    Ok(entry) => Ok(Some(entry)),
                    Err(e) => {
                        self.logger
                            .warn(&format!("Failed to deserialize cache entry: {}", e));
                        Ok(None)
                    }
                },
                Ok(None) => Ok(None),
                Err(e) => {
                    self.logger
                        .warn(&format!("Failed to get cache entry: {}", e));
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Update cache entry in KV store
    async fn update_cache_entry(&self, entry: &CacheEntry) -> ArbitrageResult<()> {
        self.store_cache_entry(entry).await
    }

    /// Delete cache entry from KV store
    async fn delete_cache_entry(&self, key: &str) -> ArbitrageResult<()> {
        if let Some(ref cache) = self.cache {
            cache.delete(key).await?;
        }
        Ok(())
    }

    /// Update statistics for cache hit
    async fn update_stats_hit(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_hits += 1;
        stats.hit_rate_percent =
            (stats.total_hits as f32 / (stats.total_hits + stats.total_misses) as f32) * 100.0;
        stats.last_updated = chrono::Utc::now().timestamp_millis() as u64;
    }

    /// Update statistics for cache miss
    async fn update_stats_miss(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_misses += 1;
        stats.hit_rate_percent =
            (stats.total_hits as f32 / (stats.total_hits + stats.total_misses) as f32) * 100.0;
        stats.last_updated = chrono::Utc::now().timestamp_millis() as u64;
    }

    /// Update statistics for cache set
    async fn update_stats_set(&self, entry: &CacheEntry) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_entries += 1;
        stats.total_size_bytes += entry.size_bytes as u64;
        stats.avg_entry_size_bytes = stats.total_size_bytes as f32 / stats.total_entries as f32;

        // Update entries by type
        let type_key = entry.entry_type.to_string();
        let count = stats.entries_by_type.get(&type_key).unwrap_or(&0) + 1;
        stats.entries_by_type.insert(type_key, count);

        stats.last_updated = chrono::Utc::now().timestamp_millis() as u64;
    }

    /// Track popular keys for cache warming
    async fn track_popular_key(&self, key: &str) {
        let mut popular = self.popular_keys.lock().unwrap();
        *popular.entry(key.to_string()).or_insert(0) += 1;

        // Keep only top 100 popular keys
        if popular.len() > 100 {
            // Collect the data first to avoid borrowing conflicts
            let mut sorted: Vec<(String, u64)> =
                popular.iter().map(|(k, v)| (k.clone(), *v)).collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));

            // Clear and repopulate with top 100
            popular.clear();
            for (key, count) in sorted.into_iter().take(100) {
                popular.insert(key, count);
            }
        }
    }

    /// Health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        if !self.config.enable_caching {
            return Ok(true); // Healthy if caching is disabled
        }

        // Test cache connectivity
        let test_key = format!("{}:health_check", self.config.cache_key_prefix);
        let test_data = "health_check_data";

        match self
            .set(
                &test_key,
                CacheEntryType::FeatureVector,
                &test_data,
                Some(60),
            )
            .await
        {
            Ok(_) => {
                // Try to retrieve the test data
                match self
                    .get::<String>(&test_key, CacheEntryType::FeatureVector)
                    .await
                {
                    Ok(Some(data)) if data == test_data => {
                        // Clean up test data
                        let _ = self.delete(&test_key, CacheEntryType::FeatureVector).await;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_cache_config_default() {
        let config = AICacheConfig::default();
        assert!(config.enable_caching);
        assert_eq!(config.default_ttl_seconds, 3600);
        assert_eq!(config.max_cache_size_mb, 256);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_ai_cache_config_high_concurrency() {
        let config = AICacheConfig::high_concurrency();
        assert_eq!(config.connection_pool_size, 25);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_cache_size_mb, 512);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cache_entry_creation() {
        let entry = CacheEntry::new(
            "test_key".to_string(),
            CacheEntryType::Embedding,
            "test_data".to_string(),
            3600,
        );

        assert_eq!(entry.key, "test_key");
        assert_eq!(entry.size_bytes, 9); // "test_data".len()
        assert!(!entry.is_expired());
        assert_eq!(entry.access_count, 0);
    }

    #[test]
    fn test_cache_entry_expiry() {
        let mut entry = CacheEntry::new(
            "test_key".to_string(),
            CacheEntryType::Embedding,
            "test_data".to_string(),
            0, // Immediate expiry
        );

        // Should be expired immediately
        assert!(entry.is_expired());

        // Test access tracking
        entry.mark_accessed();
        assert_eq!(entry.access_count, 1);
    }

    #[test]
    fn test_cache_entry_type_display() {
        assert_eq!(CacheEntryType::Embedding.to_string(), "embedding");
        assert_eq!(CacheEntryType::ModelResponse.to_string(), "model_response");
        assert_eq!(
            CacheEntryType::PersonalizationScore.to_string(),
            "personalization"
        );
    }

    #[test]
    fn test_cache_stats_default() {
        let stats = CacheStats::default();
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.total_hits, 0);
        assert_eq!(stats.hit_rate_percent, 0.0);
        assert_eq!(stats.compression_ratio, 1.0);
    }
}
