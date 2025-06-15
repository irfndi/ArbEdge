// src/services/core/infrastructure/chaos_engineering/resource_chaos.rs

//! Resource Chaos Simulation for Cloudflare Workers
//!
//! This module provides comprehensive resource exhaustion testing capabilities specifically
//! designed for the Cloudflare Workers runtime environment. It implements chaos engineering
//! patterns for:
//! - CPU exhaustion simulation through computation-heavy operations
//! - Memory pressure testing within 128MB limit
//! - Execution timeout simulation (30 seconds for HTTP requests)
//! - Concurrent request overload patterns
//! - Event Loop blocking simulation
//! - Resource leak testing
//!
//! All implementations are based on official Cloudflare Workers constraints:
//! - 128MB memory limit
//! - 30 second execution timeout for HTTP requests
//! - CPU time limitations
//! - Event loop constraints

use crate::utils::time::{now_instant, WasmInstant};
use std::collections::HashMap;
use std::time::Duration;

use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};
use worker::Env;

#[cfg(target_arch = "wasm32")]
use gloo_timers;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures;

use super::{ChaosEngineeringConfig, FaultConfig};
use crate::utils::error::ArbitrageResult;

/// Types of resource chaos that can be injected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceChaosType {
    /// CPU exhaustion simulation
    CpuExhaustion {
        /// Duration to consume CPU (milliseconds)
        duration_ms: u64,
        /// CPU intensity (0.0 to 1.0)
        intensity: f64,
    },
    /// Memory pressure simulation
    MemoryPressure {
        /// Memory to allocate (bytes)
        allocation_bytes: u64,
        /// Hold duration (milliseconds)
        hold_duration_ms: u64,
    },
    /// Execution timeout simulation
    ExecutionTimeout {
        /// Timeout to simulate (milliseconds)
        timeout_ms: u64,
    },
    /// Event loop blocking
    EventLoopBlocking {
        /// Block duration (milliseconds)
        block_duration_ms: u64,
    },
    /// Concurrent request overload
    ConcurrentOverload {
        /// Number of concurrent operations to simulate
        concurrent_operations: u32,
    },
    /// Resource leak simulation
    ResourceLeak {
        /// Type of resource to leak
        leak_type: ResourceLeakType,
        /// Amount to leak per operation
        leak_amount: u64,
    },
    /// Garbage collection pressure
    GarbageCollectionPressure {
        /// Objects to create for GC pressure
        object_count: u64,
    },
}

/// Types of resource leaks that can be simulated
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceLeakType {
    /// Memory leak simulation
    Memory,
    /// Event listener leak simulation  
    EventListeners,
    /// Timer leak simulation
    Timers,
    /// Promise leak simulation
    Promises,
}

/// Resource chaos injection parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceChaosParams {
    /// Types of chaos to inject
    pub chaos_types: Vec<ResourceChaosType>,
    /// Target endpoints/paths to affect
    pub target_paths: Vec<String>,
    /// HTTP methods to target
    pub target_methods: Vec<String>,
    /// Request headers to match for triggering chaos
    pub header_patterns: HashMap<String, String>,
    /// Enable chaos during specific time windows
    pub time_windows: Vec<TimeWindow>,
    /// Maximum concurrent chaos operations
    pub max_concurrent_chaos: u32,
    /// Enable Worker-specific resource testing
    pub enable_worker_specific_testing: bool,
    /// Memory limit for testing (bytes)
    pub memory_limit_bytes: u64,
    /// CPU time limit for testing (milliseconds)
    pub cpu_time_limit_ms: u64,
}

/// Time window for chaos injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Start hour (0-23)
    pub start_hour: u8,
    /// End hour (0-23)
    pub end_hour: u8,
    /// Days of week (0=Sunday, 6=Saturday)
    pub days_of_week: Vec<u8>,
}

/// Resource chaos injection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceChaosInjection {
    /// Type of chaos injected
    pub chaos_type: ResourceChaosType,
    /// Additional metadata for tracking
    pub metadata: HashMap<String, String>,
}

