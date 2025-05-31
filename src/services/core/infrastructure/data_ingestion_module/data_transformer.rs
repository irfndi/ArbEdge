// Data Transformer - Data Format Standardization and Schema Validation
// Provides multi-format support, schema validation, and compression optimization

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported data formats for transformation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat {
    Json,
    Avro,
    Parquet,
    Csv,
    MessagePack,
    Protobuf,
    Custom(String),
}

impl DataFormat {
    pub fn as_str(&self) -> &str {
        match self {
            DataFormat::Json => "json",
            DataFormat::Avro => "avro",
            DataFormat::Parquet => "parquet",
            DataFormat::Csv => "csv",
            DataFormat::MessagePack => "messagepack",
            DataFormat::Protobuf => "protobuf",
            DataFormat::Custom(name) => name,
        }
    }

    pub fn mime_type(&self) -> &str {
        match self {
            DataFormat::Json => "application/json",
            DataFormat::Avro => "application/avro",
            DataFormat::Parquet => "application/parquet",
            DataFormat::Csv => "text/csv",
            DataFormat::MessagePack => "application/msgpack",
            DataFormat::Protobuf => "application/protobuf",
            DataFormat::Custom(_) => "application/octet-stream",
        }
    }

    pub fn supports_compression(&self) -> bool {
        match self {
            DataFormat::Json => true,
            DataFormat::Avro => true,
            DataFormat::Parquet => false, // Already compressed
            DataFormat::Csv => true,
            DataFormat::MessagePack => false, // Already compact
            DataFormat::Protobuf => true,
            DataFormat::Custom(_) => true,
        }
    }

    pub fn default_compression(&self) -> CompressionType {
        match self {
            DataFormat::Json => CompressionType::Gzip,
            DataFormat::Avro => CompressionType::Snappy,
            DataFormat::Parquet => CompressionType::None,
            DataFormat::Csv => CompressionType::Gzip,
            DataFormat::MessagePack => CompressionType::None,
            DataFormat::Protobuf => CompressionType::Lz4,
            DataFormat::Custom(_) => CompressionType::Gzip,
        }
    }
}

/// Compression types for data optimization
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionType {
    None,
    Gzip,
    Snappy,
    Lz4,
    Zstd,
    Brotli,
}

impl CompressionType {
    pub fn as_str(&self) -> &str {
        match self {
            CompressionType::None => "none",
            CompressionType::Gzip => "gzip",
            CompressionType::Snappy => "snappy",
            CompressionType::Lz4 => "lz4",
            CompressionType::Zstd => "zstd",
            CompressionType::Brotli => "brotli",
        }
    }

    pub fn compression_ratio(&self) -> f32 {
        match self {
            CompressionType::None => 1.0,
            CompressionType::Gzip => 0.3,   // ~70% compression
            CompressionType::Snappy => 0.5, // ~50% compression
            CompressionType::Lz4 => 0.6,    // ~40% compression
            CompressionType::Zstd => 0.25,  // ~75% compression
            CompressionType::Brotli => 0.2, // ~80% compression
        }
    }

    pub fn speed_score(&self) -> u8 {
        match self {
            CompressionType::None => 10,
            CompressionType::Lz4 => 9,
            CompressionType::Snappy => 8,
            CompressionType::Gzip => 6,
            CompressionType::Zstd => 5,
            CompressionType::Brotli => 3,
        }
    }
}

/// Transformation rules for data processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationRule {
    pub rule_id: String,
    pub name: String,
    pub description: String,
    pub source_format: DataFormat,
    pub target_format: DataFormat,
    pub compression: CompressionType,
    pub schema_validation: bool,
    pub data_enrichment: bool,
    pub error_handling: ErrorHandlingStrategy,
    pub field_mappings: HashMap<String, String>,
    pub default_values: HashMap<String, serde_json::Value>,
    pub validation_rules: Vec<ValidationRule>,
    pub enabled: bool,
    pub priority: u8,
}

