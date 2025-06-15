// src/utils/error.rs

use crate::services::core::trading::kv_operations::KvOperationError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

pub type ArbitrageResult<T> = Result<T, ArbitrageError>;

/// Custom error details for additional context
pub type ErrorDetails = HashMap<String, serde_json::Value>;

/// Main error type for the arbitrage application
/// Optimized for size by boxing large fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageError {
    pub message: String,
    pub details: Option<Box<ErrorDetails>>, // Boxed to reduce enum size
    pub status: Option<u16>,
    pub error_code: Option<String>,
    pub method: Option<String>,
    pub kind: ErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorKind {
    NetworkError,
    ApiError,
    ValidationError,
    NotFound,
    Authentication,
    Authorization,
    RateLimit,
    ExchangeError,
    ParseError,
    ConfigError,
    DatabaseError,
    TelegramError,
    NotImplemented,
    Serialization,
    Internal,
    Storage,
    AccessDenied,
    InfrastructureError,
}

impl fmt::Display for ArbitrageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ArbitrageError {}

impl ArbitrageError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            details: None,
            status: None,
            error_code: None,
            method: None,
            kind,
        }
    }

    pub fn with_details(mut self, details: ErrorDetails) -> Self {
        self.details = Some(Box::new(details));
        self
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_code(mut self, error_code: impl Into<String>) -> Self {
        self.error_code = Some(error_code.into());
        self
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    // Convenience constructors for common error types
    pub fn network_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NetworkError, message)
            .with_status(503)
            .with_code("NETWORK_ERROR")
    }

    pub fn api_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ApiError, message)
            .with_status(500)
            .with_code("API_ERROR")
    }

    pub fn validation_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ValidationError, message)
            .with_status(400)
            .with_code("VALIDATION_ERROR")
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
            .with_status(404)
            .with_code("NOT_FOUND")
    }

    pub fn authentication_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Authentication, message)
            .with_status(401)
            .with_code("AUTH_ERROR")
    }

    pub fn authorization_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Authorization, message)
            .with_status(403)
            .with_code("AUTH_Z_ERROR")
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Authorization, message)
            .with_status(401)
            .with_code("UNAUTHORIZED")
    }

    pub fn rate_limit_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::RateLimit, message)
            .with_status(429)
            .with_code("RATE_LIMIT")
    }

    pub fn exchange_error(exchange: &str, message: impl Into<String>) -> Self {
        let mut details = ErrorDetails::new();
        details.insert(
            "exchange".to_string(),
            serde_json::Value::String(exchange.to_string()),
        );

        Self::new(ErrorKind::ExchangeError, message)
            .with_details(details)
            .with_status(502)
            .with_code("EXCHANGE_ERROR")
    }

    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ParseError, message)
            .with_status(400)
            .with_code("PARSE_ERROR")
    }

    pub fn config_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ConfigError, message)
            .with_status(500)
            .with_code("CONFIG_ERROR")
    }

    pub fn database_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::DatabaseError, message)
            .with_status(500)
            .with_code("DATABASE_ERROR")
    }

    pub fn telegram_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::TelegramError, message)
            .with_status(502)
            .with_code("TELEGRAM_ERROR")
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
            .with_status(500)
            .with_code("INTERNAL_ERROR")
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotImplemented, message)
            .with_status(501)
            .with_code("NOT_IMPLEMENTED")
    }

    pub fn serialization_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Serialization, message)
            .with_status(400)
            .with_code("SERIALIZATION_ERROR")
    }

    pub fn storage_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Storage, message)
    }

    pub fn kv_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Storage, message)
            .with_status(500)
            .with_code("KV_ERROR")
    }

    pub fn infrastructure_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InfrastructureError, message)
            .with_status(500)
            .with_code("INFRASTRUCTURE_ERROR")
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
            .with_status(503)
            .with_code("SERVICE_UNAVAILABLE")
    }

    pub fn parsing_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ParseError, message)
            .with_status(400)
            .with_code("PARSING_ERROR")
    }

    pub fn configuration_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ConfigError, message)
            .with_status(500)
            .with_code("CONFIGURATION_ERROR")
    }

    pub fn data_unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
            .with_status(503)
            .with_code("DATA_UNAVAILABLE")
    }

    pub fn session_not_found(identifier: impl Into<String>) -> Self {
        Self::new(
            ErrorKind::NotFound,
            format!("Session not found: {}", identifier.into()),
        )
        .with_status(404)
        .with_code("SESSION_NOT_FOUND")
    }

    pub fn rate_limit_exceeded(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::RateLimit, message)
            .with_status(429)
            .with_code("RATE_LIMIT_EXCEEDED")
    }

    pub fn quota_exceeded(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::RateLimit, message)
            .with_status(429)
            .with_code("QUOTA_EXCEEDED")
    }

    pub fn cache_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Storage, message)
            .with_status(500)
            .with_code("CACHE_ERROR")
    }

    pub fn processing_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
            .with_status(500)
            .with_code("PROCESSING_ERROR")
    }

    pub fn access_denied(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::AccessDenied, message)
            .with_status(403)
            .with_code("ACCESS_DENIED")
    }

    pub fn timeout_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NetworkError, message)
            .with_status(408)
            .with_code("TIMEOUT_ERROR")
    }

    pub fn data_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::DatabaseError, message)
            .with_status(500)
            .with_code("DATA_ERROR")
    }

    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Authorization, message)
            .with_status(403)
            .with_code("PERMISSION_DENIED")
    }

    pub fn resource_exhausted(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::RateLimit, message)
            .with_status(429)
            .with_code("RESOURCE_EXHAUSTED")
    }

    pub fn operation_not_supported(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotImplemented, message)
            .with_status(501)
            .with_code("OPERATION_NOT_SUPPORTED")
    }
}