/// Resource chaos injection statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceChaosStats {
    /// Total operations monitored
    pub total_operations: u64,
    /// Total chaos injections performed
    pub total_chaos_injected: u64,
    /// CPU exhaustion simulations
    pub cpu_exhaustion_count: u64,
    /// Memory pressure simulations
    pub memory_pressure_count: u64,
    /// Execution timeout simulations
    pub execution_timeout_count: u64,
    /// Event loop blocking simulations
    pub event_loop_blocking_count: u64,
    /// Concurrent overload simulations
    pub concurrent_overload_count: u64,
    /// Resource leak simulations
    pub resource_leak_count: u64,
    /// GC pressure simulations
    pub gc_pressure_count: u64,
    /// Total CPU time consumed (milliseconds)
    pub total_cpu_time_consumed_ms: u64,
    /// Total memory allocated (bytes)
    pub total_memory_allocated_bytes: u64,
    /// Last injection timestamp
    pub last_injection_at: Option<String>,
}

/// Active resource chaos state
#[derive(Debug)]
struct ResourceChaosState {
    #[allow(dead_code)]
    chaos_id: String,
    fault_config: FaultConfig,
    params: ResourceChaosParams,
    stats: ResourceChaosStats,
    activated_at: WasmInstant,
    active_memory_allocations: Vec<Vec<u8>>,
    active_cpu_operations: u32,
    concurrent_operations: u32,
}

/// Resource chaos injector for Cloudflare Workers
#[derive(Debug)]
pub struct ResourceChaosInjector {
    #[allow(dead_code)]
    config: ChaosEngineeringConfig,
    active_chaos: HashMap<String, ResourceChaosState>,
    global_stats: ResourceChaosStats,
    is_enabled: bool,
    is_initialized: bool,
}

impl ResourceChaosInjector {
    /// Create a new resource chaos injector
    pub fn new(config: ChaosEngineeringConfig) -> Self {
        Self {
            config,
            active_chaos: HashMap::new(),
            global_stats: ResourceChaosStats::default(),
            is_enabled: false,
            is_initialized: false,
        }
    }

    /// Initialize the resource chaos injector
    pub async fn initialize(&mut self, _env: &Env) -> ArbitrageResult<()> {
        if self.is_initialized {
            return Ok(());
        }

        self.is_enabled = self.config.feature_flags.resource_exhaustion_testing;
        self.is_initialized = true;

        Ok(())
    }

    /// Register a new resource chaos injection
    pub async fn register_chaos(
        &mut self,
        chaos_id: String,
        fault_config: FaultConfig,
        params: ResourceChaosParams,
    ) -> ArbitrageResult<()> {
        if !self.is_enabled {
            return Ok(());
        }

        let chaos_state = ResourceChaosState {
            chaos_id: chaos_id.clone(),
            fault_config,
            params,
            stats: ResourceChaosStats::default(),
            activated_at: now_instant(),
            active_memory_allocations: Vec::new(),
            active_cpu_operations: 0,
            concurrent_operations: 0,
        };

        self.active_chaos.insert(chaos_id, chaos_state);
        Ok(())
    }

