//! SQLite-backed Student Memory repositories (§33.7-§33.11, §33.16-§33.18).
//! Grouped in one adapter module since `core-memory`/atlas-memory owns all
//! of these tables (§33.7). This is durable, append-heavy data (§19) --
//! deleting a source file or workspace link MUST NOT delete any of it
//! (§7.3); nothing in this module performs cascading deletes on
//! document/workspace removal.

use atlas_memory::{
    AnalyticsRepository, AnnotationRepository, BookmarkRepository, ChatRepository,
    LearningProgressRepository, StudyRepository,
};
use atlas_types::chat::{ChatMessage, ChatMode, ChatRole, ChatSession};
use atlas_types::ids::{
    AnnotationId, BookmarkId, ChatMessageId, ChatSessionId, ConceptNodeId, DocumentId, FlashcardSetId,
    QuizId, RevisionPlanId, WorkspaceId,
};
use atlas_types::memory::{
    AnalyticsPoint, Annotation, Bookmark, Flashcard, FlashcardSet, LearningProgress, Quiz, QuizQuestion,
    RevisionHistoryEntry, RevisionOutcome, RevisionPlan, RevisionPlanItem, WeakTopic,
};
use atlas_utils::AppError;
use rusqlite::{params, OptionalExtension, Row};

use crate::connection::SqliteConnection;

// ---------------------------------------------------------------------
// Annotations (§33.8)
// ---------------------------------------------------------------------

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

