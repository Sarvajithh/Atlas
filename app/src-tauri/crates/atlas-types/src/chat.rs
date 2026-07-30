//! Workspace-scoped chat shapes (§19, §33.10, §33.11).

use serde::{Deserialize, Serialize};

use crate::ids::{ChatMessageId, ChatSessionId, DocumentId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatMode {
    Normal,
    Research,
    ExamRestricted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: ChatSessionId,
    pub workspace_id: WorkspaceId,
    pub document_id: Option<DocumentId>,
    pub title: String,
    pub mode: ChatMode,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: ChatMessageId,
    pub session_id: ChatSessionId,
    pub role: ChatRole,
    pub content: String,
    pub engine_pipeline_used: Option<String>,
    pub created_at: String,
}
