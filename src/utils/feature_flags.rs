use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use worker::console_log;

/// Feature flag configuration for production-ready feature management
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeatureFlag {
    pub name: String,
    pub enabled: bool,
    pub value: Option<serde_json::Value>,
    pub description: String,
    pub rollout_percentage: Option<u8>, // 0-100
}

/// Feature flag manager with caching and environment support
#[derive(Debug, Clone)]
pub struct FeatureFlagManager {
    flags: HashMap<String, FeatureFlag>,
    environment: String,
}

impl Default for FeatureFlagManager {
    fn default() -> Self {
        let mut flags = HashMap::new();

        // Production configuration flags
        flags.insert(
            "opportunity_engine.enhanced_logging".to_string(),
            FeatureFlag {
                name: "opportunity_engine.enhanced_logging".to_string(),
                enabled: true,
                value: None,
                description: "Enable enhanced console logging for opportunity generation debugging"
                    .to_string(),
                rollout_percentage: Some(100),
            },
        );

        flags.insert(
            "opportunity_engine.min_rate_threshold".to_string(),
            FeatureFlag {
                name: "opportunity_engine.min_rate_threshold".to_string(),
                enabled: true,
                value: Some(serde_json::json!(0.05)), // Lower threshold: 0.05%
                description: "Minimum rate difference threshold for arbitrage opportunities"
                    .to_string(),
                rollout_percentage: Some(100),
            },
        );

        flags.insert(
            "exchange_service.fault_tolerance".to_string(),
            FeatureFlag {
                name: "exchange_service.fault_tolerance".to_string(),
                enabled: true,
                value: None,
                description: "Enable fault-tolerant exchange API calls with graceful degradation"
                    .to_string(),
                rollout_percentage: Some(100),
            },
        );

        flags.insert(
            "exchange_service.concurrent_ticker_fetching".to_string(),
            FeatureFlag {
                name: "exchange_service.concurrent_ticker_fetching".to_string(),
                enabled: true,
                value: Some(serde_json::json!({"max_concurrent": 5, "timeout_ms": 3000})),
                description:
                    "Enable concurrent ticker fetching with configurable concurrency limits"
                        .to_string(),
                rollout_percentage: Some(100),
            },
        );

        flags.insert(
            "geographic_fallback.enabled".to_string(),
            FeatureFlag {
                name: "geographic_fallback.enabled".to_string(),
                enabled: true,
                value: None,
                description: "Enable geographic fallback for API endpoints".to_string(),
                rollout_percentage: Some(100),
            },
        );

        flags.insert(
            "geographic_fallback.binance_us_fallback".to_string(),
            FeatureFlag {
                name: "geographic_fallback.binance_us_fallback".to_string(),
                enabled: true,
                value: None,
                description: "Enable Binance US API fallback for geographic restrictions"
                    .to_string(),
                rollout_percentage: Some(100),
            },
        );

        flags.insert(
            "caching.aggressive_ticker_caching".to_string(),
            FeatureFlag {
                name: "caching.aggressive_ticker_caching".to_string(),
                enabled: true,
                value: Some(serde_json::json!({"ttl_seconds": 30, "stale_while_revalidate": 60})),
                description: "Enable aggressive ticker data caching with stale-while-revalidate"
                    .to_string(),
                rollout_percentage: Some(100),
            },
        );

        flags.insert(
            "monitoring.detailed_metrics".to_string(),
            FeatureFlag {
                name: "monitoring.detailed_metrics".to_string(),
                enabled: true,
                value: None,
                description: "Enable detailed performance and error metrics collection".to_string(),
                rollout_percentage: Some(100),
            },
        );

        flags.insert(
            "resilience.circuit_breaker".to_string(),
            FeatureFlag {
                name: "resilience.circuit_breaker".to_string(),
                enabled: true,
                value: Some(serde_json::json!({
                    "failure_threshold": 5,
                    "recovery_timeout_ms": 60000,
                    "half_open_max_calls": 2
                })),
                description: "Enable circuit breaker pattern for external API calls".to_string(),
                rollout_percentage: Some(100),
            },
        );

        Self {
            flags,
            environment: "production".to_string(),
        }
    }
}

impl FeatureFlagManager {
    pub fn new(environment: String) -> Self {
        Self {
            environment,
            ..Self::default()
        }
    }

    pub fn is_enabled(&self, flag_name: &str) -> bool {
        if let Some(flag) = self.flags.get(flag_name) {
            flag.enabled && self.check_rollout(flag)
        } else {
            false
        }
    }

    pub fn get_value<T>(&self, flag_name: &str) -> Option<T>
    where
        T: serde::de::DeserializeOwned,
    {
        if let Some(flag) = self.flags.get(flag_name) {
            if flag.enabled && self.check_rollout(flag) {
                if let Some(ref value) = flag.value {
                    return serde_json::from_value(value.clone()).ok();
                }
            }
        }
        None
    }

    pub fn get_numeric_value(&self, flag_name: &str) -> Option<f64> {
        self.get_value::<f64>(flag_name)
    }

    pub fn get_string_value(&self, flag_name: &str) -> Option<String> {
        self.get_value::<String>(flag_name)
    }

    fn check_rollout(&self, flag: &FeatureFlag) -> bool {
        if let Some(percentage) = flag.rollout_percentage {
            if percentage >= 100 {
                return true;
            }
            // Simple hash-based rollout (in production, use proper user ID hashing)
            let hash = flag.name.len() % 100;
            hash < percentage as usize
        } else {
            true
        }
    }

