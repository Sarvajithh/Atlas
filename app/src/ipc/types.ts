/**
 * Hand-maintained TypeScript mirrors of backend types (§12), matching
 * `atlas-types` (Rust). Kept minimal and in sync with the fields actually
 * exposed over IPC in this milestone.
 */

export type WorkspaceStatus = "Unlinked" | "Linking" | "Indexing" | "Active" | "Archived";

export interface Workspace {
  id: number;
  root_path: string;
  display_name: string;
  status: WorkspaceStatus;
  created_at: string;
  last_indexed_at: string | null;
}

export interface LearningProgress {
  concept_node_id: number;
  mastery_score: number;
  weakness_score: number;
  last_reviewed_at: string | null;
  attempt_count: number;
}

export type RelationType = "PrerequisiteOf" | "RelatedTo" | "PartOf";

export interface ConceptNode {
  id: number;
  workspace_id: number;
  label: string;
  description: string | null;
  created_at: string;
}

export type SettingsScope = "Global" | "Workspace";

export interface SettingEntry {
  key: string;
  value: string;
  value_type: string;
  scope: SettingsScope;
  workspace_id: number | null;
  updated_at: string;
}
