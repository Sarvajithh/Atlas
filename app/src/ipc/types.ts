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

/** Mirrors backend `ConceptEdge` (§20). */
export interface ConceptEdge {
  id: number;
  from_node_id: number;
  to_node_id: number;
  relation_type: RelationType;
  weight: number;
}

/**
 * Mirrors backend `commands::graph::GraphFullResponse` (`graph.getFull`):
 * every node plus every edge connecting two of that workspace's nodes, for
 * node-link rendering.
 */
export interface GraphFullResponse {
  nodes: ConceptNode[];
  edges: ConceptEdge[];
}

/** Mirrors backend `atlas_graph::ExtractionOutcome` (`graph.reextract` response). */
export interface ExtractionOutcome {
  nodes_created: number;
  nodes_reused: number;
  edges_created: number;
  edges_skipped_existing: number;
}

/**
 * Mirrors backend `commands::graph::CitationGraphEdge` (Research Mode
 * phase, `graph.citationGraph`): a real Concept Graph edge whose
 * endpoints are, between them, sourced from more than one document.
 */
export interface CitationGraphEdge {
  edge: ConceptEdge;
  from_label: string;
  to_label: string;
  source_document_ids: number[];
}

/** Mirrors backend `commands::rag::SearchResult`, also reused by `rag.researchQuery`. */
export interface SearchResult {
  content: string;
  citations: Citation[];
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
  /**
   * Best-effort publication/authored date (`YYYY-MM-DD`), distinct from
   * `mtime` (filesystem modification time). `null` when no parser found
   * genuine authored-date evidence -- never derived from `mtime`. Powers
   * Research Mode's Timeline tab.
   */
  authored_at: string | null;
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

/** Mirrors backend `GlobalSearchResult` (§9 Global Search, `search.global`). */
export interface GlobalSearchResult {
  document_id: number;
  workspace_id: number;
  workspace_name: string;
  chunk_id: number;
  relative_path: string;
  snippet: string;
  location_ref: string;
  score: number;
}

/** Mirrors backend `AssistantAnswer` (`assistant.ask` response). */
export interface AssistantAnswer {
  session_id: number;
  message: ChatMessage;
  citations: Citation[];
}

/** Mirrors backend `atlas_types::quiz::QuizQuestion`. */
export interface QuizQuestion {
  question: string;
  options: string[];
  correct_index: number;
  explanation: string;
}

/** Mirrors backend `atlas_types::quiz::GeneratedQuiz` (`assistant_quiz` response). */
export interface GeneratedQuiz {
  topic: string;
  questions: QuizQuestion[];
  citations: Citation[];
}

/** Mirrors backend `atlas_types::quiz::Flashcard`. */
export interface Flashcard {
  front: string;
  back: string;
}

/** Mirrors backend `atlas_types::quiz::GeneratedFlashcards` (`assistant_flashcards` response). */
export interface GeneratedFlashcards {
  topic: string;
  cards: Flashcard[];
  citations: Citation[];
}

/** Mirrors backend `atlas_types::quiz::QuizAnswerResult`. */
export interface QuizAnswerResult {
  question_index: number;
  selected_index: number | null;
  correct_index: number;
  correct: boolean;
}

/** Mirrors backend `atlas_types::quiz::QuizGradeResult` (`assistant_quiz_submit` response). */
export interface QuizGradeResult {
  correct_count: number;
  total_count: number;
  score: number;
  results: QuizAnswerResult[];
  matched_concept_node_id: number | null;
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

/**
 * Mirrors backend `atlas_types::model::EngineRole` (V1.0 Part 3, Model
 * Dashboard). Every value here must match the Rust enum's serde variant
 * names exactly.
 */
export type EngineRole =
  | "Vision"
  | "Ocr"
  | "Embedding"
  | "Retriever"
  | "Reranker"
  | "Tutor"
  | "Reasoning"
  | "Planner"
  | "Memory"
  | "Analytics";

/** Mirrors backend `atlas_types::model::ModelStatus`. */
export type ModelStatus = "Available" | "Loading" | "Unavailable" | "Error";

/** Mirrors backend `atlas_types::model::ModelRegistryEntry` (`model.list`). */
export interface ModelRegistryEntry {
  id: number;
  model_identifier: string;
  engine_role: EngineRole;
  capabilities: string[];
  context_length: number;
  vram_requirement: number | null;
  status: ModelStatus;
  version: string;
  supported_tasks: string[];
  is_selected_for_role: boolean;
}
