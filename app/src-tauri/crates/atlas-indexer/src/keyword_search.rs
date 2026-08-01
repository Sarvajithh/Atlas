//! `KeywordSearchRepository` interface (§18 "Keyword search (SQLite FTS5 or
//! equivalent) over parsed text"). Owned here (the Indexing Module, §14)
//! since it searches the `chunks` table (§33.3) this crate already owns;
//! implemented by atlas-db per Dependency Inversion, exactly like
//! `ChunkRepository`/`DocumentRepository` in this same crate.

use atlas_types::ids::WorkspaceId;
use atlas_types::retrieval::SearchHit;
use atlas_utils::AppError;

pub trait KeywordSearchRepository: Send + Sync {
    /// Keyword/lexical search over every chunk belonging to documents in
    /// `workspace_id` (§18: results scoped to the active workspace by
    /// default). Returns up to `limit` hits, highest score first.
    fn search(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError>;
}
