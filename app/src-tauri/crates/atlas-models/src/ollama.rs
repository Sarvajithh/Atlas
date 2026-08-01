//! Ollama Provider (§5 "Inference: Ollama (local only)"; §31 "the app
//! detects it at runtime and guides the user to install/start it if
//! missing"; §37.1 "Maintain the set of models discoverable from the local
//! Ollama instance").
//!
//! This is the *only* place an HTTP call to Ollama is made. Nothing above
//! this module (Engines, Scheduler, IPC) talks to Ollama directly (§46.4).
//! Everything configurable here (host/port) comes from `SettingsProvider`
//! (§23), never a hardcoded endpoint (Governing Principle, §46.1).

use std::io::{BufRead, BufReader};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use atlas_utils::AppError;

/// Connection settings for the local Ollama instance (§23 "Ollama
/// connection settings (host/port, defaults to local instance)"). Callers
/// build this from `SettingsProvider`; this module never hardcodes a host.
#[derive(Debug, Clone)]
pub struct OllamaConnection {
    pub host: String,
    pub port: u16,
    pub request_timeout: Duration,
}

impl OllamaConnection {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            request_timeout: Duration::from_secs(120),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

/// Raw model listing entry from `GET /api/tags`.
#[derive(Debug, Clone, Deserialize)]
struct OllamaTagEntry {
    name: String,
    #[serde(default)]
    details: Option<OllamaTagDetails>,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaTagDetails {
    #[serde(default)]
    families: Option<Vec<String>>,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    parameter_size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagEntry>,
}

/// Raw `POST /api/show` response, used to derive capabilities and context
/// length without ever asking the user to name a model (§37.1).
#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    model_info: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    details: Option<OllamaTagDetails>,
}

/// A discovered model, capability-tagged, before it is written into the
/// Model Registry (§33.13). This is the boundary type between "raw thing
/// Ollama reports" and "thing the Model Registry stores" -- capability
/// inference happens here, once, so downstream code never re-derives it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    pub model_identifier: String,
    pub capabilities: Vec<ModelCapability>,
    pub context_length: u32,
    pub parameter_size: Option<String>,
}

/// Capabilities a discovered model may have. This is a superset filter --
/// the Model Scheduler/Registry map [`atlas_types::model::EngineRole`] onto
/// models that have the matching capability (§37.2, §14.1), never onto a
/// model by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelCapability {
    TextGeneration,
    Vision,
    Embedding,
    Tools,
}

/// One streamed token/fragment from `/api/generate` or `/api/chat`.
#[derive(Debug, Clone)]
pub struct GenerationChunk {
    pub content: String,
    pub done: bool,
}

#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct GenerateStreamLine {
    #[serde(default)]
    response: String,
    #[serde(default)]
    done: bool,
}

/// The Ollama Provider itself. Owns the one HTTP client in the system that
/// is allowed to reach Ollama (§46.4). Every method is a thin, defensive
/// wrapper: network/parse failures become `AppError::model` (§45.1 "Model
/// Errors"), never a panic and never a silently fabricated result.
pub struct OllamaProvider {
    connection: OllamaConnection,
    agent: ureq::Agent,
}

