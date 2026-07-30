//! Model Scheduler (§15). The only component that decides which Engines run
//! for a given request, and in what order. MUST NOT call every engine for
//! every request (§15). The routing table is data (`core-engines` as data,
//! per §15's closing note), not hardcoded per-feature branching.

use std::collections::HashMap;
use std::sync::Arc;

use atlas_types::model::EngineRole;

use crate::context_builder::ContextBuilder;
use crate::prompt_builder::PromptBuilder;
use crate::registry::ModelProvider;
use crate::resource_manager::ResourceManager;

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
}