fn row_to_annotation(row: &Row<'_>) -> rusqlite::Result<Annotation> {
    Ok(Annotation {
        id: AnnotationId(row.get(0)?),
        document_id: DocumentId(row.get(1)?),
        location_ref: row.get(2)?,
        content: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

impl AnnotationRepository for SqliteAnnotationRepository {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Annotation>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, document_id, location_ref, content, created_at, updated_at FROM annotations WHERE document_id = ?1 ORDER BY id ASC")
            .map_err(|e| AppError::storage(format!("annotation list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![document_id.0], row_to_annotation)
            .map_err(|e| AppError::storage(format!("annotation list query failed: {e}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::storage(format!("annotation row read failed: {e}")))
    }

    fn insert(&self, annotation: Annotation) -> Result<Annotation, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO annotations (document_id, location_ref, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                annotation.document_id.0,
                annotation.location_ref,
                annotation.content,
                annotation.created_at,
                annotation.updated_at,
            ],
        )
        .map_err(|e| AppError::storage(format!("annotation insert failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(Annotation { id: AnnotationId(id), ..annotation })
    }

    fn update(&self, annotation: Annotation) -> Result<Annotation, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "UPDATE annotations SET location_ref = ?1, content = ?2, updated_at = ?3 WHERE id = ?4",
            params![annotation.location_ref, annotation.content, annotation.updated_at, annotation.id.0],
        )
        .map_err(|e| AppError::storage(format!("annotation update failed: {e}")))?;
        Ok(annotation)
    }

    fn delete(&self, id: AnnotationId) -> Result<(), AppError> {
        let conn = self.connection.lock()?;
        conn.execute("DELETE FROM annotations WHERE id = ?1", params![id.0])
            .map_err(|e| AppError::storage(format!("annotation delete failed: {e}")))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Bookmarks (§33.9)
// ---------------------------------------------------------------------

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

fn row_to_bookmark(row: &Row<'_>) -> rusqlite::Result<Bookmark> {
    Ok(Bookmark {
        id: BookmarkId(row.get(0)?),
        document_id: DocumentId(row.get(1)?),
        location_ref: row.get(2)?,
        label: row.get(3)?,
        created_at: row.get(4)?,
    })
}

impl BookmarkRepository for SqliteBookmarkRepository {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Bookmark>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, document_id, location_ref, label, created_at FROM bookmarks WHERE document_id = ?1 ORDER BY id ASC")
            .map_err(|e| AppError::storage(format!("bookmark list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![document_id.0], row_to_bookmark)
            .map_err(|e| AppError::storage(format!("bookmark list query failed: {e}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::storage(format!("bookmark row read failed: {e}")))
    }

    fn insert(&self, bookmark: Bookmark) -> Result<Bookmark, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO bookmarks (document_id, location_ref, label, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![bookmark.document_id.0, bookmark.location_ref, bookmark.label, bookmark.created_at],
        )
        .map_err(|e| AppError::storage(format!("bookmark insert failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(Bookmark { id: BookmarkId(id), ..bookmark })
    }

    fn delete(&self, id: BookmarkId) -> Result<(), AppError> {
        let conn = self.connection.lock()?;
        conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![id.0])
            .map_err(|e| AppError::storage(format!("bookmark delete failed: {e}")))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Conversation Memory: chat_sessions + chat_messages (§33.10, §33.11)
// ---------------------------------------------------------------------

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

fn mode_to_str(mode: &ChatMode) -> &'static str {
    match mode {
        ChatMode::Normal => "normal",
        ChatMode::Research => "research",
        ChatMode::ExamRestricted => "exam_restricted",
    }
}

fn mode_from_str(value: &str) -> Result<ChatMode, AppError> {
    match value {
        "normal" => Ok(ChatMode::Normal),
        "research" => Ok(ChatMode::Research),
        "exam_restricted" => Ok(ChatMode::ExamRestricted),
        other => Err(AppError::storage(format!("unrecognized chat mode in database: {other}"))),
    }
}

fn role_to_str(role: &ChatRole) -> &'static str {
    match role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}

fn role_from_str(value: &str) -> Result<ChatRole, AppError> {
    match value {
        "user" => Ok(ChatRole::User),
        "assistant" => Ok(ChatRole::Assistant),
        other => Err(AppError::storage(format!("unrecognized chat role in database: {other}"))),
    }
}

type SessionRow = (i64, i64, Option<i64>, String, String, String, String);

fn row_to_session_tuple(row: &Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
}

fn tuple_to_session(t: SessionRow) -> Result<ChatSession, AppError> {
    let (id, workspace_id, document_id, title, mode, created_at, updated_at) = t;
    Ok(ChatSession {
        id: ChatSessionId(id),
        workspace_id: WorkspaceId(workspace_id),
        document_id: document_id.map(DocumentId),
        title,
        mode: mode_from_str(&mode)?,
        created_at,
        updated_at,
    })
}

const SESSION_COLUMNS: &str = "id, workspace_id, document_id, title, mode, created_at, updated_at FROM chat_sessions";

type MessageRow = (i64, i64, String, String, Option<String>, String);

fn row_to_message_tuple(row: &Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
}

fn tuple_to_message(t: MessageRow) -> Result<ChatMessage, AppError> {
    let (id, session_id, role, content, engine_pipeline_used, created_at) = t;
    Ok(ChatMessage {
        id: ChatMessageId(id),
        session_id: ChatSessionId(session_id),
        role: role_from_str(&role)?,
        content,
        engine_pipeline_used,
        created_at,
    })
}

const MESSAGE_COLUMNS: &str = "id, session_id, role, content, engine_pipeline_used, created_at FROM chat_messages";

impl ChatRepository for SqliteChatRepository {
    fn list_sessions_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<ChatSession>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(&format!("SELECT {SESSION_COLUMNS} WHERE workspace_id = ?1 ORDER BY updated_at DESC"))
            .map_err(|e| AppError::storage(format!("chat session list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![workspace_id.0], row_to_session_tuple)
            .map_err(|e| AppError::storage(format!("chat session list query failed: {e}")))?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(tuple_to_session(
                row.map_err(|e| AppError::storage(format!("chat session row read failed: {e}")))?,
            )?);
        }
        Ok(sessions)
    }

    fn create_session(&self, session: ChatSession) -> Result<ChatSession, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO chat_sessions (workspace_id, document_id, title, mode, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.workspace_id.0,
                session.document_id.map(|d| d.0),
                session.title,
                mode_to_str(&session.mode),
                session.created_at,
                session.updated_at,
            ],
        )
        .map_err(|e| AppError::storage(format!("chat session create failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(ChatSession { id: ChatSessionId(id), ..session })
    }

    fn append_message(&self, message: ChatMessage) -> Result<ChatMessage, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO chat_messages (session_id, role, content, engine_pipeline_used, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message.session_id.0,
                role_to_str(&message.role),
                message.content,
                message.engine_pipeline_used,
                message.created_at,
            ],
        )
        .map_err(|e| AppError::storage(format!("chat message append failed: {e}")))?;
        let id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE chat_sessions SET updated_at = ?1 WHERE id = ?2",
            params![message.created_at, message.session_id.0],
        )
        .map_err(|e| AppError::storage(format!("chat session updated_at bump failed: {e}")))?;
        Ok(ChatMessage { id: ChatMessageId(id), ..message })
    }

    fn list_messages(&self, session_id: ChatSessionId) -> Result<Vec<ChatMessage>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(&format!("SELECT {MESSAGE_COLUMNS} WHERE session_id = ?1 ORDER BY created_at ASC, id ASC"))
            .map_err(|e| AppError::storage(format!("chat message list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![session_id.0], row_to_message_tuple)
            .map_err(|e| AppError::storage(format!("chat message list query failed: {e}")))?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(tuple_to_message(
                row.map_err(|e| AppError::storage(format!("chat message row read failed: {e}")))?,
            )?);
        }
        Ok(messages)
    }
}

// ---------------------------------------------------------------------
// Learning progress + revision history (§33.17, §33.18)
// ---------------------------------------------------------------------

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

fn outcome_to_str(outcome: &RevisionOutcome) -> &'static str {
    match outcome {
        RevisionOutcome::Recalled => "recalled",
        RevisionOutcome::Forgotten => "forgotten",
    }
}

fn outcome_from_str(value: &str) -> Result<RevisionOutcome, AppError> {
    match value {
        "recalled" => Ok(RevisionOutcome::Recalled),
        "forgotten" => Ok(RevisionOutcome::Forgotten),
        other => Err(AppError::storage(format!("unrecognized revision outcome in database: {other}"))),
    }
}

impl LearningProgressRepository for SqliteLearningProgressRepository {
    fn get_progress(&self, concept_node_id: ConceptNodeId) -> Result<Option<LearningProgress>, AppError> {
        let conn = self.connection.lock()?;
        conn.query_row(
            "SELECT concept_node_id, mastery_score, weakness_score, last_reviewed_at, attempt_count FROM learning_progress WHERE concept_node_id = ?1",
            params![concept_node_id.0],
            |row| {
                Ok(LearningProgress {
                    concept_node_id: ConceptNodeId(row.get(0)?),
                    mastery_score: row.get(1)?,
                    weakness_score: row.get(2)?,
                    last_reviewed_at: row.get(3)?,
                    attempt_count: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::storage(format!("learning progress get failed: {e}")))
    }

    fn upsert_progress(&self, progress: LearningProgress) -> Result<LearningProgress, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO learning_progress (concept_node_id, mastery_score, weakness_score, last_reviewed_at, attempt_count)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(concept_node_id) DO UPDATE SET
                mastery_score = excluded.mastery_score,
                weakness_score = excluded.weakness_score,
                last_reviewed_at = excluded.last_reviewed_at,
                attempt_count = excluded.attempt_count",
            params![
                progress.concept_node_id.0,
                progress.mastery_score,
                progress.weakness_score,
                progress.last_reviewed_at,
                progress.attempt_count,
            ],
        )
        .map_err(|e| AppError::storage(format!("learning progress upsert failed: {e}")))?;
        Ok(progress)
    }

    fn append_revision_history(&self, entry: RevisionHistoryEntry) -> Result<RevisionHistoryEntry, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO revision_history (concept_node_id, scheduled_at, completed_at, outcome, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.concept_node_id.0,
                entry.scheduled_at,
                entry.completed_at,
                entry.outcome.as_ref().map(outcome_to_str),
                entry.scheduled_at.clone(),
            ],
        )
        .map_err(|e| AppError::storage(format!("revision history append failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(RevisionHistoryEntry { id: atlas_types::ids::RevisionHistoryId(id), ..entry })
    }

    fn list_revision_history(&self, concept_node_id: ConceptNodeId) -> Result<Vec<RevisionHistoryEntry>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, concept_node_id, scheduled_at, completed_at, outcome FROM revision_history WHERE concept_node_id = ?1 ORDER BY scheduled_at ASC")
            .map_err(|e| AppError::storage(format!("revision history list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![concept_node_id.0], |row| {
                let outcome: Option<String> = row.get(4)?;
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?, outcome))
            })
            .map_err(|e| AppError::storage(format!("revision history list query failed: {e}")))?;

        let mut entries = Vec::new();
        for row in rows {
            let (id, concept_node_id, scheduled_at, completed_at, outcome) =
                row.map_err(|e| AppError::storage(format!("revision history row read failed: {e}")))?;
            entries.push(RevisionHistoryEntry {
                id: atlas_types::ids::RevisionHistoryId(id),
                concept_node_id: ConceptNodeId(concept_node_id),
                scheduled_at,
                completed_at,
                outcome: outcome.map(|o| outcome_from_str(&o)).transpose()?,
            });
        }
        Ok(entries)
    }
}

// ---------------------------------------------------------------------
// Analytics (§33.16)
// ---------------------------------------------------------------------

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
    fn list_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<AnalyticsPoint>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare("SELECT workspace_id, metric_key, metric_value, computed_at, period FROM analytics WHERE workspace_id = ?1 ORDER BY computed_at DESC")
            .map_err(|e| AppError::storage(format!("analytics list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![workspace_id.0], |row| {
                Ok(AnalyticsPoint {
                    workspace_id: WorkspaceId(row.get(0)?),
                    metric_key: row.get(1)?,
                    metric_value: row.get(2)?,
                    computed_at: row.get(3)?,
                    period: row.get(4)?,
                })
            })
            .map_err(|e| AppError::storage(format!("analytics list query failed: {e}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::storage(format!("analytics row read failed: {e}")))
    }

    fn upsert(&self, point: AnalyticsPoint) -> Result<AnalyticsPoint, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO analytics (workspace_id, metric_key, metric_value, computed_at, period)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(workspace_id, metric_key, period) DO UPDATE SET
                metric_value = excluded.metric_value,
                computed_at = excluded.computed_at",
            params![point.workspace_id.0, point.metric_key, point.metric_value, point.computed_at, point.period],
        )
        .map_err(|e| AppError::storage(format!("analytics upsert failed: {e}")))?;
        Ok(point)
    }

    fn record_quiz_answer(&self, workspace_id: WorkspaceId, topic: &str, correct: bool) -> Result<(), AppError> {
        let conn = self.connection.lock()?;
        let now = atlas_utils::time::now_iso8601();
        let (correct_delta, incorrect_delta) = if correct { (1, 0) } else { (0, 1) };
        conn.execute(
            "INSERT INTO quiz_topic_stats (workspace_id, topic, correct_count, incorrect_count, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(workspace_id, topic) DO UPDATE SET
                correct_count = correct_count + excluded.correct_count,
                incorrect_count = incorrect_count + excluded.incorrect_count,
                updated_at = excluded.updated_at",
            params![workspace_id.0, topic, correct_delta, incorrect_delta, now],
        )
        .map_err(|e| AppError::storage(format!("quiz topic stat record failed: {e}")))?;
        Ok(())
    }

    fn list_weak_topics(&self, workspace_id: WorkspaceId) -> Result<Vec<WeakTopic>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT topic, correct_count, incorrect_count FROM quiz_topic_stats \
                 WHERE workspace_id = ?1 \
                 ORDER BY CAST(correct_count AS REAL) / MAX(correct_count + incorrect_count, 1) ASC",
            )
            .map_err(|e| AppError::storage(format!("weak topic list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![workspace_id.0], |row| {
                let topic: String = row.get(0)?;
                let correct_count: u32 = row.get(1)?;
                let incorrect_count: u32 = row.get(2)?;
                Ok((topic, correct_count, incorrect_count))
            })
            .map_err(|e| AppError::storage(format!("weak topic list query failed: {e}")))?;

        let mut weak_topics = Vec::new();
        for row in rows {
            let (topic, correct_count, incorrect_count) =
                row.map_err(|e| AppError::storage(format!("weak topic row read failed: {e}")))?;
            let total = correct_count + incorrect_count;
            let accuracy = if total == 0 { 0.0 } else { correct_count as f32 / total as f32 };
            weak_topics.push(WeakTopic {
                topic,
                correct_count,
                incorrect_count,
                accuracy,
            });
        }
        Ok(weak_topics)
    }
}

// ---------------------------------------------------------------------
// Quiz / Flashcard / Revision Plan (§ Learning subsystem)
// ---------------------------------------------------------------------

pub struct SqliteStudyRepository {
    connection: SqliteConnection,
}

impl SqliteStudyRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

fn row_to_quiz(row: &Row<'_>) -> Result<Quiz, AppError> {
    let id: i64 = row.get(0).map_err(|e| AppError::storage(format!("quiz row read failed: {e}")))?;
    let workspace_id: i64 = row.get(1).map_err(|e| AppError::storage(format!("quiz row read failed: {e}")))?;
    let document_id: Option<i64> = row.get(2).map_err(|e| AppError::storage(format!("quiz row read failed: {e}")))?;
    let topic: String = row.get(3).map_err(|e| AppError::storage(format!("quiz row read failed: {e}")))?;
    let questions_json: String = row.get(4).map_err(|e| AppError::storage(format!("quiz row read failed: {e}")))?;
    let created_at: String = row.get(5).map_err(|e| AppError::storage(format!("quiz row read failed: {e}")))?;
    let questions: Vec<QuizQuestion> =
        serde_json::from_str(&questions_json).map_err(|e| AppError::storage(format!("stored quiz questions_json is corrupt: {e}")))?;
    Ok(Quiz {
        id: QuizId(id),
        workspace_id: WorkspaceId(workspace_id),
        document_id: document_id.map(DocumentId),
        topic,
        questions,
        created_at,
    })
}

fn row_to_flashcard_set(row: &Row<'_>) -> Result<FlashcardSet, AppError> {
    let id: i64 = row.get(0).map_err(|e| AppError::storage(format!("flashcard set row read failed: {e}")))?;
    let workspace_id: i64 = row.get(1).map_err(|e| AppError::storage(format!("flashcard set row read failed: {e}")))?;
    let document_id: Option<i64> = row.get(2).map_err(|e| AppError::storage(format!("flashcard set row read failed: {e}")))?;
    let topic: String = row.get(3).map_err(|e| AppError::storage(format!("flashcard set row read failed: {e}")))?;
    let cards_json: String = row.get(4).map_err(|e| AppError::storage(format!("flashcard set row read failed: {e}")))?;
    let created_at: String = row.get(5).map_err(|e| AppError::storage(format!("flashcard set row read failed: {e}")))?;
    let cards: Vec<Flashcard> =
        serde_json::from_str(&cards_json).map_err(|e| AppError::storage(format!("stored flashcard cards_json is corrupt: {e}")))?;
    Ok(FlashcardSet {
        id: FlashcardSetId(id),
        workspace_id: WorkspaceId(workspace_id),
        document_id: document_id.map(DocumentId),
        topic,
        cards,
        created_at,
    })
}

fn row_to_revision_plan(row: &Row<'_>) -> Result<RevisionPlan, AppError> {
    let id: i64 = row.get(0).map_err(|e| AppError::storage(format!("revision plan row read failed: {e}")))?;
    let workspace_id: i64 = row.get(1).map_err(|e| AppError::storage(format!("revision plan row read failed: {e}")))?;
    let items_json: String = row.get(2).map_err(|e| AppError::storage(format!("revision plan row read failed: {e}")))?;
    let created_at: String = row.get(3).map_err(|e| AppError::storage(format!("revision plan row read failed: {e}")))?;
    let items: Vec<RevisionPlanItem> =
        serde_json::from_str(&items_json).map_err(|e| AppError::storage(format!("stored revision plan items_json is corrupt: {e}")))?;
    Ok(RevisionPlan {
        id: RevisionPlanId(id),
        workspace_id: WorkspaceId(workspace_id),
        items,
        created_at,
    })
}

impl StudyRepository for SqliteStudyRepository {
    fn insert_quiz(&self, quiz: Quiz) -> Result<Quiz, AppError> {
        let questions_json = serde_json::to_string(&quiz.questions)?;
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO quizzes (workspace_id, document_id, topic, questions_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![quiz.workspace_id.0, quiz.document_id.map(|d| d.0), quiz.topic, questions_json, quiz.created_at],
        )
        .map_err(|e| AppError::storage(format!("quiz insert failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(Quiz { id: QuizId(id), ..quiz })
    }

    fn get_quiz(&self, id: QuizId) -> Result<Option<Quiz>, AppError> {
        let conn = self.connection.lock()?;
        let result = conn
            .query_row(
                "SELECT id, workspace_id, document_id, topic, questions_json, created_at FROM quizzes WHERE id = ?1",
                params![id.0],
                |row| Ok(row_to_quiz(row)),
            )
            .optional()
            .map_err(|e| AppError::storage(format!("quiz get failed: {e}")))?;
        result.transpose()
    }

    fn list_quizzes_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<Quiz>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, workspace_id, document_id, topic, questions_json, created_at FROM quizzes WHERE workspace_id = ?1 ORDER BY id DESC")
            .map_err(|e| AppError::storage(format!("quiz list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![workspace_id.0], |row| Ok(row_to_quiz(row)))
            .map_err(|e| AppError::storage(format!("quiz list query failed: {e}")))?;
        let mut quizzes = Vec::new();
        for row in rows {
            quizzes.push(row.map_err(|e| AppError::storage(format!("quiz row read failed: {e}")))??);
        }
        Ok(quizzes)
    }

    fn list_quizzes_for_document(&self, document_id: DocumentId) -> Result<Vec<Quiz>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, workspace_id, document_id, topic, questions_json, created_at FROM quizzes WHERE document_id = ?1 ORDER BY id DESC")
            .map_err(|e| AppError::storage(format!("quiz list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![document_id.0], |row| Ok(row_to_quiz(row)))
            .map_err(|e| AppError::storage(format!("quiz list query failed: {e}")))?;
        let mut quizzes = Vec::new();
        for row in rows {
            quizzes.push(row.map_err(|e| AppError::storage(format!("quiz row read failed: {e}")))??);
        }
        Ok(quizzes)
    }

    fn insert_flashcard_set(&self, set: FlashcardSet) -> Result<FlashcardSet, AppError> {
        let cards_json = serde_json::to_string(&set.cards)?;
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO flashcard_sets (workspace_id, document_id, topic, cards_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![set.workspace_id.0, set.document_id.map(|d| d.0), set.topic, cards_json, set.created_at],
        )
        .map_err(|e| AppError::storage(format!("flashcard set insert failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(FlashcardSet { id: FlashcardSetId(id), ..set })
    }

    fn get_flashcard_set(&self, id: FlashcardSetId) -> Result<Option<FlashcardSet>, AppError> {
        let conn = self.connection.lock()?;
        let result = conn
            .query_row(
                "SELECT id, workspace_id, document_id, topic, cards_json, created_at FROM flashcard_sets WHERE id = ?1",
                params![id.0],
                |row| Ok(row_to_flashcard_set(row)),
            )
            .optional()
            .map_err(|e| AppError::storage(format!("flashcard set get failed: {e}")))?;
        result.transpose()
    }

    fn list_flashcard_sets_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<FlashcardSet>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, workspace_id, document_id, topic, cards_json, created_at FROM flashcard_sets WHERE workspace_id = ?1 ORDER BY id DESC")
            .map_err(|e| AppError::storage(format!("flashcard set list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![workspace_id.0], |row| Ok(row_to_flashcard_set(row)))
            .map_err(|e| AppError::storage(format!("flashcard set list query failed: {e}")))?;
        let mut sets = Vec::new();
        for row in rows {
            sets.push(row.map_err(|e| AppError::storage(format!("flashcard set row read failed: {e}")))??);
        }
        Ok(sets)
    }

    fn insert_revision_plan(&self, plan: RevisionPlan) -> Result<RevisionPlan, AppError> {
        let items_json = serde_json::to_string(&plan.items)?;
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO revision_plans (workspace_id, items_json, created_at) VALUES (?1, ?2, ?3)",
            params![plan.workspace_id.0, items_json, plan.created_at],
        )
        .map_err(|e| AppError::storage(format!("revision plan insert failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(RevisionPlan { id: RevisionPlanId(id), ..plan })
    }

    fn list_revision_plans_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<RevisionPlan>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, workspace_id, items_json, created_at FROM revision_plans WHERE workspace_id = ?1 ORDER BY id DESC")
            .map_err(|e| AppError::storage(format!("revision plan list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![workspace_id.0], |row| Ok(row_to_revision_plan(row)))
            .map_err(|e| AppError::storage(format!("revision plan list query failed: {e}")))?;
        let mut plans = Vec::new();
        for row in rows {
            plans.push(row.map_err(|e| AppError::storage(format!("revision plan row read failed: {e}")))??);
        }
        Ok(plans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> SqliteConnection {
        SqliteConnection::open(":memory:")
    }

    #[test]
    fn annotation_insert_then_list_round_trips() {
        let repo = SqliteAnnotationRepository::new(conn());
        let annotation = repo
            .insert(Annotation {
                id: AnnotationId(0),
                document_id: DocumentId(1),
                location_ref: "page:3".to_string(),
                content: "important".to_string(),
                created_at: "t1".to_string(),
                updated_at: "t1".to_string(),
            })
            .unwrap();
        assert_ne!(annotation.id.0, 0);
        assert_eq!(repo.list_for_document(DocumentId(1)).unwrap().len(), 1);
    }

    #[test]
    fn bookmark_insert_then_delete() {
        let repo = SqliteBookmarkRepository::new(conn());
        let bookmark = repo
            .insert(Bookmark {
                id: BookmarkId(0),
                document_id: DocumentId(1),
                location_ref: "page:1".to_string(),
                label: "start here".to_string(),
                created_at: "t1".to_string(),
            })
            .unwrap();
        repo.delete(bookmark.id).unwrap();
        assert!(repo.list_for_document(DocumentId(1)).unwrap().is_empty());
    }

    #[test]
    fn chat_session_and_messages_round_trip_in_order() {
        let shared = conn();
        let repo = SqliteChatRepository::new(shared);
        let session = repo
            .create_session(ChatSession {
                id: ChatSessionId(0),
                workspace_id: WorkspaceId(1),
                document_id: None,
                title: "Untitled".to_string(),
                mode: ChatMode::Normal,
                created_at: "t1".to_string(),
                updated_at: "t1".to_string(),
            })
            .unwrap();

        repo.append_message(ChatMessage {
            id: ChatMessageId(0),
            session_id: session.id,
            role: ChatRole::User,
            content: "explain gradient descent".to_string(),
            engine_pipeline_used: None,
            created_at: "t1".to_string(),
        })
        .unwrap();
        repo.append_message(ChatMessage {
            id: ChatMessageId(0),
            session_id: session.id,
            role: ChatRole::Assistant,
            content: "it minimizes...".to_string(),
            engine_pipeline_used: Some("Retriever,Reranker,Tutor".to_string()),
            created_at: "t2".to_string(),
        })
        .unwrap();

        let messages = repo.list_messages(session.id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[1].role, ChatRole::Assistant);

        let sessions = repo.list_sessions_for_workspace(WorkspaceId(1)).unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn learning_progress_upsert_is_idempotent_per_concept() {
        let repo = SqliteLearningProgressRepository::new(conn());
        repo.upsert_progress(LearningProgress {
            concept_node_id: ConceptNodeId(1),
            mastery_score: 0.3,
            weakness_score: 0.7,
            last_reviewed_at: Some("t1".to_string()),
            attempt_count: 1,
        })
        .unwrap();
        repo.upsert_progress(LearningProgress {
            concept_node_id: ConceptNodeId(1),
            mastery_score: 0.6,
            weakness_score: 0.4,
            last_reviewed_at: Some("t2".to_string()),
            attempt_count: 2,
        })
        .unwrap();

        let progress = repo.get_progress(ConceptNodeId(1)).unwrap().unwrap();
        assert_eq!(progress.attempt_count, 2);
        assert_eq!(progress.mastery_score, 0.6);
    }

    #[test]
    fn revision_history_appends_and_lists_in_schedule_order() {
        let repo = SqliteLearningProgressRepository::new(conn());
        repo.append_revision_history(RevisionHistoryEntry {
            id: atlas_types::ids::RevisionHistoryId(0),
            concept_node_id: ConceptNodeId(1),
            scheduled_at: "2026-08-05".to_string(),
            completed_at: None,
            outcome: None,
        })
        .unwrap();
        repo.append_revision_history(RevisionHistoryEntry {
            id: atlas_types::ids::RevisionHistoryId(0),
            concept_node_id: ConceptNodeId(1),
            scheduled_at: "2026-08-01".to_string(),
            completed_at: Some("2026-08-01".to_string()),
            outcome: Some(RevisionOutcome::Recalled),
        })
        .unwrap();

        let history = repo.list_revision_history(ConceptNodeId(1)).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].scheduled_at, "2026-08-01");
        assert_eq!(history[0].outcome, Some(RevisionOutcome::Recalled));
    }

    #[test]
    fn analytics_upsert_updates_existing_metric_for_same_period() {
        let repo = SqliteAnalyticsRepository::new(conn());
        repo.upsert(AnalyticsPoint {
            workspace_id: WorkspaceId(1),
            metric_key: "quiz_accuracy".to_string(),
            metric_value: 0.5,
            computed_at: "t1".to_string(),
            period: "week".to_string(),
        })
        .unwrap();
        repo.upsert(AnalyticsPoint {
            workspace_id: WorkspaceId(1),
            metric_key: "quiz_accuracy".to_string(),
            metric_value: 0.8,
            computed_at: "t2".to_string(),
            period: "week".to_string(),
        })
        .unwrap();

        let points = repo.list_for_workspace(WorkspaceId(1)).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].metric_value, 0.8);
    }

    #[test]
    fn weak_topic_aggregate_accumulates_across_multiple_record_calls() {
        let repo = SqliteAnalyticsRepository::new(conn());
        repo.record_quiz_answer(WorkspaceId(1), "Thermodynamics", false).unwrap();
        repo.record_quiz_answer(WorkspaceId(1), "Thermodynamics", false).unwrap();
        repo.record_quiz_answer(WorkspaceId(1), "Thermodynamics", true).unwrap();
        repo.record_quiz_answer(WorkspaceId(1), "Optics", true).unwrap();
        repo.record_quiz_answer(WorkspaceId(1), "Optics", true).unwrap();

        let weak = repo.list_weak_topics(WorkspaceId(1)).unwrap();
        assert_eq!(weak.len(), 2);
        // Weakest (lowest accuracy) first.
        assert_eq!(weak[0].topic, "Thermodynamics");
        assert_eq!(weak[0].correct_count, 1);
        assert_eq!(weak[0].incorrect_count, 2);
        assert!((weak[0].accuracy - (1.0 / 3.0)).abs() < 1e-6);
        assert_eq!(weak[1].topic, "Optics");
        assert_eq!(weak[1].accuracy, 1.0);
    }

    #[test]
    fn weak_topics_scoped_per_workspace_and_empty_when_none_recorded() {
        let shared = conn();
        let repo = SqliteAnalyticsRepository::new(shared);
        assert!(repo.list_weak_topics(WorkspaceId(1)).unwrap().is_empty());
        repo.record_quiz_answer(WorkspaceId(1), "Topic A", false).unwrap();
        repo.record_quiz_answer(WorkspaceId(2), "Topic A", true).unwrap();
        assert_eq!(repo.list_weak_topics(WorkspaceId(1)).unwrap()[0].correct_count, 0);
        assert_eq!(repo.list_weak_topics(WorkspaceId(2)).unwrap()[0].correct_count, 1);
    }

    #[test]
    fn quiz_insert_then_get_round_trips_questions_through_json() {
        let repo = SqliteStudyRepository::new(conn());
        let quiz = repo
            .insert_quiz(Quiz {
                id: QuizId(0),
                workspace_id: WorkspaceId(1),
                document_id: Some(DocumentId(7)),
                topic: "Photosynthesis".to_string(),
                questions: vec![QuizQuestion {
                    question: "What pigment absorbs light?".to_string(),
                    options: vec!["Chlorophyll".to_string(), "Melanin".to_string()],
                    correct_answer: "Chlorophyll".to_string(),
                    source_citations: vec!["[1]".to_string()],
                }],
                created_at: "t1".to_string(),
            })
            .unwrap();
        assert_ne!(quiz.id.0, 0);

        let fetched = repo.get_quiz(quiz.id).unwrap().unwrap();
        assert_eq!(fetched.topic, "Photosynthesis");
        assert_eq!(fetched.questions.len(), 1);
        assert_eq!(fetched.questions[0].correct_answer, "Chlorophyll");
        assert_eq!(fetched.document_id, Some(DocumentId(7)));
    }

    #[test]
    fn quiz_get_of_missing_id_returns_none_not_error() {
        let repo = SqliteStudyRepository::new(conn());
        assert!(repo.get_quiz(QuizId(999)).unwrap().is_none());
    }

    #[test]
    fn quizzes_list_by_workspace_and_by_document_are_both_scoped_correctly() {
        let shared = conn();
        let repo = SqliteStudyRepository::new(shared);
        repo.insert_quiz(Quiz {
            id: QuizId(0),
            workspace_id: WorkspaceId(1),
            document_id: Some(DocumentId(1)),
            topic: "t".to_string(),
            questions: vec![],
            created_at: "t1".to_string(),
        })
        .unwrap();
        repo.insert_quiz(Quiz {
            id: QuizId(0),
            workspace_id: WorkspaceId(1),
            document_id: None,
            topic: "t2".to_string(),
            questions: vec![],
            created_at: "t2".to_string(),
        })
        .unwrap();
        repo.insert_quiz(Quiz {
            id: QuizId(0),
            workspace_id: WorkspaceId(2),
            document_id: None,
            topic: "t3".to_string(),
            questions: vec![],
            created_at: "t3".to_string(),
        })
        .unwrap();

        assert_eq!(repo.list_quizzes_for_workspace(WorkspaceId(1)).unwrap().len(), 2);
        assert_eq!(repo.list_quizzes_for_workspace(WorkspaceId(2)).unwrap().len(), 1);
        assert_eq!(repo.list_quizzes_for_document(DocumentId(1)).unwrap().len(), 1);
    }

    #[test]
    fn flashcard_set_insert_then_get_round_trips_cards_through_json() {
        let repo = SqliteStudyRepository::new(conn());
        let set = repo
            .insert_flashcard_set(FlashcardSet {
                id: FlashcardSetId(0),
                workspace_id: WorkspaceId(1),
                document_id: None,
                topic: "Cell Biology".to_string(),
                cards: vec![Flashcard {
                    front: "What is a ribosome?".to_string(),
                    back: "Protein synthesis site".to_string(),
                    source_citations: vec![],
                }],
                created_at: "t1".to_string(),
            })
            .unwrap();
        assert_ne!(set.id.0, 0);

        let fetched = repo.get_flashcard_set(set.id).unwrap().unwrap();
        assert_eq!(fetched.cards.len(), 1);
        assert_eq!(fetched.cards[0].front, "What is a ribosome?");
        assert_eq!(repo.list_flashcard_sets_for_workspace(WorkspaceId(1)).unwrap().len(), 1);
    }

    #[test]
    fn revision_plan_insert_then_list_round_trips_items_through_json() {
        let repo = SqliteStudyRepository::new(conn());
        let plan = repo
            .insert_revision_plan(RevisionPlan {
                id: RevisionPlanId(0),
                workspace_id: WorkspaceId(1),
                items: vec![RevisionPlanItem {
                    topic: "Thermodynamics".to_string(),
                    recommendation: "Review chapter 4".to_string(),
                    priority: 1,
                }],
                created_at: "t1".to_string(),
            })
            .unwrap();
        assert_ne!(plan.id.0, 0);

        let plans = repo.list_revision_plans_for_workspace(WorkspaceId(1)).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].items[0].topic, "Thermodynamics");
        assert_eq!(plans[0].items[0].priority, 1);
    }

    #[test]
    fn quiz_flashcard_and_revision_plan_data_survive_alongside_annotations_no_cascading_delete() {
        // §7.3: nothing in this module cascade-deletes Student Memory when
        // a document is removed elsewhere. This module doesn't implement
        // document deletion itself, but proves the tables are independent:
        // deleting an annotation for a document must not touch a quiz
        // that references the same document_id.
        let shared = conn();
        let study_repo = SqliteStudyRepository::new(shared.clone());
        let annotation_repo = SqliteAnnotationRepository::new(shared);

        study_repo
            .insert_quiz(Quiz {
                id: QuizId(0),
                workspace_id: WorkspaceId(1),
                document_id: Some(DocumentId(1)),
                topic: "t".to_string(),
                questions: vec![],
                created_at: "t1".to_string(),
            })
            .unwrap();
        let annotation = annotation_repo
            .insert(Annotation {
                id: AnnotationId(0),
                document_id: DocumentId(1),
                location_ref: "page:1".to_string(),
                content: "note".to_string(),
                created_at: "t1".to_string(),
                updated_at: "t1".to_string(),
            })
            .unwrap();
        annotation_repo.delete(annotation.id).unwrap();

        assert_eq!(study_repo.list_quizzes_for_document(DocumentId(1)).unwrap().len(), 1);
    }
}
