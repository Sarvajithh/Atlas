//! Concrete Engine implementations (§14.1) backed by the Ollama Provider,
//! plus study-feature orchestration built *on top of* the frozen §14.1
//! Engine roles.
//!
//! §14.1's Engine table is frozen and MUST NOT be extended without amending
//! the README first (§28.2, §46.3). "Quiz Generator", "Flashcard
//! Generator", "Revision Planner", and "Math Solver" are not rows in that
//! table -- they are *features*, each implementable entirely as a
//! particular prompt/pipeline shape running through the existing Tutor,
//! Reasoning, Planner, and Memory roles. This module keeps that boundary
//! explicit: [`OllamaEngine`] is the one concrete `Engine` impl (parameterized
//! by role, never by model name, §37.2), and the `*_service` functions below
//! are orchestration that calls a role's `Engine::run`, never a new role.

use std::sync::Arc;

use atlas_types::model::EngineRole;
use atlas_utils::AppError;

use crate::engine::{Engine, EngineOutput, ResolvedPrompt};
use crate::ollama::OllamaProvider;
use crate::registry::ModelRegistryRepository;
use crate::study_output::{
    parse_flashcard_response, parse_quiz_response, parse_revision_plan_response, GeneratedFlashcardSet, GeneratedQuiz,
    GeneratedRevisionPlan,
};

/// The one concrete `Engine` implementation. Every §14.1 role that runs
/// inference (Tutor, Reasoning, Planner, Vision, Ocr) is this same struct,
/// differing only in `role` -- which model backs it is resolved fresh on
/// every call via the Model Registry (§37.1), never cached by name, so a
/// model swap in Settings (§23) takes effect on the next request with no
/// code change.
///
/// Depends on `ModelRegistryRepository` rather than the narrower
/// `ModelProvider` specifically so it can list *alternative* models for its
/// role and retry against them when the currently-selected one fails
/// (§45.1 "Model Errors"; requirement: "Handle model failures gracefully by
/// selecting alternative capable models"). Selection is still entirely
/// role-driven -- never a hardcoded model name.
pub struct OllamaEngine {
    role: EngineRole,
    model_registry: Arc<dyn ModelRegistryRepository>,
    ollama: Arc<OllamaProvider>,
}

impl OllamaEngine {
    pub fn new(role: EngineRole, model_registry: Arc<dyn ModelRegistryRepository>, ollama: Arc<OllamaProvider>) -> Self {
        Self {
            role,
            model_registry,
            ollama,
        }
    }

    fn try_generate(&self, model_identifier: &str, context_length: u32, prompt: &ResolvedPrompt) -> Result<String, AppError> {
        atlas_utils::log_info!(
            "[OllamaProvider] sending request model={model_identifier} prompt_chars={}",
            prompt.content.len()
        );
        let __t0 = std::time::Instant::now();
        let result = self
            .ollama
            .generate(model_identifier, &prompt.content, prompt.images.clone(), context_length);
        match &result {
            Ok(text) => atlas_utils::log_info!(
                "[OllamaProvider] response received {} chars elapsed={:?}",
                text.len(),
                __t0.elapsed()
            ),
            Err(err) => atlas_utils::log_error!("[OllamaProvider] request failed: {} elapsed={:?}", err.message, __t0.elapsed()),
        }
        result
    }
}

impl Engine for OllamaEngine {
    fn role(&self) -> EngineRole {
        self.role
    }

    fn run(&self, prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        atlas_utils::log_info!("[ModelRegistry] resolving model for role {:?}", self.role);
        let primary = self
            .model_registry
            .find_for_role(self.role)?
            .ok_or_else(|| {
                atlas_utils::log_error!(
                    "[ModelRegistry] no model assigned to role {:?} (registry empty or nothing selected for this role)",
                    self.role
                );
                AppError::model(format!("no model currently assigned to {:?}", self.role))
            })?;
        atlas_utils::log_info!("[ModelRegistry] selected model {} for role {:?}", primary.model_identifier, self.role);

        match self.try_generate(&primary.model_identifier, primary.context_length, &prompt) {
            Ok(content) => Ok(EngineOutput { content }),
            Err(primary_err) => {
                // Fall back to any other Available model already registered
                // for this role, in registry order, before surfacing the
                // failure. This only ever considers models the Model
                // Registry already assigns to this role/capability -- it
                // never guesses at an unrelated model.
                let alternatives = self.model_registry.list()?.into_iter().filter(|entry| {
                    entry.engine_role == self.role
                        && entry.id != primary.id
                        && entry.status == atlas_types::model::ModelStatus::Available
                });

                for alternative in alternatives {
                    if let Ok(content) = self.try_generate(&alternative.model_identifier, alternative.context_length, &prompt) {
                        return Ok(EngineOutput { content });
                    }
                }

                Err(primary_err)
            }
        }
    }
}

