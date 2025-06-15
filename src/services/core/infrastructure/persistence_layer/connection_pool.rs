//! Connection Management Service for D1/R2 Persistence Layer
//!
//! Provides connection pooling, health monitoring, failover strategies, and circuit breaker patterns
//! for both D1 database connections and R2 storage access with comprehensive resource management

use crate::utils::now_system_time;
use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use worker::{Bucket as R2Bucket, D1Database, Env};

use super::{D1Config, R2Config};

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub min_connections: u32,
    pub max_connections: u32,
    pub connection_timeout_ms: u64,
    pub idle_timeout_ms: u64,
    pub health_check_interval_ms: u64,
    pub enable_circuit_breaker: bool,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_ms: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 5,
            max_connections: 50,
            connection_timeout_ms: 30000,
            idle_timeout_ms: 300000,         // 5 minutes
            health_check_interval_ms: 60000, // 1 minute
            enable_circuit_breaker: true,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_ms: 60000, // 1 minute
        }
    }
}

/// Circuit breaker states for connection management
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Failing fast, not allowing connections
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker for connection management
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub failure_threshold: u32,
    pub timeout: Duration,
    pub last_failure_time: Option<SystemTime>,
    pub success_count: u32,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, timeout_ms: u64) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            failure_threshold,
            timeout: Duration::from_millis(timeout_ms),
            last_failure_time: None,
            success_count: 0,
        }
    }

    pub fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= 3 {
                    // Require multiple successes to close
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                }
            }
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            _ => {}
        }
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(now_system_time());
        self.success_count = 0;

        if self.failure_count >= self.failure_threshold {
            self.state = CircuitBreakerState::Open;
        }
    }

    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    if now_system_time()
                        .duration_since(last_failure)
                        .unwrap_or(Duration::ZERO)
                        > self.timeout
                    {
                        self.state = CircuitBreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }
}

/// Connection information for monitoring
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_id: String,
    pub created_at: Instant,
    pub last_used: Instant,
    pub is_healthy: bool,
    pub connection_type: ConnectionType,
    pub usage_count: u64,
    pub last_error: Option<String>,
}

/// Type of connection managed by the pool
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionType {
    D1Database,
    R2Storage,
}

/// Connection pool manager for D1 and R2 resources
pub struct ConnectionPool {
    /// D1 database connections
    d1_connections: Arc<Mutex<Vec<Arc<D1Database>>>>,
    /// R2 storage connections  
    r2_connections: Arc<Mutex<Vec<Arc<R2Bucket>>>>,
    /// Connection information for monitoring
    connection_info: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
    /// D1 circuit breaker
    d1_circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    /// R2 circuit breaker
    r2_circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    /// Pool configuration
    config: PoolConfig,
    /// D1 configuration
    d1_config: D1Config,
    /// R2 configuration
    r2_config: R2Config,
    /// Connection statistics
    stats: Arc<Mutex<ConnectionStats>>,
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub d1_active_connections: u32,
    pub r2_active_connections: u32,
    pub total_connections_created: u64,
    pub total_connections_destroyed: u64,
    pub connection_requests: u64,
    pub connection_failures: u64,
    pub average_connection_time_ms: f64,
    pub last_health_check: u64,
}

impl Default for ConnectionStats {
    fn default() -> Self {
        Self {
            d1_active_connections: 0,
            r2_active_connections: 0,
            total_connections_created: 0,
            total_connections_destroyed: 0,
            connection_requests: 0,
            connection_failures: 0,
            average_connection_time_ms: 0.0,
            last_health_check: 0,
        }
    }
}

/// Connection health status for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionHealth {
    pub is_healthy: bool,
    pub d1_status: ServiceHealth,
    pub r2_status: ServiceHealth,
    pub pool_utilization: f64,
    pub circuit_breaker_status: CircuitBreakerStatus,
    pub last_check: u64,
}

