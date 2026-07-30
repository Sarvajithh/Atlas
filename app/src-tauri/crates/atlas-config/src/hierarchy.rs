//! Hierarchical configuration loading (§23, Governing Principle: "No
//! hardcoded configuration"). Four layers, lowest to highest precedence:
//!
//! ```text
//! Default  <  User  <  Workspace  <  Runtime
//! ```
//!
//! - **Default**: values shipped with the application, the fallback when
//!   nothing else overrides them.
//! - **User**: the user's own global preferences (Settings screen, §8.2.7).
//! - **Workspace**: per-workspace overrides (`scope = Workspace`, §33.12).
//! - **Runtime**: in-memory overrides for the current process only (e.g. a
//!   `--flag` or a test), never persisted.
//!
//! [`LayeredSettingsProvider`] implements the single [`SettingsProvider`]
//! interface (§33.12: "no crate reads the `settings` table directly") by
//! resolving a key through these layers, highest precedence first. The
//! concrete, persistent storage for the User and Workspace layers is still
//! `atlas-db`'s `settings` table (Dependency Inversion is preserved: this
//! module only holds the *merge policy*, not a SQLite dependency); Default
//! and Runtime are always in-memory, since neither is meant to survive a
//! restart.

use std::collections::HashMap;
use std::sync::RwLock;

use atlas_types::ids::WorkspaceId;
use atlas_types::settings::{SettingEntry, SettingsScope};
use atlas_utils::AppError;

use crate::provider::SettingsProvider;
use crate::validation::validate_setting_entry;

/// Which configuration layer a value comes from or is being written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigLayer {
    Default,
    User,
    Workspace,
    Runtime,
}

impl ConfigLayer {
    /// Precedence order, highest first -- the order [`LayeredSettingsProvider`]
    /// searches in when resolving a key.
    const RESOLUTION_ORDER: [ConfigLayer; 4] = [
        ConfigLayer::Runtime,
        ConfigLayer::Workspace,
        ConfigLayer::User,
        ConfigLayer::Default,
    ];
}

/// Key for a single stored value: the setting key, plus the workspace it's
/// scoped to (`None` for global/Default/Runtime entries).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StoreKey {
    key: String,
    workspace_id: Option<i64>,
}

/// A [`SettingsProvider`] backed by four in-memory layers with override
/// precedence (§23). Suitable as-is for the Default and Runtime layers;
/// User/Workspace layers are expected to be kept in sync with `atlas-db`'s
/// persistent store by whoever composes this provider (§46: composition is
/// atlas-core's job, not this module's).
pub struct LayeredSettingsProvider {
    layers: RwLock<HashMap<ConfigLayer, HashMap<StoreKey, SettingEntry>>>,
}

impl LayeredSettingsProvider {
    pub fn new() -> Self {
        let mut layers = HashMap::new();
        for layer in ConfigLayer::RESOLUTION_ORDER {
            layers.insert(layer, HashMap::new());
        }
        Self {
            layers: RwLock::new(layers),
        }
    }

    /// Seed or overwrite a value in a specific layer (§23: "default config",
    /// "user config", "workspace overrides", "runtime overrides"),
    /// validating it first (Governing Principle: configuration is never
    /// accepted without validation).
    pub fn set_in_layer(&self, layer: ConfigLayer, entry: SettingEntry) -> Result<(), AppError> {
        validate_setting_entry(&entry)?;
        let store_key = StoreKey {
            key: entry.key.clone(),
            workspace_id: entry.workspace_id.map(|id| id.0),
        };
        let mut layers = self
            .layers
            .write()
            .map_err(|_| AppError::user("configuration store lock poisoned"))?;
        layers
            .get_mut(&layer)
            .expect("all ConfigLayer variants are pre-populated in new()")
            .insert(store_key, entry);
        Ok(())
    }

    /// Resolve a key by walking layers highest-precedence first, optionally
    /// scoped to a workspace.
    fn resolve(
        &self,
        key: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Option<SettingEntry>, AppError> {
        let layers = self
            .layers
            .read()
            .map_err(|_| AppError::user("configuration store lock poisoned"))?;

        for layer in ConfigLayer::RESOLUTION_ORDER {
            let store_key = StoreKey {
                key: key.to_string(),
                workspace_id: workspace_id.map(|id| id.0),
            };
            if let Some(entry) = layers.get(&layer).and_then(|l| l.get(&store_key)) {
                return Ok(Some(entry.clone()));
            }
            // A workspace-scoped lookup also falls back to that layer's
            // global (workspace_id = None) entry, so a workspace override
            // only needs to be set where it actually differs.
            if workspace_id.is_some() {
                let global_key = StoreKey {
                    key: key.to_string(),
                    workspace_id: None,
                };
                if let Some(entry) = layers.get(&layer).and_then(|l| l.get(&global_key)) {
                    return Ok(Some(entry.clone()));
                }
            }
        }
        Ok(None)
    }
}

impl Default for LayeredSettingsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsProvider for LayeredSettingsProvider {
    fn get_global(&self, key: &str) -> Result<Option<SettingEntry>, AppError> {
        self.resolve(key, None)
    }

