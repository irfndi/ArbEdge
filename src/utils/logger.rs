// src/utils/logger.rs

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use worker::console_log;

/// Log levels supported by the logger
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
        }
    }

    pub fn from_str(s: &str) -> LogLevel {
        match s.to_lowercase().as_str() {
            "error" => LogLevel::Error,
            "warn" | "warning" => LogLevel::Warn,
            "info" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            _ => LogLevel::Info, // default
        }
    }
}

/// Simple logger for Cloudflare Workers
pub struct Logger {
    level: LogLevel,
    context: HashMap<String, Value>,
}

impl Logger {
    pub fn new(level: LogLevel) -> Self {
        Self {
            level,
            context: HashMap::new(),
        }
    }

    pub fn from_env() -> Self {
        // Try to get log level from environment, default to Info
        let level_str = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        Self::new(LogLevel::from_str(&level_str))
    }

    pub fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }

    pub fn get_level(&self) -> &LogLevel {
        &self.level
    }

    pub fn add_context(&mut self, key: &str, value: Value) {
        self.context.insert(key.to_string(), value);
    }

    pub fn child(&self, context: HashMap<String, Value>) -> Self {
        let mut new_context = self.context.clone();
        new_context.extend(context);

        Self {
            level: self.level.clone(),
            context: new_context,
        }
    }

    fn should_log(&self, level: &LogLevel) -> bool {
        level <= &self.level
    }

    fn format_message(&self, level: &LogLevel, message: &str, meta: Option<&Value>) -> String {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f UTC");

        let mut log_obj = serde_json::json!({
            "timestamp": timestamp.to_string(),
            "level": level.as_str(),
            "message": message,
        });

        // Add context
        if !self.context.is_empty() {
            log_obj["context"] = serde_json::Value::Object(
                self.context
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
        }

        // Add metadata if provided
        if let Some(meta) = meta {
            log_obj["meta"] = meta.clone();
        }

        serde_json::to_string(&log_obj)
            .unwrap_or_else(|_| format!("[{}] {}: {}", timestamp, level.as_str(), message))
    }

    pub fn error(&self, message: &str) {
        self.error_with_meta(message, None);
    }

    pub fn error_with_meta(&self, message: &str, meta: Option<&Value>) {
        if self.should_log(&LogLevel::Error) {
            let formatted = self.format_message(&LogLevel::Error, message, meta);
            console_log!("{}", formatted);
        }
    }

    pub fn warn(&self, message: &str) {
        self.warn_with_meta(message, None);
    }

    pub fn warn_with_meta(&self, message: &str, meta: Option<&Value>) {
        if self.should_log(&LogLevel::Warn) {
            let formatted = self.format_message(&LogLevel::Warn, message, meta);
            console_log!("{}", formatted);
        }
    }

    pub fn info(&self, message: &str) {
        self.info_with_meta(message, None);
    }

    pub fn info_with_meta(&self, message: &str, meta: Option<&Value>) {
        if self.should_log(&LogLevel::Info) {
            let formatted = self.format_message(&LogLevel::Info, message, meta);
            console_log!("{}", formatted);
        }
    }

    pub fn debug(&self, message: &str) {
        self.debug_with_meta(message, None);
    }

    pub fn debug_with_meta(&self, message: &str, meta: Option<&Value>) {
        if self.should_log(&LogLevel::Debug) {
            let formatted = self.format_message(&LogLevel::Debug, message, meta);
            console_log!("{}", formatted);
        }
    }

    pub fn add_error(&self, error: &dyn std::error::Error, context: Option<&Value>) {
        let error_meta = serde_json::json!({
            "error": error.to_string(),
            "error_type": std::any::type_name_of_val(error),
        });

        let combined_meta = match context {
            Some(ctx) => {
                let mut combined = ctx.clone();
                if let Value::Object(ref mut map) = combined {
                    if let Value::Object(error_map) = error_meta {
                        map.extend(error_map);
                    }
                }
                combined
            }
            None => error_meta,
        };

        self.error_with_meta("An error occurred", Some(&combined_meta));
    }
}

/// Global logger instance - thread-safe singleton
static GLOBAL_LOGGER: OnceLock<Logger> = OnceLock::new();

/// Initialize the global logger
pub fn init_logger(level: LogLevel) {
    GLOBAL_LOGGER.set(Logger::new(level)).ok();
}

/// Get a reference to the global logger
pub fn logger() -> &'static Logger {
    GLOBAL_LOGGER.get_or_init(|| Logger::from_env())
}

/// Convenience macros for logging
#[macro_export]
macro_rules! log_error {
    ($msg:expr) => {
        $crate::utils::logger::logger().error($msg)
    };
    ($msg:expr, $meta:expr) => {
        $crate::utils::logger::logger().error_with_meta($msg, Some(&$meta))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($msg:expr) => {
        $crate::utils::logger::logger().warn($msg)
    };
    ($msg:expr, $meta:expr) => {
        $crate::utils::logger::logger().warn_with_meta($msg, Some(&$meta))
    };
}

#[macro_export]
macro_rules! log_info {
    ($msg:expr) => {
        $crate::utils::logger::logger().info($msg)
    };
    ($msg:expr, $meta:expr) => {
        $crate::utils::logger::logger().info_with_meta($msg, Some(&$meta))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($msg:expr) => {
        $crate::utils::logger::logger().debug($msg)
    };
    ($msg:expr, $meta:expr) => {
        $crate::utils::logger::logger().debug_with_meta($msg, Some(&$meta))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
    }

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("error"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("WARN"), LogLevel::Warn);
        assert_eq!(LogLevel::from_str("info"), LogLevel::Info);
        assert_eq!(LogLevel::from_str("debug"), LogLevel::Debug);
        assert_eq!(LogLevel::from_str("invalid"), LogLevel::Info);
    }

    #[test]
    fn test_logger_should_log() {
        let logger = Logger::new(LogLevel::Warn);
        assert!(logger.should_log(&LogLevel::Error));
        assert!(logger.should_log(&LogLevel::Warn));
        assert!(!logger.should_log(&LogLevel::Info));
        assert!(!logger.should_log(&LogLevel::Debug));
    }
}
