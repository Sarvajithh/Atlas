//! Configuration validation. Every [`SettingEntry`] is validated before it
//! is accepted into any layer (§23; Governing Principle). Kept as a
//! standalone function (rather than a trait) since there is currently
//! exactly one validation policy; a `ConfigValidator` trait can be
//! introduced if a second policy is ever needed (§28: extend, don't
//! redesign).

use atlas_types::settings::{SettingEntry, SettingsScope};
use atlas_utils::validation::{require_non_empty, require_one_of};
use atlas_utils::AppError;

const ALLOWED_VALUE_TYPES: [&str; 4] = ["string", "number", "bool", "json"];

/// Validate a [`SettingEntry`] before it is stored in any configuration
/// layer.
pub fn validate_setting_entry(entry: &SettingEntry) -> Result<(), AppError> {
    require_non_empty("settings.key", &entry.key)?;
    require_one_of(
        "settings.value_type",
        &entry.value_type,
        &ALLOWED_VALUE_TYPES,
    )?;

    match (&entry.scope, &entry.workspace_id) {
        (SettingsScope::Workspace, None) => {
            return Err(AppError::configuration(
                "settings.workspace_id is required when scope = Workspace",
            ));
        }
        (SettingsScope::Global, Some(_)) => {
            return Err(AppError::configuration(
                "settings.workspace_id must be empty when scope = Global",
            ));
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::ids::WorkspaceId;

    fn base_entry() -> SettingEntry {
        SettingEntry {
            key: "ollama.host".to_string(),
            value: "localhost".to_string(),
            value_type: "string".to_string(),
            scope: SettingsScope::Global,
            workspace_id: None,
            updated_at: "1970-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn valid_global_entry_passes() {
        assert!(validate_setting_entry(&base_entry()).is_ok());
    }

    #[test]
    fn empty_key_is_rejected() {
        let mut entry = base_entry();
        entry.key = String::new();
        assert!(validate_setting_entry(&entry).is_err());
    }

    #[test]
    fn unknown_value_type_is_rejected() {
        let mut entry = base_entry();
        entry.value_type = "float".to_string();
        assert!(validate_setting_entry(&entry).is_err());
    }

    #[test]
    fn workspace_scope_without_workspace_id_is_rejected() {
        let mut entry = base_entry();
        entry.scope = SettingsScope::Workspace;
        assert!(validate_setting_entry(&entry).is_err());
    }

    #[test]
    fn global_scope_with_workspace_id_is_rejected() {
        let mut entry = base_entry();
        entry.workspace_id = Some(WorkspaceId(1));
        assert!(validate_setting_entry(&entry).is_err());
    }

    #[test]
    fn workspace_scope_with_workspace_id_passes() {
        let mut entry = base_entry();
        entry.scope = SettingsScope::Workspace;
        entry.workspace_id = Some(WorkspaceId(1));
        assert!(validate_setting_entry(&entry).is_ok());
    }
}