    fn get_for_workspace(
        &self,
        key: &str,
        workspace_id: WorkspaceId,
    ) -> Result<Option<SettingEntry>, AppError> {
        self.resolve(key, Some(workspace_id))
    }

    fn set(&self, entry: SettingEntry) -> Result<(), AppError> {
        // A plain `set` through the SettingsProvider interface (as called
        // by, e.g., the `settings.set` IPC command, §43.1) writes to the
        // layer implied by the entry's own scope: User for global settings,
        // Workspace for workspace-scoped settings. Runtime and Default are
        // only ever written via `set_in_layer` directly.
        let layer = match entry.scope {
            SettingsScope::Global => ConfigLayer::User,
            SettingsScope::Workspace => ConfigLayer::Workspace,
        };
        self.set_in_layer(layer, entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, value: &str) -> SettingEntry {
        SettingEntry {
            key: key.to_string(),
            value: value.to_string(),
            value_type: "string".to_string(),
            scope: SettingsScope::Global,
            workspace_id: None,
            updated_at: "1970-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn falls_back_to_default_when_no_override_exists() {
        let provider = LayeredSettingsProvider::new();
        provider
            .set_in_layer(ConfigLayer::Default, entry("ollama.host", "localhost"))
            .unwrap();

        assert_eq!(
            provider.get_global("ollama.host").unwrap().unwrap().value,
            "localhost"
        );
    }

    #[test]
    fn user_layer_overrides_default_layer() {
        let provider = LayeredSettingsProvider::new();
        provider
            .set_in_layer(ConfigLayer::Default, entry("ollama.host", "localhost"))
            .unwrap();
        provider
            .set_in_layer(ConfigLayer::User, entry("ollama.host", "192.168.1.10"))
            .unwrap();

        assert_eq!(
            provider.get_global("ollama.host").unwrap().unwrap().value,
            "192.168.1.10"
        );
    }

    #[test]
    fn runtime_layer_overrides_everything() {
        let provider = LayeredSettingsProvider::new();
        provider
            .set_in_layer(ConfigLayer::Default, entry("ollama.host", "localhost"))
            .unwrap();
        provider
            .set_in_layer(ConfigLayer::User, entry("ollama.host", "192.168.1.10"))
            .unwrap();
        provider
            .set_in_layer(ConfigLayer::Runtime, entry("ollama.host", "test-double"))
            .unwrap();

        assert_eq!(
            provider.get_global("ollama.host").unwrap().unwrap().value,
            "test-double"
        );
    }

    #[test]
    fn workspace_scoped_lookup_prefers_workspace_override() {
        let provider = LayeredSettingsProvider::new();
        provider
            .set_in_layer(ConfigLayer::Default, entry("indexing.ocr_enabled", "true"))
            .unwrap();

        let mut workspace_entry = entry("indexing.ocr_enabled", "false");
        workspace_entry.scope = SettingsScope::Workspace;
        workspace_entry.workspace_id = Some(WorkspaceId(7));
        provider
            .set_in_layer(ConfigLayer::Workspace, workspace_entry)
            .unwrap();

        assert_eq!(
            provider
                .get_for_workspace("indexing.ocr_enabled", WorkspaceId(7))
                .unwrap()
                .unwrap()
                .value,
            "false"
        );
        // A different workspace still falls back to the global default.
        assert_eq!(
            provider
                .get_for_workspace("indexing.ocr_enabled", WorkspaceId(8))
                .unwrap()
                .unwrap()
                .value,
            "true"
        );
    }

    #[test]
    fn missing_key_resolves_to_none() {
        let provider = LayeredSettingsProvider::new();
        assert!(provider.get_global("does.not.exist").unwrap().is_none());
    }

    #[test]
    fn set_via_settings_provider_trait_routes_by_scope() {
        let provider = LayeredSettingsProvider::new();
        provider.set(entry("ollama.host", "from-trait")).unwrap();
        assert_eq!(
            provider.get_global("ollama.host").unwrap().unwrap().value,
            "from-trait"
        );
    }

    #[test]
    fn set_in_layer_rejects_invalid_entry() {
        let provider = LayeredSettingsProvider::new();
        let mut bad = entry("", "value");
        bad.key = String::new();
        assert!(provider.set_in_layer(ConfigLayer::Default, bad).is_err());
    }
}