impl TransformationRule {
    pub fn new(name: String, source_format: DataFormat, target_format: DataFormat) -> Self {
        Self {
            rule_id: uuid::Uuid::new_v4().to_string(),
            name,
            description: String::new(),
            source_format: source_format.clone(),
            target_format: target_format.clone(),
            compression: target_format.default_compression(),
            schema_validation: true,
            data_enrichment: true,
            error_handling: ErrorHandlingStrategy::RetryWithFallback,
            field_mappings: HashMap::new(),
            default_values: HashMap::new(),
            validation_rules: Vec::new(),
            enabled: true,
            priority: 5,
        }
    }

    pub fn with_compression(mut self, compression: CompressionType) -> Self {
        self.compression = compression;
        self
    }

    pub fn with_field_mapping(mut self, source_field: String, target_field: String) -> Self {
        self.field_mappings.insert(source_field, target_field);
        self
    }

    pub fn with_default_value(mut self, field: String, value: serde_json::Value) -> Self {
        self.default_values.insert(field, value);
        self
    }

    pub fn with_validation_rule(mut self, rule: ValidationRule) -> Self {
        self.validation_rules.push(rule);
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Validation rules for data quality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    pub field_name: String,
    pub rule_type: ValidationType,
    pub required: bool,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub pattern: Option<String>,
    pub allowed_values: Option<Vec<String>>,
    pub custom_validator: Option<String>,
}

/// Validation types for data fields
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationType {
    String,
    Number,
    Integer,
    Boolean,
    Email,
    Url,
    Timestamp,
    Uuid,
    Json,
    Custom(String),
}

/// Error handling strategies for transformation failures
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorHandlingStrategy {
    Fail,
    Skip,
    UseDefault,
    RetryWithFallback,
    LogAndContinue,
}

/// Transformation metrics for performance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationMetrics {
    pub total_transformations: u64,
    pub successful_transformations: u64,
    pub failed_transformations: u64,
    pub skipped_transformations: u64,
    pub average_transformation_time_ms: f64,
    pub min_transformation_time_ms: f64,
    pub max_transformation_time_ms: f64,
    pub compression_savings_bytes: u64,
    pub validation_failures: u64,
    pub schema_errors: u64,
    pub transformations_by_format: HashMap<DataFormat, u64>,
    pub transformations_by_rule: HashMap<String, u64>,
    pub last_updated: u64,
}