/// Individual service health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub is_available: bool,
    pub active_connections: u32,
    pub response_time_ms: f64,
    pub error_rate: f64,
    pub last_error: Option<String>,
}

/// Circuit breaker status for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStatus {
    pub d1_state: String,
    pub r2_state: String,
    pub d1_failure_count: u32,
    pub r2_failure_count: u32,
}

impl ConnectionPool {
    /// Create new connection pool
    pub async fn new(env: &Env, config: &PoolConfig) -> ArbitrageResult<Self> {
        let d1_config = D1Config::default();
        let r2_config = R2Config::default();

        let pool = Self {
            d1_connections: Arc::new(Mutex::new(Vec::new())),
            #[allow(clippy::arc_with_non_send_sync)]
            r2_connections: Arc::new(Mutex::new(Vec::new())),
            connection_info: Arc::new(Mutex::new(HashMap::new())),
            d1_circuit_breaker: Arc::new(Mutex::new(CircuitBreaker::new(
                config.circuit_breaker_threshold,
                config.circuit_breaker_timeout_ms,
            ))),
            r2_circuit_breaker: Arc::new(Mutex::new(CircuitBreaker::new(
                config.circuit_breaker_threshold,
                config.circuit_breaker_timeout_ms,
            ))),
            config: config.clone(),
            d1_config,
            r2_config,
            stats: Arc::new(Mutex::new(ConnectionStats::default())),
        };

        // Initialize minimum connections
        pool.initialize_connections(env).await?;

        Ok(pool)
    }

    /// Initialize minimum connections for both D1 and R2
    async fn initialize_connections(&self, env: &Env) -> ArbitrageResult<()> {
        // Initialize D1 connections
        for _ in 0..self.config.min_connections {
            if let Ok(d1_conn) = self.create_d1_connection(env).await {
                let mut connections = self.d1_connections.lock().unwrap();
                connections.push(Arc::new(d1_conn));
            }
        }

        // Initialize R2 connections
        for _ in 0..self.config.min_connections {
            if let Ok(r2_conn) = self.create_r2_connection(env).await {
                let mut connections = self.r2_connections.lock().unwrap();
                #[allow(clippy::arc_with_non_send_sync)]
                connections.push(Arc::new(r2_conn));
            }
        }

        Ok(())
    }

    /// Get D1 database connection from pool
    pub async fn get_d1_connection(&self, env: &Env) -> ArbitrageResult<Arc<D1Database>> {
        // Check circuit breaker
        {
            let mut circuit_breaker = self.d1_circuit_breaker.lock().unwrap();
            if !circuit_breaker.can_execute() {
                return Err(ArbitrageError::database_error("D1 circuit breaker is open"));
            }
        }

        // Try to get existing connection
        {
            let mut connections = self.d1_connections.lock().unwrap();
            if let Some(conn) = connections.pop() {
                // Update stats
                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.connection_requests += 1;
                    stats.d1_active_connections -= 1;
                }

                // Record usage
                self.record_connection_usage(&conn, ConnectionType::D1Database);

                return Ok(conn);
            }
        }

        // Create new connection if under limit
        let can_create_connection = {
            let stats = self.stats.lock().unwrap();
            stats.d1_active_connections < self.config.max_connections
        };

        if can_create_connection {
            let conn = self.create_d1_connection(env).await?;

            // Update stats
            {
                let mut stats = self.stats.lock().unwrap();
                stats.connection_requests += 1;
                stats.total_connections_created += 1;
            }

            // Record success
            {
                let mut circuit_breaker = self.d1_circuit_breaker.lock().unwrap();
                circuit_breaker.record_success();
            }

            #[allow(clippy::arc_with_non_send_sync)]
            return Ok(Arc::new(conn));
        }

