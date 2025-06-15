//! KV Store Service Module
//!
//! Provides key-value storage operations for the ArbEdge platform.
//! Supports Cloudflare KV and other KV store implementations.

use crate::utils::now_system_time;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

/// KV service implementation using cache-based storage with interior mutability
#[derive(Debug, Clone)]
pub struct KVService {
    #[allow(dead_code)] // Used for future KV store implementations
    namespace: String,
    cache: Arc<Mutex<HashMap<String, CachedValue>>>,
    default_ttl: Duration,
}

/// Cached value with expiration time
#[derive(Debug, Clone)]
struct CachedValue {
    value: String,
    expires_at: u64,
}

/// Result type for KV operations
pub type KVResult<T> = Result<T, KVError>;

/// KV operation errors
#[derive(Debug, Clone)]
pub enum KVError {
    NotFound(String),
    SerializationError(String),
    NetworkError(String),
    PermissionDenied(String),
    QuotaExceeded(String),
    InvalidKey(String),
    ServiceUnavailable(String),
}

impl std::fmt::Display for KVError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KVError::NotFound(msg) => write!(f, "Not found: {}", msg),
            KVError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            KVError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            KVError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            KVError::QuotaExceeded(msg) => write!(f, "Quota exceeded: {}", msg),
            KVError::InvalidKey(msg) => write!(f, "Invalid key: {}", msg),
            KVError::ServiceUnavailable(msg) => write!(f, "Service unavailable: {}", msg),
        }
    }
}

impl std::error::Error for KVError {}

/// KV store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KVConfig {
    pub namespace: String,
    pub default_ttl_seconds: u64,
    pub max_key_size: usize,
    pub max_value_size: usize,
    pub enable_cache: bool,
}

impl Default for KVConfig {
    fn default() -> Self {
        Self {
            namespace: "arbedge".to_string(),
            default_ttl_seconds: 3600, // 1 hour
            max_key_size: 512,
            max_value_size: 25 * 1024 * 1024, // 25MB
            enable_cache: true,
        }
    }
}

impl KVService {
    /// Create a new KV service instance
    pub fn new(config: KVConfig) -> Self {
        Self {
            namespace: config.namespace,
            cache: Arc::new(Mutex::new(HashMap::new())),
            default_ttl: Duration::from_secs(config.default_ttl_seconds),
        }
    }

    /// Put a serializable value (used by AI access and other components)
    pub async fn put<T: Serialize>(&self, key: &str, value: &T) -> KVResult<()> {
        let json_value =
            serde_json::to_string(value).map_err(|e| KVError::SerializationError(e.to_string()))?;
        self.set(key, &json_value, None).await
    }

    /// Get a value by key
    pub async fn get(&self, key: &str) -> KVResult<Option<String>> {
        self.validate_key(key)?;

        // Check cache first
        let cache = self.cache.lock().unwrap();
        if let Some(cached) = cache.get(key) {
            let now = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if cached.expires_at > now {
                return Ok(Some(cached.value.clone()));
            }
        }

        // TODO: Implement actual KV store retrieval
        // For now, return None (not found)
        Ok(None)
    }

    /// Set a value with optional TTL
    pub async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> KVResult<()> {
        self.validate_key(key)?;
        self.validate_value(value)?;

        let ttl = ttl.unwrap_or(self.default_ttl);
        let expires_at = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + ttl.as_secs();

        // Update cache
        let mut cache = self.cache.lock().unwrap();
        cache.insert(
            key.to_string(),
            CachedValue {
                value: value.to_string(),
                expires_at,
            },
        );

        // TODO: Implement actual KV store write
        Ok(())
    }

    /// Delete a key
    pub async fn delete(&self, key: &str) -> KVResult<bool> {
        self.validate_key(key)?;

        // Remove from cache
        let mut cache = self.cache.lock().unwrap();
        let was_cached = cache.remove(key).is_some();

        // TODO: Implement actual KV store deletion
        Ok(was_cached)
    }

    /// List keys with optional prefix
    pub async fn list_keys(&self, prefix: Option<&str>) -> KVResult<Vec<String>> {
        let cache = self.cache.lock().unwrap();
        let mut keys: Vec<String> = cache.keys().cloned().collect();

        if let Some(prefix) = prefix {
            keys.retain(|k| k.starts_with(prefix));
        }

        // TODO: Implement actual KV store key listing
        Ok(keys)
    }

    /// Get multiple values at once
    pub async fn get_multiple(&self, keys: &[String]) -> KVResult<HashMap<String, Option<String>>> {
        let mut results = HashMap::new();

        for key in keys {
            let value = self.get(key).await?;
            results.insert(key.clone(), value);
        }

        Ok(results)
    }

