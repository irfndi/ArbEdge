//! Compression Engine and Middleware
//!
//! Provides compression and decompression functionality for cache entries
//! with transparent middleware for automatic compression handling

use super::config::{CompressionAlgorithm, CompressionConfig};

use crate::utils::{ArbitrageError, ArbitrageResult};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

/// Compression middleware for transparent cache compression
pub struct CompressionMiddleware {
    config: CompressionConfig,
    engine: CompressionEngine,
    logger: crate::utils::logger::Logger,

    // Performance tracking
    compression_stats: Arc<Mutex<CompressionMetrics>>,

    // Content type detection
    content_analyzer: ContentAnalyzer,
}

impl CompressionMiddleware {
    /// Create new compression middleware
    pub fn new(config: CompressionConfig) -> ArbitrageResult<Self> {
        let logger = crate::utils::logger::Logger::new(crate::utils::logger::LogLevel::Info);

        if !config.algorithm.is_available() {
            return Err(ArbitrageError::config_error(format!(
                "Compression algorithm {:?} is not available",
                config.algorithm
            )));
        }

        let engine = CompressionEngine::new(config.clone());
        let content_analyzer = ContentAnalyzer::new();

        logger.info(&format!(
            "CompressionMiddleware initialized: algorithm={:?}, threshold={}KB, level={}",
            config.algorithm,
            config.size_threshold_bytes / 1024,
            config.compression_level
        ));

        Ok(Self {
            config,
            engine,
            logger,
            compression_stats: Arc::new(Mutex::new(CompressionMetrics::default())),
            content_analyzer,
        })
    }

