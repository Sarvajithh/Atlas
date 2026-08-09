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

use crate::ollama::{DiscoveredModel, ModelCapability, OllamaProvider};
use crate::registry::ModelRegistryRepository;

/// Which §14.1 Engine roles a given capability can serve. A single
/// discovered model can back several roles at once (e.g. one general
/// text-generation model serving Tutor, Reasoning, and Planner) --
/// Retriever/Reranker/Analytics are algorithmic (not Ollama-backed) and
/// intentionally absent here. OCR (`EngineRole::Ocr`) is also absent:
/// `atlas_models::OllamaVisionOcrEngine` reads whichever model is
/// assigned to `EngineRole::Vision` instead of a separate Ocr-role
/// assignment, since a general vision-capable model is exactly what OCR
/// needs and Vision already gets auto-selected here.
fn roles_for_capability(capability: ModelCapability) -> &'static [EngineRole] {
    match capability {
        ModelCapability::Vision => &[EngineRole::Vision],
        ModelCapability::Embedding => &[EngineRole::Embedding],
        ModelCapability::TextGeneration => &[EngineRole::Tutor, EngineRole::Reasoning, EngineRole::Planner],
        ModelCapability::Tools => &[],
    }
}

/// Parses Ollama's reported `parameter_size` (e.g. `"8B"`, `"4b"`,
/// `"270M"`, `"70.6B"`) into a comparable size in billions of parameters.
/// Returns `None` when nothing usable was reported -- notably, several
/// cloud-hosted models report no local parameter size at all -- so an
/// unparseable/absent size can be ranked honestly last rather than
/// defaulting to a guessed value.
fn parameter_size_in_billions(parameter_size: &Option<String>) -> Option<f64> {
    let raw = parameter_size.as_ref()?.trim();
    if raw.is_empty() {
        return None;
    }
    let split_at = raw.len().checked_sub(1)?;
    let (numeric, unit) = raw.split_at(split_at);
    let value: f64 = numeric.parse().ok()?;
    match unit.to_ascii_uppercase().as_str() {
        "B" => Some(value),
        "M" => Some(value / 1_000.0),
        "K" => Some(value / 1_000_000.0),
        _ => None,
    }
}

/// Ranking key for "best available model for this role", built entirely
/// from what Ollama actually reported about the model -- never from its
/// name (§37.2 "Assignment, Not Hardcoding"). Different roles weight
/// different real signals:
/// - `Embedding`: embedding quality/coverage tracks the model's context
///   window far more than parameter count, so context length is the
///   primary signal (parameter size is usually unreported for dedicated
///   embedding models anyway).
/// - Every other role reachable here (`Vision`, and the
///   `TextGeneration`-backed `Tutor`/`Reasoning`/`Planner`): a larger
///   parameter count is the primary real-world signal of generation/
///   reasoning capability; context length is the tiebreaker between
///   models of comparable size.
/// A model with no parseable parameter size (e.g. a cloud-hosted model
/// Ollama doesn't report local weights for) ranks behind any model that
/// does report one, instead of winning by list order.
fn rank_key(role: EngineRole, model: &DiscoveredModel) -> (f64, u32) {
    match role {
        EngineRole::Embedding => (model.context_length as f64, model.context_length),
        _ => (
            parameter_size_in_billions(&model.parameter_size).unwrap_or(0.0),
            model.context_length,
        ),
    }
}

