//! Adapter wiring `atlas-graph`'s narrow [`ConceptExtractionModel`] seam to
//! the real inference stack (`atlas-models::EnginePool`, §14.1). Lives here
//! (not in `atlas-graph` or `atlas-models`) because `atlas-core` is the one
//! crate allowed to see both sides (§ composition-root doc comment on
//! `AppFacade`); `atlas-graph` must not depend on `atlas-models` directly
//! (that would cycle back through `atlas-models -> atlas-indexer`).
//!
//! Concept extraction always runs through `EngineRole::Reasoning` -- it is
//! a structured-output reasoning task over retrieved text, not a new
//! Engine role (§14.1's frozen role table; same "feature built on an
//! existing role" pattern `atlas-models::engines` already uses for Quiz/
//! Flashcard/Revision Planner).

use std::sync::Arc;

use atlas_graph::ConceptExtractionModel;
use atlas_models::engine::ResolvedPrompt;
use atlas_models::EnginePool;
use atlas_types::model::EngineRole;
use atlas_utils::AppError;

pub struct EnginePoolConceptExtractor {
    pool: Arc<EnginePool>,
}

impl EnginePoolConceptExtractor {
    pub fn new(pool: Arc<EnginePool>) -> Self {
        Self { pool }
    }
}

impl ConceptExtractionModel for EnginePoolConceptExtractor {
    fn extract(&self, prompt: &str) -> Result<String, AppError> {
        let output = self
            .pool
            .run_role(EngineRole::Reasoning, ResolvedPrompt::text(prompt))?;
        Ok(output.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_models::engine::{Engine, EngineOutput};

    struct StubEngine;
    impl Engine for StubEngine {
        fn role(&self) -> EngineRole {
            EngineRole::Reasoning
        }
        fn run(&self, prompt: ResolvedPrompt) -> Result<EngineOutput, AppError> {
            Ok(EngineOutput {
                content: format!("echo: {}", prompt.content),
            })
        }
    }

    #[test]
    fn extract_routes_through_the_reasoning_role() {
        let pool = Arc::new(EnginePool::new(vec![Arc::new(StubEngine)]));
        let extractor = EnginePoolConceptExtractor::new(pool);
        let result = extractor.extract("prompt text").unwrap();
        assert_eq!(result, "echo: prompt text");
    }
}