// Implement From conversions for common error types
impl From<serde_json::Error> for ArbitrageError {
    fn from(err: serde_json::Error) -> Self {
        ArbitrageError::parse_error(format!("JSON parsing error: {}", err))
    }
}

impl From<worker::Error> for ArbitrageError {
    fn from(err: worker::Error) -> Self {
        ArbitrageError::internal_error(format!("Worker error: {:?}", err))
    }
}

impl From<worker::kv::KvError> for ArbitrageError {
    fn from(err: worker::kv::KvError) -> Self {
        Self::storage_error(format!("KV error: {:?}", err))
    }
}

impl From<&str> for ArbitrageError {
    fn from(err: &str) -> Self {
        Self::validation_error(err.to_string())
    }
}

impl From<url::ParseError> for ArbitrageError {
    fn from(err: url::ParseError) -> Self {
        ArbitrageError::validation_error(format!("URL parse error: {}", err))
    }
}

// Implement From<KvOperationError> for ArbitrageError
impl From<KvOperationError> for ArbitrageError {
    fn from(err: KvOperationError) -> Self {
        match err {
            KvOperationError::NotFound(key_or_msg) => {
                ArbitrageError::not_found(format!("KV item not found: {}", key_or_msg))
            }
            KvOperationError::Serialization(e) => ArbitrageError::serialization_error(format!(
                "KV serialization/deserialization error: {}",
                e
            )),
            KvOperationError::Storage(msg) => {
                ArbitrageError::storage_error(format!("KV storage error: {}", msg))
            }
            KvOperationError::SdkError(msg) => {
                ArbitrageError::internal_error(format!("KV SDK error: {}", msg))
            }
        }
    }
}

// Implementation to convert ArbitrageError into worker::Error
impl From<ArbitrageError> for worker::Error {
    fn from(err: ArbitrageError) -> Self {
        // Log the original error for more detailed debugging if necessary
        // For example, using a logger if available: log::error!("Converting ArbitrageError to worker::Error: {:?}", err);

        // Convert our detailed ArbitrageError into a simpler worker::Error.
        // We'll try to preserve the status code if available, otherwise use a generic one.
        // The message will be the primary information carried over.
        let message = if let Some(status_code) = err.status {
            format!(
                "[Status: {}] ArbitrageError (Kind: {:?}): {}",
                status_code, err.kind, err.message
            )
        } else {
            format!("ArbitrageError (Kind: {:?}): {}", err.kind, err.message)
        };

        worker::Error::RustError(message)
    }
}

// Helper macro for creating errors with context
#[macro_export]
macro_rules! arbitrage_error {
    ($kind:expr, $msg:expr) => {
        ArbitrageError::new($kind, $msg)
    };
    ($kind:expr, $msg:expr, $($key:expr => $value:expr),+) => {{
        let mut details = std::collections::HashMap::new();
        $(
            details.insert($key.to_string(), serde_json::json!($value));
        )+
        ArbitrageError::new($kind, $msg).with_details(details)
    }};
}

#[cfg(test)]
mod tests {
    // Import necessary items from the outer module
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn test_arbitrage_error_creation() {
        let error = ArbitrageError::new(ErrorKind::NetworkError, "Network issue");
        assert_eq!(error.kind, ErrorKind::NetworkError);
        assert_eq!(error.message, "Network issue");
        assert!(error.details.is_none());
        assert!(error.status.is_none()); // Status is not set by default by `new`
    }

    #[test]
    fn test_error_with_details() {
        let mut details = HashMap::new();
        details.insert("info".to_string(), json!("extra data"));
        let error = ArbitrageError::new(ErrorKind::ValidationError, "Validation failed")
            .with_details(details.clone());
        assert_eq!(error.kind, ErrorKind::ValidationError);
        assert_eq!(*error.details.unwrap(), details);
    }

