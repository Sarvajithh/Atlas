//! Serialization helpers. Thin wrappers around `serde_json` that return
//! [`AppError`] instead of `serde_json::Error` directly, so call sites
//! across the workspace get one consistent error shape (§24, §45).

use serde::{de::DeserializeOwned, Serialize};

use crate::error::AppError;

/// Serialize a value to a JSON string.
pub fn to_json<T: Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(AppError::from)
}

/// Serialize a value to a `serde_json::Value` (used for the `payload`/
/// `capabilities`/`supported_tasks` JSON columns in §33).
pub fn to_json_value<T: Serialize>(value: &T) -> Result<serde_json::Value, AppError> {
    serde_json::to_value(value).map_err(AppError::from)
}

/// Deserialize a JSON string into a value.
pub fn from_json<T: DeserializeOwned>(json: &str) -> Result<T, AppError> {
    serde_json::from_str(json).map_err(AppError::from)
}

/// Deserialize a `serde_json::Value` into a value.
pub fn from_json_value<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, AppError> {
    serde_json::from_value(value).map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        name: String,
        count: u32,
    }

    #[test]
    fn round_trips_through_json_string() {
        let sample = Sample {
            name: "atlas".into(),
            count: 3,
        };
        let json = to_json(&sample).unwrap();
        let back: Sample = from_json(&json).unwrap();
        assert_eq!(sample, back);
    }

    #[test]
    fn round_trips_through_json_value() {
        let sample = Sample {
            name: "atlas".into(),
            count: 3,
        };
        let value = to_json_value(&sample).unwrap();
        let back: Sample = from_json_value(value).unwrap();
        assert_eq!(sample, back);
    }

    #[test]
    fn invalid_json_is_an_app_error_not_a_panic() {
        let result: Result<Sample, AppError> = from_json("not json");
        assert!(result.is_err());
    }
}