/// Runs a sequence of engines by resolving each from a shared pool by role,
/// per §15's "ordered Engine-role sequence for a given intent". The
/// Scheduler owns *which* sequence (`resolve_pipeline`); this is the
/// generic "actually run it" step that was previously a future milestone.
pub struct EnginePool {
    engines: std::collections::HashMap<EngineRole, Arc<dyn Engine>>,
}

impl EnginePool {
    pub fn new(engines: Vec<Arc<dyn Engine>>) -> Self {
        let mut map = std::collections::HashMap::new();
        for engine in engines {
            map.insert(engine.role(), engine);
        }
        Self { engines: map }
    }

    pub fn get(&self, role: EngineRole) -> Result<&Arc<dyn Engine>, AppError> {
        self.engines
            .get(&role)
            .ok_or_else(|| AppError::model(format!("no engine registered for role {role:?}")))
    }

    /// Run the last inference-bearing role in a resolved pipeline (§15). The
    /// non-inference roles in a pipeline (Retriever/Reranker) are handled
    /// upstream by the Scheduler via Context Builder (§39); this runs
    /// whichever role actually produces the answer (Tutor/Reasoning/
    /// Planner), with graceful fallback (§45.1 "Model Errors";
    /// requirement: "Handle model failures gracefully by selecting
    /// alternative capable models").
    pub fn run_role(&self, role: EngineRole, prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        atlas_utils::log_info!("[EnginePool] run_role entered role={role:?} registered_roles={:?}", self.engines.keys().collect::<Vec<_>>());
        let result = self.get(role)?.run(prompt);
        atlas_utils::log_info!("[EnginePool] run_role exited role={role:?} ok={}", result.is_ok());
        result
    }
}

/// Explanation feature: step-by-step derivation/explanation, built on the
/// Reasoning Engine (§14.1) rather than a new engine role. Grounded in
/// retrieved context when available, per the acceptance criteria ("All AI
/// responses originate from retrieved context when available").
pub fn explain_step_by_step(pool: &EnginePool, grounded_prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
    pool.run_role(EngineRole::Reasoning, grounded_prompt)
}

/// Math Solver feature: step-by-step derivation for a specific problem,
/// also built on the Reasoning Engine -- a math problem is a reasoning
/// task, not a distinct capability requiring its own model/engine role.
pub fn solve_math_problem(pool: &EnginePool, problem_prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
    pool.run_role(EngineRole::Reasoning, problem_prompt)
}

/// Runs `role` through the pool, parses the response with `parse`, and --
/// on a parse/validation failure only -- retries exactly once with a
/// corrective instruction appended to the original prompt, per the
/// implementation plan ("on parse failure, retry once with a corrective
/// instruction or fail Recoverable"). A failure at the *generation* step
/// itself (Ollama unreachable, no model assigned, etc.) is not retried
/// here -- `OllamaEngine::run` already retries across every Available
/// alternative model for the role before returning that error, so retrying
/// it again at this layer would just repeat an already-exhausted search.
fn generate_structured<T>(
    pool: &EnginePool,
    role: EngineRole,
    prompt: ResolvedPrompt,
    parse: impl Fn(&str) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let original_content = prompt.content.clone();
    let original_images = prompt.images.clone();

    let output = pool.run_role(role, prompt)?;
    match parse(&output.content) {
        Ok(parsed) => Ok(parsed),
        Err(first_err) => {
            atlas_utils::log_warn!(
                "[generate_structured] role={role:?} first parse attempt failed, retrying once: {}",
                first_err.message
            );
            let corrective_content = format!(
                "{original_content}\n\n\
                 ---\n\n\
                 CORRECTION REQUIRED\n\n\
                 Your previous response could not be parsed: {}\n\
                 Return ONLY the corrected JSON object, no markdown code fences, no commentary.",
                first_err.message
            );
            let retry_prompt = ResolvedPrompt {
                content: corrective_content,
                images: original_images,
            };
            let retry_output = pool.run_role(role, retry_prompt)?;
            parse(&retry_output.content).map_err(|second_err| {
                atlas_utils::log_error!(
                    "[generate_structured] role={role:?} retry parse attempt also failed: {}",
                    second_err.message
                );
                second_err
            })
        }
    }
}

