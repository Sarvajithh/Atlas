//! `ChatRepository` interface (§33.10, §33.11). Implemented by atlas-db.

use atlas_types::chat::{ChatMessage, ChatSession};
use atlas_types::ids::{ChatSessionId, WorkspaceId};
use atlas_utils::AppError;

pub trait ChatRepository: Send + Sync {
    fn list_sessions_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ChatSession>, AppError>;

    fn create_session(&self, session: ChatSession) -> Result<ChatSession, AppError>;

    fn append_message(&self, message: ChatMessage) -> Result<ChatMessage, AppError>;

    fn list_messages(&self, session_id: ChatSessionId) -> Result<Vec<ChatMessage>, AppError>;
}