        Err(ArbitrageError::database_error(
            "Connection pool exhausted for D1",
        ))
    }

    /// Get R2 bucket connection from pool
    pub async fn get_r2_connection(&self, env: &Env) -> ArbitrageResult<Arc<R2Bucket>> {
        // Check circuit breaker
        {
            let mut circuit_breaker = self.r2_circuit_breaker.lock().unwrap();
            if !circuit_breaker.can_execute() {
                return Err(ArbitrageError::database_error("R2 circuit breaker is open"));
            }
        }

        // Try to get existing connection
        {
            let mut connections = self.r2_connections.lock().unwrap();
            if let Some(conn) = connections.pop() {
                // Update stats
                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.connection_requests += 1;
                    stats.r2_active_connections -= 1;
                }

                // Record usage
                self.record_connection_usage(&conn, ConnectionType::R2Storage);

                return Ok(conn);
            }
        }

        // Create new connection if under limit
        let can_create_connection = {
            let stats = self.stats.lock().unwrap();
            stats.r2_active_connections < self.config.max_connections
        };

        if can_create_connection {
            let conn = self.create_r2_connection(env).await?;

            // Update stats
            {
                let mut stats = self.stats.lock().unwrap();
                stats.connection_requests += 1;
                stats.total_connections_created += 1;
            }

            // Record success
            {
                let mut circuit_breaker = self.r2_circuit_breaker.lock().unwrap();
                circuit_breaker.record_success();
            }

            #[allow(clippy::arc_with_non_send_sync)]
            return Ok(Arc::new(conn));
        }

        Err(ArbitrageError::database_error(
            "Connection pool exhausted for R2",
        ))
    }

    /// Return D1 connection to pool
    pub fn return_d1_connection(&self, conn: Arc<D1Database>) {
        // Check if connection is still healthy
        if self.is_connection_healthy(&conn, ConnectionType::D1Database) {
            let mut connections = self.d1_connections.lock().unwrap();
            if connections.len() < self.config.max_connections as usize {
                connections.push(conn);

                // Update stats
                let mut stats = self.stats.lock().unwrap();
                stats.d1_active_connections += 1;
            }
        }
    }

    /// Return R2 connection to pool
    pub fn return_r2_connection(&self, conn: Arc<R2Bucket>) {
        // Check if connection is still healthy
        if self.is_connection_healthy(&conn, ConnectionType::R2Storage) {
            let mut connections = self.r2_connections.lock().unwrap();
            if connections.len() < self.config.max_connections as usize {
                connections.push(conn);

                // Update stats
                let mut stats = self.stats.lock().unwrap();
                stats.r2_active_connections += 1;
            }
        }
    }

    /// Create new D1 database connection
    async fn create_d1_connection(&self, env: &Env) -> ArbitrageResult<D1Database> {
        let start_time = Instant::now();

        match env.d1(&self.d1_config.database_name) {
            Ok(db) => {
                // Update connection time stats
                let connection_time_ms = start_time.elapsed().as_millis() as f64;
                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.average_connection_time_ms =
                        (stats.average_connection_time_ms + connection_time_ms) / 2.0;
                }

                Ok(db)
            }
            Err(e) => {
                // Record failure
                {
                    let mut circuit_breaker = self.d1_circuit_breaker.lock().unwrap();
                    circuit_breaker.record_failure();
                }

                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.connection_failures += 1;
                }

                Err(ArbitrageError::database_error(format!(
                    "Failed to create D1 connection: {}",
                    e
                )))
            }
        }
    }

    /// Create new R2 bucket connection
    async fn create_r2_connection(&self, env: &Env) -> ArbitrageResult<R2Bucket> {
        let start_time = Instant::now();

        match env.bucket(&self.r2_config.bucket_name) {
            Ok(bucket) => {
                // Update connection time stats
                let connection_time_ms = start_time.elapsed().as_millis() as f64;
                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.average_connection_time_ms =
                        (stats.average_connection_time_ms + connection_time_ms) / 2.0;
                }

                Ok(bucket)
            }
            Err(e) => {
                // Record failure
                {
                    let mut circuit_breaker = self.r2_circuit_breaker.lock().unwrap();
                    circuit_breaker.record_failure();
                }

                {
                    let mut stats = self.stats.lock().unwrap();
                    stats.connection_failures += 1;
                }

                Err(ArbitrageError::database_error(format!(
                    "Failed to create R2 connection: {}",
                    e
                )))
            }
        }
    }

    /// Record connection usage for monitoring
    fn record_connection_usage<T>(&self, _conn: &Arc<T>, conn_type: ConnectionType) {
        let connection_id = self.generate_connection_id(&conn_type);
        let mut info_map = self.connection_info.lock().unwrap();

        if let Some(info) = info_map.get_mut(&connection_id) {
            info.last_used = Instant::now();
            info.usage_count += 1;
        } else {
            let info = ConnectionInfo {
                connection_id: connection_id.clone(),
                created_at: Instant::now(),
                last_used: Instant::now(),
                is_healthy: true,
                connection_type: conn_type,
                usage_count: 1,
                last_error: None,
            };
            info_map.insert(connection_id, info);
        }
    }

    /// Check if connection is healthy
    fn is_connection_healthy<T>(&self, _conn: &Arc<T>, _conn_type: ConnectionType) -> bool {
        // For now, assume all connections are healthy
        // In a real implementation, this would perform actual health checks
        true
    }

    /// Generate unique connection ID
    fn generate_connection_id(&self, conn_type: &ConnectionType) -> String {
        let timestamp = now_system_time()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        match conn_type {
            ConnectionType::D1Database => format!("d1_{}", timestamp),
            ConnectionType::R2Storage => format!("r2_{}", timestamp),
        }
    }

    /// Perform health check on all connections
    pub async fn health_check(&self) -> ArbitrageResult<ConnectionHealth> {
        let start_time = Instant::now();

        // Check D1 health
        let d1_health = self.check_d1_health().await;

        // Check R2 health
        let r2_health = self.check_r2_health().await;

        // Calculate pool utilization
        let pool_utilization = {
            let stats = self.stats.lock().unwrap();
            let total_active = stats.d1_active_connections + stats.r2_active_connections;
            let total_max = self.config.max_connections * 2; // D1 + R2
            total_active as f64 / total_max as f64
        };

        // Get circuit breaker status
        let circuit_breaker_status = {
            let d1_breaker = self.d1_circuit_breaker.lock().unwrap();
            let r2_breaker = self.r2_circuit_breaker.lock().unwrap();

            CircuitBreakerStatus {
                d1_state: format!("{:?}", d1_breaker.state),
                r2_state: format!("{:?}", r2_breaker.state),
                d1_failure_count: d1_breaker.failure_count,
                r2_failure_count: r2_breaker.failure_count,
            }
        };

        // Update health check time
        {
            let mut stats = self.stats.lock().unwrap();
            stats.last_health_check = now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
        }

        let overall_healthy = d1_health.is_available && r2_health.is_available;

        Ok(ConnectionHealth {
            is_healthy: overall_healthy,
            d1_status: d1_health,
            r2_status: r2_health,
            pool_utilization,
            circuit_breaker_status,
            last_check: start_time.elapsed().as_millis() as u64,
        })
    }

    /// Check D1 database health
    async fn check_d1_health(&self) -> ServiceHealth {
        let stats = self.stats.lock().unwrap();
        let circuit_breaker = self.d1_circuit_breaker.lock().unwrap();

        ServiceHealth {
            is_available: circuit_breaker.state != CircuitBreakerState::Open,
            active_connections: stats.d1_active_connections,
            response_time_ms: stats.average_connection_time_ms,
            error_rate: if stats.connection_requests > 0 {
                stats.connection_failures as f64 / stats.connection_requests as f64
            } else {
                0.0
            },
            last_error: None,
        }
    }

    /// Check R2 storage health
    async fn check_r2_health(&self) -> ServiceHealth {
        let stats = self.stats.lock().unwrap();
        let circuit_breaker = self.r2_circuit_breaker.lock().unwrap();

        ServiceHealth {
            is_available: circuit_breaker.state != CircuitBreakerState::Open,
            active_connections: stats.r2_active_connections,
            response_time_ms: stats.average_connection_time_ms,
            error_rate: if stats.connection_requests > 0 {
                stats.connection_failures as f64 / stats.connection_requests as f64
            } else {
                0.0
            },
            last_error: None,
        }
    }

    /// Get connection pool metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<ConnectionMetrics> {
        let stats = self.stats.lock().unwrap();
        let connection_info = self.connection_info.lock().unwrap();

        Ok(ConnectionMetrics {
            total_connections: stats.d1_active_connections + stats.r2_active_connections,
            d1_connections: stats.d1_active_connections,
            r2_connections: stats.r2_active_connections,
            pool_utilization: {
                let total_active = stats.d1_active_connections + stats.r2_active_connections;
                let total_max = self.config.max_connections * 2;
                total_active as f64 / total_max as f64
            },
            average_connection_time_ms: stats.average_connection_time_ms,
            connection_requests: stats.connection_requests,
            connection_failures: stats.connection_failures,
            active_connection_count: connection_info.len() as u32,
            collected_at: now_system_time()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        })
    }

    /// Clean up idle connections
    pub async fn cleanup_idle_connections(&self) -> ArbitrageResult<u32> {
        let idle_timeout = Duration::from_millis(self.config.idle_timeout_ms);
        let now = Instant::now();
        let mut cleaned_count = 0;

        // Clean up connection info for idle connections
        {
            let mut info_map = self.connection_info.lock().unwrap();
            info_map.retain(|_, info| {
                if now.duration_since(info.last_used) > idle_timeout {
                    cleaned_count += 1;
                    false
                } else {
                    true
                }
            });
        }

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_connections_destroyed += cleaned_count as u64;
        }

        Ok(cleaned_count)
    }
}

