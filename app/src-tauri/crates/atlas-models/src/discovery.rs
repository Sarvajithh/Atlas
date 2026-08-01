//! Model Discovery (§37.1, §41 step 5). Reconciles whatever models the
//! local Ollama instance currently reports into the `model_registry` table,
//! auto-assigning a default model per role the first time one becomes
//! available, without ever overwriting a role the user (or a previous run)
//! already assigned a model to (Settings §23 always wins).

use std::sync::Arc;

use atlas_events::EventBus;
use atlas_types::event::{AppEvent, EventType};
use atlas_types::model::{EngineRole, ModelRegistryEntry, ModelStatus};
use atlas_utils::AppError;

use crate::ollama::{ModelCapability, OllamaProvider};
use crate::registry::ModelRegistryRepository;

/// Which §14.1 Engine roles a given capability can serve. A single
/// discovered model can back several roles at once (e.g. one general
/// text-generation model serving Tutor, Reasoning, and Planner) --
/// Retriever/Reranker/Analytics are algorithmic (not Ollama-backed) and
/// intentionally absent here; OCR continues to run through the existing
/// Tesseract pipeline (§14.1 Ocr row) rather than an Ollama model in this
/// milestone.
fn roles_for_capability(capability: ModelCapability) -> &'static [EngineRole] {
    match capability {
        ModelCapability::Vision => &[EngineRole::Vision],
        ModelCapability::Embedding => &[EngineRole::Embedding],
        ModelCapability::TextGeneration => &[EngineRole::Tutor, EngineRole::Reasoning, EngineRole::Planner],
        ModelCapability::Tools => &[],
    }
}

pub struct ModelDiscoveryService {
    ollama: Arc<OllamaProvider>,
    registry: Arc<dyn ModelRegistryRepository>,
    events: Arc<dyn EventBus>,
}

impl ModelDiscoveryService {
    pub fn new(ollama: Arc<OllamaProvider>, registry: Arc<dyn ModelRegistryRepository>, events: Arc<dyn EventBus>) -> Self {
        Self { ollama, registry, events }
    }

    /// Run discovery once: list Ollama's installed models, upsert each into
    /// the registry for every role its capabilities cover, and auto-select
    /// the first discovered model for any role that doesn't already have a
    /// selection. On failure to reach Ollama, publishes `ModelUnavailable`
    /// (§34.2) and returns the error -- callers (the Startup Sequence,
    /// §41) are expected to log and continue rather than abort (§41
    /// closing note: "gracefully degrade").
    pub fn run(&self) -> Result<Vec<ModelRegistryEntry>, AppError> {
        let discovered = match self.ollama.discover_models() {
            Ok(models) => models,
            Err(err) => {
                let _ = self.events.publish(AppEvent {
                    id: None,
                    event_type: EventType::ModelUnavailable,
                    payload: serde_json::json!({ "reason": err.message }),
                    occurred_at: atlas_utils::time::now_iso8601(),
                });
                return Err(err);
            }
        };

        let existing = self.registry.list()?;
        let mut written = Vec::new();

        for model in &discovered {
            for &role in model.capabilities.iter().flat_map(|c| roles_for_capability(*c)) {
                let already_selected_for_role = existing.iter().any(|e| e.engine_role == role && e.is_selected_for_role);
                let existing_entry = existing
                    .iter()
                    .find(|e| e.engine_role == role && e.model_identifier == model.model_identifier)
                    .cloned();

                let entry = ModelRegistryEntry {
                    id: existing_entry.as_ref().map(|e| e.id).unwrap_or(atlas_types::ids::ModelRegistryId(0)),
                    model_identifier: model.model_identifier.clone(),
                    engine_role: role,
                    capabilities: serde_json::to_value(&model.capabilities).unwrap_or(serde_json::json!([])),
                    context_length: model.context_length,
                    vram_requirement: None,
                    status: ModelStatus::Available,
                    version: model.parameter_size.clone().unwrap_or_else(|| "unknown".to_string()),
                    supported_tasks: serde_json::json!([]),
                    is_selected_for_role: existing_entry.as_ref().map(|e| e.is_selected_for_role).unwrap_or(!already_selected_for_role),
                };
                written.push(self.registry.upsert(entry)?);
            }
        }

        let _ = self.events.publish(AppEvent {
            id: None,
            event_type: EventType::ModelLoaded,
            payload: serde_json::json!({ "discovered_count": discovered.len(), "registry_entries_written": written.len() }),
            occurred_at: atlas_utils::time::now_iso8601(),
        });

        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::OllamaConnection;
    use crate::registry::InMemoryModelRegistry;
    use atlas_events::InMemoryEventBus;
    use std::io::Write;
    use std::net::TcpListener;

    fn mock_ollama_tags(models: serde_json::Value) -> (Arc<OllamaProvider>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            use std::io::Read;
            loop {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(_) => return,
                };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let body = if request.starts_with("GET /api/tags") {
                    models.clone()
                } else {
                    serde_json::json!({ "capabilities": ["completion"], "model_info": {} })
                };
                let payload = body.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                let _ = stream.write_all(response.as_bytes());
                if !request.starts_with("GET /api/tags") {
                    return;
                }
            }
        });
        (Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", port))), handle)
    }

    #[test]
    fn run_auto_selects_first_discovered_model_for_each_covered_role() {
        let (ollama, _handle) = mock_ollama_tags(serde_json::json!({ "models": [{ "name": "llama3.1" }] }));
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let service = ModelDiscoveryService::new(ollama, registry.clone(), events);

        let written = service.run().unwrap();
        assert!(!written.is_empty());
        assert!(registry.find_for_role(EngineRole::Tutor).unwrap().is_some());
        assert!(registry.find_for_role(EngineRole::Reasoning).unwrap().is_some());
        assert!(registry.find_for_role(EngineRole::Planner).unwrap().is_some());
    }

    #[test]
    fn run_does_not_overwrite_an_already_selected_model_for_a_role() {
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        registry
            .upsert(ModelRegistryEntry {
                id: atlas_types::ids::ModelRegistryId(0),
                model_identifier: "user-chosen-model".to_string(),
                engine_role: EngineRole::Tutor,
                capabilities: serde_json::json!([]),
                context_length: 4096,
                vram_requirement: None,
                status: ModelStatus::Available,
                version: "1".to_string(),
                supported_tasks: serde_json::json!([]),
                is_selected_for_role: true,
            })
            .unwrap();

        let (ollama, _handle) = mock_ollama_tags(serde_json::json!({ "models": [{ "name": "newly-pulled-model" }] }));
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let service = ModelDiscoveryService::new(ollama, registry.clone(), events);
        service.run().unwrap();

        assert_eq!(
            registry.find_for_role(EngineRole::Tutor).unwrap().unwrap().model_identifier,
            "user-chosen-model"
        );
    }

    #[test]
    fn run_returns_a_model_error_and_publishes_unavailable_when_ollama_unreachable() {
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1)));
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let service = ModelDiscoveryService::new(ollama, registry, events);

        let err = service.run().unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }
}