    /// Check if resource operation should be affected by chaos injection
    pub async fn should_inject_chaos(
        &mut self,
        path: &str,
        method: &str,
        headers: &HashMap<String, String>,
    ) -> ArbitrageResult<Option<ResourceChaosInjection>> {
        if !self.is_enabled || self.active_chaos.is_empty() {
            return Ok(None);
        }

        for chaos_state in self.active_chaos.values_mut() {
            // Check if chaos has expired
            if chaos_state.activated_at.elapsed()
                > Duration::from_secs(chaos_state.fault_config.duration_seconds)
            {
                continue;
            }

            // Check if this operation matches the chaos criteria
            if Self::matches_chaos_criteria_static(&chaos_state.params, path, method, headers) {
                chaos_state.stats.total_operations += 1;

                // Check concurrent operation limits
                if chaos_state.concurrent_operations >= chaos_state.params.max_concurrent_chaos {
                    continue;
                }

                // Check time windows
                if !chaos_state.params.time_windows.is_empty()
                    && !Self::is_within_time_window(&chaos_state.params.time_windows)
                {
                    continue;
                }

                // Determine chaos injection based on probability
                if let Some(injection) = Self::determine_chaos_injection_static(
                    &chaos_state.fault_config,
                    &chaos_state.params,
                )? {
                    chaos_state.stats.total_chaos_injected += 1;
                    chaos_state.concurrent_operations += 1;

                    // Update specific chaos counters and execute chaos
                    match &injection.chaos_type {
                        ResourceChaosType::CpuExhaustion {
                            duration_ms,
                            intensity,
                        } => {
                            chaos_state.stats.cpu_exhaustion_count += 1;
                            chaos_state.stats.total_cpu_time_consumed_ms += duration_ms;
                            chaos_state.active_cpu_operations += 1;
                            // Execute CPU exhaustion in background (non-blocking)
                            #[cfg(target_arch = "wasm32")]
                            wasm_bindgen_futures::spawn_local(Self::execute_cpu_exhaustion(
                                *duration_ms,
                                *intensity,
                            ));
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let duration_ms = *duration_ms;
                                let intensity = *intensity;
                                tokio::spawn(Self::execute_cpu_exhaustion(duration_ms, intensity));
                            }
                        }
                        ResourceChaosType::MemoryPressure {
                            allocation_bytes,
                            hold_duration_ms,
                        } => {
                            chaos_state.stats.memory_pressure_count += 1;
                            chaos_state.stats.total_memory_allocated_bytes += allocation_bytes;
                            // Execute memory pressure
                            Self::execute_memory_pressure(
                                chaos_state,
                                *allocation_bytes,
                                *hold_duration_ms,
                            );
                        }
                        ResourceChaosType::ExecutionTimeout { timeout_ms } => {
                            chaos_state.stats.execution_timeout_count += 1;
                            // Execute timeout simulation
                            #[cfg(target_arch = "wasm32")]
                            wasm_bindgen_futures::spawn_local(Self::execute_timeout_simulation(
                                *timeout_ms,
                            ));
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let timeout_ms = *timeout_ms;
                                tokio::spawn(Self::execute_timeout_simulation(timeout_ms));
                            }
                        }
                        ResourceChaosType::EventLoopBlocking { block_duration_ms } => {
                            chaos_state.stats.event_loop_blocking_count += 1;
                            // Execute event loop blocking (synchronous)
                            Self::execute_event_loop_blocking(*block_duration_ms);
                        }
                        ResourceChaosType::ConcurrentOverload {
                            concurrent_operations,
                        } => {
                            chaos_state.stats.concurrent_overload_count += 1;
                            // Execute concurrent overload simulation
                            #[cfg(target_arch = "wasm32")]
                            wasm_bindgen_futures::spawn_local(Self::execute_concurrent_overload(
                                *concurrent_operations,
                            ));
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let concurrent_operations = *concurrent_operations;
                                tokio::spawn(Self::execute_concurrent_overload(
                                    concurrent_operations,
                                ));
                            }
                        }
                        ResourceChaosType::ResourceLeak {
                            leak_type,
                            leak_amount,
                        } => {
                            chaos_state.stats.resource_leak_count += 1;
                            // Execute resource leak simulation
                            Self::execute_resource_leak(leak_type, *leak_amount);
                        }
                        ResourceChaosType::GarbageCollectionPressure { object_count } => {
                            chaos_state.stats.gc_pressure_count += 1;
                            // Execute GC pressure simulation
                            Self::execute_gc_pressure(*object_count);
                        }
                    }

                    chaos_state.stats.last_injection_at = Some(chrono::Utc::now().to_rfc3339());

                    return Ok(Some(injection));
                }
            }
        }

        Ok(None)
    }

    /// Check if operation matches chaos criteria (instance method for tests)
    #[allow(dead_code)]
    fn matches_chaos_criteria(
        &self,
        params: &ResourceChaosParams,
        path: &str,
        method: &str,
        headers: &HashMap<String, String>,
    ) -> bool {
        Self::matches_chaos_criteria_static(params, path, method, headers)
    }

    /// Check if operation matches chaos criteria (static version)
    fn matches_chaos_criteria_static(
        params: &ResourceChaosParams,
        path: &str,
        method: &str,
        headers: &HashMap<String, String>,
    ) -> bool {
        // Check HTTP method
        if !params.target_methods.is_empty()
            && !params.target_methods.contains(&method.to_uppercase())
        {
            return false;
        }

        // Check path patterns
        if !params.target_paths.is_empty() {
            let matches_pattern = params
                .target_paths
                .iter()
                .any(|pattern| Self::matches_path_pattern_static(path, pattern));
            if !matches_pattern {
                return false;
            }
        }

        // Check header patterns
        for (header_name, header_pattern) in &params.header_patterns {
            if let Some(header_value) = headers.get(header_name) {
                if !Self::matches_header_pattern_static(header_value, header_pattern) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Simple path pattern matching (static version)
    fn matches_path_pattern_static(path: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        // Simple wildcard matching for paths
        if let Some(stripped) = pattern.strip_prefix('*') {
            if let Some(suffix) = stripped.strip_suffix('*') {
                return path.contains(suffix);
            } else {
                return path.ends_with(stripped);
            }
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            return path.starts_with(prefix);
        }

        path == pattern
    }

    /// Simple header pattern matching (static version)
    fn matches_header_pattern_static(header_value: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        // Simple wildcard matching for headers
        if let Some(stripped) = pattern.strip_prefix('*') {
            if let Some(suffix) = stripped.strip_suffix('*') {
                return header_value.contains(suffix);
            } else {
                return header_value.ends_with(stripped);
            }
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            return header_value.starts_with(prefix);
        }

        header_value == pattern
    }

    /// Check if current time is within any of the specified time windows
    fn is_within_time_window(time_windows: &[TimeWindow]) -> bool {
        let now = chrono::Utc::now();
        let current_hour = now.hour() as u8;
        let current_weekday = now.weekday().num_days_from_sunday() as u8;

        time_windows.iter().any(|window| {
            let hour_matches = if window.start_hour <= window.end_hour {
                current_hour >= window.start_hour && current_hour <= window.end_hour
            } else {
                // Handle overnight windows (e.g., 22-6)
                current_hour >= window.start_hour || current_hour <= window.end_hour
            };

            let day_matches =
                window.days_of_week.is_empty() || window.days_of_week.contains(&current_weekday);

            hour_matches && day_matches
        })
    }

    /// Determine what chaos injection to apply (static version)
    fn determine_chaos_injection_static(
        fault_config: &FaultConfig,
        params: &ResourceChaosParams,
    ) -> ArbitrageResult<Option<ResourceChaosInjection>> {
        // Use deterministic pseudo-random based on current time and intensity
        let random_value =
            (chrono::Utc::now().timestamp_millis() as f64 * fault_config.intensity) % 1.0;

        if random_value > fault_config.intensity {
            return Ok(None);
        }

        // Select chaos type based on configuration
        let chaos_type = if !params.chaos_types.is_empty() {
            let index = (random_value * params.chaos_types.len() as f64) as usize;
            params.chaos_types[index.min(params.chaos_types.len() - 1)].clone()
        } else {
            // Default chaos types based on fault configuration
            match fault_config.fault_type.as_str() {
                "cpu" => ResourceChaosType::CpuExhaustion {
                    duration_ms: ((fault_config.intensity * 1000.0) as u64).max(10),
                    intensity: fault_config.intensity,
                },
                "memory" => ResourceChaosType::MemoryPressure {
                    allocation_bytes: ((fault_config.intensity * 10.0 * 1024.0 * 1024.0) as u64)
                        .max(1024), // Max 10MB
                    hold_duration_ms: 1000,
                },
                "timeout" => ResourceChaosType::ExecutionTimeout {
                    timeout_ms: ((fault_config.intensity * 5000.0) as u64).max(100),
                },
                "eventloop" => ResourceChaosType::EventLoopBlocking {
                    block_duration_ms: ((fault_config.intensity * 100.0) as u64).max(1),
                },
                "concurrent" => ResourceChaosType::ConcurrentOverload {
                    concurrent_operations: ((fault_config.intensity * 10.0) as u32).max(1),
                },
                "leak" => ResourceChaosType::ResourceLeak {
                    leak_type: ResourceLeakType::Memory,
                    leak_amount: ((fault_config.intensity * 1024.0 * 1024.0) as u64).max(1024),
                },
                "gc" => ResourceChaosType::GarbageCollectionPressure {
                    object_count: ((fault_config.intensity * 10000.0) as u64).max(100),
                },
                _ => ResourceChaosType::CpuExhaustion {
                    duration_ms: 100,
                    intensity: 0.5,
                },
            }
        };

        Ok(Some(ResourceChaosInjection {
            chaos_type,
            metadata: Self::create_chaos_metadata_static(fault_config, params),
        }))
    }

    /// Create metadata for chaos injection (static version)
    fn create_chaos_metadata_static(
        fault_config: &FaultConfig,
        params: &ResourceChaosParams,
    ) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("fault_id".to_string(), fault_config.id.clone());
        metadata.insert("fault_type".to_string(), fault_config.fault_type.clone());
        metadata.insert("intensity".to_string(), fault_config.intensity.to_string());
        metadata.insert(
            "duration_seconds".to_string(),
            fault_config.duration_seconds.to_string(),
        );
        metadata.insert(
            "chaos_types_count".to_string(),
            params.chaos_types.len().to_string(),
        );
        metadata.insert(
            "target_paths_count".to_string(),
            params.target_paths.len().to_string(),
        );
        metadata.insert(
            "target_methods_count".to_string(),
            params.target_methods.len().to_string(),
        );
        metadata.insert(
            "max_concurrent_chaos".to_string(),
            params.max_concurrent_chaos.to_string(),
        );
        metadata.insert(
            "memory_limit_bytes".to_string(),
            params.memory_limit_bytes.to_string(),
        );
        metadata.insert(
            "cpu_time_limit_ms".to_string(),
            params.cpu_time_limit_ms.to_string(),
        );
        metadata.insert(
            "worker_specific_testing".to_string(),
            params.enable_worker_specific_testing.to_string(),
        );
        metadata.insert("injected_at".to_string(), chrono::Utc::now().to_rfc3339());
        metadata
    }

    /// Execute CPU exhaustion simulation
    async fn execute_cpu_exhaustion(duration_ms: u64, intensity: f64) {
        let start = now_instant();
        let target_duration = Duration::from_millis(duration_ms);

        while start.elapsed() < target_duration {
            // CPU-intensive calculation based on intensity
            let iterations = (intensity * 10000.0) as u64;
            let mut result = 0u64;
            for i in 0..iterations {
                result = result.wrapping_add(i.wrapping_mul(i));
            }

            // Prevent compiler optimization
            std::hint::black_box(result);

            // Yield control occasionally to prevent complete blocking
            if intensity < 1.0 {
                // In WASM, yielding is handled by the runtime automatically
                #[cfg(not(target_arch = "wasm32"))]
                tokio::task::yield_now().await;
            }
        }
    }

    /// Execute memory pressure simulation
    fn execute_memory_pressure(
        chaos_state: &mut ResourceChaosState,
        allocation_bytes: u64,
        _hold_duration_ms: u64,
    ) {
        // Allocate memory blocks to create pressure
        let block_size = 1024; // 1KB blocks
        let num_blocks = allocation_bytes / block_size;

        for _ in 0..num_blocks {
            let block = vec![0u8; block_size as usize];
            chaos_state.active_memory_allocations.push(block);
        }

        // Note: Memory will be held until the chaos state is dropped
        // In a real implementation, you might want to schedule cleanup
    }

    /// Execute timeout simulation
    async fn execute_timeout_simulation(timeout_ms: u64) {
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::sleep(Duration::from_millis(timeout_ms)).await;
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
    }

    /// Execute event loop blocking (synchronous operation)
    fn execute_event_loop_blocking(block_duration_ms: u64) {
        let start = now_instant();
        let target_duration = Duration::from_millis(block_duration_ms);

        // Busy wait to block the event loop
        while start.elapsed() < target_duration {
            std::hint::black_box(());
        }
    }

    /// Execute concurrent overload simulation
    async fn execute_concurrent_overload(concurrent_operations: u32) {
        #[cfg(target_arch = "wasm32")]
        {
            for _ in 0..concurrent_operations {
                wasm_bindgen_futures::spawn_local(async {
                    // Simulate some work
                    gloo_timers::future::sleep(Duration::from_millis(100)).await;
                    let mut result = 0u64;
                    for i in 0..1000 {
                        result = result.wrapping_add(i);
                    }
                    std::hint::black_box(result);
                });
            }
            // Note: In WASM, we don't wait for completion to avoid blocking
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut handles = Vec::new();
            for _ in 0..concurrent_operations {
                let handle = tokio::spawn(async {
                    // Simulate some work
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    let mut result = 0u64;
                    for i in 0..1000 {
                        result = result.wrapping_add(i);
                    }
                    std::hint::black_box(result);
                });
                handles.push(handle);
            }

            // Wait for all operations to complete
            for handle in handles {
                let _ = handle.await;
            }
        }
    }

    /// Execute resource leak simulation
    fn execute_resource_leak(leak_type: &ResourceLeakType, leak_amount: u64) {
        match leak_type {
            ResourceLeakType::Memory => {
                // Simulate memory leak by allocating and "forgetting" memory
                let _leaked_memory = vec![0u8; leak_amount as usize];
                std::mem::forget(_leaked_memory);
            }
            ResourceLeakType::EventListeners => {
                // Simulate event listener leak (limited in Workers environment)
                // This would be more relevant in browser contexts
            }
            ResourceLeakType::Timers => {
                // Simulate timer leak (adapted for WASM environment)
                for _ in 0..(leak_amount / 100) {
                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(async {
                        gloo_timers::future::sleep(Duration::from_millis(1000)).await;
                    });
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::spawn(async {
                            tokio::time::sleep(Duration::from_millis(1000)).await;
                        });
                    }
                }
            }
            ResourceLeakType::Promises => {
                // Simulate promise leak by creating unresolved promises
                for _ in 0..(leak_amount / 100) {
                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(async {
                        // Never-resolving future
                        std::future::pending::<()>().await;
                    });
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::spawn(async {
                            // Never-resolving future
                            std::future::pending::<()>().await;
                        });
                    }
                }
            }
        }
    }

    /// Execute garbage collection pressure simulation
    fn execute_gc_pressure(object_count: u64) {
        // Create many objects to trigger GC pressure
        let mut objects = Vec::new();
        for i in 0..object_count {
            let object = format!("gc_pressure_object_{}", i);
            objects.push(object);
        }

        // Force deallocation to trigger GC
        drop(objects);
    }

    /// Remove a chaos injection
    pub async fn remove_chaos(
        &mut self,
        chaos_id: &str,
    ) -> ArbitrageResult<Option<ResourceChaosStats>> {
        if let Some(chaos_state) = self.active_chaos.remove(chaos_id) {
            // Update global stats
            self.global_stats.total_operations += chaos_state.stats.total_operations;
            self.global_stats.total_chaos_injected += chaos_state.stats.total_chaos_injected;
            self.global_stats.cpu_exhaustion_count += chaos_state.stats.cpu_exhaustion_count;
            self.global_stats.memory_pressure_count += chaos_state.stats.memory_pressure_count;
            self.global_stats.execution_timeout_count += chaos_state.stats.execution_timeout_count;
            self.global_stats.event_loop_blocking_count +=
                chaos_state.stats.event_loop_blocking_count;
            self.global_stats.concurrent_overload_count +=
                chaos_state.stats.concurrent_overload_count;
            self.global_stats.resource_leak_count += chaos_state.stats.resource_leak_count;
            self.global_stats.gc_pressure_count += chaos_state.stats.gc_pressure_count;
            self.global_stats.total_cpu_time_consumed_ms +=
                chaos_state.stats.total_cpu_time_consumed_ms;
            self.global_stats.total_memory_allocated_bytes +=
                chaos_state.stats.total_memory_allocated_bytes;

            Ok(Some(chaos_state.stats))
        } else {
            Ok(None)
        }
    }

    /// Get global resource chaos statistics
    pub fn get_global_stats(&self) -> &ResourceChaosStats {
        &self.global_stats
    }

    /// Get chaos statistics for a specific chaos injection
    pub fn get_chaos_stats(&self, chaos_id: &str) -> Option<&ResourceChaosStats> {
        self.active_chaos.get(chaos_id).map(|state| &state.stats)
    }

    /// List all active chaos IDs
    pub fn list_active_chaos(&self) -> Vec<String> {
        self.active_chaos.keys().cloned().collect()
    }

    /// Clear all active chaos injections
    pub async fn clear_all_chaos(&mut self) -> ArbitrageResult<()> {
        self.active_chaos.clear();
        Ok(())
    }

    /// Check if resource chaos injection is enabled
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }
}

