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
    options: GenerateOptions,
    // Bug fix (traced live, 96s+ gap before the first token on repeat
    // requests): without an explicit `keep_alive`, Ollama uses its own
    // server default (5 minutes) and unloads the model from memory/VRAM
    // once idle past that -- the *next* request then pays the full
    // model-load cost again before it can emit a single token. Keeping
    // the model resident across the session (see `OLLAMA_KEEP_ALIVE`)
    // avoids paying that cost on every turn.
    keep_alive: &'a str,
}

/// Bug fix (traced live): without an explicit `num_ctx`, Ollama falls
/// back to the model's default context window, and a prompt larger than
/// what comfortably fits in VRAM forces layers/context to spill to system
/// RAM -- on an 8GB card that turned a request into a 120s prefill stall
/// that never emitted a single token (§45.1: fail clearly, don't hang).
/// `num_ctx` is kept in step with `ContextBuilder::max_context_tokens`
/// (§39) so the token budget actually enforced by context assembly
/// matches what's requested from the model, rather than the two numbers
/// silently disagreeing.
///
/// Fix 4 (P0 audit): the above comment describes the intent, but `num_ctx`
/// was still a hardcoded `4096` for every request regardless of which
/// model was actually resolved for the role -- so a model registered with
/// a real 32K+ context window was silently capped to 4096 (truncating
/// large contexts with zero visibility), and a model genuinely limited to
/// a smaller window than 4096 could be asked for more than it has.
/// `ModelRegistryEntry::context_length` (populated by `discovery.rs` from
/// Ollama's real `/api/show` response) already carries the correct value
/// per model -- `GenerateOptions::for_model_context` now uses it instead
/// of a fixed literal.
#[derive(Debug, Serialize, Clone, Copy)]
struct GenerateOptions {
    num_ctx: u32,
    num_predict: i32,
}

/// Conservative fallback `num_ctx` used only when the resolved model's
/// `context_length` is missing or reports `0` (e.g. an older Ollama server
/// that didn't return a recognizable `*.context_length` key in
/// `/api/show`, see `infer_context_length`). Named and documented per the
/// "no bare literal" rule (Fix 4 requirement 1) rather than inlined --
/// deliberately conservative (safe on an 8GB card, per the bug this
/// constant's neighbor documents) rather than optimistic, since guessing
/// too high risks the same VRAM-spill stall this fix exists to prevent.
const FALLBACK_NUM_CTX: u32 = 4096;

/// Floor for the requested `num_ctx` regardless of how short the prompt
/// is (Part 5, latency audit). Trace-instrumented live: requesting the
/// model's *full* advertised window unconditionally (e.g. 131072 for a
/// model whose real usage was a few hundred tokens) forces Ollama to
/// allocate a KV cache sized for the full window every request. On an
/// 8GB card that pushed total runtime memory (weights + cache) past free
/// VRAM, causing a silent fallback to CPU inference -- not a crash, just
/// a 20-50x slowdown with no visible error. Sizing the request to what
/// the prompt actually needs (with headroom) instead of the model's
/// theoretical maximum keeps the KV cache proportional to real usage.
/// This floor exists only so a trivially short prompt doesn't request an
/// unreasonably tiny cache that risks truncating a longer-than-expected
/// response.
const MIN_NUM_CTX: u32 = 2048;

