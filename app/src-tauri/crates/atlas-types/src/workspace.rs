//! Workspace-related shapes (§6, §33.1).

use serde::{Deserialize, Serialize};

use crate::ids::WorkspaceId;

/// Mirrors the `workspaces` table (§33.1) and the lifecycle in §6.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceStatus {
    Unlinked,
    Linking,
    Indexing,
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub root_path: String,
    pub display_name: String,
    pub status: WorkspaceStatus,
    pub created_at: String,
    pub last_indexed_at: Option<String>,
}
