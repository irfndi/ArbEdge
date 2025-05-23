use serde_json::Value;

/// Safely parses a value to a floating-point number.
/// If parsing fails or results in NaN, returns a default value.
pub fn safe_parse_float(value: &Value, default_value: f64) -> f64 {
    match value {
        Value::Null => default_value,
        Value::Number(n) => n.as_f64().unwrap_or(default_value),
        Value::String(s) => {
            if s.trim().is_empty() {
                default_value
            } else {
                s.parse::<f64>().unwrap_or(default_value)
            }
        }
        Value::Bool(b) => if *b { 1.0 } else { 0.0 },
        _ => default_value,
    }
}

/// Safely parses a string to a floating-point number.
pub fn safe_parse_float_str(value: &str, default_value: f64) -> f64 {
    if value.trim().is_empty() {
        return default_value;
    }
    
    value.parse::<f64>().unwrap_or(default_value)
}

/// Safely parses an optional string to a floating-point number.
pub fn safe_parse_float_opt(value: Option<&str>, default_value: f64) -> f64 {
    match value {
        Some(s) => safe_parse_float_str(s, default_value),
        None => default_value,
    }
}

/// Performs a deep clone of a JSON-serializable value.
/// This is equivalent to JSON.parse(JSON.stringify()) in JavaScript.
pub fn deep_clone<T>(value: &T) -> Result<T, serde_json::Error> 
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    let json_str = serde_json::to_string(value)?;
    serde_json::from_str(&json_str)
}

/// Clamps a value between a minimum and maximum
pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Rounds a float to a specified number of decimal places
pub fn round_to_decimal_places(value: f64, decimal_places: u32) -> f64 {
    let multiplier = 10_f64.powi(decimal_places as i32);
    (value * multiplier).round() / multiplier
}

/// Converts a percentage string (e.g., "1.5%") to a decimal
pub fn percentage_to_decimal(percentage_str: &str) -> Result<f64, String> {
    let cleaned = percentage_str.trim().trim_end_matches('%');
    cleaned.parse::<f64>()
        .map(|p| p / 100.0)
        .map_err(|_| format!("Invalid percentage format: {}", percentage_str))
}

/// Converts a decimal to a percentage string
pub fn decimal_to_percentage(decimal: f64, decimal_places: u32) -> String {
    format!("{:.prec$}%", decimal * 100.0, prec = decimal_places as usize)
}

/// Checks if a float is approximately equal to another within a tolerance
pub fn approximately_equal(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() < tolerance
}

/// Calculates the absolute percentage difference between two values
pub fn percentage_difference(value1: f64, value2: f64) -> f64 {
    if value1 == 0.0 && value2 == 0.0 {
        0.0
    } else if value1 == 0.0 || value2 == 0.0 {
        f64::INFINITY
    } else {
        ((value1 - value2).abs() / ((value1 + value2) / 2.0)) * 100.0
    }
}

/// Validates that a value is within a specific range
pub fn validate_range<T: PartialOrd + std::fmt::Display>(
    value: T, 
    min: T, 
    max: T, 
    field_name: &str
) -> Result<T, String> {
    if value < min || value > max {
        Err(format!(
            "{} must be between {} and {}, got {}",
            field_name, min, max, value
        ))
    } else {
        Ok(value)
    }
}

/// Calculates the moving average of a slice of values
pub fn moving_average(values: &[f64], window_size: usize) -> Vec<f64> {
    if window_size == 0 || values.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    for i in 0..values.len() {
        let start = if i + 1 >= window_size { i + 1 - window_size } else { 0 };
        let end = i + 1;
        let window = &values[start..end];
        let avg = window.iter().sum::<f64>() / window.len() as f64;
        result.push(avg);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_safe_parse_float() {
        assert_eq!(safe_parse_float(&json!(42.5), 0.0), 42.5);
        assert_eq!(safe_parse_float(&json!("123.45"), 0.0), 123.45);
        assert_eq!(safe_parse_float(&json!(null), 10.0), 10.0);
        assert_eq!(safe_parse_float(&json!(""), 5.0), 5.0);
        assert_eq!(safe_parse_float(&json!("invalid"), 7.0), 7.0);
        assert_eq!(safe_parse_float(&json!(true), 0.0), 1.0);
        assert_eq!(safe_parse_float(&json!(false), 0.0), 0.0);
    }

    #[test]
    fn test_safe_parse_float_str() {
        assert_eq!(safe_parse_float_str("123.45", 0.0), 123.45);
        assert_eq!(safe_parse_float_str("", 10.0), 10.0);
        assert_eq!(safe_parse_float_str("  ", 5.0), 5.0);
        assert_eq!(safe_parse_float_str("invalid", 7.0), 7.0);
    }

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(5, 1, 10), 5);
        assert_eq!(clamp(0, 1, 10), 1);
        assert_eq!(clamp(15, 1, 10), 10);
    }

    #[test]
    fn test_round_to_decimal_places() {
        assert_eq!(round_to_decimal_places(3.14159, 2), 3.14);
        assert_eq!(round_to_decimal_places(3.14159, 4), 3.1416);
    }

    #[test]
    fn test_percentage_to_decimal() {
        assert_eq!(percentage_to_decimal("50%").unwrap(), 0.5);
        assert_eq!(percentage_to_decimal("1.5%").unwrap(), 0.015);
        assert_eq!(percentage_to_decimal("100").unwrap(), 1.0);
        assert!(percentage_to_decimal("invalid%").is_err());
    }

    #[test]
    fn test_decimal_to_percentage() {
        assert_eq!(decimal_to_percentage(0.5, 1), "50.0%");
        assert_eq!(decimal_to_percentage(0.015, 3), "1.500%");
    }

    #[test]
    fn test_approximately_equal() {
        assert!(approximately_equal(1.0, 1.001, 0.01));
        assert!(!approximately_equal(1.0, 1.1, 0.01));
    }

    #[test]
    fn test_percentage_difference() {
        assert_eq!(percentage_difference(100.0, 90.0), 200.0 * 10.0 / 190.0);
        assert_eq!(percentage_difference(0.0, 0.0), 0.0);
    }

    #[test]
    fn test_validate_range() {
        assert_eq!(validate_range(5, 1, 10, "test").unwrap(), 5);
        assert!(validate_range(0, 1, 10, "test").is_err());
        assert!(validate_range(15, 1, 10, "test").is_err());
    }

    #[test]
    fn test_moving_average() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = moving_average(&values, 3);
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], 1.0); // [1] avg = 1
        assert_eq!(result[1], 1.5); // [1,2] avg = 1.5
        assert_eq!(result[2], 2.0); // [1,2,3] avg = 2
        assert_eq!(result[3], 3.0); // [2,3,4] avg = 3
        assert_eq!(result[4], 4.0); // [3,4,5] avg = 4
    }
} 