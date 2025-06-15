// src/utils/time.rs

#[cfg(target_arch = "wasm32")]
use js_sys::Date;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
use web_sys;

/// Service for handling time-related operations.
/// WASM-compatible implementation using JavaScript Date API
#[derive(Debug, Clone)]
pub struct TimeService;

impl TimeService {
    /// Creates a new instance of `TimeService`.
    pub fn new() -> Self {
        TimeService
    }

    /// Gets the current timestamp in seconds since Unix epoch.
    /// WASM-compatible using JavaScript Date.now()
    pub fn current_timestamp(&self) -> u64 {
        #[cfg(target_arch = "wasm32")]
        {
            (Date::now() / 1000.0) as u64
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        }
    }

    /// Gets the current timestamp in milliseconds since Unix epoch.
    /// WASM-compatible using JavaScript Date.now()
    pub fn current_timestamp_ms(&self) -> i64 {
        #[cfg(target_arch = "wasm32")]
        {
            Date::now() as i64
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
        }
    }

    /// Create a SystemTime from current JavaScript time
    /// WASM-compatible using JavaScript Date.now()
    pub fn now_system_time(&self) -> SystemTime {
        #[cfg(target_arch = "wasm32")]
        {
            let millis = Date::now() as u64;
            UNIX_EPOCH + Duration::from_millis(millis)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            SystemTime::now()
        }
    }
}

/// Gets the current timestamp in seconds since Unix epoch (standalone function).
/// WASM-compatible using JavaScript Date.now()
pub fn get_current_timestamp() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        (Date::now() / 1000.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

/// Get current timestamp in milliseconds since UNIX epoch
/// WASM-compatible using JavaScript Date.now()
pub fn now_millis() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Get current timestamp in seconds since UNIX epoch
/// WASM-compatible using JavaScript Date.now()
pub fn now_secs() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        (Date::now() / 1000.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

/// Create a SystemTime from current JavaScript time
/// WASM-compatible using JavaScript Date.now()
pub fn now_system_time() -> SystemTime {
    #[cfg(target_arch = "wasm32")]
    {
        let millis = Date::now() as u64;
        UNIX_EPOCH + Duration::from_millis(millis)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        SystemTime::now()
    }
}

/// Convert milliseconds since UNIX epoch to SystemTime
pub fn millis_to_system_time(millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(millis)
}

/// Convert SystemTime to milliseconds since UNIX epoch
pub fn system_time_to_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64
}

/// Convert SystemTime to seconds since UNIX epoch
pub fn system_time_to_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

/// WASM-compatible Instant alternative for measuring elapsed time
/// Uses high-resolution timestamp for performance measurements
#[derive(Debug, Clone, Copy)]
pub struct WasmInstant {
    timestamp_ms: f64,
}

impl WasmInstant {
    /// Create a new WasmInstant representing the current time
    /// WASM-compatible using JavaScript performance.now() or Date.now()
    pub fn now() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            // Use performance.now() for high-resolution timing in WASM
            let timestamp_ms = web_sys::window()
                .and_then(|w| w.performance())
                .map(|p| p.now())
                .unwrap_or_else(|| Date::now());

            WasmInstant { timestamp_ms }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Use system time for native builds
            let timestamp_ms = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64;

            WasmInstant { timestamp_ms }
        }
    }

    /// Calculate the duration elapsed since this instant
    pub fn elapsed(&self) -> Duration {
        let now = Self::now();
        let elapsed_ms = now.timestamp_ms - self.timestamp_ms;
        Duration::from_millis(elapsed_ms.max(0.0) as u64)
    }

    /// Calculate the duration between two instants
    pub fn duration_since(&self, earlier: WasmInstant) -> Duration {
        let elapsed_ms = self.timestamp_ms - earlier.timestamp_ms;
        Duration::from_millis(elapsed_ms.max(0.0) as u64)
    }
}

/// WASM-compatible instant function for measuring elapsed time
/// Use this instead of std::time::Instant::now() for WASM compatibility
pub fn now_instant() -> WasmInstant {
    WasmInstant::now()
}

impl Default for TimeService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_functions() {
        let service = TimeService::new();
        let now_ms = now_millis();
        let now_s = now_secs();
        let sys_time = now_system_time();

        // Basic sanity checks
        assert!(now_ms > 0);
        assert!(now_s > 0);
        assert!(service.current_timestamp() > 0);
        assert!(service.current_timestamp_ms() > 0);
        assert!(now_ms / 1000 >= now_s - 1); // Allow for rounding differences

        // Test conversions
        let converted_ms = system_time_to_millis(sys_time);
        assert!((converted_ms as i64 - now_ms as i64).abs() < 1000); // Within 1 second
    }
}
