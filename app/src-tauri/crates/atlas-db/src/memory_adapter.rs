//! SQLite-backed Student Memory repositories (§33.7-§33.11, §33.16-§33.18).
//! Grouped in one adapter module since `core-memory`/atlas-memory owns all
//! of these tables (§33.7).

use atlas_memory::{
    AnalyticsRepository, AnnotationRepository, BookmarkRepository, ChatRepository,
    LearningProgressRepository,
};
use atlas_types::chat::{ChatMessage, ChatSession};
use atlas_types::ids::{
    AnnotationId, BookmarkId, ChatSessionId, ConceptNodeId, DocumentId, WorkspaceId,
};
use atlas_types::memory::{
    AnalyticsPoint, Annotation, Bookmark, LearningProgress, RevisionHistoryEntry,
};
use atlas_utils::AppError;

use crate::connection::SqliteConnection;

pub struct SqliteAnnotationRepository {
    connection: SqliteConnection,
}

impl SqliteAnnotationRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl AnnotationRepository for SqliteAnnotationRepository {
    fn list_for_document(&self, _document_id: DocumentId) -> Result<Vec<Annotation>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn insert(&self, _annotation: Annotation) -> Result<Annotation, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn update(&self, _annotation: Annotation) -> Result<Annotation, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn delete(&self, _id: AnnotationId) -> Result<(), AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}

pub struct SqliteBookmarkRepository {
    connection: SqliteConnection,
}

impl SqliteBookmarkRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl BookmarkRepository for SqliteBookmarkRepository {
    fn list_for_document(&self, _document_id: DocumentId) -> Result<Vec<Bookmark>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn insert(&self, _bookmark: Bookmark) -> Result<Bookmark, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn delete(&self, _id: BookmarkId) -> Result<(), AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}

pub struct SqliteChatRepository {
    connection: SqliteConnection,
}

impl SqliteChatRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl ChatRepository for SqliteChatRepository {
    fn list_sessions_for_workspace(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<Vec<ChatSession>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn create_session(&self, _session: ChatSession) -> Result<ChatSession, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn append_message(&self, _message: ChatMessage) -> Result<ChatMessage, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn list_messages(&self, _session_id: ChatSessionId) -> Result<Vec<ChatMessage>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}

pub struct SqliteLearningProgressRepository {
    connection: SqliteConnection,
}

impl SqliteLearningProgressRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl LearningProgressRepository for SqliteLearningProgressRepository {
    fn get_progress(
        &self,
        _concept_node_id: ConceptNodeId,
    ) -> Result<Option<LearningProgress>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn upsert_progress(&self, _progress: LearningProgress) -> Result<LearningProgress, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn append_revision_history(
        &self,
        _entry: RevisionHistoryEntry,
    ) -> Result<RevisionHistoryEntry, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn list_revision_history(
        &self,
        _concept_node_id: ConceptNodeId,
    ) -> Result<Vec<RevisionHistoryEntry>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}

pub struct SqliteAnalyticsRepository {
    connection: SqliteConnection,
}

impl SqliteAnalyticsRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl AnalyticsRepository for SqliteAnalyticsRepository {
    fn list_for_workspace(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<Vec<AnalyticsPoint>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn upsert(&self, _point: AnalyticsPoint) -> Result<AnalyticsPoint, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}
