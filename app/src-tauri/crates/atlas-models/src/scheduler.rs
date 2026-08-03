//! Model Scheduler (§15). The only component that decides which Engines run
//! for a given request, and in what order. MUST NOT call every engine for
//! every request (§15). The routing table is data (`core-engines` as data,
//! per §15's closing note), not hardcoded per-feature branching.

use std::collections::HashMap;
use std::sync::Arc;

use atlas_types::model::EngineRole;
use atlas_types::retrieval::Citation;
use atlas_utils::AppError;

use crate::context_builder::ContextBuilder;
use crate::engine::{EngineOutput, ResolvedPrompt};
use crate::engines::EnginePool;
use crate::prompt_builder::PromptBuilder;
use crate::registry::ModelProvider;
use crate::resource_manager::ResourceManager;
use crate::retriever::Retriever;

/// A request intent, classified up front (§15: "Intent Detection").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Intent {
    FactualLookup,
    Tutoring,
    Quiz,
    Research,
    Planning,
}

/// The ordered sequence of Engine roles a given intent routes through.
/// Stored as data so new intents can be added by extending the table rather
/// than rewriting control flow (§15).
pub type RoutingTable = HashMap<Intent, Vec<EngineRole>>;

pub struct ModelScheduler {
    routing_table: RoutingTable,
    model_provider: Arc<dyn ModelProvider>,
    resource_manager: Arc<ResourceManager>,
    context_builder: Arc<ContextBuilder>,
    prompt_builder: Arc<PromptBuilder>,
}

impl ModelScheduler {
    pub fn new(
        routing_table: RoutingTable,
        model_provider: Arc<dyn ModelProvider>,
        resource_manager: Arc<ResourceManager>,
        context_builder: Arc<ContextBuilder>,
        prompt_builder: Arc<PromptBuilder>,
    ) -> Self {
        Self {
            routing_table,
            model_provider,
            resource_manager,
            context_builder,
            prompt_builder,
        }
    }

    pub fn routing_table(&self) -> &RoutingTable {
        &self.routing_table
    }

    pub fn model_provider(&self) -> &Arc<dyn ModelProvider> {
        &self.model_provider
    }

    pub fn resource_manager(&self) -> &Arc<ResourceManager> {
        &self.resource_manager
    }

    pub fn context_builder(&self) -> &Arc<ContextBuilder> {
        &self.context_builder
    }

    pub fn prompt_builder(&self) -> &Arc<PromptBuilder> {
        &self.prompt_builder
    }

    /// Resolve the ordered Engine-role sequence for a given intent (§15).
    /// Returns an empty sequence for an intent with no routing-table entry
    /// rather than panicking -- an unrecognized intent is a data gap, not a
    /// control-flow error (§15: "not hardcoded per-feature branching").
    pub fn resolve_pipeline(&self, intent: &Intent) -> Vec<EngineRole> {
        self.routing_table.get(intent).cloned().unwrap_or_default()
    }