    pub fn add_flag(&mut self, flag: FeatureFlag) {
        self.flags.insert(flag.name.clone(), flag);
    }

    pub fn remove_flag(&mut self, flag_name: &str) {
        self.flags.remove(flag_name);
    }

    /// Get the current environment
    pub fn get_environment(&self) -> &str {
        &self.environment
    }

    pub fn list_active_flags(&self) -> Vec<&FeatureFlag> {
        self.flags
            .values()
            .filter(|flag| flag.enabled && self.check_rollout(flag))
            .collect()
    }
}

// Global feature flag instance
lazy_static::lazy_static! {
    static ref FEATURE_FLAGS: std::sync::Mutex<FeatureFlagManager> =
        std::sync::Mutex::new(FeatureFlagManager::default());
}

/// Load feature flags from JSON file
fn load_json_feature_flags() -> Result<serde_json::Value, String> {
    #[cfg(target_arch = "wasm32")]
    {
        // For WASM/Cloudflare Workers, embed the JSON file
        let json_str = include_str!("../../feature_flags.json");
        serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse feature flags JSON: {}", e))
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // For native, read from file system
        let json_str = std::fs::read_to_string("feature_flags.json")
            .map_err(|e| format!("Failed to read feature flags file: {}", e))?;
        serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse feature flags JSON: {}", e))
    }
}

/// Get nested flag value from JSON structure
fn get_nested_flag_value(json: &serde_json::Value, flag_path: &str) -> Option<bool> {
    let parts: Vec<&str> = flag_path.split('.').collect();
    let mut current = json;

    for part in parts {
        current = current.get(part)?;
    }

    current.as_bool()
}

/// Check if a feature flag is enabled
pub fn is_feature_enabled(flag_name: &str) -> Result<bool, String> {
    // First try to load from JSON file for nested flags
    if let Ok(json_flags) = load_json_feature_flags() {
        if let Some(value) = get_nested_flag_value(&json_flags, flag_name) {
            return Ok(value);
        }
    }

    // Fallback to the old manager system
    match FEATURE_FLAGS.lock() {
        Ok(manager) => Ok(manager.is_enabled(flag_name)),
        Err(_) => {
            console_log!(
                "⚠️ FEATURE_FLAGS - Failed to acquire lock for {}",
                flag_name
            );
            Err("Failed to access feature flags".to_string())
        }
    }
}

/// Get feature flag numeric value
pub fn get_feature_value<T>(flag_name: &str) -> Result<Option<T>, String>
where
    T: serde::de::DeserializeOwned,
{
    match FEATURE_FLAGS.lock() {
        Ok(manager) => Ok(manager.get_value(flag_name)),
        Err(_) => {
            console_log!(
                "⚠️ FEATURE_FLAGS - Failed to acquire lock for value {}",
                flag_name
            );
            Err("Failed to access feature flags".to_string())
        }
    }
}

/// Get feature flag numeric value with fallback
pub fn get_numeric_feature_value(flag_name: &str, default: f64) -> f64 {
    match FEATURE_FLAGS.lock() {
        Ok(manager) => manager.get_numeric_value(flag_name).unwrap_or(default),
        Err(_) => {
            console_log!(
                "⚠️ FEATURE_FLAGS - Failed to get numeric value for {}, using default: {}",
                flag_name,
                default
            );
            default
        }
    }
}

/// Initialize feature flags with environment configuration
pub fn initialize_feature_flags(environment: &str) -> Result<(), String> {
    match FEATURE_FLAGS.lock() {
        Ok(mut manager) => {
            *manager = FeatureFlagManager::new(environment.to_string());
            console_log!(
                "✅ FEATURE_FLAGS - Initialized for environment: {}",
                environment
            );
            Ok(())
        }
        Err(_) => {
            console_log!("❌ FEATURE_FLAGS - Failed to initialize");
            Err("Failed to initialize feature flags".to_string())
        }
    }
}

/// Update feature flag at runtime
pub fn update_feature_flag(flag: FeatureFlag) -> Result<(), String> {
    match FEATURE_FLAGS.lock() {
        Ok(mut manager) => {
            manager.add_flag(flag.clone());
            console_log!("✅ FEATURE_FLAGS - Updated flag: {}", flag.name);
            Ok(())
        }
        Err(_) => {
            console_log!("❌ FEATURE_FLAGS - Failed to update flag: {}", flag.name);
            Err("Failed to update feature flag".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_flag_manager() {
        let manager = FeatureFlagManager::default();

        // Test enabled flag
        assert!(manager.is_enabled("opportunity_engine.enhanced_logging"));

        // Test disabled flag (non-existent)
        assert!(!manager.is_enabled("non_existent_flag"));

        // Test value retrieval
        let threshold: Option<f64> = manager.get_value("opportunity_engine.min_rate_threshold");
        assert_eq!(threshold, Some(0.05));
    }

    #[test]
    fn test_global_feature_flag_access() {
        assert!(is_feature_enabled("opportunity_engine.enhanced_logging").unwrap_or(false));

        let threshold = get_numeric_feature_value("opportunity_engine.min_rate_threshold", 0.1);
        assert_eq!(threshold, 0.05);
    }
}