impl OllamaProvider {
    pub fn new(connection: OllamaConnection) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(connection.request_timeout)
            .build();
        Self { connection, agent }
    }

    /// Model Discovery (§37.1, §41 step 5): list every model the local
    /// Ollama instance currently has pulled, with inferred capabilities.
    /// Returns `AppError::model` (not a panic, not an empty silent list)
    /// when Ollama is unreachable, so callers can publish
    /// `ModelUnavailable` (§34.2) rather than proceed as if nothing were
    /// wrong.
    pub fn discover_models(&self) -> Result<Vec<DiscoveredModel>, AppError> {
        let url = format!("{}/api/tags", self.connection.base_url());
        let response: OllamaTagsResponse = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| AppError::model(format!("ollama unreachable at {url}: {e}")))?
            .into_json()
            .map_err(|e| AppError::model(format!("malformed /api/tags response: {e}")))?;

        response
            .models
            .into_iter()
            .map(|entry| self.describe_model(entry))
            .collect()
    }

    /// Resolve full capability/context-length detail for one already-listed
    /// model via `POST /api/show` (§37.1 "context length, VRAM
    /// requirement... version, supported tasks").
    fn describe_model(&self, entry: OllamaTagEntry) -> Result<DiscoveredModel, AppError> {
        let url = format!("{}/api/show", self.connection.base_url());
        let show: OllamaShowResponse = self
            .agent
            .post(&url)
            .send_json(serde_json::json!({ "model": entry.name }))
            .map_err(|e| AppError::model(format!("ollama /api/show failed for {}: {e}", entry.name)))?
            .into_json()
            .map_err(|e| AppError::model(format!("malformed /api/show response: {e}")))?;

        let details = show.details.or(entry.details);
        let capabilities = infer_capabilities(&show.capabilities, &details);
        let context_length = infer_context_length(&show.model_info);
        let parameter_size = details.and_then(|d| d.parameter_size);

        Ok(DiscoveredModel {
            model_identifier: entry.name,
            capabilities,
            context_length,
            parameter_size,
        })
    }

    /// Non-streaming generation, used where the caller needs the full
    /// answer before proceeding (e.g. quiz/flashcard generation composing
    /// structured output from the Reasoning Engine).
    pub fn generate(&self, model: &str, prompt: &str, images: Option<Vec<String>>) -> Result<String, AppError> {
        let url = format!("{}/api/generate", self.connection.base_url());
        let body = GenerateRequest {
            model,
            prompt,
            stream: false,
            images,
        };
        let line: GenerateStreamLine = self
            .agent
            .post(&url)
            .send_json(serde_json::to_value(&body).map_err(|e| AppError::model(e.to_string()))?)
            .map_err(|e| AppError::model(format!("ollama generate failed: {e}")))?
            .into_json()
            .map_err(|e| AppError::model(format!("malformed /api/generate response: {e}")))?;
        Ok(line.response)
    }

    /// Streaming generation (§12 "Long-running operations... use Tauri's
    /// event system to stream progress/tokens back to the frontend"). This
    /// method itself is transport-agnostic: it returns an iterator of
    /// [`GenerationChunk`]s; `app-tauri`'s IPC layer is what forwards each
    /// chunk onward as an `assistant://answer-stream` event.
    pub fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        images: Option<Vec<String>>,
    ) -> Result<impl Iterator<Item = Result<GenerationChunk, AppError>>, AppError> {
        let url = format!("{}/api/generate", self.connection.base_url());
        let body = GenerateRequest {
            model,
            prompt,
            stream: true,
            images,
        };
        let response = self
            .agent
            .post(&url)
            .send_json(serde_json::to_value(&body).map_err(|e| AppError::model(e.to_string()))?)
            .map_err(|e| AppError::model(format!("ollama generate (stream) failed: {e}")))?;

        let reader = BufReader::new(response.into_reader());
        Ok(reader.lines().filter_map(|line| {
            let line = match line {
                Ok(l) if l.trim().is_empty() => return None,
                Ok(l) => l,
                Err(e) => return Some(Err(AppError::model(format!("stream read error: {e}")))),
            };
            match serde_json::from_str::<GenerateStreamLine>(&line) {
                Ok(parsed) => Some(Ok(GenerationChunk {
                    content: parsed.response,
                    done: parsed.done,
                })),
                Err(e) => Some(Err(AppError::model(format!("malformed stream line: {e}")))),
            }
        }))
    }
}

/// Infer capabilities from `/api/show`'s reported `capabilities` list where
/// present, falling back to family-name heuristics for older Ollama
/// versions that don't report `capabilities` at all. Every model is at
/// minimum assumed capable of text generation, since Ollama refuses to load
/// anything that can't do that.
fn infer_capabilities(reported: &[String], details: &Option<OllamaTagDetails>) -> Vec<ModelCapability> {
    let mut caps = vec![ModelCapability::TextGeneration];

    let reported_lower: Vec<String> = reported.iter().map(|s| s.to_lowercase()).collect();
    if reported_lower.iter().any(|c| c == "vision") {
        caps.push(ModelCapability::Vision);
    }
    if reported_lower.iter().any(|c| c == "embedding") {
        caps.push(ModelCapability::Embedding);
    }
    if reported_lower.iter().any(|c| c == "tools") {
        caps.push(ModelCapability::Tools);
    }

    // Fallback heuristics from family metadata, only for capabilities not
    // already confirmed above (older Ollama servers omit `capabilities`).
    if let Some(details) = details {
        let family_terms: Vec<String> = details
            .family
            .iter()
            .cloned()
            .chain(details.families.clone().unwrap_or_default())
            .map(|f| f.to_lowercase())
            .collect();
        let is_vision_family = family_terms
            .iter()
            .any(|f| ["clip", "vit", "llava", "vision", "mllama"].iter().any(|k| f.contains(k)));
        if is_vision_family && !caps.contains(&ModelCapability::Vision) {
            caps.push(ModelCapability::Vision);
        }
        let is_embedding_family = family_terms.iter().any(|f| f.contains("bert") || f.contains("embed"));
        if is_embedding_family && !caps.contains(&ModelCapability::Embedding) {
            caps.push(ModelCapability::Embedding);
        }
    }

    caps
}

