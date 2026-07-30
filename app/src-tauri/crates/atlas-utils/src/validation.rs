//! Validation helpers. Small, reusable checks used by configuration
//! validation (§23), IPC input validation (§26: "handlers validate input"),
//! and anywhere else a value needs to be checked before use.

use crate::error::AppError;

/// Require a string to be non-empty (after trimming), returning a
/// [`AppError::user`] otherwise.
pub fn require_non_empty(field: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::user(format!("{field} must not be empty")));
    }
    Ok(())
}

/// Require a numeric value to fall within an inclusive range.
pub fn require_in_range(field: &str, value: i64, min: i64, max: i64) -> Result<(), AppError> {
    if value < min || value > max {
        return Err(AppError::user(format!(
            "{field} must be between {min} and {max} (got {value})"
        )));
    }
    Ok(())
}

/// Require a value to be one of a fixed set of allowed values (e.g.
/// validating a `settings.value_type` or workspace status string).
pub fn require_one_of(field: &str, value: &str, allowed: &[&str]) -> Result<(), AppError> {
    if !allowed.contains(&value) {
        return Err(AppError::user(format!(
            "{field} must be one of {allowed:?} (got {value:?})"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_non_empty_accepts_non_blank_string() {
        assert!(require_non_empty("name", "atlas").is_ok());
    }

    #[test]
    fn require_non_empty_rejects_blank_string() {
        assert!(require_non_empty("name", "   ").is_err());
    }

    #[test]
    fn require_in_range_accepts_boundary_values() {
        assert!(require_in_range("concurrency", 1, 1, 8).is_ok());
        assert!(require_in_range("concurrency", 8, 1, 8).is_ok());
    }

    #[test]
    fn require_in_range_rejects_out_of_range() {
        assert!(require_in_range("concurrency", 0, 1, 8).is_err());
        assert!(require_in_range("concurrency", 9, 1, 8).is_err());
    }

    #[test]
    fn require_one_of_accepts_allowed_value() {
        assert!(require_one_of("scope", "global", &["global", "workspace"]).is_ok());
    }

    #[test]
    fn require_one_of_rejects_disallowed_value() {
        assert!(require_one_of("scope", "cluster", &["global", "workspace"]).is_err());
    }
}