impl Default for TransformationMetrics {
    fn default() -> Self {
        Self {
            total_transformations: 0,
            successful_transformations: 0,
            failed_transformations: 0,
            skipped_transformations: 0,
            average_transformation_time_ms: 0.0,
            min_transformation_time_ms: f64::MAX,
            max_transformation_time_ms: 0.0,
            compression_savings_bytes: 0,
            validation_failures: 0,
            schema_errors: 0,
            transformations_by_format: HashMap::new(),
            transformations_by_rule: HashMap::new(),
            last_updated: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

/// Configuration for DataTransformer
#[derive(Debug, Clone)]
pub struct DataTransformerConfig {
    pub enable_schema_validation: bool,
    pub enable_data_enrichment: bool,
    pub enable_compression: bool,
    pub enable_error_recovery: bool,
    pub enable_performance_optimization: bool,
    pub max_concurrent_transformations: u32,
    pub transformation_timeout_seconds: u64,
    pub compression_threshold_bytes: usize,
    pub validation_timeout_seconds: u64,
    pub enable_caching: bool,
    pub cache_ttl_seconds: u64,
    pub enable_metrics: bool,
    pub default_error_handling: ErrorHandlingStrategy,
    pub max_retries: u32,
    pub retry_delay_seconds: u64,
}

impl Default for DataTransformerConfig {
    fn default() -> Self {
        Self {
            enable_schema_validation: true,
            enable_data_enrichment: true,
            enable_compression: true,
            enable_error_recovery: true,
            enable_performance_optimization: true,
            max_concurrent_transformations: 20,
            transformation_timeout_seconds: 30,
            compression_threshold_bytes: 1024, // 1KB
            validation_timeout_seconds: 5,
            enable_caching: true,
            cache_ttl_seconds: 300, // 5 minutes
            enable_metrics: true,
            default_error_handling: ErrorHandlingStrategy::RetryWithFallback,
            max_retries: 3,
            retry_delay_seconds: 1,
        }
    }
}

impl DataTransformerConfig {
    /// Create configuration optimized for high performance
    pub fn high_performance() -> Self {
        Self {
            max_concurrent_transformations: 50,
            transformation_timeout_seconds: 15,
            compression_threshold_bytes: 512, // More aggressive compression
            validation_timeout_seconds: 2,
            enable_performance_optimization: true,
            enable_caching: true,
            cache_ttl_seconds: 600, // 10 minutes
            ..Default::default()
        }
    }

    /// Create configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            max_concurrent_transformations: 10,
            transformation_timeout_seconds: 60,
            validation_timeout_seconds: 10,
            enable_error_recovery: true,
            max_retries: 5,
            retry_delay_seconds: 2,
            default_error_handling: ErrorHandlingStrategy::RetryWithFallback,
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.max_concurrent_transformations == 0 {
            return Err(ArbitrageError::validation_error(
                "max_concurrent_transformations must be greater than 0",
            ));
        }
        if self.transformation_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "transformation_timeout_seconds must be greater than 0",
            ));
        }
        if self.compression_threshold_bytes == 0 {
            return Err(ArbitrageError::validation_error(
                "compression_threshold_bytes must be greater than 0",
            ));
        }
        if self.validation_timeout_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "validation_timeout_seconds must be greater than 0",
            ));
        }
        if self.cache_ttl_seconds == 0 {
            return Err(ArbitrageError::validation_error(
                "cache_ttl_seconds must be greater than 0",
            ));
        }
        if self.max_retries == 0 {
            return Err(ArbitrageError::validation_error(
                "max_retries must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Data transformation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationRequest {
    pub request_id: String,
    pub source_data: serde_json::Value,
    pub source_format: DataFormat,
    pub target_format: DataFormat,
    pub transformation_rule_id: Option<String>,
    pub metadata: HashMap<String, String>,
    pub timestamp: u64,
    pub priority: u8,
    pub timeout_seconds: Option<u64>,
}

impl TransformationRequest {
    pub fn new(
        source_data: serde_json::Value,
        source_format: DataFormat,
        target_format: DataFormat,
    ) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            source_data,
            source_format,
            target_format,
            transformation_rule_id: None,
            metadata: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            priority: 5,
            timeout_seconds: None,
        }
    }

    pub fn with_rule(mut self, rule_id: String) -> Self {
        self.transformation_rule_id = Some(rule_id);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Data transformation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationResponse {
    pub request_id: String,
    pub success: bool,
    pub transformed_data: Option<serde_json::Value>,
    pub target_format: DataFormat,
    pub compression_type: CompressionType,
    pub original_size_bytes: usize,
    pub compressed_size_bytes: usize,
    pub transformation_time_ms: u64,
    pub validation_errors: Vec<String>,
    pub warnings: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub timestamp: u64,
}

/// Data Transformer for format standardization and validation
pub struct DataTransformer {
    config: DataTransformerConfig,
    logger: crate::utils::logger::Logger,

    // Transformation rules
    transformation_rules: std::sync::Arc<std::sync::Mutex<HashMap<String, TransformationRule>>>,

    // Metrics tracking
    metrics: std::sync::Arc<std::sync::Mutex<TransformationMetrics>>,

    // Cache for transformed data
    transformation_cache:
        std::sync::Arc<std::sync::Mutex<HashMap<String, (serde_json::Value, u64)>>>, // hash -> (data, timestamp)

    // Performance tracking
    startup_time: u64,
}

impl DataTransformer {
    /// Create new DataTransformer instance
    pub fn new(config: DataTransformerConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        // Validate configuration
        config.validate()?;

        // Initialize default transformation rules
        let mut rules = HashMap::new();

        // JSON to JSON (validation only)
        let json_validation_rule = TransformationRule::new(
            "json_validation".to_string(),
            DataFormat::Json,
            DataFormat::Json,
        )
        .with_compression(CompressionType::Gzip);
        rules.insert(json_validation_rule.rule_id.clone(), json_validation_rule);

        // CSV to JSON
        let csv_to_json_rule =
            TransformationRule::new("csv_to_json".to_string(), DataFormat::Csv, DataFormat::Json)
                .with_compression(CompressionType::Gzip);
        rules.insert(csv_to_json_rule.rule_id.clone(), csv_to_json_rule);

        logger.info(&format!(
            "DataTransformer initialized: schema_validation={}, compression={}, max_concurrent={}",
            config.enable_schema_validation,
            config.enable_compression,
            config.max_concurrent_transformations
        ));

        Ok(Self {
            config,
            logger,
            transformation_rules: std::sync::Arc::new(std::sync::Mutex::new(rules)),
            metrics: std::sync::Arc::new(std::sync::Mutex::new(TransformationMetrics::default())),
            transformation_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            startup_time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Transform data according to request
    pub async fn transform(
        &self,
        request: TransformationRequest,
    ) -> ArbitrageResult<TransformationResponse> {
        let start_time = chrono::Utc::now().timestamp_millis() as u64;

        // Check cache first
        if self.config.enable_caching {
            if let Some(cached_result) = self.get_from_cache(&request).await {
                return Ok(cached_result);
            }
        }

        // Get transformation rule
        let rule = if let Some(rule_id) = &request.transformation_rule_id {
            self.get_transformation_rule(rule_id).await
        } else {
            self.find_transformation_rule(&request.source_format, &request.target_format)
                .await
        };

        let rule = match rule {
            Some(r) => r,
            None => {
                return Ok(self
                    .create_error_response(
                        &request,
                        "No transformation rule found".to_string(),
                        start_time,
                    )
                    .await);
            }
        };

        // Perform transformation
        match self
            .execute_transformation(&request, &rule, start_time)
            .await
        {
            Ok(response) => {
                // Cache successful result
                if self.config.enable_caching && response.success {
                    self.cache_result(&request, &response).await;
                }

                self.record_success(&request, &rule, start_time).await;
                Ok(response)
            }
            Err(e) => {
                self.record_failure(&request, &rule, start_time, &e).await;

                // Handle error according to strategy
                match rule.error_handling {
                    ErrorHandlingStrategy::Fail => Err(e),
                    ErrorHandlingStrategy::Skip => {
                        Ok(self.create_skip_response(&request, start_time).await)
                    }
                    ErrorHandlingStrategy::UseDefault => Ok(self
                        .create_default_response(&request, &rule, start_time)
                        .await),
                    ErrorHandlingStrategy::RetryWithFallback => {
                        // Try with fallback rule
                        self.retry_with_fallback(&request, start_time).await
                    }
                    ErrorHandlingStrategy::LogAndContinue => {
                        self.logger
                            .warn(&format!("Transformation failed but continuing: {}", e));
                        Ok(self.create_skip_response(&request, start_time).await)
                    }
                }
            }
        }
    }

    /// Transform multiple requests in batch
    pub async fn transform_batch(
        &self,
        requests: Vec<TransformationRequest>,
    ) -> ArbitrageResult<Vec<TransformationResponse>> {
        let mut responses = Vec::with_capacity(requests.len());

        for request in requests {
            let response = self.transform(request).await?;
            responses.push(response);
        }

        Ok(responses)
    }

    /// Add a new transformation rule
    pub async fn add_transformation_rule(&self, rule: TransformationRule) -> ArbitrageResult<()> {
        if let Ok(mut rules) = self.transformation_rules.lock() {
            rules.insert(rule.rule_id.clone(), rule);
            Ok(())
        } else {
            Err(ArbitrageError::internal_error(
                "Failed to acquire transformation rules lock",
            ))
        }
    }

    /// Get transformation metrics
    pub async fn get_metrics(&self) -> TransformationMetrics {
        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            TransformationMetrics::default()
        }
    }

    /// Perform health check
    pub async fn health_check(&self) -> ArbitrageResult<bool> {
        // Test basic transformation functionality
        let test_request = TransformationRequest::new(
            serde_json::json!({"test": "data"}),
            DataFormat::Json,
            DataFormat::Json,
        );

        match self.transform(test_request).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // Don't fail health check for transformation errors
        }
    }

    /// Execute transformation with rule
    async fn execute_transformation(
        &self,
        request: &TransformationRequest,
        rule: &TransformationRule,
        start_time: u64,
    ) -> ArbitrageResult<TransformationResponse> {
        let mut data = request.source_data.clone();
        let original_size = serde_json::to_string(&data)?.len();

        // Apply field mappings
        if !rule.field_mappings.is_empty() {
            data = self.apply_field_mappings(data, &rule.field_mappings)?;
        }

        // Apply default values
        if !rule.default_values.is_empty() {
            data = self.apply_default_values(data, &rule.default_values)?;
        }

        // Validate data if enabled
        if self.config.enable_schema_validation && rule.schema_validation {
            self.validate_data(&data, &rule.validation_rules)?;
        }

        // Enrich data if enabled
        if self.config.enable_data_enrichment && rule.data_enrichment {
            data = self.enrich_data(data, request)?;
        }

        // Apply compression if enabled
        let (compressed_data, compressed_size) = if self.config.enable_compression
            && rule.compression != CompressionType::None
            && original_size >= self.config.compression_threshold_bytes
        {
            let compressed = self.compress_data(&data, &rule.compression)?;
            let compressed_size = serde_json::to_string(&compressed)?.len();
            (compressed, compressed_size)
        } else {
            let size = serde_json::to_string(&data)?.len();
            (data.clone(), size)
        };

        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        Ok(TransformationResponse {
            request_id: request.request_id.clone(),
            success: true,
            transformed_data: Some(compressed_data),
            target_format: rule.target_format.clone(),
            compression_type: rule.compression.clone(),
            original_size_bytes: original_size,
            compressed_size_bytes: compressed_size,
            transformation_time_ms: end_time - start_time,
            validation_errors: Vec::new(),
            warnings: Vec::new(),
            metadata: request.metadata.clone(),
            timestamp: end_time,
        })
    }

    /// Apply field mappings to data
    fn apply_field_mappings(
        &self,
        mut data: serde_json::Value,
        mappings: &HashMap<String, String>,
    ) -> ArbitrageResult<serde_json::Value> {
        if let serde_json::Value::Object(ref mut obj) = data {
            let mut new_obj = serde_json::Map::new();

            for (key, value) in obj.iter() {
                let new_key = mappings.get(key).unwrap_or(key);
                new_obj.insert(new_key.clone(), value.clone());
            }

            Ok(serde_json::Value::Object(new_obj))
        } else {
            Ok(data)
        }
    }

    /// Apply default values to data
    fn apply_default_values(
        &self,
        mut data: serde_json::Value,
        defaults: &HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<serde_json::Value> {
        if let serde_json::Value::Object(ref mut obj) = data {
            for (key, default_value) in defaults {
                if !obj.contains_key(key) {
                    obj.insert(key.clone(), default_value.clone());
                }
            }
        }
        Ok(data)
    }

    /// Validate data according to rules
    fn validate_data(
        &self,
        data: &serde_json::Value,
        rules: &[ValidationRule],
    ) -> ArbitrageResult<()> {
        for rule in rules {
            self.validate_field(data, rule)?;
        }
        Ok(())
    }

    /// Validate individual field
    fn validate_field(
        &self,
        data: &serde_json::Value,
        rule: &ValidationRule,
    ) -> ArbitrageResult<()> {
        if let serde_json::Value::Object(obj) = data {
            let field_value = obj.get(&rule.field_name);

            if rule.required && field_value.is_none() {
                return Err(ArbitrageError::validation_error(format!(
                    "Required field '{}' is missing",
                    rule.field_name
                )));
            }

            if let Some(value) = field_value {
                match rule.rule_type {
                    ValidationType::String => {
                        if !value.is_string() {
                            return Err(ArbitrageError::validation_error(format!(
                                "Field '{}' must be a string",
                                rule.field_name
                            )));
                        }
                    }
                    ValidationType::Number => {
                        if !value.is_number() {
                            return Err(ArbitrageError::validation_error(format!(
                                "Field '{}' must be a number",
                                rule.field_name
                            )));
                        }

                        if let Some(num) = value.as_f64() {
                            if let Some(min) = rule.min_value {
                                if num < min {
                                    return Err(ArbitrageError::validation_error(format!(
                                        "Field '{}' value {} is below minimum {}",
                                        rule.field_name, num, min
                                    )));
                                }
                            }
                            if let Some(max) = rule.max_value {
                                if num > max {
                                    return Err(ArbitrageError::validation_error(format!(
                                        "Field '{}' value {} is above maximum {}",
                                        rule.field_name, num, max
                                    )));
                                }
                            }
                        }
                    }
                    ValidationType::Boolean => {
                        if !value.is_boolean() {
                            return Err(ArbitrageError::validation_error(format!(
                                "Field '{}' must be a boolean",
                                rule.field_name
                            )));
                        }
                    }
                    _ => {
                        // Other validation types would be implemented here
                    }
                }
            }
        }

        Ok(())
    }

    /// Enrich data with metadata
    fn enrich_data(
        &self,
        mut data: serde_json::Value,
        request: &TransformationRequest,
    ) -> ArbitrageResult<serde_json::Value> {
        if let serde_json::Value::Object(ref mut obj) = data {
            // Add transformation metadata
            obj.insert(
                "_transformation_id".to_string(),
                serde_json::Value::String(request.request_id.clone()),
            );
            obj.insert(
                "_transformation_timestamp".to_string(),
                serde_json::Value::Number(serde_json::Number::from(request.timestamp)),
            );
            obj.insert(
                "_source_format".to_string(),
                serde_json::Value::String(request.source_format.as_str().to_string()),
            );
        }
        Ok(data)
    }

    /// Compress data using specified compression type
    fn compress_data(
        &self,
        data: &serde_json::Value,
        compression_type: &CompressionType,
    ) -> ArbitrageResult<serde_json::Value> {
        // In a real implementation, this would use actual compression libraries
        // For now, we'll simulate compression by adding metadata
        let mut compressed = data.clone();
        if let serde_json::Value::Object(ref mut obj) = compressed {
            obj.insert(
                "_compression".to_string(),
                serde_json::Value::String(compression_type.as_str().to_string()),
            );
            obj.insert("_compressed".to_string(), serde_json::Value::Bool(true));
        }
        Ok(compressed)
    }

    /// Get transformation rule by ID
    async fn get_transformation_rule(&self, rule_id: &str) -> Option<TransformationRule> {
        if let Ok(rules) = self.transformation_rules.lock() {
            rules.get(rule_id).cloned()
        } else {
            None
        }
    }

    /// Find transformation rule by formats
    async fn find_transformation_rule(
        &self,
        source_format: &DataFormat,
        target_format: &DataFormat,
    ) -> Option<TransformationRule> {
        if let Ok(rules) = self.transformation_rules.lock() {
            for rule in rules.values() {
                if rule.enabled
                    && rule.source_format == *source_format
                    && rule.target_format == *target_format
                {
                    return Some(rule.clone());
                }
            }
        }
        None
    }

    /// Get result from cache
    async fn get_from_cache(
        &self,
        request: &TransformationRequest,
    ) -> Option<TransformationResponse> {
        if let Ok(cache) = self.transformation_cache.lock() {
            let cache_key = self.generate_cache_key(request);
            if let Some((cached_data, timestamp)) = cache.get(&cache_key) {
                let current_time = chrono::Utc::now().timestamp_millis() as u64;
                if current_time - timestamp < self.config.cache_ttl_seconds * 1000 {
                    return Some(TransformationResponse {
                        request_id: request.request_id.clone(),
                        success: true,
                        transformed_data: Some(cached_data.clone()),
                        target_format: request.target_format.clone(),
                        compression_type: CompressionType::None,
                        original_size_bytes: 0,
                        compressed_size_bytes: 0,
                        transformation_time_ms: 0,
                        validation_errors: Vec::new(),
                        warnings: vec!["Result from cache".to_string()],
                        metadata: request.metadata.clone(),
                        timestamp: current_time,
                    });
                }
            }
        }
        None
    }

    /// Cache transformation result
    async fn cache_result(
        &self,
        request: &TransformationRequest,
        response: &TransformationResponse,
    ) {
        if let Some(ref data) = response.transformed_data {
            if let Ok(mut cache) = self.transformation_cache.lock() {
                let cache_key = self.generate_cache_key(request);
                let timestamp = chrono::Utc::now().timestamp_millis() as u64;
                cache.insert(cache_key, (data.clone(), timestamp));

                // Clean up old cache entries
                let cutoff_time = timestamp - self.config.cache_ttl_seconds * 1000;
                cache.retain(|_, (_, ts)| *ts > cutoff_time);
            }
        }
    }

    /// Generate cache key for request
    fn generate_cache_key(&self, request: &TransformationRequest) -> String {
        // In a real implementation, this would use a proper hash function
        format!(
            "{}_{}_{}_{}",
            request.source_format.as_str(),
            request.target_format.as_str(),
            request
                .transformation_rule_id
                .as_deref()
                .unwrap_or("default"),
            serde_json::to_string(&request.source_data)
                .unwrap_or_default()
                .len()
        )
    }

    /// Create error response
    async fn create_error_response(
        &self,
        request: &TransformationRequest,
        error: String,
        start_time: u64,
    ) -> TransformationResponse {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        TransformationResponse {
            request_id: request.request_id.clone(),
            success: false,
            transformed_data: None,
            target_format: request.target_format.clone(),
            compression_type: CompressionType::None,
            original_size_bytes: 0,
            compressed_size_bytes: 0,
            transformation_time_ms: end_time - start_time,
            validation_errors: vec![error],
            warnings: Vec::new(),
            metadata: request.metadata.clone(),
            timestamp: end_time,
        }
    }

    /// Create skip response
    async fn create_skip_response(
        &self,
        request: &TransformationRequest,
        start_time: u64,
    ) -> TransformationResponse {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;

        TransformationResponse {
            request_id: request.request_id.clone(),
            success: true,
            transformed_data: Some(request.source_data.clone()),
            target_format: request.source_format.clone(),
            compression_type: CompressionType::None,
            original_size_bytes: serde_json::to_string(&request.source_data)
                .unwrap_or_default()
                .len(),
            compressed_size_bytes: serde_json::to_string(&request.source_data)
                .unwrap_or_default()
                .len(),
            transformation_time_ms: end_time - start_time,
            validation_errors: Vec::new(),
            warnings: vec!["Transformation skipped due to error".to_string()],
            metadata: request.metadata.clone(),
            timestamp: end_time,
        }
    }

    /// Create default response
    async fn create_default_response(
        &self,
        request: &TransformationRequest,
        rule: &TransformationRule,
        start_time: u64,
    ) -> TransformationResponse {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let default_data = serde_json::json!({
            "default": true,
            "original_request_id": request.request_id
        });

        TransformationResponse {
            request_id: request.request_id.clone(),
            success: true,
            transformed_data: Some(default_data.clone()),
            target_format: rule.target_format.clone(),
            compression_type: CompressionType::None,
            original_size_bytes: serde_json::to_string(&request.source_data)
                .unwrap_or_default()
                .len(),
            compressed_size_bytes: serde_json::to_string(&default_data)
                .unwrap_or_default()
                .len(),
            transformation_time_ms: end_time - start_time,
            validation_errors: Vec::new(),
            warnings: vec!["Using default response due to transformation error".to_string()],
            metadata: request.metadata.clone(),
            timestamp: end_time,
        }
    }

    /// Retry transformation with fallback
    async fn retry_with_fallback(
        &self,
        request: &TransformationRequest,
        start_time: u64,
    ) -> ArbitrageResult<TransformationResponse> {
        // Try with a simpler transformation (JSON to JSON with no compression)
        let fallback_rule = TransformationRule::new(
            "fallback".to_string(),
            request.source_format.clone(),
            DataFormat::Json,
        )
        .with_compression(CompressionType::None);

        match self
            .execute_transformation(request, &fallback_rule, start_time)
            .await
        {
            Ok(response) => Ok(response),
            Err(_) => Ok(self.create_skip_response(request, start_time).await),
        }
    }

    /// Record successful transformation
    async fn record_success(
        &self,
        request: &TransformationRequest,
        rule: &TransformationRule,
        start_time: u64,
    ) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let transformation_time = end_time - start_time;

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_transformations += 1;
            metrics.successful_transformations += 1;
            metrics.average_transformation_time_ms = (metrics.average_transformation_time_ms
                * (metrics.total_transformations - 1) as f64
                + transformation_time as f64)
                / metrics.total_transformations as f64;
            metrics.min_transformation_time_ms = metrics
                .min_transformation_time_ms
                .min(transformation_time as f64);
            metrics.max_transformation_time_ms = metrics
                .max_transformation_time_ms
                .max(transformation_time as f64);

            *metrics
                .transformations_by_format
                .entry(request.target_format.clone())
                .or_insert(0) += 1;
            *metrics
                .transformations_by_rule
                .entry(rule.rule_id.clone())
                .or_insert(0) += 1;
            metrics.last_updated = end_time;
        }
    }

    /// Record failed transformation
    async fn record_failure(
        &self,
        _request: &TransformationRequest,
        _rule: &TransformationRule,
        start_time: u64,
        _error: &ArbitrageError,
    ) {
        let end_time = chrono::Utc::now().timestamp_millis() as u64;
        let transformation_time = end_time - start_time;

        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.total_transformations += 1;
            metrics.failed_transformations += 1;
            metrics.average_transformation_time_ms = (metrics.average_transformation_time_ms
                * (metrics.total_transformations - 1) as f64
                + transformation_time as f64)
                / metrics.total_transformations as f64;
            metrics.min_transformation_time_ms = metrics
                .min_transformation_time_ms
                .min(transformation_time as f64);
            metrics.max_transformation_time_ms = metrics
                .max_transformation_time_ms
                .max(transformation_time as f64);
            metrics.last_updated = end_time;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_format_properties() {
        assert_eq!(DataFormat::Json.as_str(), "json");
        assert_eq!(DataFormat::Json.mime_type(), "application/json");
        assert!(DataFormat::Json.supports_compression());
        assert_eq!(
            DataFormat::Json.default_compression(),
            CompressionType::Gzip
        );
        assert!(!DataFormat::Parquet.supports_compression());
    }

    #[test]
    fn test_compression_type_properties() {
        assert_eq!(CompressionType::Gzip.as_str(), "gzip");
        assert_eq!(CompressionType::Gzip.compression_ratio(), 0.3);
        assert_eq!(CompressionType::Lz4.speed_score(), 9);
        assert_eq!(CompressionType::None.compression_ratio(), 1.0);
    }

    #[test]
    fn test_transformation_rule_creation() {
        let rule =
            TransformationRule::new("test_rule".to_string(), DataFormat::Json, DataFormat::Avro)
                .with_compression(CompressionType::Snappy)
                .with_priority(3);

        assert_eq!(rule.name, "test_rule");
        assert_eq!(rule.source_format, DataFormat::Json);
        assert_eq!(rule.target_format, DataFormat::Avro);
        assert_eq!(rule.compression, CompressionType::Snappy);
        assert_eq!(rule.priority, 3);
        assert!(rule.enabled);
    }

    #[test]
    fn test_transformation_request_creation() {
        let data = serde_json::json!({"test": "data"});
        let request = TransformationRequest::new(data.clone(), DataFormat::Json, DataFormat::Avro)
            .with_priority(2);

        assert_eq!(request.source_data, data);
        assert_eq!(request.source_format, DataFormat::Json);
        assert_eq!(request.target_format, DataFormat::Avro);
        assert_eq!(request.priority, 2);
    }

    #[test]
    fn test_data_transformer_config_validation() {
        let mut config = DataTransformerConfig::default();
        assert!(config.validate().is_ok());

        config.max_concurrent_transformations = 0;
        assert!(config.validate().is_err());

        config.max_concurrent_transformations = 10;
        config.compression_threshold_bytes = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_performance_config() {
        let config = DataTransformerConfig::high_performance();
        assert_eq!(config.max_concurrent_transformations, 50);
        assert_eq!(config.transformation_timeout_seconds, 15);
        assert_eq!(config.compression_threshold_bytes, 512);
    }

    #[test]
    fn test_high_reliability_config() {
        let config = DataTransformerConfig::high_reliability();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_seconds, 2);
        assert!(config.enable_error_recovery);
    }
}
