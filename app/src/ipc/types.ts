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

export type ParseStatus = "Pending" | "Parsing" | "Parsed" | "Failed";

/** Mirrors backend `DocumentRecord` (§33.2), returned by `document.list`/`document.get`. */
export interface DocumentRecord {
  id: number;
  workspace_id: number;
  relative_path: string;
  content_hash: string;
  file_type: "md" | "pdf" | "docx" | "image" | string;
  size: number;
  mtime: string;
  parse_status: ParseStatus;
  last_indexed_hash: string | null;
}

/** Mirrors backend `DocumentContent` DTO, returned by `document.read`. */
export interface DocumentContent {
  relative_path: string;
  file_type: string;
  mime: string;
  is_base64: boolean;
  content: string;
}

/** Mirrors backend `Bookmark` (§33.9). */
export interface Bookmark {
  id: number;
  document_id: number;
  location_ref: string;
  label: string;
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