fn infer_context_length(model_info: &serde_json::Map<String, serde_json::Value>) -> u32 {
    // Ollama reports this under a family-prefixed key, e.g.
    // "llama.context_length". Scan for any key ending in
    // "context_length" rather than hardcoding a family prefix.
    model_info
        .iter()
        .find(|(k, _)| k.ends_with("context_length"))
        .and_then(|(_, v)| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(4096)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A minimal single-connection-at-a-time HTTP/1.1 mock server, used
    /// instead of a crates.io mocking library so these tests don't drag in
    /// a hyper/tokio dependency chain (that chain requires a newer
    /// toolchain than this container's disclosed Rust 1.75 -- see README
    /// "Known Environment Limitations"; the pattern here has no such
    /// requirement anywhere). Routes are matched by HTTP method + path
    /// prefix, served in order, one response body (JSON) per registered
    /// route, then the server thread exits.
    struct MockServer {
        port: u16,
    }

    impl MockServer {
        fn start(routes: Vec<(&'static str, &'static str, serde_json::Value)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            std::thread::spawn(move || {
                for (_method, _path, body) in routes {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf); // drain the request
                    let payload = body.to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            Self { port }
        }

        fn provider(&self) -> OllamaProvider {
            OllamaProvider::new(OllamaConnection::new("127.0.0.1", self.port))
        }
    }

    #[test]
    fn discover_models_returns_empty_when_ollama_reports_no_models() {
        let server = MockServer::start(vec![("GET", "/api/tags", serde_json::json!({ "models": [] }))]);
        assert_eq!(server.provider().discover_models().unwrap().len(), 0);
    }

    #[test]
    fn discover_models_infers_vision_capability_from_reported_capabilities() {
        let server = MockServer::start(vec![
            (
                "GET",
                "/api/tags",
                serde_json::json!({ "models": [{ "name": "llava:7b" }] }),
            ),
            (
                "POST",
                "/api/show",
                serde_json::json!({
                    "capabilities": ["completion", "vision"],
                    "model_info": { "llama.context_length": 4096 },
                }),
            ),
        ]);
        let models = server.provider().discover_models().unwrap();
        assert_eq!(models.len(), 1);
        assert!(models[0].capabilities.contains(&ModelCapability::Vision));
        assert_eq!(models[0].context_length, 4096);
    }

    #[test]
    fn discover_models_defaults_context_length_when_absent() {
        let server = MockServer::start(vec![
            (
                "GET",
                "/api/tags",
                serde_json::json!({ "models": [{ "name": "llama3.1" }] }),
            ),
            (
                "POST",
                "/api/show",
                serde_json::json!({ "capabilities": ["completion"], "model_info": {} }),
            ),
        ]);
        let models = server.provider().discover_models().unwrap();
        assert_eq!(models[0].context_length, 4096);
        assert!(!models[0].capabilities.contains(&ModelCapability::Vision));
    }

    #[test]
    fn discover_models_is_a_model_error_when_ollama_unreachable() {
        // Port 1 is reserved/unroutable, so this deterministically fails
        // fast without needing an actual dead server (§45.1 Model Errors).
        let provider = OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1));
        let err = provider.discover_models().unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn generate_returns_the_full_response_text() {
        let server = MockServer::start(vec![(
            "POST",
            "/api/generate",
            serde_json::json!({ "response": "hello world", "done": true }),
        )]);
        let text = server.provider().generate("llama3.1", "hi", None).unwrap();
        assert_eq!(text, "hello world");
    }
}
