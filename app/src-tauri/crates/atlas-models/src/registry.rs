//! Model Registry (§37, §33.13).
//!
//! `ModelRegistryRepository` is the storage interface (implemented by
//! atlas-db). `ModelProvider` is what Engines actually depend on: "give me
//! the current model for `TutorEngine`," never "load `some-model-name`"
//! (§37.1).

use std::sync::Mutex;

use atlas_types::ids::ModelRegistryId;
use atlas_types::model::{EngineRole, ModelRegistryEntry};
use atlas_utils::AppError;

pub trait ModelRegistryRepository: Send + Sync {
    fn list(&self) -> Result<Vec<ModelRegistryEntry>, AppError>;
    fn find_for_role(&self, role: EngineRole) -> Result<Option<ModelRegistryEntry>, AppError>;
    fn upsert(&self, entry: ModelRegistryEntry) -> Result<ModelRegistryEntry, AppError>;

    /// Manual selection (Model Dashboard): mark `model_identifier` as the
    /// selected model for `role`, unselecting any other row currently
    /// selected for that same role first, so at most one entry per role is
    /// ever selected at a time. This is the ONLY place a role's selection
    /// changes -- there is no automatic/background selection anywhere else
    /// (`ModelDiscoveryService::run` deliberately never sets
    /// `is_selected_for_role = true`; see its doc comment). Errors if
    /// `model_identifier` isn't actually a registered candidate for `role`
    /// (i.e. it was never discovered as compatible with that role), which
    /// doubles as the capability-validation step: only models the registry
    /// already knows are compatible with a role can be selected for it.
    fn select_for_role(&self, role: EngineRole, model_identifier: &str) -> Result<ModelRegistryEntry, AppError> {
        let entries = self.list()?;
        let target = entries
            .iter()
            .find(|e| e.engine_role == role && e.model_identifier == model_identifier)
            .cloned()
            .ok_or_else(|| {
                AppError::model(format!(
                    "{model_identifier} is not a discovered/compatible candidate for role {role:?}"
                ))
            })?;

        for mut other in entries
            .into_iter()
            .filter(|e| e.engine_role == role && e.model_identifier != model_identifier && e.is_selected_for_role)
        {
            other.is_selected_for_role = false;
            self.upsert(other)?;
        }

        let mut selected = target;
        selected.is_selected_for_role = true;
        self.upsert(selected)
    }
}

/// The interface Engines depend on (§37.1, §37.2). Assignment of models to
/// roles is data (`model_registry` table), never a compiled-in mapping.
pub trait ModelProvider: Send + Sync {
    fn current_model_for(&self, role: EngineRole) -> Result<ModelRegistryEntry, AppError>;
}

/// A dependency-free, in-memory `ModelRegistryRepository` + `ModelProvider`.
///
/// This is also where the "Capability Registry" concept requested for this
/// crate lives conceptually: [`EngineRole`] *is* the capability enumeration
/// (Vision, OCR, Embedding, Retriever, Reranker, Tutor, Reasoning, Planner,
/// Memory, Analytics -- §14.1), and [`ModelProvider::current_model_for`] is
/// the capability lookup itself -- callers ask for a capability/role and
/// never see a model name until this registry resolves one (§37.1). A
/// second, separate "capability registry" module would duplicate this
/// exact responsibility (§46.2), so none is introduced; this in-memory
/// implementation is the reusable, swappable-for-testing instance of the
/// one registry the architecture already defines.
pub struct InMemoryModelRegistry {
    entries: Mutex<Vec<ModelRegistryEntry>>,
    next_id: Mutex<i64>,
}

impl InMemoryModelRegistry {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
        }
    }
}

impl Default for InMemoryModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistryRepository for InMemoryModelRegistry {
    fn list(&self) -> Result<Vec<ModelRegistryEntry>, AppError> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| AppError::user("model registry lock poisoned"))?
            .clone())
    }

    fn find_for_role(&self, role: EngineRole) -> Result<Option<ModelRegistryEntry>, AppError> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| AppError::user("model registry lock poisoned"))?;
        Ok(entries
            .iter()
            .find(|e| e.engine_role == role && e.is_selected_for_role)
            .cloned())
    }

    fn upsert(&self, mut entry: ModelRegistryEntry) -> Result<ModelRegistryEntry, AppError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| AppError::user("model registry lock poisoned"))?;

        // id 0 is the "not yet persisted" sentinel (matches the SQLite
        // adapter's AUTOINCREMENT convention) -- assign a fresh id rather
        // than colliding every new entry on id 0.
        if entry.id.0 == 0 {
            let mut next_id = self
                .next_id
                .lock()
                .map_err(|_| AppError::user("model registry id counter lock poisoned"))?;
            entry.id = ModelRegistryId(*next_id);
            *next_id += 1;
        }

        if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry.clone();
        } else {
            entries.push(entry.clone());
        }
        Ok(entry)
    }
}

