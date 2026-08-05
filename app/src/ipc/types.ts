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

export type ParseStatus = "Pending" | "Parsing" | "Parsed" | "ParsedEmpty" | "Failed";

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

/** Mirrors backend `ChatMode` (§33.10). */
export type ChatMode = "Normal" | "Research" | "ExamRestricted";

/** Mirrors backend `ChatSession` (§33.10). */
export interface ChatSession {
  id: number;
  workspace_id: number;
  document_id: number | null;
  title: string;
  mode: ChatMode;
  created_at: string;
  updated_at: string;
}

/** Mirrors backend `ChatRole` (§33.11). */
export type ChatRole = "User" | "Assistant";

/** Mirrors backend `ChatMessage` (§33.11). */
export interface ChatMessage {
  id: number;
  session_id: number;
  role: ChatRole;
  content: string;
  engine_pipeline_used: string | null;
  created_at: string;
}

/** Mirrors backend `Citation` (§39.1, §44.1). */
export interface Citation {
  document_id: number;
  chunk_id: number;
  location_ref: string;
  snippet: string;
}

/** Mirrors backend `AssistantAnswer` (`assistant.ask` response). */
export interface AssistantAnswer {
  session_id: number;
  message: ChatMessage;
  citations: Citation[];
}

/** Mirrors backend `GeneratedContent` (quiz/flashcards responses). */
export interface GeneratedContent {
  content: string;
  citations: Citation[];
}

/** Mirrors backend `RunningIndexingJob` (§21, `atlas_types::job`). */
export interface RunningIndexingJob {
  job_id: number;
  relative_path: string;
  started_at: string | null;
  retry_count: number;
}

/** Mirrors backend `IndexingStatus` (§21, §4 "progress percentage"). */
export interface IndexingStatus {
  queued: number;
  running: RunningIndexingJob | null;
  succeeded: number;
  failed: number;
  total: number;
  progress_percent: number | null;
  last_indexed_at: string | null;
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