    /// Set multiple values at once
    pub async fn set_multiple(
        &self,
        values: HashMap<String, String>,
        ttl: Option<Duration>,
    ) -> KVResult<()> {
        for (key, value) in values {
            self.set(&key, &value, ttl).await?;
        }
        Ok(())
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> KVResult<bool> {
        self.validate_key(key)?;

        // Check cache first
        let cache = self.cache.lock().unwrap();
        if let Some(cached) = cache.get(key) {
            let now = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if cached.expires_at > now {
                return Ok(true);
            }
        }

        // TODO: Implement actual KV store existence check
        Ok(false)
    }

    /// Get TTL for a key
    pub async fn get_ttl(&self, key: &str) -> KVResult<Option<Duration>> {
        self.validate_key(key)?;

        let cache = self.cache.lock().unwrap();
        if let Some(cached) = cache.get(key) {
            let now = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if cached.expires_at > now {
                let remaining = cached.expires_at - now;
                return Ok(Some(Duration::from_secs(remaining)));
            }
        }

        // TODO: Implement actual KV store TTL retrieval
        Ok(None)
    }

    /// Clear expired entries from cache
    pub fn cleanup_cache(&self) {
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut cache = self.cache.lock().unwrap();
        cache.retain(|_, cached| cached.expires_at > now);
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> CacheStats {
        let now = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cache = self.cache.lock().unwrap();
        let total_entries = cache.len();
        let expired_entries = cache
            .values()
            .filter(|cached| cached.expires_at <= now)
            .count();

        CacheStats {
            total_entries,
            active_entries: total_entries - expired_entries,
            expired_entries,
        }
    }

    /// Validate key format and size
    fn validate_key(&self, key: &str) -> KVResult<()> {
        if key.is_empty() {
            return Err(KVError::InvalidKey("Key cannot be empty".to_string()));
        }

        if key.len() > 512 {
            return Err(KVError::InvalidKey(
                "Key too long (max 512 characters)".to_string(),
            ));
        }

        // Check for invalid characters
        if key.contains(['\0', '\n', '\r']) {
            return Err(KVError::InvalidKey(
                "Key contains invalid characters".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate value size
    fn validate_value(&self, value: &str) -> KVResult<()> {
        if value.len() > 25 * 1024 * 1024 {
            return Err(KVError::InvalidKey(
                "Value too large (max 25MB)".to_string(),
            ));
        }

        Ok(())
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub active_entries: usize,
    pub expired_entries: usize,
}

/// Convenience functions for common operations
pub async fn get_string(service: &KVService, key: &str) -> KVResult<Option<String>> {
    service.get(key).await
}

pub async fn set_string(service: &KVService, key: &str, value: &str) -> KVResult<()> {
    service.set(key, value, None).await
}

pub async fn get_json<T>(service: &KVService, key: &str) -> KVResult<Option<T>>
where
    T: for<'de> Deserialize<'de>,
{
    if let Some(value) = service.get(key).await? {
        let parsed: T =
            serde_json::from_str(&value).map_err(|e| KVError::SerializationError(e.to_string()))?;
        Ok(Some(parsed))
    } else {
        Ok(None)
    }
}

pub async fn set_json<T>(service: &KVService, key: &str, value: &T) -> KVResult<()>
where
    T: Serialize,
{
    let json =
        serde_json::to_string(value).map_err(|e| KVError::SerializationError(e.to_string()))?;
    service.set(key, &json, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kv_basic_operations() {
        let config = KVConfig::default();
        let kv = KVService::new(config);

        // Test set and get
        kv.set("test_key", "test_value", None).await.unwrap();
        let value = kv.get("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        // Test exists
        let exists = kv.exists("test_key").await.unwrap();
        assert!(exists);

        // Test delete
        let deleted1 = kv.delete("test_key").await.unwrap();
        assert!(deleted1); // Should return true - key was cached

        let deleted2 = kv.delete("test_key").await.unwrap();
        assert!(!deleted2); // Should return false - key was already removed

        // Test get after delete
        let value = kv.get("test_key").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_kv_validation() {
        let config = KVConfig::default();
        let kv = KVService::new(config);

        // Test empty key
        let result = kv.set("", "value", None).await;
        assert!(result.is_err());

        // Test key with invalid characters
        let result = kv.set("key\0with\nnull", "value", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_json_operations() {
        let config = KVConfig::default();
        let kv = KVService::new(config);

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestData {
            name: String,
            value: i32,
        }

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        // Test JSON set and get via put method
        kv.put("json_key", &data).await.unwrap();
        let retrieved: Option<TestData> = get_json(&kv, "json_key").await.unwrap();
        assert_eq!(retrieved, Some(data));
    }
}
