//! Indexing pipeline orchestration skeleton (§17, §18, §22).
//! File change detected -> type detection -> parse -> (OCR if needed) ->
//! normalize -> persist to AI Cache. Step implementations are deferred to a
//! future milestone; this defines the shape only.

use std::sync::Arc;

use atlas_config::SettingsProvider;
use atlas_events::EventBus;

use crate::{ChunkRepository, DocumentRepository, ParserSelector};

pub struct IndexingPipeline {
    documents: Arc<dyn DocumentRepository>,
    chunks: Arc<dyn ChunkRepository>,
    parsers: Arc<ParserSelector>,
    settings: Arc<dyn SettingsProvider>,
    events: Arc<dyn EventBus>,
}

impl IndexingPipeline {
    pub fn new(
        documents: Arc<dyn DocumentRepository>,
        chunks: Arc<dyn ChunkRepository>,
        parsers: Arc<ParserSelector>,
        settings: Arc<dyn SettingsProvider>,
        events: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            documents,
            chunks,
            parsers,
            settings,
            events,
        }
    }

    pub fn documents(&self) -> &Arc<dyn DocumentRepository> {
        &self.documents
    }

    pub fn chunks(&self) -> &Arc<dyn ChunkRepository> {
        &self.chunks
    }

    pub fn parsers(&self) -> &Arc<ParserSelector> {
        &self.parsers
    }

    pub fn settings(&self) -> &Arc<dyn SettingsProvider> {
        &self.settings
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_config::hierarchy::LayeredSettingsProvider;
    use atlas_events::InMemoryEventBus;
    use atlas_types::ids::WorkspaceId;

    use crate::testing::{InMemoryChunkRepository, InMemoryDocumentRepository};

    #[test]
    fn pipeline_exposes_all_injected_dependencies() {
        let pipeline = IndexingPipeline::new(
            Arc::new(InMemoryDocumentRepository::new()),
            Arc::new(InMemoryChunkRepository::new()),
            Arc::new(ParserSelector::new()),
            Arc::new(LayeredSettingsProvider::new()),
            Arc::new(InMemoryEventBus::new()),
        );

        assert!(pipeline
            .documents()
            .list_for_workspace(WorkspaceId(1))
            .unwrap()
            .is_empty());
        assert!(pipeline.parsers().resolve("pdf").is_none());
    }
}