impl Default for ResourceChaosParams {
    fn default() -> Self {
        Self {
            chaos_types: vec![
                ResourceChaosType::CpuExhaustion {
                    duration_ms: 100,
                    intensity: 0.5,
                },
                ResourceChaosType::MemoryPressure {
                    allocation_bytes: 1024 * 1024, // 1MB
                    hold_duration_ms: 1000,
                },
            ],
            target_paths: vec!["*".to_string()],
            target_methods: vec!["GET".to_string(), "POST".to_string()],
            header_patterns: HashMap::new(),
            time_windows: Vec::new(),
            max_concurrent_chaos: 5,
            enable_worker_specific_testing: true,
            memory_limit_bytes: 128 * 1024 * 1024, // 128MB Workers limit
            cpu_time_limit_ms: 30000,              // 30 seconds Workers limit
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_chaos_params_default() {
        let params = ResourceChaosParams::default();
        assert!(!params.chaos_types.is_empty());
        assert!(!params.target_paths.is_empty());
        assert!(!params.target_methods.is_empty());
        assert_eq!(params.max_concurrent_chaos, 5);
        assert_eq!(params.memory_limit_bytes, 128 * 1024 * 1024);
        assert_eq!(params.cpu_time_limit_ms, 30000);
    }

    #[test]
    fn test_path_pattern_matching() {
        // Test wildcard patterns
        assert!(ResourceChaosInjector::matches_path_pattern_static(
            "/api/test",
            "*"
        ));
        assert!(ResourceChaosInjector::matches_path_pattern_static(
            "/api/test",
            "/api/*"
        ));
        assert!(ResourceChaosInjector::matches_path_pattern_static(
            "/api/test",
            "*api*"
        ));
        assert!(ResourceChaosInjector::matches_path_pattern_static(
            "/api/test",
            "*/test"
        ));

        // Test exact match
        assert!(ResourceChaosInjector::matches_path_pattern_static(
            "/api/test",
            "/api/test"
        ));
        assert!(!ResourceChaosInjector::matches_path_pattern_static(
            "/api/test",
            "/api/other"
        ));
    }

    #[test]
    fn test_time_window_matching() {
        let time_windows = vec![TimeWindow {
            start_hour: 9,
            end_hour: 17,
            days_of_week: vec![1, 2, 3, 4, 5], // Monday to Friday
        }];

        // This test would need to be more sophisticated to test actual time matching
        // For now, just test that the function doesn't panic
        let _result = ResourceChaosInjector::is_within_time_window(&time_windows);
    }

    #[test]
    fn test_chaos_criteria_matching() {
        let params = ResourceChaosParams {
            chaos_types: vec![ResourceChaosType::CpuExhaustion {
                duration_ms: 100,
                intensity: 0.5,
            }],
            target_paths: vec!["/api/*".to_string()],
            target_methods: vec!["GET".to_string()],
            header_patterns: HashMap::new(),
            time_windows: Vec::new(),
            max_concurrent_chaos: 5,
            enable_worker_specific_testing: true,
            memory_limit_bytes: 128 * 1024 * 1024,
            cpu_time_limit_ms: 30000,
        };

        let headers = HashMap::new();

        // Should match
        assert!(ResourceChaosInjector::matches_chaos_criteria_static(
            &params,
            "/api/test",
            "GET",
            &headers
        ));

        // Should not match - wrong method
        assert!(!ResourceChaosInjector::matches_chaos_criteria_static(
            &params,
            "/api/test",
            "POST",
            &headers
        ));

        // Should not match - wrong path pattern
        assert!(!ResourceChaosInjector::matches_chaos_criteria_static(
            &params,
            "/other/test",
            "GET",
            &headers
        ));
    }
}