    /// Run a request end-to-end through the pipeline configured for
    /// `intent` (§15, §39, §40): retrieve + rerank + assemble context when
    /// the pipeline calls for it, then hand the resolved prompt to whichever
    /// role actually produces the answer. This is the one place a request
    /// is "routed through the Model Scheduler" (per the requirements) --
    /// callers (Tutor Engine consumers, IPC handlers) never call
    /// `Retriever`/`EnginePool` directly for a scheduled request.
    ///
    /// `images` carries base64 image data for a Vision Engine pipeline
    /// (§35.2); `None` for text-only requests.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        engines: &EnginePool,
        retriever: &Retriever,
        workspace_id: atlas_types::ids::WorkspaceId,
        intent: &Intent,
        query: &str,
        retrieval_limit: usize,
        images: Option<Vec<String>>,
    ) -> Result<(EngineOutput, Vec<Citation>), AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        let __t0 = std::time::Instant::now();
        atlas_utils::log_info!("[Scheduler] execute entered workspace_id={} intent={intent:?}", workspace_id.0);

        let pipeline = self.resolve_pipeline(intent);
        atlas_utils::log_info!("[Scheduler] resolved pipeline = {pipeline:?}");

        // The terminal role is whichever role in the pipeline actually
        // produces the answer -- the first non-retrieval role. Retriever/
        // Reranker are handled as the context-assembly step below, not as
        // `Engine::run` calls, since they aren't LLM-inference roles here.
        let terminal_role = pipeline
            .iter()
            .copied()
            .rev()
            .find(|role| !matches!(role, EngineRole::Retriever | EngineRole::Reranker))
            .ok_or_else(|| AppError::model(format!("routing table has no answer-producing role for {intent:?}")))?;
        atlas_utils::log_info!("[Scheduler] terminal role = {terminal_role:?}");

        let prompt = if pipeline.contains(&EngineRole::Retriever) {
            let hits = retriever.retrieve(workspace_id, query, retrieval_limit)?;
            let context = self.context_builder.assemble(query, hits)?;
            let citations = context.citations.clone();
            let mut resolved = self.prompt_builder.build(context);
            resolved.images = images;
            atlas_utils::log_info!("[Scheduler] handing prompt to EnginePool.run_role({terminal_role:?})");
            let output = engines.run_role(terminal_role, resolved)?;
            atlas_utils::log_info!("[Scheduler] execute exited OK elapsed={:?}", __t0.elapsed());
            return Ok((output, citations));
        } else if let Some(images) = images {
            ResolvedPrompt::with_images(query, images)
        } else {
            ResolvedPrompt::text(query)
        };

        atlas_utils::log_info!("[Scheduler] handing prompt to EnginePool.run_role({terminal_role:?}) (no retrieval)");
        let output = engines.run_role(terminal_role, prompt)?;
        atlas_utils::log_info!("[Scheduler] execute exited OK (no retrieval) elapsed={:?}", __t0.elapsed());
        Ok((output, Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::InMemoryModelRegistry;
    use atlas_config::hierarchy::LayeredSettingsProvider;

    fn scheduler_with_table(routing_table: RoutingTable) -> ModelScheduler {
        let model_provider: Arc<dyn ModelProvider> = Arc::new(InMemoryModelRegistry::new());
        ModelScheduler::new(
            routing_table,
            model_provider,
            Arc::new(ResourceManager::new(4)),
            Arc::new(ContextBuilder::new(4096)),
            Arc::new(PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()))),
        )
    }

    /// Table-driven test (§30: "given an intent, assert the expected engine
    /// sequence") mirroring §15's illustrative pipeline shapes.
    #[test]
    fn resolve_pipeline_matches_configured_routing_table() {
        let mut table = RoutingTable::new();
        table.insert(
            Intent::Tutoring,
            vec![
                EngineRole::Retriever,
                EngineRole::Reranker,
                EngineRole::Tutor,
            ],
        );
        table.insert(
            Intent::Planning,
            vec![EngineRole::Memory, EngineRole::Planner],
        );

        let scheduler = scheduler_with_table(table);

        assert_eq!(
            scheduler.resolve_pipeline(&Intent::Tutoring),
            vec![
                EngineRole::Retriever,
                EngineRole::Reranker,
                EngineRole::Tutor
            ]
        );
        assert_eq!(
            scheduler.resolve_pipeline(&Intent::Planning),
            vec![EngineRole::Memory, EngineRole::Planner]
        );
    }

    #[test]
    fn planning_intent_skips_retrieval_per_section_15() {
        let mut table = RoutingTable::new();
        table.insert(
            Intent::Planning,
            vec![EngineRole::Memory, EngineRole::Planner],
        );
        let scheduler = scheduler_with_table(table);

        let pipeline = scheduler.resolve_pipeline(&Intent::Planning);
        assert!(!pipeline.contains(&EngineRole::Retriever));
    }

    #[test]
    fn unrecognized_intent_resolves_to_empty_pipeline_not_a_panic() {
        let scheduler = scheduler_with_table(RoutingTable::new());
        assert_eq!(
            scheduler.resolve_pipeline(&Intent::Quiz),
            Vec::<EngineRole>::new()
        );
    }

    // --- execute() ----------------------------------------------------

    use crate::engine::{Engine, EngineOutput as EngOutput};
    use crate::engines::EnginePool;
    use crate::retriever::Retriever;
    use atlas_indexer::embedding::HashEmbeddingEngine;
    use atlas_indexer::keyword_search::KeywordSearchRepository;
    use atlas_indexer::vector_search::VectorSearchRepository;
    use atlas_indexer::ChunkRepository;
    use atlas_types::chunk::Chunk;
    use atlas_types::ids::{ChunkId, DocumentId, WorkspaceId};
    use atlas_types::retrieval::SearchHit;

    struct StubEngine {
        role: EngineRole,
        response: String,
    }
    impl Engine for StubEngine {
        fn role(&self) -> EngineRole {
            self.role
        }
        fn run(&self, _prompt: ResolvedPrompt) -> Result<EngOutput, AppError> {
            Ok(EngOutput { content: self.response.clone() })
        }
    }

    struct FixedKeywordSearch(Vec<SearchHit>);
    impl KeywordSearchRepository for FixedKeywordSearch {
        fn search(&self, _workspace_id: WorkspaceId, _query: &str, _limit: usize) -> Result<Vec<SearchHit>, AppError> {
            Ok(self.0.clone())
        }
    }
    struct EmptyVectorSearch;
    impl VectorSearchRepository for EmptyVectorSearch {
        fn search(
            &self,
            _workspace_id: WorkspaceId,
            _query_vector: &atlas_indexer::embedding::Embedding,
            _limit: usize,
        ) -> Result<Vec<SearchHit>, AppError> {
            Ok(Vec::new())
        }
    }
    struct FixedChunks(Vec<Chunk>);
    impl ChunkRepository for FixedChunks {
        fn list_for_document(&self, _document_id: DocumentId) -> Result<Vec<Chunk>, AppError> {
            Ok(self.0.clone())
        }
        fn insert(&self, chunk: Chunk) -> Result<Chunk, AppError> {
            Ok(chunk)
        }
        fn delete_for_document(&self, _document_id: DocumentId) -> Result<(), AppError> {
            Ok(())
        }
        fn find_by_id(&self, id: ChunkId) -> Result<Option<Chunk>, AppError> {
            Ok(self.0.iter().find(|c| c.id == id).cloned())
        }
    }

    fn hit(chunk_id: i64) -> SearchHit {
        SearchHit {
            chunk_id: ChunkId(chunk_id),
            document_id: DocumentId(1),
            text_content: "gradient descent minimizes loss".to_string(),
            page_or_location_ref: "1".to_string(),
            score: 1.0,
        }
    }

    fn chunk(id: i64) -> Chunk {
        Chunk {
            id: ChunkId(id),
            document_id: DocumentId(1),
            sequence_index: 0,
            text_content: "gradient descent minimizes loss".to_string(),
            page_or_location_ref: "1".to_string(),
            token_count: 4,
            parser_version: "v1".to_string(),
        }
    }

    fn test_retriever() -> Retriever {
        Retriever::new(
            Arc::new(FixedKeywordSearch(vec![hit(1)])),
            Arc::new(EmptyVectorSearch),
            Arc::new(HashEmbeddingEngine::default()),
            Arc::new(FixedChunks(vec![chunk(1)])),
        )
    }

    #[test]
    fn execute_runs_retrieval_pipeline_and_returns_citations() {
        let mut table = RoutingTable::new();
        table.insert(Intent::Tutoring, vec![EngineRole::Retriever, EngineRole::Reranker, EngineRole::Tutor]);
        let scheduler = scheduler_with_table(table);
        let engines = EnginePool::new(vec![Arc::new(StubEngine { role: EngineRole::Tutor, response: "answer".to_string() })]);

        let (output, citations) = scheduler
            .execute(&engines, &test_retriever(), WorkspaceId(1), &Intent::Tutoring, "gradient descent", 5, None)
            .unwrap();

        assert_eq!(output.content, "answer");
        assert!(!citations.is_empty());
    }

    #[test]
    fn execute_skips_retrieval_for_a_pipeline_without_it_per_section_15() {
        let mut table = RoutingTable::new();
        table.insert(Intent::Planning, vec![EngineRole::Memory, EngineRole::Planner]);
        let scheduler = scheduler_with_table(table);
        let engines = EnginePool::new(vec![Arc::new(StubEngine { role: EngineRole::Planner, response: "plan".to_string() })]);

        let (output, citations) = scheduler
            .execute(&engines, &test_retriever(), WorkspaceId(1), &Intent::Planning, "revise chapter 3", 5, None)
            .unwrap();

        assert_eq!(output.content, "plan");
        assert!(citations.is_empty());
    }

    #[test]
    fn execute_errors_cleanly_when_routing_table_has_no_answer_role() {
        let scheduler = scheduler_with_table(RoutingTable::new());
        let engines = EnginePool::new(vec![]);
        let err = scheduler
            .execute(&engines, &test_retriever(), WorkspaceId(1), &Intent::Quiz, "x", 5, None)
            .unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }
}