/// Picks the best of several models competing for the same role. Ties
/// (including "every candidate reported nothing usable") fall back to
/// the first candidate in discovery order rather than panicking or
/// picking arbitrarily on every call -- `max_by` is stable for equal
/// keys, so this is deterministic, just not meaningfully differentiated
/// when there's genuinely no signal to differentiate on.
fn select_best_for_role<'a>(
    role: EngineRole,
    candidates: impl Iterator<Item = &'a DiscoveredModel>,
) -> Option<&'a DiscoveredModel> {
    candidates.fold(None, |best: Option<&'a DiscoveredModel>, candidate| match best {
        None => Some(candidate),
        Some(current_best) => {
            if rank_key(role, candidate) > rank_key(role, current_best) {
                Some(candidate)
            } else {
                Some(current_best)
            }
        }
    })
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
    /// the registry for every role its capabilities cover, and -- for any
    /// role whose current selection is missing or stale (see below) --
    /// select the single best-ranked candidate for that role (§37.2: by
    /// real reported capability/size/context data, never by name or list
    /// order; see `select_best_for_role`). On failure to reach Ollama,
    /// publishes `ModelUnavailable` (§34.2) and returns the error --
    /// callers (the Startup Sequence, §41) are expected to log and
    /// continue rather than abort (§41 closing note: "gracefully
    /// degrade").
    ///
    /// A role's existing `is_selected_for_role` row is only left alone
    /// when the model it points at is *still actually installed* (i.e.
    /// present in `discovered`, this run's real `ollama list`). A
    /// selection pointing at a model no longer installed -- e.g. it was
    /// uninstalled, replaced, or renamed since it was selected -- is
    /// stale, not a preference to preserve: it's explicitly unselected
    /// here (so it doesn't linger as a phantom "selected" row forever)
    /// and the role falls through to normal best-candidate selection
    /// among what's actually available now. This was a real production
    /// bug: `embed()` failing with Ollama's "model ... not found, try
    /// pulling it first" for a model that was never even part of this
    /// codebase's discovery output (traced to a stale `qwen3-embedding`
    /// selection surviving indefinitely once the model was no longer
    /// installed, because the previous version of this method treated
    /// *any* existing selection as untouchable).
    pub fn run(&self) -> Result<Vec<ModelRegistryEntry>, AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        atlas_utils::log_info!("[ModelDiscovery] run() entered");
        let discovered = match self.ollama.discover_models() {
            Ok(models) => {
                atlas_utils::log_info!("[ModelDiscovery] ollama reported {} installed model(s): {:?}", models.len(), models.iter().map(|m| &m.model_identifier).collect::<Vec<_>>());
                models
            }
            Err(err) => {
                atlas_utils::log_error!("[ModelDiscovery] discover_models() FAILED: {} (registry will stay empty until discovery is re-run)", err.message);
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
        let installed_identifiers: std::collections::HashSet<&str> =
            discovered.iter().map(|m| m.model_identifier.as_str()).collect();

        // Any row that's currently marked selected but whose model is no
        // longer installed: explicitly unselect it up front (rather than
        // silently ignoring it), and log it, since a user losing their
        // embedding/tutor model out from under them is worth being able
        // to see in the logs, not just infer from a later failure.
        let mut written = Vec::new();
        for stale in existing
            .iter()
            .filter(|e| e.is_selected_for_role && !installed_identifiers.contains(e.model_identifier.as_str()))
        {
            atlas_utils::log_info!(
                "[ModelDiscovery] unselecting stale role assignment: {:?} was selected for {:?} but is no longer installed",
                stale.model_identifier,
                stale.engine_role
            );
            let mut corrected = stale.clone();
            corrected.is_selected_for_role = false;
            corrected.status = ModelStatus::Unavailable;
            written.push(self.registry.upsert(corrected)?);
        }
        // Re-read so the loop below sees the corrections just made,
        // instead of the pre-correction snapshot.
        let existing = self.registry.list()?;

        // Every (role, model) pair a discovered model covers, computed up
        // front so the "best for this role" ranking (below) considers
        // every candidate from this run at once, rather than the previous
        // one-model-at-a-time loop that could -- and, with more than one
        // installed model sharing a role, did -- mark more than one entry
        // `is_selected_for_role = true` for the same role in a single run
        // (both `InMemoryModelRegistry::find_for_role` and the SQLite
        // adapter's equivalent query just return whichever matching row
        // storage happens to return first when that happens, which is not
        // "the best model", it's arbitrary).
        let mut role_candidates: std::collections::HashMap<EngineRole, Vec<&DiscoveredModel>> =
            std::collections::HashMap::new();
        for model in &discovered {
            for &role in model.capabilities.iter().flat_map(|c| roles_for_capability(*c)) {
                role_candidates.entry(role).or_default().push(model);
            }
        }

        for (role, candidates) in &role_candidates {
            let role = *role;
            // A selection only counts as "already selected" if it's for a
            // model that's still actually installed -- see doc comment
            // above. Stale rows were already corrected (unselected)
            // above, so `existing` here never has a stale row marked
            // selected.
            let already_selected_for_role = existing.iter().any(|e| e.engine_role == role && e.is_selected_for_role);
            let best_identifier = if already_selected_for_role {
                None // an existing, still-valid (user or prior-run) selection wins; don't recompute one.
            } else {
                select_best_for_role(role, candidates.iter().copied()).map(|m| m.model_identifier.clone())
            };

            for model in candidates {
                let existing_entry = existing
                    .iter()
                    .find(|e| e.engine_role == role && e.model_identifier == model.model_identifier)
                    .cloned();

                let is_selected_for_role = match &existing_entry {
                    // Never move a selection an existing row already has,
                    // even between runs of this same loop.
                    Some(e) => e.is_selected_for_role,
                    None => best_identifier.as_deref() == Some(model.model_identifier.as_str()),
                };

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
                    is_selected_for_role,
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

        // TEMPORARY TRACE LOGGING
        atlas_utils::log_info!("[ModelDiscovery] run() exited, wrote {} registry entries", written.len());
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::OllamaConnection;
    use crate::registry::InMemoryModelRegistry;
    use atlas_events::InMemoryEventBus;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    /// Reads one full HTTP/1.1 request (headers + body, if `Content-Length`
    /// is present) off `stream`. A single `stream.read()` call is not
    /// reliable here: ureq (and most HTTP clients) can write request
    /// headers and a POST body as two separate TCP segments, so one read
    /// immediately after `accept()` can return only the headers before the
    /// body has actually arrived -- which silently produced a request with
    /// an empty/missing body (and so, for `/api/show`, no way to tell
    /// which model was being asked about) rather than a visible error.
    /// This loops until the declared `Content-Length` worth of body bytes
    /// (or, for a body-less request like `GET`, just the header
    /// terminator) has actually been read, with a short read timeout so a
    /// malformed test request can't hang the mock server thread forever.
    fn read_full_http_request(stream: &mut TcpStream) -> String {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
        let mut data: Vec<u8> = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let Some(headers_end) = find_subslice(&data, b"\r\n\r\n") else {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => return String::from_utf8_lossy(&data).into_owned(),
                    Ok(n) => {
                        data.extend_from_slice(&buf[..n]);
                        continue;
                    }
                }
            };
            let body_start = headers_end + 4;
            let headers = String::from_utf8_lossy(&data[..headers_end]);
            let content_length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().ok())?
                })
                .unwrap_or(0);
            while data.len() < body_start + content_length {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => data.extend_from_slice(&buf[..n]),
                }
            }
            return String::from_utf8_lossy(&data).into_owned();
        }
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|window| window == needle)
    }

    fn mock_ollama_tags(models: serde_json::Value) -> (Arc<OllamaProvider>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            loop {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(_) => return,
                };
                let request = read_full_http_request(&mut stream);
                let is_tags = request.starts_with("GET /api/tags");
                let body = if is_tags {
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
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
                if !is_tags {
                    return;
                }
            }
        });
        (Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", port))), handle)
    }

    /// Like `mock_ollama_tags`, but `/api/show` responses differ per
    /// model name (keyed from the request body), so tests can give
    /// different competing models different reported capabilities/sizes
    /// -- needed to exercise "pick the best of several candidates", which
    /// a single fixed `/api/show` response can't do.
    fn mock_ollama_tags_with_show(
        models: serde_json::Value,
        show_by_name: std::collections::HashMap<&'static str, serde_json::Value>,
    ) -> (Arc<OllamaProvider>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            loop {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(_) => return,
                };
                let request = read_full_http_request(&mut stream);
                let is_tags = request.starts_with("GET /api/tags");
                let body = if is_tags {
                    models.clone()
                } else {
                    let name = show_by_name
                        .keys()
                        .find(|name| request.contains(name.as_ref() as &str))
                        .copied();
                    name.and_then(|n| show_by_name.get(n))
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({ "capabilities": ["completion"], "model_info": {} }))
                };
                let payload = body.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
                // Keep serving further requests (one `/api/show` call per
                // discovered model, each its own connection since the
                // response declares `Connection: close`) rather than
                // returning after the first one -- unlike
                // `mock_ollama_tags`, which only ever needs to answer a
                // single `/api/show` call in the tests that use it, tests
                // using this helper exercise several models competing for
                // the same role, each needing its own `/api/show`
                // response. The thread is abandoned (not joined) once the
                // test's assertions are done, same as `mock_ollama_tags`.
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
    fn run_does_not_overwrite_an_already_selected_model_that_is_still_installed() {
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

        // "user-chosen-model" is still among the models this run
        // discovers -- a still-valid selection, so it must survive even
        // though "newly-pulled-model" is also present and would
        // otherwise be a candidate. Two models means two `/api/show`
        // calls, so this needs the multi-request-capable mock rather than
        // `mock_ollama_tags`, which only ever answers one.
        let (ollama, _handle) = mock_ollama_tags_with_show(
            serde_json::json!({ "models": [{ "name": "user-chosen-model" }, { "name": "newly-pulled-model" }] }),
            std::collections::HashMap::new(),
        );
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let service = ModelDiscoveryService::new(ollama, registry.clone(), events);
        service.run().unwrap();

        assert_eq!(
            registry.find_for_role(EngineRole::Tutor).unwrap().unwrap().model_identifier,
            "user-chosen-model"
        );
    }

    #[test]
    fn run_unselects_a_stale_selection_and_picks_a_real_replacement() {
        // Regression test for a real production failure: the registry had
        // `qwen3-embedding:latest` selected for `EngineRole::Embedding`,
        // but that model was never actually installed in this Ollama
        // instance (`ollama list` reported `nomic-embed-text:latest`
        // instead) -- so every embed() call failed with Ollama's "model
        // ... not found, try pulling it first", and the old `run()` kept
        // the dead selection forever because it treated any existing
        // selection as untouchable, with no check that the model behind
        // it still actually existed.
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        registry
            .upsert(ModelRegistryEntry {
                id: atlas_types::ids::ModelRegistryId(0),
                model_identifier: "qwen3-embedding:latest".to_string(),
                engine_role: EngineRole::Embedding,
                capabilities: serde_json::json!(["embedding"]),
                context_length: 4096,
                vram_requirement: None,
                status: ModelStatus::Available,
                version: "1".to_string(),
                supported_tasks: serde_json::json!([]),
                is_selected_for_role: true,
            })
            .unwrap();

        let mut show_by_name = std::collections::HashMap::new();
        show_by_name.insert(
            "nomic-embed-text",
            serde_json::json!({ "capabilities": ["embedding"], "details": {}, "model_info": {} }),
        );
        let (ollama, _handle) = mock_ollama_tags_with_show(
            serde_json::json!({ "models": [{ "name": "nomic-embed-text" }] }),
            show_by_name,
        );
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let service = ModelDiscoveryService::new(ollama, registry.clone(), events);
        service.run().unwrap();

        let selected = registry.find_for_role(EngineRole::Embedding).unwrap().unwrap();
        assert_eq!(selected.model_identifier, "nomic-embed-text");

        // The stale row itself was corrected in place, not just ignored:
        // querying the registry directly (not via find_for_role, which
        // would just skip it) confirms it's no longer marked selected.
        let all = registry.list().unwrap();
        let stale_row = all.iter().find(|e| e.model_identifier == "qwen3-embedding:latest").unwrap();
        assert!(!stale_row.is_selected_for_role);
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

    #[test]
    fn run_selects_exactly_one_model_per_role_even_when_several_compete() {
        // Regression test for the bug this fix closes: with more than one
        // newly-discovered model sharing a role and no prior selection,
        // `run()` used to mark *every one* of them `is_selected_for_role`
        // in the same pass, and `find_for_role` would then return
        // whichever one storage happened to return first -- arbitrary,
        // not "best". Five real models compete here, matching a real
        // `ollama list` output shape (mixed sizes, one with no reported
        // parameter size at all, like a cloud-hosted model).
        let mut show_by_name = std::collections::HashMap::new();
        show_by_name.insert(
            "granite4.1",
            serde_json::json!({ "capabilities": ["completion"], "details": { "parameter_size": "8B" }, "model_info": {} }),
        );
        show_by_name.insert(
            "llama3.1",
            serde_json::json!({ "capabilities": ["completion"], "details": { "parameter_size": "8B" }, "model_info": {} }),
        );
        show_by_name.insert(
            "gemma4:12b",
            serde_json::json!({ "capabilities": ["completion"], "details": { "parameter_size": "12B" }, "model_info": {} }),
        );
        show_by_name.insert(
            "deepseek-r1",
            serde_json::json!({ "capabilities": ["completion"], "details": { "parameter_size": "8B" }, "model_info": {} }),
        );
        show_by_name.insert(
            "minimax-m3",
            // A cloud-hosted model with no locally-reported parameter size
            // (Ollama's real `ollama list` shows a bare "-" for `SIZE` in
            // exactly this case) -- must not win by virtue of appearing
            // first or last in discovery order.
            serde_json::json!({ "capabilities": ["completion"], "details": {}, "model_info": {} }),
        );

        let (ollama, _handle) = mock_ollama_tags_with_show(
            serde_json::json!({
                "models": [
                    { "name": "granite4.1" },
                    { "name": "llama3.1" },
                    { "name": "gemma4:12b" },
                    { "name": "deepseek-r1" },
                    { "name": "minimax-m3" },
                ]
            }),
            show_by_name,
        );
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let service = ModelDiscoveryService::new(ollama, registry.clone(), events);

        let written = service.run().unwrap();

        // Exactly one written entry per role is selected, never zero and
        // never more than one.
        for role in [EngineRole::Tutor, EngineRole::Reasoning, EngineRole::Planner] {
            let selected_count = written.iter().filter(|e| e.engine_role == role && e.is_selected_for_role).count();
            assert_eq!(selected_count, 1, "role {role:?} should have exactly one selected model, got {selected_count}");
        }

        // The largest reported model (gemma4:12b, 12B) wins over every
        // 8B competitor and over the model with no reported size at all.
        assert_eq!(
            registry.find_for_role(EngineRole::Tutor).unwrap().unwrap().model_identifier,
            "gemma4:12b"
        );
    }

    #[test]
    fn parameter_size_in_billions_parses_common_ollama_formats() {
        assert_eq!(parameter_size_in_billions(&Some("8B".to_string())), Some(8.0));
        assert_eq!(parameter_size_in_billions(&Some("12b".to_string())), Some(12.0));
        assert_eq!(parameter_size_in_billions(&Some("270M".to_string())), Some(0.27));
        assert_eq!(parameter_size_in_billions(&None), None);
        assert_eq!(parameter_size_in_billions(&Some("".to_string())), None);
        assert_eq!(parameter_size_in_billions(&Some("unknown".to_string())), None);
    }

    #[test]
    fn select_best_for_role_prefers_larger_text_generation_models() {
        let small = DiscoveredModel {
            model_identifier: "small".to_string(),
            capabilities: vec![ModelCapability::TextGeneration],
            context_length: 8192,
            parameter_size: Some("4B".to_string()),
        };
        let large = DiscoveredModel {
            model_identifier: "large".to_string(),
            capabilities: vec![ModelCapability::TextGeneration],
            context_length: 8192,
            parameter_size: Some("70B".to_string()),
        };
        let best = select_best_for_role(EngineRole::Tutor, [&small, &large].into_iter()).unwrap();
        assert_eq!(best.model_identifier, "large");
    }

    #[test]
    fn select_best_for_role_prefers_longer_context_for_embedding() {
        let short_ctx = DiscoveredModel {
            model_identifier: "short-ctx".to_string(),
            capabilities: vec![ModelCapability::Embedding],
            context_length: 2048,
            parameter_size: None,
        };
        let long_ctx = DiscoveredModel {
            model_identifier: "long-ctx".to_string(),
            capabilities: vec![ModelCapability::Embedding],
            context_length: 8192,
            parameter_size: None,
        };
        let best = select_best_for_role(EngineRole::Embedding, [&short_ctx, &long_ctx].into_iter()).unwrap();
        assert_eq!(best.model_identifier, "long-ctx");
    }
}