impl ModelProvider for InMemoryModelRegistry {
    fn current_model_for(&self, role: EngineRole) -> Result<ModelRegistryEntry, AppError> {
        self.find_for_role(role)?
            .ok_or_else(|| AppError::model(format!("no model currently assigned to {role:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::ids::ModelRegistryId;
    use atlas_types::model::ModelStatus;

    fn sample_entry(role: EngineRole, selected: bool) -> ModelRegistryEntry {
        ModelRegistryEntry {
            id: ModelRegistryId(1),
            model_identifier: "llama3.1".to_string(),
            engine_role: role,
            capabilities: serde_json::json!({}),
            context_length: 8192,
            vram_requirement: None,
            status: ModelStatus::Available,
            version: "1".to_string(),
            supported_tasks: serde_json::json!([]),
            is_selected_for_role: selected,
        }
    }

    #[test]
    fn upsert_then_list_returns_the_entry() {
        let registry = InMemoryModelRegistry::new();
        registry
            .upsert(sample_entry(EngineRole::Tutor, true))
            .unwrap();
        assert_eq!(registry.list().unwrap().len(), 1);
    }

    #[test]
    fn find_for_role_only_returns_selected_entry() {
        let registry = InMemoryModelRegistry::new();
        registry
            .upsert(sample_entry(EngineRole::Tutor, false))
            .unwrap();
        assert!(registry.find_for_role(EngineRole::Tutor).unwrap().is_none());
    }

    #[test]
    fn current_model_for_resolves_selected_entry() {
        let registry = InMemoryModelRegistry::new();
        registry
            .upsert(sample_entry(EngineRole::Tutor, true))
            .unwrap();
        assert_eq!(
            registry
                .current_model_for(EngineRole::Tutor)
                .unwrap()
                .model_identifier,
            "llama3.1"
        );
    }

    #[test]
    fn current_model_for_missing_role_is_a_model_error() {
        let registry = InMemoryModelRegistry::new();
        let err = registry.current_model_for(EngineRole::Vision).unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn select_for_role_switches_selection_between_two_candidates() {
        let registry = InMemoryModelRegistry::new();
        let mut a = sample_entry(EngineRole::Tutor, true);
        a.model_identifier = "model-a".to_string();
        let mut b = sample_entry(EngineRole::Tutor, false);
        b.model_identifier = "model-b".to_string();
        registry.upsert(a).unwrap();
        registry.upsert(b).unwrap();

        registry.select_for_role(EngineRole::Tutor, "model-b").unwrap();

        assert_eq!(
            registry.find_for_role(EngineRole::Tutor).unwrap().unwrap().model_identifier,
            "model-b"
        );
        let all = registry.list().unwrap();
        assert!(!all.iter().find(|e| e.model_identifier == "model-a").unwrap().is_selected_for_role);
    }

    #[test]
    fn select_for_role_rejects_a_model_not_registered_for_that_role() {
        let registry = InMemoryModelRegistry::new();
        registry.upsert(sample_entry(EngineRole::Tutor, false)).unwrap();
        let err = registry.select_for_role(EngineRole::Tutor, "not-a-candidate").unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn upsert_with_same_id_replaces_existing_entry() {
        let registry = InMemoryModelRegistry::new();
        registry
            .upsert(sample_entry(EngineRole::Tutor, true))
            .unwrap();
        let mut updated = sample_entry(EngineRole::Tutor, true);
        updated.model_identifier = "llama3.2".to_string();
        registry.upsert(updated).unwrap();

        assert_eq!(registry.list().unwrap().len(), 1);
        assert_eq!(registry.list().unwrap()[0].model_identifier, "llama3.2");
    }
}