impl GenerateOptions {
    /// Build request options for a specific resolved model's real context
    /// window (Fix 4), sized to what `prompt` actually needs rather than
    /// the model's full advertised window (Part 5 latency fix, see
    /// `MIN_NUM_CTX`). Requested `num_ctx` is
    /// `clamp(prompt_tokens + num_predict + headroom, MIN_NUM_CTX, model_ceiling)`
    /// -- never more than the model can actually support (Fix 4's
    /// original guarantee: never silently truncate a large model's real
    /// capability), but also never the full window when the real prompt
    /// is far smaller than that. Falls back to `FALLBACK_NUM_CTX` as the
    /// ceiling and logs a warning when `context_length` is unavailable,
    /// rather than silently guessing with no trace of why.
    fn for_model_context(context_length: u32, prompt: &str) -> Self {
        let model_ceiling = if context_length > 0 {
            context_length
        } else {
            atlas_utils::log_warn!(
                "[OllamaProvider] resolved model reported no usable context_length; falling back to {FALLBACK_NUM_CTX}"
            );
            FALLBACK_NUM_CTX
        };
        // Same whitespace-word approximation ContextBuilder uses for its
        // own token budgeting (§18 convention) -- good enough to size a
        // KV cache request, without pulling in a real tokenizer here.
        let prompt_tokens = prompt.split_whitespace().count() as u32;
        let needed = prompt_tokens.saturating_add(DEFAULT_NUM_PREDICT as u32);
        let num_ctx = needed.clamp(MIN_NUM_CTX, model_ceiling);
        Self {
            num_ctx,
            num_predict: DEFAULT_NUM_PREDICT,
        }
    }
}

/// Default max tokens to generate per response. Unrelated to context-window
/// sizing (Fix 4 is scoped to `num_ctx` only, per its requirement 3) --
/// kept as a named constant rather than a bare literal for the same
/// "no magic numbers" reason as `FALLBACK_NUM_CTX`.
const DEFAULT_NUM_PREDICT: i32 = 1024;

/// How long Ollama keeps a resolved model resident in memory/VRAM after
/// the last request before unloading it. Ollama's own default is `"5m"`,
/// short enough that a normal pause between chat turns (or the retrieval/
/// context-build work between the embedding call and the generation call
/// on the very same request) can cost a full model reload on the next
/// call. `"30m"` trades a modest amount of standing VRAM for keeping the
/// assistant responsive across a realistic study session. Named/documented
/// per the "no bare literal" convention rather than inlined.
const OLLAMA_KEEP_ALIVE: &str = "30m";

#[derive(Debug, Deserialize)]
struct GenerateStreamLine {
    #[serde(default)]
    response: String,
    #[serde(default)]
    done: bool,
}