/// Connection pool metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetrics {
    pub total_connections: u32,
    pub d1_connections: u32,
    pub r2_connections: u32,
    pub pool_utilization: f64,
    pub average_connection_time_ms: f64,
    pub connection_requests: u64,
    pub connection_failures: u64,
    pub active_connection_count: u32,
    pub collected_at: u64,
}

/// Connection manager wrapper for simplified usage
pub struct ConnectionManager {
    pool: Arc<ConnectionPool>,
}

impl ConnectionManager {
    /// Create new connection manager
    pub async fn new(env: &Env, config: &PoolConfig) -> ArbitrageResult<Self> {
        #[allow(clippy::arc_with_non_send_sync)]
        let pool = Arc::new(ConnectionPool::new(env, config).await?);

        Ok(Self { pool })
    }

    /// Execute D1 operation with automatic connection management
    pub async fn with_d1_connection<F, R>(&self, env: &Env, operation: F) -> ArbitrageResult<R>
    where
        F: FnOnce(
            Arc<D1Database>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = ArbitrageResult<R>> + 'static>,
        >,
    {
        let conn = self.pool.get_d1_connection(env).await?;
        let result = operation(conn.clone()).await;
        self.pool.return_d1_connection(conn);
        result
    }

    /// Execute R2 operation with automatic connection management
    pub async fn with_r2_connection<F, R>(&self, env: &Env, operation: F) -> ArbitrageResult<R>
    where
        F: FnOnce(
            Arc<R2Bucket>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = ArbitrageResult<R>> + 'static>,
        >,
    {
        let conn = self.pool.get_r2_connection(env).await?;
        let result = operation(conn.clone()).await;
        self.pool.return_r2_connection(conn);
        result
    }

    /// Get connection pool health
    pub async fn health_check(&self) -> ArbitrageResult<ConnectionHealth> {
        self.pool.health_check().await
    }

    /// Get connection pool metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<ConnectionMetrics> {
        self.pool.get_metrics().await
    }

    /// Clean up idle connections
    pub async fn cleanup_idle_connections(&self) -> ArbitrageResult<u32> {
        self.pool.cleanup_idle_connections().await
    }
}