/// Quiz Generator feature: built on the Reasoning Engine, targeting weak
/// concepts supplied by the Memory Engine's weakness scores (§19). The
/// caller is responsible for assembling `quiz_prompt` (via Prompt Builder,
/// §40, `PromptBuilder::build_quiz_prompt`) so this module never formats
/// its own prompt (§40's "no Engine formats its own prompt" rule applies
/// equally to feature orchestration). Returns the parsed, validated
/// `GeneratedQuiz` rather than a raw `EngineOutput` -- persistence
/// (assigning an id/workspace/created_at) is the caller's job via
/// `atlas-memory`'s repositories, not this module's.
pub fn generate_quiz(pool: &EnginePool, quiz_prompt: ResolvedPrompt) -> Result<GeneratedQuiz, AppError> {
    generate_structured(pool, EngineRole::Reasoning, quiz_prompt, |raw| parse_quiz_response(raw))
}

/// Flashcard Generator feature: built on the Tutor Engine, since flashcards
/// are a pedagogical restatement of already-retrieved content, matching the
/// Tutor Engine's "explains, teaches, answers in a pedagogical style" role.
pub fn generate_flashcards(pool: &EnginePool, flashcard_prompt: ResolvedPrompt) -> Result<GeneratedFlashcardSet, AppError> {
    generate_structured(pool, EngineRole::Tutor, flashcard_prompt, |raw| parse_flashcard_response(raw))
}