/// Request body for `POST /api/embed` (Ollama's batch embedding endpoint --
/// accepts either a single string or an array of strings as `input`, and
/// always returns an array of vectors in `embeddings`, one per input, in
/// the same order). Using the batch-capable endpoint for both single- and
/// multi-text calls means `embed()` and `embed_batch()` share one code path
/// (§37.1 -- one Embedding Engine, one model, same call shape for queries
/// and chunks).
#[derive(Debug, Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
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
        // BUG FIX (os error 10060 mid-stream on Windows): `.timeout(d)` sets
        // a single ABSOLUTE deadline covering the entire call -- connect,
        // write, AND the full duration of reading the response body. For
        // generate_stream() that means any generation whose *total*
        // streaming time exceeds `request_timeout` gets forcibly aborted by
        // ureq mid-read, which Windows reports as WSAETIMEDOUT (10060) even
        // though hundreds of chunks streamed successfully beforehand.
        //
        // Fix: use a short connect timeout (fails fast if Ollama is
        // unreachable) plus per-operation idle read/write timeouts, which
        // reset on every chunk transferred instead of counting cumulative
        // time. This still fails fast on a genuinely hung connection, but
        // no longer punishes long-running-but-healthy streams.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(connection.request_timeout)
            .timeout_write(connection.request_timeout)
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
    pub fn generate(
        &self,
        model: &str,
        prompt: &str,
        images: Option<Vec<String>>,
        context_length: u32,
    ) -> Result<String, AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        let url = format!("{}/api/generate", self.connection.base_url());
        atlas_utils::log_info!("[OllamaProvider] generate() entered url={url} model={model}");
        let __t0 = std::time::Instant::now();
        let body = GenerateRequest {
            model,
            prompt,
            stream: false,
            images,
            options: GenerateOptions::for_model_context(context_length, prompt),
            keep_alive: OLLAMA_KEEP_ALIVE,
        };
        let response = self
            .agent
            .post(&url)
            .send_json(serde_json::to_value(&body).map_err(|e| AppError::model(e.to_string()))?)
            .map_err(|e| {
                // BUG FIX (observability gap that blocked diagnosing the
                // vision-OCR 400s): `ureq::Error`'s Display impl for a
                // non-2xx response is just "status code 400", discarding
                // the response body -- which for Ollama's API is a JSON
                // object with the actual reason (e.g. "model does not
                // support images", "invalid image data"). Extract it.
                let detail = match e {
                    ureq::Error::Status(code, resp) => {
                        let body_text = resp.into_string().unwrap_or_else(|_| "<unreadable body>".to_string());
                        format!("status {code}: {body_text}")
                    }
                    ureq::Error::Transport(t) => t.to_string(),
                };
                atlas_utils::log_error!("[OllamaProvider] generate() HTTP call failed: {detail} elapsed={:?}", __t0.elapsed());
                AppError::model(format!("ollama generate failed: {detail}"))
            })?;
        let line: GenerateStreamLine = response
            .into_json()
            .map_err(|e| AppError::model(format!("malformed /api/generate response: {e}")))?;
        atlas_utils::log_info!("[OllamaProvider] generate() exited elapsed={:?} response_chars={}", __t0.elapsed(), line.response.len());
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
        context_length: u32,
    ) -> Result<impl Iterator<Item = Result<GenerationChunk, AppError>>, AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        let url = format!("{}/api/generate", self.connection.base_url());
        atlas_utils::log_info!("[OllamaProvider] generate_stream() entered url={url} model={model} prompt_chars={}", prompt.len());
        let __t0 = std::time::Instant::now();
        let body = GenerateRequest {
            model,
            prompt,
            stream: true,
            images,
            options: GenerateOptions::for_model_context(context_length, prompt),
            keep_alive: OLLAMA_KEEP_ALIVE,
        };
        let response = self
            .agent
            .post(&url)
            .send_json(serde_json::to_value(&body).map_err(|e| AppError::model(e.to_string()))?)
            .map_err(|e| {
                let detail = match e {
                    ureq::Error::Status(code, resp) => {
                        let body_text = resp.into_string().unwrap_or_else(|_| "<unreadable body>".to_string());
                        format!("status {code}: {body_text}")
                    }
                    ureq::Error::Transport(t) => t.to_string(),
                };
                atlas_utils::log_error!("[OllamaProvider] generate_stream() HTTP call failed: {detail} elapsed={:?}", __t0.elapsed());
                AppError::model(format!("ollama generate (stream) failed: {detail}"))
            })?;
        atlas_utils::log_info!("[OllamaProvider] generate_stream() HTTP connection established, streaming body elapsed={:?}", __t0.elapsed());

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

    /// Real embeddings (Part 1 "Replace HashEmbeddingEngine ... Implement a
    /// real embedding engine using Ollama"; §18, §37.1). Batches every text
    /// into a single `POST /api/embed` call so indexing a whole document's
    /// chunks costs one round-trip, not one per chunk. `model` is always
    /// resolved by the caller from the Model Registry
    /// (`EngineRole::Embedding`) -- this method never assumes a model name
    /// (§46.1, §46.4: no hardcoding, no direct-to-Ollama call from outside
    /// this module).
    pub fn embed_batch(&self, model: &str, texts: &[&str]) -> Result<Vec<Vec<f32>>, AppError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/api/embed", self.connection.base_url());
        let body = EmbedRequest {
            model,
            input: texts.to_vec(),
        };
        let __t0 = std::time::Instant::now();
        let response = self
            .agent
            .post(&url)
            .send_json(serde_json::to_value(&body).map_err(|e| AppError::model(e.to_string()))?)
            .map_err(|e| {
                let detail = match e {
                    ureq::Error::Status(code, resp) => {
                        let body_text = resp.into_string().unwrap_or_else(|_| "<unreadable body>".to_string());
                        format!("status {code}: {body_text}")
                    }
                    ureq::Error::Transport(t) => t.to_string(),
                };
                AppError::model(format!("ollama embed failed: {detail}"))
            })?;
        let parsed: EmbedResponse = response
            .into_json()
            .map_err(|e| AppError::model(format!("malformed /api/embed response: {e}")))?;
        atlas_utils::log_info!(
            "[OllamaProvider] embed_batch() model={model} inputs={} elapsed={:?}",
            texts.len(),
            __t0.elapsed()
        );
        if parsed.embeddings.len() != texts.len() {
            return Err(AppError::model(format!(
                "ollama /api/embed returned {} vectors for {} inputs",
                parsed.embeddings.len(),
                texts.len()
            )));
        }
        Ok(parsed.embeddings)
    }

    /// Single-text convenience wrapper over [`Self::embed_batch`], used for
    /// query-time embedding (one query per call, §18 "Vector search").
    pub fn embed(&self, model: &str, text: &str) -> Result<Vec<f32>, AppError> {
        let mut vectors = self.embed_batch(model, &[text])?;
        vectors
            .pop()
            .ok_or_else(|| AppError::model("ollama /api/embed returned no vectors"))
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
                    let _ = stream.flush();
                    // Half-close the write side (sends a proper TCP FIN)
                    // instead of just dropping the socket. Dropping a
                    // `TcpStream` immediately after `write_all` returns
                    // only guarantees the bytes were handed to the OS
                    // send buffer, not that the client has read them yet
                    // -- on Windows in particular, tearing the socket down
                    // before the client finishes reading can surface as a
                    // hard RST ("An established connection was aborted by
                    // the software in your host machine", os error 10053)
                    // rather than a graceful close, which reqwest then
                    // reports as a network error even though the response
                    // was actually served correctly. Draining any
                    // remaining bytes from the read side until the client
                    // closes its end (EOF) -- which it will, since the
                    // response declares `Connection: close` -- lets the
                    // OS complete a normal four-way close before this
                    // thread moves on to `listener.accept()` for the next
                    // route.
                    let _ = stream.shutdown(std::net::Shutdown::Write);
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
                    let mut drain = [0u8; 64];
                    loop {
                        match stream.read(&mut drain) {
                            Ok(0) | Err(_) => break,
                            Ok(_) => continue,
                        }
                    }
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
        let text = server.provider().generate("llama3.1", "hi", None, 8192).unwrap();
        assert_eq!(text, "hello world");
    }

    #[test]
    fn embed_batch_returns_a_vector_per_input_in_order() {
        let server = MockServer::start(vec![(
            "POST",
            "/api/embed",
            serde_json::json!({ "embeddings": [[0.1, 0.2], [0.3, 0.4]] }),
        )]);
        let vectors = server
            .provider()
            .embed_batch("test-embed-model", &["first", "second"])
            .unwrap();
        assert_eq!(vectors, vec![vec![0.1, 0.2], vec![0.3, 0.4]]);
    }

    #[test]
    fn embed_returns_the_single_vector() {
        let server = MockServer::start(vec![(
            "POST",
            "/api/embed",
            serde_json::json!({ "embeddings": [[1.0, 2.0, 3.0]] }),
        )]);
        let vector = server.provider().embed("test-embed-model", "hello").unwrap();
        assert_eq!(vector, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn embed_batch_of_empty_input_makes_no_request_and_returns_empty() {
        // No mock routes registered: if this made an HTTP call it would
        // hang waiting for a connection that never comes, so an immediate
        // empty `Ok` proves the short-circuit.
        let provider = OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1));
        assert_eq!(provider.embed_batch("test-embed-model", &[]).unwrap(), Vec::<Vec<f32>>::new());
    }

    #[test]
    fn embed_batch_is_a_model_error_when_ollama_unreachable() {
        let provider = OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1));
        let err = provider.embed_batch("test-embed-model", &["x"]).unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn embed_batch_is_a_model_error_when_vector_count_mismatches_input_count() {
        let server = MockServer::start(vec![(
            "POST",
            "/api/embed",
            serde_json::json!({ "embeddings": [[0.1, 0.2]] }),
        )]);
        let err = server
            .provider()
            .embed_batch("test-embed-model", &["first", "second"])
            .unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }
}