    #[test]
    fn test_error_with_status() {
        let error = ArbitrageError::new(ErrorKind::NotFound, "Item not found").with_status(404);
        assert_eq!(error.kind, ErrorKind::NotFound);
        assert_eq!(error.status, Some(404));
    }

    #[test]
    fn test_error_with_code() {
        let error = ArbitrageError::new(ErrorKind::ApiError, "API problem").with_code("API_001");
        assert_eq!(error.kind, ErrorKind::ApiError);
        assert_eq!(error.error_code, Some("API_001".to_string()));
    }

    #[test]
    fn test_convenience_constructors() {
        let net_err = ArbitrageError::network_error("Timeout");
        assert_eq!(net_err.kind, ErrorKind::NetworkError);
        assert_eq!(net_err.status, Some(503));
        assert_eq!(net_err.error_code, Some("NETWORK_ERROR".to_string()));

        let val_err = ArbitrageError::validation_error("Bad input");
        assert_eq!(val_err.kind, ErrorKind::ValidationError);
        assert_eq!(val_err.status, Some(400));

        let nf_err = ArbitrageError::not_found("Resource missing");
        assert_eq!(nf_err.kind, ErrorKind::NotFound);
        assert_eq!(nf_err.status, Some(404));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err_str = "{\"key\": invalid_json}"; // Malformed JSON
        let serde_error = serde_json::from_str::<Value>(json_err_str).unwrap_err();
        let arbitrage_error = ArbitrageError::from(serde_error);
        assert_eq!(arbitrage_error.kind, ErrorKind::ParseError);
        assert!(arbitrage_error.message.contains("JSON parsing error"));
    }

    #[test]
    fn test_from_worker_error() {
        let worker_err = worker::Error::RustError("Worker failed".to_string());
        let arbitrage_error = ArbitrageError::from(worker_err);
        assert_eq!(arbitrage_error.kind, ErrorKind::Internal);
        assert!(arbitrage_error.message.contains("Worker error"));
    }

    #[test]
    fn test_from_kv_operation_error_not_found() {
        let kv_err = KvOperationError::NotFound("test_key".to_string());
        let arb_err = ArbitrageError::from(kv_err);
        assert_eq!(arb_err.kind, ErrorKind::NotFound);
        assert_eq!(arb_err.status, Some(404));
        assert!(arb_err.message.contains("KV item not found: test_key"));
    }

    #[test]
    fn test_from_kv_operation_error_serialization() {
        // Create a real serde_json::Error by trying to parse malformed JSON
        let malformed_json = "{\"key\": invalid_value}"; // Missing quotes around invalid_value
        let json_error = serde_json::from_str::<Value>(malformed_json).unwrap_err();
        let kv_err = KvOperationError::Serialization(json_error);
        let arb_err = ArbitrageError::from(kv_err);
        assert_eq!(arb_err.kind, ErrorKind::Serialization);
        assert_eq!(arb_err.status, Some(400)); // Assuming serialization error maps to 400
        assert!(arb_err
            .message
            .contains("KV serialization/deserialization error"));
    }

    #[test]
    fn test_into_worker_error() {
        let arbitrage_error =
            ArbitrageError::internal_error("Something went wrong").with_status(500);
        let worker_error: worker::Error = arbitrage_error.into();

        match worker_error {
            worker::Error::RustError(msg) => {
                assert!(msg.contains("[Status: 500]"));
                assert!(msg.contains("ArbitrageError (Kind: Internal): Something went wrong"));
            }
            _ => panic!("Expected RustError variant"),
        }

        let arbitrage_error_no_status = ArbitrageError::network_error("Network down");
        // Clear status to test the other branch, network_error sets a status by default
        let arbitrage_error_no_status = ArbitrageError {
            status: None,
            ..arbitrage_error_no_status
        };
        let worker_error_no_status: worker::Error = arbitrage_error_no_status.into();
        match worker_error_no_status {
            worker::Error::RustError(msg) => {
                assert!(msg.contains("ArbitrageError (Kind: NetworkError): Network down"));
            }
            _ => panic!("Expected RustError variant for error with no status code"),
        }
    }

    #[test]
    fn test_arbitrage_error_macro() {
        let error_simple = arbitrage_error!(ErrorKind::ConfigError, "Bad config file");
        assert_eq!(error_simple.kind, ErrorKind::ConfigError);
        assert_eq!(error_simple.message, "Bad config file");

        let error_with_details = arbitrage_error!(
            ErrorKind::DatabaseError,
            "Query failed",
            "query" => "SELECT * FROM users",
            "params" => vec!["id1", "id2"]
        );
        assert_eq!(error_with_details.kind, ErrorKind::DatabaseError);
        assert_eq!(error_with_details.message, "Query failed");
        let details = error_with_details.details.unwrap();
        assert_eq!(details.get("query").unwrap(), &json!("SELECT * FROM users"));
        assert_eq!(details.get("params").unwrap(), &json!(vec!["id1", "id2"]));
    }
}
