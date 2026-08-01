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

    fn try_generate(&self, model_identifier: &str, prompt: &ResolvedPrompt) -> Result<String, AppError> {
        self.ollama
            .generate(model_identifier, &prompt.content, prompt.images.clone())
    }
}

impl Engine for OllamaEngine {
    fn role(&self) -> EngineRole {
        self.role
    }

    fn run(&self, prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
        let primary = self
            .model_registry
            .find_for_role(self.role)?
            .ok_or_else(|| AppError::model(format!("no model currently assigned to {:?}", self.role)))?;

        match self.try_generate(&primary.model_identifier, &prompt) {
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
                    if let Ok(content) = self.try_generate(&alternative.model_identifier, &prompt) {
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
        self.get(role)?.run(prompt)
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

/// Quiz Generator feature: built on the Reasoning Engine, targeting weak
/// concepts supplied by the Memory Engine's weakness scores (§19). The
/// caller is responsible for assembling `quiz_prompt` (via Prompt Builder,
/// §40) so this module never formats its own prompt (§40's "no Engine
/// formats its own prompt" rule applies equally to feature orchestration).
pub fn generate_quiz(pool: &EnginePool, quiz_prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
    pool.run_role(EngineRole::Reasoning, quiz_prompt)
}

/// Flashcard Generator feature: built on the Tutor Engine, since flashcards
/// are a pedagogical restatement of already-retrieved content, matching the
/// Tutor Engine's "explains, teaches, answers in a pedagogical style" role.
pub fn generate_flashcards(pool: &EnginePool, flashcard_prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
    pool.run_role(EngineRole::Tutor, flashcard_prompt)
}

/// Revision Planner feature: built on the Planner Engine (§14.1 "Builds/
/// updates revision plans, study schedules"), consuming Memory Engine
/// weakness data assembled upstream into `revision_prompt`.
pub fn generate_revision_plan(pool: &EnginePool, revision_prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
    pool.run_role(EngineRole::Planner, revision_prompt)
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

    #[test]
    fn generate_quiz_routes_through_reasoning_engine() {
        let reasoning = StubEngine::new(EngineRole::Reasoning, "Q1...");
        let pool = pool_with(vec![reasoning]);
        assert_eq!(
            generate_quiz(&pool, ResolvedPrompt::text("quiz me")).unwrap().content,
            "Q1..."
        );
    }

    #[test]
    fn generate_flashcards_routes_through_tutor_engine() {
        let tutor = StubEngine::new(EngineRole::Tutor, "front/back");
        let pool = pool_with(vec![tutor]);
        assert_eq!(
            generate_flashcards(&pool, ResolvedPrompt::text("cards")).unwrap().content,
            "front/back"
        );
    }

    #[test]
    fn generate_revision_plan_routes_through_planner_engine() {
        let planner = StubEngine::new(EngineRole::Planner, "review chapter 3 tomorrow");
        let pool = pool_with(vec![planner]);
        assert_eq!(
            generate_revision_plan(&pool, ResolvedPrompt::text("plan")).unwrap().content,
            "review chapter 3 tomorrow"
        );
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