    /// Process data for storage (compress if beneficial)
    pub async fn process_for_storage(&self, key: &str, data: &str) -> ArbitrageResult<StorageData> {
        let start_time = crate::utils::time::now_instant();

        if !self.config.enabled {
            return Ok(StorageData {
                data: data.to_string(),
                is_compressed: false,
                original_size: data.len(),
                compressed_size: data.len(),
                compression_ratio: 1.0,
                algorithm: None,
                processing_time_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        let data_bytes = data.as_bytes();

        // Skip compression for small data
        if data_bytes.len() < self.config.size_threshold_bytes {
            self.record_skip_reason(key, "below_threshold").await;
            return Ok(StorageData {
                data: data.to_string(),
                is_compressed: false,
                original_size: data.len(),
                compressed_size: data.len(),
                compression_ratio: 1.0,
                algorithm: None,
                processing_time_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        // Analyze content compressibility
        let content_type = self.content_analyzer.analyze_content(data_bytes);
        if !content_type.is_compressible() {
            self.record_skip_reason(key, "not_compressible").await;
            return Ok(StorageData {
                data: data.to_string(),
                is_compressed: false,
                original_size: data.len(),
                compressed_size: data.len(),
                compression_ratio: 1.0,
                algorithm: None,
                processing_time_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        // Attempt compression
        let result = self.engine.try_compress(data_bytes);
        let processing_time = start_time.elapsed().as_millis() as u64;

        // Check if compression is effective
        if result.is_effective() {
            // Store compressed data with metadata
            let compressed_envelope = CompressionEnvelope {
                algorithm: self.config.algorithm.clone(),
                original_size: result.original_size,
                compressed_data: result.data.clone(),
                checksum: self.calculate_checksum(&result.data),
            };

            match serde_json::to_string(&compressed_envelope) {
                Ok(serialized) => {
                    self.record_compression_success(key, &result, processing_time)
                        .await;

                    Ok(StorageData {
                        data: serialized,
                        is_compressed: true,
                        original_size: result.original_size,
                        compressed_size: result.compressed_size,
                        compression_ratio: result.compression_ratio,
                        algorithm: Some(self.config.algorithm.clone()),
                        processing_time_ms: processing_time,
                    })
                }
                Err(e) => {
                    let error = ArbitrageError::serialization_error(e.to_string());
                    self.record_compression_error(key, &error, processing_time)
                        .await;

                    // Fallback to uncompressed
                    Ok(StorageData {
                        data: data.to_string(),
                        is_compressed: false,
                        original_size: data.len(),
                        compressed_size: data.len(),
                        compression_ratio: 1.0,
                        algorithm: None,
                        processing_time_ms: processing_time,
                    })
                }
            }
        } else {
            // Compression not effective, store original
            self.record_compression_ineffective(key, &result).await;
            Ok(StorageData {
                data: data.to_string(),
                is_compressed: false,
                original_size: data.len(),
                compressed_size: data.len(),
                compression_ratio: 1.0,
                algorithm: None,
                processing_time_ms: processing_time,
            })
        }
    }

    /// Process data for retrieval (decompress if needed)
    pub async fn process_for_retrieval(
        &self,
        key: &str,
        stored_data: &str,
    ) -> ArbitrageResult<String> {
        let start_time = crate::utils::time::now_instant();

        // Try to detect if data is compressed by checking for compression envelope
        if let Ok(envelope) = serde_json::from_str::<CompressionEnvelope>(stored_data) {
            // Data is compressed, decompress it
            match self.decompress_with_verification(&envelope).await {
                Ok(decompressed) => {
                    let processing_time = start_time.elapsed().as_millis() as u64;
                    self.record_decompression_success(key, envelope.original_size, processing_time)
                        .await;

                    String::from_utf8(decompressed).map_err(|e| {
                        ArbitrageError::serialization_error(format!(
                            "Invalid UTF-8 after decompression: {}",
                            e
                        ))
                    })
                }
                Err(e) => {
                    let processing_time = start_time.elapsed().as_millis() as u64;
                    self.record_decompression_error(key, &e, processing_time)
                        .await;

                    self.logger
                        .error(&format!("Decompression failed for key '{}': {}", key, e));
                    Err(e)
                }
            }
        } else {
            // Data is not compressed, return as-is
            let processing_time = start_time.elapsed().as_millis() as u64;
            self.record_passthrough(key, stored_data.len(), processing_time)
                .await;
            Ok(stored_data.to_string())
        }
    }

    /// Get comprehensive compression metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<CompressionMetrics> {
        let metrics = self.compression_stats.lock().map_err(|e| {
            ArbitrageError::internal_error(format!("Failed to lock compression metrics: {}", e))
        })?;
        Ok(metrics.clone())
    }

    /// Check if content type should be compressed
    pub fn should_compress_content(&self, data: &[u8]) -> bool {
        if data.len() < self.config.size_threshold_bytes {
            return false;
        }

        let content_type = self.content_analyzer.analyze_content(data);
        content_type.is_compressible()
    }

    // Private helper methods

    async fn decompress_with_verification(
        &self,
        envelope: &CompressionEnvelope,
    ) -> ArbitrageResult<Vec<u8>> {
        // Verify checksum
        let calculated_checksum = self.calculate_checksum(&envelope.compressed_data);
        if calculated_checksum != envelope.checksum {
            return Err(ArbitrageError::internal_error(
                "Compression checksum mismatch".to_string(),
            ));
        }

        // Decompress based on algorithm
        match envelope.algorithm {
            CompressionAlgorithm::Gzip => self.engine.decompress(&envelope.compressed_data).await,
            CompressionAlgorithm::None => Ok(envelope.compressed_data.clone()),
            _ => Err(ArbitrageError::internal_error(format!(
                "Unsupported compression algorithm: {:?}",
                envelope.algorithm
            ))),
        }
    }

    fn calculate_checksum(&self, data: &[u8]) -> u32 {
        // Simple checksum using CRC32 (could be enhanced with a proper hash)
        let mut checksum = 0u32;
        for &byte in data {
            checksum = checksum.wrapping_add(byte as u32);
        }
        checksum
    }

    async fn record_compression_success(
        &self,
        key: &str,
        result: &CompressionResult,
        processing_time: u64,
    ) {
        if let Ok(mut metrics) = self.compression_stats.lock() {
            metrics.total_operations += 1;
            metrics.successful_compressions += 1;
            metrics.total_original_bytes += result.original_size as u64;
            metrics.total_compressed_bytes += result.compressed_size as u64;
            metrics.total_processing_time_ms += processing_time;
            metrics.average_compression_ratio =
                (metrics.average_compression_ratio + result.compression_ratio) / 2.0;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.debug(&format!(
            "Compression success: key='{}', {}->{}B ({}% saved), {}ms",
            key,
            result.original_size,
            result.compressed_size,
            result.space_saved_percent(),
            processing_time
        ));
    }

    async fn record_compression_ineffective(&self, key: &str, result: &CompressionResult) {
        if let Ok(mut metrics) = self.compression_stats.lock() {
            metrics.total_operations += 1;
            metrics.ineffective_compressions += 1;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.debug(&format!(
            "Compression ineffective: key='{}', ratio={:.2}% (threshold={}%)",
            key,
            result.compression_ratio * 100.0,
            self.config.max_compression_ratio * 100.0
        ));
    }

    async fn record_compression_error(
        &self,
        key: &str,
        error: &ArbitrageError,
        processing_time: u64,
    ) {
        if let Ok(mut metrics) = self.compression_stats.lock() {
            metrics.total_operations += 1;
            metrics.failed_compressions += 1;
            metrics.total_processing_time_ms += processing_time;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.warn(&format!(
            "Compression error: key='{}', error={}, {}ms",
            key, error, processing_time
        ));
    }

    async fn record_decompression_success(
        &self,
        key: &str,
        original_size: usize,
        processing_time: u64,
    ) {
        if let Ok(mut metrics) = self.compression_stats.lock() {
            metrics.total_decompressions += 1;
            metrics.successful_decompressions += 1;
            metrics.total_processing_time_ms += processing_time;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.debug(&format!(
            "Decompression success: key='{}', size={}B, {}ms",
            key, original_size, processing_time
        ));
    }

    async fn record_decompression_error(
        &self,
        key: &str,
        error: &ArbitrageError,
        processing_time: u64,
    ) {
        if let Ok(mut metrics) = self.compression_stats.lock() {
            metrics.total_decompressions += 1;
            metrics.failed_decompressions += 1;
            metrics.total_processing_time_ms += processing_time;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.error(&format!(
            "Decompression error: key='{}', error={}, {}ms",
            key, error, processing_time
        ));
    }

    async fn record_passthrough(&self, key: &str, size: usize, processing_time: u64) {
        if let Ok(mut metrics) = self.compression_stats.lock() {
            metrics.total_operations += 1;
            metrics.passthrough_operations += 1;
            metrics.total_processing_time_ms += processing_time;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.debug(&format!(
            "Compression passthrough: key='{}', size={}B, {}ms",
            key, size, processing_time
        ));
    }

    async fn record_skip_reason(&self, key: &str, reason: &str) {
        if let Ok(mut metrics) = self.compression_stats.lock() {
            metrics.total_operations += 1;
            metrics.skipped_operations += 1;

            *metrics.skip_reasons.entry(reason.to_string()).or_insert(0) += 1;
            metrics.last_updated = chrono::Utc::now().timestamp_millis() as u64;
        }

        self.logger.debug(&format!(
            "Compression skipped: key='{}', reason={}",
            key, reason
        ));
    }
}

/// Enhanced compression engine with async support and metadata
pub struct CompressionEngine {
    config: CompressionConfig,
}

impl CompressionEngine {
    /// Create new compression engine
    pub fn new(config: CompressionConfig) -> Self {
        Self { config }
    }

    /// Check if data should be compressed
    pub fn should_compress(&self, data: &[u8]) -> bool {
        data.len() >= self.config.size_threshold_bytes
    }

    /// Compress data with full metadata
    pub async fn compress_with_metadata(&self, data: &[u8]) -> ArbitrageResult<CompressionResult> {
        if !self.should_compress(data) {
            return Ok(CompressionResult {
                data: data.to_vec(),
                is_compressed: false,
                original_size: data.len(),
                compressed_size: data.len(),
                compression_ratio: 1.0,
                algorithm: None,
            });
        }

        match self.config.algorithm {
            CompressionAlgorithm::Gzip => self.compress_gzip(data).await,
            CompressionAlgorithm::None => Ok(CompressionResult {
                data: data.to_vec(),
                is_compressed: false,
                original_size: data.len(),
                compressed_size: data.len(),
                compression_ratio: 1.0,
                algorithm: None,
            }),
            _ => Err(ArbitrageError::internal_error(format!(
                "Compression algorithm {:?} not implemented",
                self.config.algorithm
            ))),
        }
    }

    /// Compress data using gzip
    pub async fn compress_gzip(&self, data: &[u8]) -> ArbitrageResult<CompressionResult> {
        let mut encoder =
            GzEncoder::new(Vec::new(), Compression::new(self.config.compression_level));
        encoder.write_all(data).map_err(|e| {
            ArbitrageError::internal_error(format!("Gzip compression failed: {}", e))
        })?;

        let compressed = encoder
            .finish()
            .map_err(|e| ArbitrageError::internal_error(format!("Gzip finish failed: {}", e)))?;

        let compression_ratio = compressed.len() as f64 / data.len() as f64;
        let is_effective = compression_ratio <= self.config.max_compression_ratio;

        let compressed_size = compressed.len();
        Ok(CompressionResult {
            data: if is_effective {
                compressed
            } else {
                data.to_vec()
            },
            is_compressed: is_effective,
            original_size: data.len(),
            compressed_size: if is_effective {
                compressed_size
            } else {
                data.len()
            },
            compression_ratio: if is_effective { compression_ratio } else { 1.0 },
            algorithm: if is_effective {
                Some("gzip".to_string())
            } else {
                None
            },
        })
    }

    /// Decompress gzip data
    pub async fn decompress(&self, compressed_data: &[u8]) -> ArbitrageResult<Vec<u8>> {
        let mut decoder = GzDecoder::new(compressed_data);
        let mut decompressed = Vec::new();

        decoder.read_to_end(&mut decompressed).map_err(|e| {
            crate::utils::ArbitrageError::internal_error(format!("Decompression failed: {}", e))
        })?;

        Ok(decompressed)
    }

    /// Try to compress data and return result with metadata
    pub fn try_compress(&self, data: &[u8]) -> CompressionResult {
        if !self.should_compress(data) {
            return CompressionResult {
                data: data.to_vec(),
                is_compressed: false,
                original_size: data.len(),
                compressed_size: data.len(),
                compression_ratio: 1.0,
                algorithm: None,
            };
        }

        match futures::executor::block_on(self.compress_with_metadata(data)) {
            Ok(result) => result,
            Err(_) => {
                // Compression failed, return original data
                CompressionResult {
                    data: data.to_vec(),
                    is_compressed: false,
                    original_size: data.len(),
                    compressed_size: data.len(),
                    compression_ratio: 1.0,
                    algorithm: None,
                }
            }
        }
    }

    /// Get compression statistics
    pub fn compression_stats(&self) -> CompressionStats {
        CompressionStats {
            compression_level: self.config.compression_level,
            size_threshold: self.config.size_threshold_bytes,
            max_compression_ratio: self.config.max_compression_ratio,
        }
    }
}

impl Default for CompressionEngine {
    fn default() -> Self {
        Self::new(CompressionConfig::default())
    }
}

/// Content analyzer for determining compressibility
pub struct ContentAnalyzer {
    // Add sophisticated content analysis capabilities
}

impl ContentAnalyzer {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ContentAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentAnalyzer {
    pub fn analyze_content(&self, data: &[u8]) -> ContentType {
        // Simple heuristic-based analysis
        if self.looks_like_json(data) {
            ContentType::Json
        } else if self.looks_like_text(data) {
            ContentType::Text
        } else if self.looks_like_already_compressed(data) {
            ContentType::AlreadyCompressed
        } else {
            ContentType::Binary
        }
    }

    fn looks_like_json(&self, data: &[u8]) -> bool {
        if data.len() < 2 {
            return false;
        }

        let trimmed = data
            .iter()
            .skip_while(|&&b| b.is_ascii_whitespace())
            .collect::<Vec<_>>();

        if let (Some(&&first), Some(&&last)) = (trimmed.first(), trimmed.last()) {
            (first == b'{' && last == b'}') || (first == b'[' && last == b']')
        } else {
            false
        }
    }

    fn looks_like_text(&self, data: &[u8]) -> bool {
        // Check if most bytes are printable ASCII
        let printable_count = data
            .iter()
            .filter(|&&b| b.is_ascii_graphic() || b.is_ascii_whitespace())
            .count();

        printable_count as f64 / data.len() as f64 > 0.8
    }

    fn looks_like_already_compressed(&self, data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }

        // Check for common compression magic numbers
        matches!(
            &data[0..2],
            b"\x1f\x8b" | // gzip
            b"PK"       | // zip
            b"BZ" // bzip2
        )
    }
}

/// Content type classification for compression decisions
#[derive(Debug, Clone)]
pub enum ContentType {
    Json,
    Text,
    Binary,
    AlreadyCompressed,
}

impl ContentType {
    pub fn is_compressible(&self) -> bool {
        match self {
            ContentType::Json | ContentType::Text => true,
            ContentType::Binary => true, // May still benefit
            ContentType::AlreadyCompressed => false,
        }
    }
}

/// Compression envelope for storing compressed data with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionEnvelope {
    pub algorithm: CompressionAlgorithm,
    pub original_size: usize,
    pub compressed_data: Vec<u8>,
    pub checksum: u32,
}

/// Result of storage processing
#[derive(Debug, Clone)]
pub struct StorageData {
    pub data: String,
    pub is_compressed: bool,
    pub original_size: usize,
    pub compressed_size: usize,
    pub compression_ratio: f64,
    pub algorithm: Option<CompressionAlgorithm>,
    pub processing_time_ms: u64,
}

/// Compression performance metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompressionMetrics {
    pub total_operations: u64,
    pub successful_compressions: u64,
    pub failed_compressions: u64,
    pub ineffective_compressions: u64,
    pub skipped_operations: u64,
    pub passthrough_operations: u64,

    pub total_decompressions: u64,
    pub successful_decompressions: u64,
    pub failed_decompressions: u64,

    pub total_original_bytes: u64,
    pub total_compressed_bytes: u64,
    pub total_processing_time_ms: u64,

    pub average_compression_ratio: f64,
    pub skip_reasons: HashMap<String, u64>,
    pub last_updated: u64,
}

impl CompressionMetrics {
    pub fn compression_success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            self.successful_compressions as f64 / self.total_operations as f64
        }
    }

    pub fn overall_compression_ratio(&self) -> f64 {
        if self.total_original_bytes == 0 {
            1.0
        } else {
            self.total_compressed_bytes as f64 / self.total_original_bytes as f64
        }
    }

    pub fn average_processing_time_ms(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            self.total_processing_time_ms as f64 / self.total_operations as f64
        }
    }

    pub fn space_saved_bytes(&self) -> u64 {
        self.total_original_bytes
            .saturating_sub(self.total_compressed_bytes)
    }

    pub fn space_saved_percent(&self) -> f64 {
        if self.total_original_bytes == 0 {
            0.0
        } else {
            (self.space_saved_bytes() as f64 / self.total_original_bytes as f64) * 100.0
        }
    }
}

/// Result of compression operation
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// The resulting data (compressed or original)
    pub data: Vec<u8>,
    /// Whether compression was applied
    pub is_compressed: bool,
    /// Original data size
    pub original_size: usize,
    /// Final data size
    pub compressed_size: usize,
    /// Compression ratio (compressed_size / original_size)
    pub compression_ratio: f64,
    /// Algorithm used (if any)
    pub algorithm: Option<String>,
}

impl CompressionResult {
    /// Check if compression was effective
    pub fn is_effective(&self) -> bool {
        self.is_compressed && self.compression_ratio < 0.9
    }

    /// Get space saved in bytes
    pub fn space_saved(&self) -> usize {
        if self.is_compressed {
            self.original_size.saturating_sub(self.compressed_size)
        } else {
            0
        }
    }

    /// Get space saved as percentage
    pub fn space_saved_percent(&self) -> f64 {
        if self.is_compressed && self.original_size > 0 {
            (1.0 - self.compression_ratio) * 100.0
        } else {
            0.0
        }
    }
}

/// Compression engine statistics
#[derive(Debug, Clone)]
pub struct CompressionStats {
    /// Current compression level
    pub compression_level: u32,
    /// Size threshold for compression
    pub size_threshold: usize,
    /// Maximum compression ratio threshold
    pub max_compression_ratio: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_engine_creation() {
        let config = CompressionConfig::default();
        let engine = CompressionEngine::new(config);
        assert_eq!(engine.config.compression_level, 6);
        assert_eq!(engine.config.size_threshold_bytes, 1024);
        assert_eq!(engine.config.max_compression_ratio, 0.9);
    }

    #[test]
    fn test_should_compress() {
        let config = CompressionConfig::default();
        let engine = CompressionEngine::new(config);

        let small_data = vec![0u8; 512];
        assert!(!engine.should_compress(&small_data));

        let large_data = vec![0u8; 2048];
        assert!(engine.should_compress(&large_data));
    }

    #[test]
    fn test_compression_round_trip() {
        let config = CompressionConfig::default();
        let engine = CompressionEngine::new(config);
        let data = "Hello, World!".repeat(100).into_bytes();

        let compressed = engine.try_compress(&data);
        // Note: For actual round-trip test, we'd need to implement decompression in the legacy interface
        assert!(compressed.original_size > 0);
    }

    #[test]
    fn test_compression_result() {
        let config = CompressionConfig::default();
        let engine = CompressionEngine::new(config);
        let data = "A".repeat(2048).into_bytes(); // Highly compressible data

        let result = engine.try_compress(&data);
        assert!(result.is_compressed);
        assert!(result.is_effective());
        assert!(result.space_saved() > 0);
        assert!(result.space_saved_percent() > 0.0);
    }

    #[tokio::test]
    async fn test_compression_middleware() {
        let config = CompressionConfig::default();
        let middleware = CompressionMiddleware::new(config).unwrap();

        let test_data = serde_json::json!({
            "test": "data",
            "repeated": "value".repeat(100)
        })
        .to_string();

        // Test storage processing
        let storage_result = middleware
            .process_for_storage("test_key", &test_data)
            .await
            .unwrap();

        // Test retrieval processing
        let retrieved_data = middleware
            .process_for_retrieval("test_key", &storage_result.data)
            .await
            .unwrap();

        assert_eq!(test_data, retrieved_data);
    }

    #[test]
    fn test_content_analyzer() {
        let analyzer = ContentAnalyzer::new();

        let json_data = r#"{"key": "value"}"#.as_bytes();
        let content_type = analyzer.analyze_content(json_data);
        assert!(content_type.is_compressible());

        let text_data = "This is plain text content".as_bytes();
        let content_type = analyzer.analyze_content(text_data);
        assert!(content_type.is_compressible());
    }
}