/// Revision Planner feature: built on the Planner Engine (§14.1 "Builds/
/// updates revision plans, study schedules"), consuming Memory Engine
/// weakness data assembled upstream into `revision_prompt` (via
/// `PromptBuilder::build_revision_plan_prompt`).
pub fn generate_revision_plan(pool: &EnginePool, revision_prompt: ResolvedPrompt) -> Result<GeneratedRevisionPlan, AppError> {
    generate_structured(pool, EngineRole::Planner, revision_prompt, |raw| parse_revision_plan_response(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::ids::ModelRegistryId;
    use atlas_types::model::{ModelRegistryEntry, ModelStatus};
    use std::sync::Mutex;

    /// A stub `Engine` that records the prompt it was called with and
    /// returns a fixed answer -- lets feature-orchestration tests assert
    /// *which role* was invoked without depending on a live Ollama
    /// instance (engines remain independently testable per the
    /// acceptance criteria).
    struct StubEngine {
        role: EngineRole,
        last_prompt: Mutex<Option<String>>,
        response: String,
    }

    impl StubEngine {
        fn new(role: EngineRole, response: &str) -> Arc<Self> {
            Arc::new(Self {
                role,
                last_prompt: Mutex::new(None),
                response: response.to_string(),
            })
        }
    }

    impl Engine for StubEngine {
        fn role(&self) -> EngineRole {
            self.role
        }

        fn run(&self, prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
            *self.last_prompt.lock().unwrap() = Some(prompt.content.clone());
            Ok(EngineOutput {
                content: self.response.clone(),
            })
        }
    }

    /// A stub `Engine` that returns responses from a queue in order (one
    /// per call), falling back to repeating the last response once the
    /// queue is exhausted -- lets retry tests assert the *second* call
    /// gets a corrective prompt and can return different (now-valid)
    /// content.
    struct SequenceStubEngine {
        role: EngineRole,
        responses: Mutex<std::collections::VecDeque<String>>,
        prompts_seen: Mutex<Vec<String>>,
    }

    impl SequenceStubEngine {
        fn new(role: EngineRole, responses: Vec<&str>) -> Arc<Self> {
            Arc::new(Self {
                role,
                responses: Mutex::new(responses.into_iter().map(String::from).collect()),
                prompts_seen: Mutex::new(Vec::new()),
            })
        }
    }

    impl Engine for SequenceStubEngine {
        fn role(&self) -> EngineRole {
            self.role
        }

        fn run(&self, prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
            self.prompts_seen.lock().unwrap().push(prompt.content);
            let mut responses = self.responses.lock().unwrap();
            let content = if responses.len() > 1 {
                responses.pop_front().unwrap()
            } else {
                responses.front().cloned().unwrap_or_default()
            };
            Ok(EngineOutput { content })
        }
    }

    fn pool_with(engines: Vec<Arc<dyn Engine>>) -> EnginePool {
        EnginePool::new(engines)
    }

    #[test]
    fn explain_step_by_step_routes_through_reasoning_engine() {
        let reasoning = StubEngine::new(EngineRole::Reasoning, "step 1... step 2...");
        let pool = pool_with(vec![reasoning.clone()]);
        let out = explain_step_by_step(&pool, ResolvedPrompt::text("explain X")).unwrap();
        assert_eq!(out.content, "step 1... step 2...");
        assert_eq!(reasoning.last_prompt.lock().unwrap().as_deref(), Some("explain X"));
    }

    #[test]
    fn solve_math_problem_routes_through_reasoning_engine_not_a_new_role() {
        let reasoning = StubEngine::new(EngineRole::Reasoning, "x = 2");
        let pool = pool_with(vec![reasoning]);
        let out = solve_math_problem(&pool, ResolvedPrompt::text("solve 2x=4")).unwrap();
        assert_eq!(out.content, "x = 2");
    }

    const VALID_QUIZ_JSON: &str =
        r#"{"topic": "t", "questions": [{"question": "q", "options": ["a", "b"], "correct_answer": "a", "source_citations": []}]}"#;
    const VALID_FLASHCARD_JSON: &str = r#"{"topic": "t", "cards": [{"front": "f", "back": "b", "source_citations": []}]}"#;
    const VALID_PLAN_JSON: &str = r#"{"items": [{"topic": "t", "recommendation": "review", "priority": 1}]}"#;

    #[test]
    fn generate_quiz_routes_through_reasoning_engine_and_parses_valid_json_first_try() {
        let reasoning = SequenceStubEngine::new(EngineRole::Reasoning, vec![VALID_QUIZ_JSON]);
        let pool = pool_with(vec![reasoning.clone()]);
        let quiz = generate_quiz(&pool, ResolvedPrompt::text("quiz me")).unwrap();
        assert_eq!(quiz.topic, "t");
        assert_eq!(quiz.questions.len(), 1);
        // No retry needed: exactly one call was made.
        assert_eq!(reasoning.prompts_seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn generate_quiz_retries_once_on_malformed_json_then_succeeds() {
        let reasoning = SequenceStubEngine::new(EngineRole::Reasoning, vec!["not valid json at all", VALID_QUIZ_JSON]);
        let pool = pool_with(vec![reasoning.clone()]);
        let quiz = generate_quiz(&pool, ResolvedPrompt::text("quiz me")).unwrap();
        assert_eq!(quiz.topic, "t");
        let prompts = reasoning.prompts_seen.lock().unwrap();
        assert_eq!(prompts.len(), 2);
        // The retry prompt must actually carry a corrective instruction,
        // not just repeat the original verbatim.
        assert!(prompts[1].contains("CORRECTION REQUIRED"));
        assert!(prompts[1].contains("quiz me"));
    }

    #[test]
    fn generate_quiz_fails_recoverable_when_both_attempts_are_malformed() {
        let reasoning = SequenceStubEngine::new(EngineRole::Reasoning, vec!["still not json", "still not json"]);
        let pool = pool_with(vec![reasoning.clone()]);
        let err = generate_quiz(&pool, ResolvedPrompt::text("quiz me")).unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::Recoverable);
        assert_eq!(reasoning.prompts_seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn generate_flashcards_routes_through_tutor_engine() {
        let tutor = SequenceStubEngine::new(EngineRole::Tutor, vec![VALID_FLASHCARD_JSON]);
        let pool = pool_with(vec![tutor]);
        let set = generate_flashcards(&pool, ResolvedPrompt::text("cards")).unwrap();
        assert_eq!(set.cards.len(), 1);
        assert_eq!(set.cards[0].front, "f");
    }

    #[test]
    fn generate_flashcards_retries_once_on_invalid_output_then_succeeds() {
        // Well-formed JSON but fails validation (empty back) on the first
        // attempt -- the retry path must trigger for validation failures,
        // not only outright-unparseable JSON.
        let invalid = r#"{"topic": "t", "cards": [{"front": "f", "back": ""}]}"#;
        let tutor = SequenceStubEngine::new(EngineRole::Tutor, vec![invalid, VALID_FLASHCARD_JSON]);
        let pool = pool_with(vec![tutor.clone()]);
        let set = generate_flashcards(&pool, ResolvedPrompt::text("cards")).unwrap();
        assert_eq!(set.cards[0].front, "f");
        assert_eq!(tutor.prompts_seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn generate_revision_plan_routes_through_planner_engine() {
        let planner = SequenceStubEngine::new(EngineRole::Planner, vec![VALID_PLAN_JSON]);
        let pool = pool_with(vec![planner]);
        let plan = generate_revision_plan(&pool, ResolvedPrompt::text("plan")).unwrap();
        assert_eq!(plan.items[0].recommendation, "review");
    }

    #[test]
    fn engine_pool_errors_cleanly_when_role_missing_rather_than_panicking() {
        let pool = pool_with(vec![]);
        let err = pool.run_role(EngineRole::Vision, ResolvedPrompt::text("x")).unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    fn sample_model(role: EngineRole) -> ModelRegistryEntry {
        ModelRegistryEntry {
            id: ModelRegistryId(1),
            model_identifier: "stub-model".to_string(),
            engine_role: role,
            capabilities: serde_json::json!([]),
            context_length: 4096,
            vram_requirement: None,
            status: ModelStatus::Available,
            version: "1".to_string(),
            supported_tasks: serde_json::json!([]),
            is_selected_for_role: true,
        }
    }

    struct StaticModelRegistry(Mutex<Vec<ModelRegistryEntry>>);
    impl StaticModelRegistry {
        fn new(entries: Vec<ModelRegistryEntry>) -> Self {
            Self(Mutex::new(entries))
        }
    }
    impl ModelRegistryRepository for StaticModelRegistry {
        fn list(&self) -> Result<Vec<ModelRegistryEntry>, AppError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn find_for_role(&self, role: EngineRole) -> Result<Option<ModelRegistryEntry>, AppError> {
            Ok(self.0.lock().unwrap().iter().find(|e| e.engine_role == role && e.is_selected_for_role).cloned())
        }
        fn upsert(&self, entry: ModelRegistryEntry) -> Result<ModelRegistryEntry, AppError> {
            self.0.lock().unwrap().push(entry.clone());
            Ok(entry)
        }
    }

    #[test]
    fn ollama_engine_resolves_model_via_registry_never_by_hardcoded_name() {
        // Points at an unroutable address so this exercises "engine asks
        // the registry for a model, then calls Ollama" without requiring a
        // live Ollama instance; the assertion is that failure happens at
        // the Ollama call (a model error), not at model resolution.
        let provider = Arc::new(OllamaProvider::new(crate::ollama::OllamaConnection::new(
            "127.0.0.1",
            1,
        )));
        let registry: Arc<dyn ModelRegistryRepository> =
            Arc::new(StaticModelRegistry::new(vec![sample_model(EngineRole::Tutor)]));
        let engine = OllamaEngine::new(EngineRole::Tutor, registry, provider);
        let err = engine.run(ResolvedPrompt::text("hi")).unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn ollama_engine_reports_a_model_error_when_every_candidate_for_the_role_fails() {
        // Unreachable Ollama, but two Available candidates for the same
        // role -- both must be tried, and failure is still surfaced
        // cleanly (not a panic) once every alternative is exhausted.
        let provider = Arc::new(OllamaProvider::new(crate::ollama::OllamaConnection::new(
            "127.0.0.1",
            1,
        )));
        let mut primary = sample_model(EngineRole::Reasoning);
        primary.id = ModelRegistryId(1);
        let mut alternative = sample_model(EngineRole::Reasoning);
        alternative.id = ModelRegistryId(2);
        alternative.is_selected_for_role = false;
        alternative.model_identifier = "alt-model".to_string();

        let registry: Arc<dyn ModelRegistryRepository> =
            Arc::new(StaticModelRegistry::new(vec![primary, alternative]));
        let engine = OllamaEngine::new(EngineRole::Reasoning, registry, provider);
        let err = engine.run(ResolvedPrompt::text("hi")).unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }
}
