//! Settings shapes (§23, §33.12). Backing store for the Governing Principle's
//! "no hardcoded configuration" rule.

use serde::{Deserialize, Serialize};

use crate::ids::WorkspaceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsScope {
    Global,
    Workspace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingEntry {
    pub key: String,
    pub value: String,
    pub value_type: String,
    pub scope: SettingsScope,
    pub workspace_id: Option<WorkspaceId>,
    pub updated_at: String,
}
