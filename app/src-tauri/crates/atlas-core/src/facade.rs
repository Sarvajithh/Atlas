//! `AppFacade`: the single surface app-tauri's IPC command handlers call
//! into. Nothing in app-tauri reaches into atlas-db, atlas-vector, or
//! atlas-models directly -- only through this facade (§46.3, §46.4).

use std::sync::Arc;

use std::collections::HashMap;
use std::sync::Mutex;

use atlas_config::SettingsProvider;
use atlas_db::chunk_adapter::SqliteChunkRepository;
use atlas_db::connection::SqliteConnection;
use atlas_db::document_adapter::SqliteDocumentRepository;
use atlas_db::event_bus_adapter::SqliteEventBus;
use atlas_db::graph_adapter::SqliteGraphRepository;
use atlas_db::jobs_adapter::SqliteJobRepository;
use atlas_db::keyword_search_adapter::SqliteKeywordSearchRepository;
use atlas_db::memory_adapter::{
    SqliteAnalyticsRepository, SqliteAnnotationRepository, SqliteBookmarkRepository,
    SqliteChatRepository, SqliteLearningProgressRepository,
};
use atlas_db::model_registry_adapter::SqliteModelRegistryRepository;
use atlas_db::settings_adapter::SqliteSettingsProvider;
use atlas_db::workspace_adapter::SqliteWorkspaceRepository;
use atlas_events::EventBus;
use atlas_graph::GraphEngine;
use atlas_indexer::job_queue::JobQueue;
use atlas_indexer::ocr::TesseractCliOcrEngine;
use atlas_indexer::parser::default_parser_selector;
use atlas_indexer::pipeline::IndexingPipeline;
use atlas_types::job::IndexingStatus;
use atlas_memory::{ChatRepository, MemoryEngine};
use atlas_models::context_builder::AssembledContext;
use atlas_models::{
    ContextBuilder, EnginePool, Intent, ModelDiscoveryService, ModelRegistryRepository, ModelScheduler,
    OllamaConnection, OllamaEmbeddingEngine, OllamaEngine, OllamaProvider, PromptBuilder, Retriever, RoutingTable,
};
use atlas_types::chat::{ChatMessage, ChatMode, ChatRole, ChatSession};
use atlas_types::ids::{ChatSessionId, WorkspaceId};
use atlas_types::model::EngineRole;
use atlas_types::retrieval::Citation;
use atlas_utils::AppError;
use atlas_vector::VectorDbEmbeddingRepository;
use atlas_watcher::FolderWatcher;
use atlas_workspace::lifecycle::WorkspaceEngine;

use crate::state::AppState;
use crate::worker::IndexingWorker;

/// The default Intent -> Engine-role routing table (§15's illustrative
/// pipelines). Kept as ordinary constructed data, not a `const`/hardcoded
/// match in the Scheduler itself, so a future Settings-driven override
/// only has to replace what's passed into `ModelScheduler::new` (§15
/// closing note: "core-engines as data").
fn default_routing_table() -> RoutingTable {
    let mut table = RoutingTable::new();
    table.insert(
        Intent::FactualLookup,
        vec![EngineRole::Retriever, EngineRole::Reranker, EngineRole::Tutor],
    );
    table.insert(
        Intent::Tutoring,
        vec![EngineRole::Retriever, EngineRole::Reranker, EngineRole::Tutor],
    );
    table.insert(
        Intent::Quiz,
        vec![EngineRole::Retriever, EngineRole::Reranker, EngineRole::Reasoning],
    );
    table.insert(
        Intent::Research,
        vec![EngineRole::Retriever, EngineRole::Reranker, EngineRole::Reasoning],
    );
    table.insert(Intent::Planning, vec![EngineRole::Memory, EngineRole::Planner]);
    table
}

/// The composed application. Each field is a high-level engine depending
/// only on interfaces; concrete adapters are wired in `AppFacade::new`.
pub struct AppFacade {
    workspace_engine: Arc<WorkspaceEngine>,
    memory_engine: Arc<MemoryEngine>,
    graph_engine: Arc<GraphEngine>,
    settings: Arc<dyn SettingsProvider>,
    model_registry: Arc<dyn ModelRegistryRepository>,
    events: Arc<dyn EventBus>,
    state: Arc<AppState>,
    job_queue: Arc<JobQueue>,
    /// One `FolderWatcher` per actively-watched workspace (§21). Behind a
    /// `Mutex<HashMap<..>>` rather than per-workspace `Arc`s, since watcher
    /// registration/deregistration is an infrequent, whole-map operation
    /// (workspace link/unlink/archive), not a hot path.
    watchers: Mutex<HashMap<WorkspaceId, FolderWatcher>>,
    /// Knowledge Engine (§14, Phase 3): parse -> chunk -> embed -> index.
    indexing_pipeline: Arc<IndexingPipeline>,
    /// Background Indexing Worker (§21): the single consumer of the
    /// `jobs` table `job_queue` above produces. Behind a `Mutex<Option<_>>`
    /// like `watchers`, since start/stop is an infrequent whole-worker
    /// operation (app startup/shutdown), not a hot path.
    indexing_worker: Mutex<Option<IndexingWorker>>,
    /// Hybrid retrieval + reranking + context/prompt assembly (§14.1, §18,
    /// §39, §40), sitting downstream of the indexing pipeline above.
    retriever: Arc<Retriever>,
    context_builder: Arc<ContextBuilder>,
    prompt_builder: Arc<PromptBuilder>,
    /// Phase 4 (§14.1 Engines Module, §15 Model Scheduler, §37 Model
    /// Registry, §37.1 Model Discovery). `engine_pool` holds the concrete
    /// Ollama-backed Engines for every inference-bearing role the default
    /// routing table can terminate on (Vision, Tutor, Reasoning, Planner);
    /// which model actually backs each is resolved per-call from
    /// `model_registry`, never fixed at construction time.
    ollama: Arc<OllamaProvider>,
    engine_pool: Arc<EnginePool>,
    scheduler: Arc<ModelScheduler>,
    model_discovery: Arc<ModelDiscoveryService>,
    /// Conversation Memory / Session Manager (§33.10/§33.11): the same
    /// `ChatRepository` instance `memory_engine` was built with, exposed
    /// here too so `chat()` can drive Session Manager behavior without a
    /// second connection to the same table.
    chat: Arc<SqliteChatRepository>,
}

impl AppFacade {
    /// Compose the full application from a single SQLite connection. This
    /// is the Dependency Injection skeleton: concrete adapters (`atlas-db`)
    /// are constructed here and injected behind the interfaces domain
    /// crates depend on.
    pub fn new(connection: SqliteConnection) -> Self {
        let events: Arc<dyn EventBus> = Arc::new(SqliteEventBus::new(connection.clone()));
        let settings: Arc<dyn SettingsProvider> =
            Arc::new(SqliteSettingsProvider::new(connection.clone()));
        let model_registry_concrete = Arc::new(SqliteModelRegistryRepository::new(connection.clone()));
        let model_registry: Arc<dyn ModelRegistryRepository> = model_registry_concrete.clone();
        let model_provider: Arc<dyn atlas_models::ModelProvider> = model_registry_concrete;

        let workspace_repository = Arc::new(SqliteWorkspaceRepository::new(connection.clone()));
        let workspace_engine = Arc::new(WorkspaceEngine::new(workspace_repository, events.clone()));

        // Engines Module (Phase 4, §14.1, §15, §37): Ollama connection
        // settings come from Settings (§23), never a hardcoded host/port
        // (Governing Principle §46.1). "ollama.port" is stored as a string
        // setting like every other `SettingEntry` (§23); an unparsable or
        // absent value falls back to Ollama's own documented default port
        // rather than failing composition -- discovery itself surfaces any
        // real connectivity problem (§41 "gracefully degrade"). Built here
        // (earlier than previously) because the OCR engine below now also
        // needs it.
        let ollama_host = settings
            .get_global("ollama.host")
            .ok()
            .flatten()
            .map(|e| e.value)
            .unwrap_or_else(|| "localhost".to_string());
        let ollama_port = settings
            .get_global("ollama.port")
            .ok()
            .flatten()
            .and_then(|e| e.value.parse::<u16>().ok())
            .unwrap_or(11434);
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new(ollama_host, ollama_port)));

        let annotations = Arc::new(SqliteAnnotationRepository::new(connection.clone()));
        let bookmarks = Arc::new(SqliteBookmarkRepository::new(connection.clone()));
        let chat = Arc::new(SqliteChatRepository::new(connection.clone()));
        let progress = Arc::new(SqliteLearningProgressRepository::new(connection.clone()));
        let analytics = Arc::new(SqliteAnalyticsRepository::new(connection.clone()));
        let memory_engine = Arc::new(MemoryEngine::new(
            annotations,
            bookmarks,
            chat.clone(),
            progress,
            analytics,
            events.clone(),
        ));

        let graph_repository = Arc::new(SqliteGraphRepository::new(connection.clone()));
        let graph_engine = Arc::new(GraphEngine::new(graph_repository, events.clone()));

        let job_repository = Arc::new(SqliteJobRepository::new(connection.clone()));
        let job_queue = Arc::new(JobQueue::new(job_repository));

        // Knowledge Engine (Phase 3): Document Abstraction Layer + Parser
        // Framework + Chunking + Embedding + Vector storage + Retrieval
        // (§14, §17, §18, §35, §36).
        let document_repository = Arc::new(SqliteDocumentRepository::new(connection.clone()));
        let chunk_repository = Arc::new(SqliteChunkRepository::new(connection.clone()));
        let keyword_search = Arc::new(SqliteKeywordSearchRepository::new(connection.clone()));
        // Embedding (Part 1 "Replace HashEmbeddingEngine ... real
        // embedding engine using Ollama", §18, §37.1): resolved per-call
        // from whichever model the Model Registry currently has selected
        // for `EngineRole::Embedding` (e.g. qwen3-embedding), never a
        // hardcoded model name. `embedding.dimensions` is a Settings value
        // (§23) like `ollama.host`/`ollama.port` above -- it only sizes
        // storage up front (`VectorDbEmbeddingRepository`); the real
        // per-call vector length always comes from Ollama's response.
        let embedding_dimensions = settings
            .get_global("embedding.dimensions")
            .ok()
            .flatten()
            .and_then(|e| e.value.parse::<usize>().ok())
            .unwrap_or(1024);
        let embedder = Arc::new(OllamaEmbeddingEngine::new(
            ollama.clone(),
            model_registry.clone(),
            embedding_dimensions,
        ));
        let vector_repository = Arc::new(VectorDbEmbeddingRepository::new("workspace"));

        // OCR (§14.1, §17): prefer whichever model the Model Registry has
        // selected for EngineRole::Vision -- handwriting-capable, unlike
        // Tesseract -- falling back to the Tesseract CLI (kept as
        // `fallback` below) if no Vision-role model is assigned or the
        // Ollama call itself fails (§45.1 Recoverable).
        let ocr_engine: Arc<dyn atlas_indexer::OcrEngine> = Arc::new(atlas_models::OllamaVisionOcrEngine::new(
            ollama.clone(),
            model_registry.clone(),
            Arc::new(TesseractCliOcrEngine::default()),
        ));

        let indexing_pipeline = Arc::new(IndexingPipeline::new(
            document_repository,
            chunk_repository.clone(),
            Arc::new(default_parser_selector()),
            settings.clone(),
            events.clone(),
            ocr_engine,
            embedder.clone(),
            vector_repository.clone(),
            vector_repository.clone(),
        ));

        let retriever = Arc::new(Retriever::new(
            keyword_search,
            vector_repository,
            embedder,
            chunk_repository,
        ));
        let context_builder = Arc::new(ContextBuilder::new(4096));
        let prompt_builder = Arc::new(PromptBuilder::new(settings.clone()));

        let engine_pool = Arc::new(EnginePool::new(vec![
            Arc::new(OllamaEngine::new(EngineRole::Vision, model_registry.clone(), ollama.clone())),
            Arc::new(OllamaEngine::new(EngineRole::Tutor, model_registry.clone(), ollama.clone())),
            Arc::new(OllamaEngine::new(EngineRole::Reasoning, model_registry.clone(), ollama.clone())),
            Arc::new(OllamaEngine::new(EngineRole::Planner, model_registry.clone(), ollama.clone())),
        ]));

        let scheduler = Arc::new(ModelScheduler::new(
            default_routing_table(),
            model_provider,
            Arc::new(atlas_models::ResourceManager::new(4)),
            context_builder.clone(),
            prompt_builder.clone(),
        ));

        let model_discovery = Arc::new(ModelDiscoveryService::new(ollama.clone(), model_registry.clone(), events.clone()));

        Self {
            workspace_engine,
            memory_engine,
            graph_engine,
            settings,
            model_registry,
            events,
            state: Arc::new(AppState::new()),
            job_queue,
            watchers: Mutex::new(HashMap::new()),
            indexing_pipeline,
            indexing_worker: Mutex::new(None),
            retriever,
            context_builder,
            prompt_builder,
            ollama,
            engine_pool,
            scheduler,
            model_discovery,
            chat,
        }
    }

    pub fn job_queue(&self) -> &Arc<JobQueue> {
        &self.job_queue
    }

    pub fn indexing_pipeline(&self) -> &Arc<IndexingPipeline> {
        &self.indexing_pipeline
    }

    /// Start the Background Indexing Worker (§41 step 7, §21). Called once
    /// from `startup::startup`, after watchers are resumed, so any jobs
    /// left over from a prior session (or enqueued by `resume_watchers`'
    /// watch registration racing with a live file change) have a consumer
    /// as soon as the app is ready. Idempotent via
    /// `IndexingWorker::start`'s own idempotence.
    pub fn start_indexing_worker(&self) -> Result<(), AppError> {
        let mut guard = self
            .indexing_worker
            .lock()
            .map_err(|_| AppError::user("indexing worker lock poisoned"))?;
        let worker = guard.get_or_insert_with(|| {
            IndexingWorker::new(
                self.workspace_engine.clone(),
                self.indexing_pipeline.clone(),
                self.job_queue.clone(),
                self.events.clone(),
            )
        });
        worker.start();
        Ok(())
    }

    /// Stop the Background Indexing Worker (§42 steps 2-4: stop accepting
    /// new jobs, let any in-flight job finish, stop the worker).
    pub fn stop_indexing_worker(&self) -> Result<(), AppError> {
        let mut guard = self
            .indexing_worker
            .lock()
            .map_err(|_| AppError::user("indexing worker lock poisoned"))?;
        if let Some(worker) = guard.as_mut() {
            worker.stop();
        }
        Ok(())
    }

    pub fn indexing_worker_running(&self) -> bool {
        self.indexing_worker
            .lock()
            .map(|guard| guard.as_ref().map(IndexingWorker::is_running).unwrap_or(false))
            .unwrap_or(false)
    }

    /// Minimal backend state for a future Learning Progress UI (task
    /// scope: queued/running/completed/failed jobs, current document,
    /// processed/total counts, timestamps, progress percentage),
    /// aggregated live from the existing `jobs` table -- see
    /// `worker::compute_indexing_status` for the read itself.
    pub fn indexing_status(&self, workspace_id: WorkspaceId) -> Result<IndexingStatus, AppError> {
        crate::worker::compute_indexing_status(&self.job_queue, workspace_id)
    }

    pub fn retriever(&self) -> &Arc<Retriever> {
        &self.retriever
    }

    /// Index (or re-index) a single file within a workspace right now
    /// (§43.1 `ocr.reprocess`, or any other synchronous "index this file"
    /// call), resolving `relative_path` against the workspace root via the
    /// same safe-join convention the Folder Watcher uses (§21).
    pub fn index_document_now(
        &self,
        workspace_id: WorkspaceId,
        relative_path: &str,
    ) -> Result<atlas_indexer::pipeline::IndexOutcome, AppError> {
        let absolute_path = crate::paths::resolve_absolute_path(&self.workspace_engine, workspace_id, relative_path)?;
        self.indexing_pipeline.index_document(workspace_id, relative_path, &absolute_path)
    }

    /// Full Knowledge Engine read path (§18, §39, §40): hybrid retrieval ->
    /// rerank -> assemble context -> build prompt. Returns the resolved
    /// prompt content plus the citations it carries (§44.1), so a caller
    /// (currently the `rag.*` IPC commands; a future Tutor Engine later)
    /// gets both in one call.
    pub fn search(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        limit: usize,
    ) -> Result<(String, Vec<Citation>), AppError> {
        let hits = self.retriever.retrieve(workspace_id, query, limit)?;
        let context: AssembledContext = self.context_builder.assemble(query, hits)?;
        let citations = context.citations.clone();
        let prompt = self.prompt_builder.build(context);
        Ok((prompt.content, citations))
    }

    /// Link a folder (§6) and start watching it (§21: initial scan +
    /// incremental watch), all through the facade so `app-tauri` never
    /// reaches past this single surface (§46.3, §46.4). This is the
    /// concrete subscriber-shaped reaction to `WorkspaceEngine::link`'s
    /// `WorkspaceAdded` event described in that method's doc comment --
    /// implemented here (rather than as a registered `EventSubscriber`)
    /// for this milestone, since `AppFacade` is already the single place
    /// that owns both the Workspace Engine and the Folder Watcher registry
    /// and a full async subscriber dispatch adds no behavior a direct call
    /// doesn't already provide at this stage.
    pub fn link_workspace(
        &self,
        root_path: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<atlas_types::workspace::Workspace, AppError> {
        let workspace = self.workspace_engine.link(root_path, display_name)?;
        self.start_watching(workspace.id, &workspace.root_path)?;
        Ok(workspace)
    }

    /// §6.1 "Archived: Watching stops." Archives the workspace and tears
    /// down its `FolderWatcher`, if one is registered.
    pub fn archive_workspace(
        &self,
        id: WorkspaceId,
    ) -> Result<atlas_types::workspace::Workspace, AppError> {
        let workspace = self.workspace_engine.archive(id)?;
        self.stop_watching(id)?;
        Ok(workspace)
    }

    /// §6.1: restoring an archived workspace resumes watching.
    pub fn restore_workspace(
        &self,
        id: WorkspaceId,
    ) -> Result<atlas_types::workspace::Workspace, AppError> {
        let workspace = self.workspace_engine.restore(id)?;
        self.start_watching(workspace.id, &workspace.root_path)?;
        Ok(workspace)
    }

    /// §6.1 "Deleting a workspace link removes the workspace's row and
    /// watcher registration".
    pub fn unlink_workspace(&self, id: WorkspaceId) -> Result<(), AppError> {
        self.workspace_engine.unlink(id)?;
        self.stop_watching(id)?;
        Ok(())
    }

    /// Rebuild a workspace's index from scratch (Assistant Panel "Rebuild
    /// Workspace Index" action): re-walks the workspace root and
    /// re-enqueues an indexing job for every file, the same as the
    /// initial scan `link_workspace` runs when a folder is first linked
    /// (§6.1). Safe to call on an already-indexed workspace --
    /// `IndexingPipeline::index_document` always fully deletes and
    /// rewrites a document's chunk/embedding rows rather than appending
    /// (§22), so this reprocesses every file's chunks/embeddings with
    /// whatever the current chunker/embedder logic is, without leaving
    /// stale rows behind. Does not touch watcher registration -- if the
    /// workspace is Active, its `FolderWatcher` keeps running as-is.
    pub fn reindex_workspace(&self, id: WorkspaceId) -> Result<usize, AppError> {
        let workspace = self
            .workspace_engine
            .get(id)?
            .ok_or_else(|| AppError::workspace(format!("workspace {} not found", id.0)))?;
        let scanner = FolderWatcher::new(self.events.clone(), self.job_queue.clone());
        scanner.initial_scan(id, &workspace.root_path)
    }

    fn start_watching(&self, id: WorkspaceId, root_path: &str) -> Result<(), AppError> {
        let mut watcher = FolderWatcher::new(self.events.clone(), self.job_queue.clone());
        watcher.initial_scan(id, root_path)?;
        watcher.watch(id, root_path)?;

        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| AppError::user("watcher registry lock poisoned"))?;
        watchers.insert(id, watcher);
        Ok(())
    }

    /// §41 step 6: "Start Watchers (Folder Watcher per active workspace)".
    /// Unlike [`Self::start_watching`] (used on a fresh `link`), resuming
    /// at startup does not repeat the initial full scan -- the workspace
    /// was already scanned when it was first linked; only incremental
    /// watching needs to (re)start. Any changes made while the app was
    /// closed are picked up as the watcher observes them going forward,
    /// consistent with §21's incremental-indexing model (a full
    /// reconciliation scan on every restart is a possible future
    /// enhancement, not required by this contract).
    pub fn resume_watchers(&self) -> Result<usize, AppError> {
        let mut resumed = 0;
        for workspace in self.workspace_engine.list()? {
            if workspace.status != atlas_types::workspace::WorkspaceStatus::Active {
                continue;
            }
            let mut watcher = FolderWatcher::new(self.events.clone(), self.job_queue.clone());
            watcher.watch(workspace.id, &workspace.root_path)?;

            let mut watchers = self
                .watchers
                .lock()
                .map_err(|_| AppError::user("watcher registry lock poisoned"))?;
            watchers.insert(workspace.id, watcher);
            resumed += 1;
        }
        Ok(resumed)
    }

    fn stop_watching(&self, id: WorkspaceId) -> Result<(), AppError> {
        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| AppError::user("watcher registry lock poisoned"))?;
        if let Some(mut watcher) = watchers.remove(&id) {
            watcher.stop();
        }
        Ok(())
    }

    pub fn watched_workspace_count(&self) -> usize {
        self.watchers.lock().map(|w| w.len()).unwrap_or_default()
    }

    pub fn workspace_engine(&self) -> &Arc<WorkspaceEngine> {
        &self.workspace_engine
    }

    pub fn memory_engine(&self) -> &Arc<MemoryEngine> {
        &self.memory_engine
    }

    /// Conversation Memory read path (§33.10, "Resume previous chats"):
    /// list a workspace's chat sessions, most-recently-updated first, so
    /// the Assistant Panel can offer a session picker. Thin passthrough to
    /// the same `ChatRepository` `chat()`/`chat_stream()` write through
    /// (§46.4: handlers/facade methods don't duplicate `core-memory`'s
    /// ownership of this table, they only read through its interface).
    pub fn list_chat_sessions(&self, workspace_id: WorkspaceId) -> Result<Vec<ChatSession>, AppError> {
        let mut sessions = self.chat.list_sessions_for_workspace(workspace_id)?;
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Resume a previous chat: full message history for one session
    /// (oldest first), so the UI can replay a conversation exactly as
    /// `chat_messages` (§33.11) recorded it.
    pub fn list_chat_messages(&self, session_id: ChatSessionId) -> Result<Vec<ChatMessage>, AppError> {
        self.chat.list_messages(session_id)
    }


    pub fn graph_engine(&self) -> &Arc<GraphEngine> {
        &self.graph_engine
    }

    pub fn settings(&self) -> &Arc<dyn SettingsProvider> {
        &self.settings
    }

    pub fn model_registry(&self) -> &Arc<dyn ModelRegistryRepository> {
        &self.model_registry
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }

    pub fn state(&self) -> &Arc<AppState> {
        &self.state
    }

    pub fn ollama(&self) -> &Arc<OllamaProvider> {
        &self.ollama
    }

    pub fn scheduler(&self) -> &Arc<ModelScheduler> {
        &self.scheduler
    }

    /// §41 step 5 "Model Discovery": reconcile whatever models the local
    /// Ollama instance currently reports into the Model Registry. Errors
    /// (most commonly: Ollama isn't installed/running, §31) are returned
    /// to the caller rather than panicking, so the Startup Sequence can
    /// log and continue (§41 closing note) while an IPC-triggered manual
    /// re-discovery can surface the error to the user instead.
    pub fn run_model_discovery(&self) -> Result<usize, AppError> {
        Ok(self.model_discovery.run()?.len())
    }

    /// Session Manager (§33.10/§33.11) + full AI read path: get-or-create
    /// the chat session, append the user's message, route the request
    /// through the Model Scheduler (§15) -- retrieval + context/prompt
    /// assembly when the intent calls for it, then whichever Engine
    /// produces the answer -- and append the assistant's reply. Returns
    /// the session id (so the caller can continue the conversation) and
    /// the assistant's persisted message with its citations.
    #[allow(clippy::too_many_arguments)]
    pub fn chat(
        &self,
        workspace_id: WorkspaceId,
        session_id: Option<ChatSessionId>,
        message: &str,
        intent: Intent,
        images: Option<Vec<String>>,
    ) -> Result<(ChatSessionId, ChatMessage, Vec<Citation>), AppError> {
        let now = atlas_utils::time::now_iso8601();

        let session = match session_id {
            Some(id) => id,
            None => {
                let title: String = message.chars().take(60).collect();
                self.chat
                    .create_session(ChatSession {
                        id: ChatSessionId(0),
                        workspace_id,
                        document_id: None,
                        title,
                        mode: ChatMode::Normal,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    })?
                    .id
            }
        };

        self.chat.append_message(ChatMessage {
            id: atlas_types::ids::ChatMessageId(0),
            session_id: session,
            role: ChatRole::User,
            content: message.to_string(),
            engine_pipeline_used: None,
            created_at: now.clone(),
        })?;

        let (output, citations) = self
            .scheduler
            .execute(&self.engine_pool, &self.retriever, workspace_id, &intent, message, 5, images)?;

        let pipeline = self.scheduler.resolve_pipeline(&intent);
        let assistant_message = self.chat.append_message(ChatMessage {
            id: atlas_types::ids::ChatMessageId(0),
            session_id: session,
            role: ChatRole::Assistant,
            content: output.content,
            engine_pipeline_used: Some(format!("{pipeline:?}")),
            created_at: atlas_utils::time::now_iso8601(),
        })?;

        Ok((session, assistant_message, citations))
    }

    /// Streaming counterpart to [`Self::chat`] (§12 "use Tauri's event
    /// system to stream progress/tokens back to the frontend"; requirement
    /// "Stream responses to the frontend"). Same Session Manager behavior
    /// (session get-or-create, user message persisted up front, assistant
    /// message persisted once the full text is known) -- the only
    /// difference is that `on_chunk` is invoked as tokens arrive, before
    /// the final assistant message is appended. Only used for pipelines
    /// whose terminal role is inference-bearing and reachable directly
    /// (Vision/Tutor/Reasoning/Planner); retrieval/context assembly still
    /// happens up front, synchronously, exactly as in `chat`.
    #[allow(clippy::too_many_arguments)]
    pub fn chat_stream(
        &self,
        workspace_id: WorkspaceId,
        session_id: Option<ChatSessionId>,
        message: &str,
        intent: Intent,
        images: Option<Vec<String>>,
        mut on_chunk: impl FnMut(&str),
    ) -> Result<(ChatSessionId, ChatMessage, Vec<Citation>), AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        let __t0 = std::time::Instant::now();
        atlas_utils::log_info!("[Facade] chat_stream entered workspace_id={} intent={intent:?}", workspace_id.0);

        let now = atlas_utils::time::now_iso8601();

        let session = match session_id {
            Some(id) => id,
            None => {
                let title: String = message.chars().take(60).collect();
                self.chat
                    .create_session(ChatSession {
                        id: ChatSessionId(0),
                        workspace_id,
                        document_id: None,
                        title,
                        mode: ChatMode::Normal,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    })?
                    .id
            }
        };
        atlas_utils::log_info!("[Facade] session resolved id={} elapsed={:?}", session.0, __t0.elapsed());

        self.chat.append_message(ChatMessage {
            id: atlas_types::ids::ChatMessageId(0),
            session_id: session,
            role: ChatRole::User,
            content: message.to_string(),
            engine_pipeline_used: None,
            created_at: now.clone(),
        })?;
        atlas_utils::log_info!("[Facade] user message persisted elapsed={:?}", __t0.elapsed());

        let pipeline = self.scheduler.resolve_pipeline(&intent);
        atlas_utils::log_info!("[Scheduler] resolved pipeline = {pipeline:?}");
        let terminal_role = pipeline
            .iter()
            .copied()
            .rev()
            .find(|role| !matches!(role, EngineRole::Retriever | EngineRole::Reranker))
            .ok_or_else(|| AppError::model(format!("routing table has no answer-producing role for {intent:?}")))?;
        atlas_utils::log_info!("[Scheduler] terminal role = {terminal_role:?}");

        let (prompt_content, prompt_images, citations) = if pipeline.contains(&EngineRole::Retriever) {
            atlas_utils::log_info!("[Retriever] searching... workspace_id={} query_len={}", workspace_id.0, message.len());
            let __t_retr = std::time::Instant::now();
            let hits = self.retriever.retrieve(workspace_id, message, 5)?;
            atlas_utils::log_info!("[Retriever] returned {} chunks elapsed={:?}", hits.len(), __t_retr.elapsed());

            atlas_utils::log_info!("[ContextBuilder] entered with {} hits", hits.len());
            let __t_ctx = std::time::Instant::now();
            let context = self.context_builder.assemble(message, hits)?;
            atlas_utils::log_info!(
                "[ContextBuilder] exited hits_kept={} citations={} total_tokens={} elapsed={:?}",
                context.hits.len(),
                context.citations.len(),
                context.total_tokens,
                __t_ctx.elapsed()
            );

            let citations = context.citations.clone();
            atlas_utils::log_info!("[PromptBuilder] entered");
            let __t_pb = std::time::Instant::now();
            let resolved = self.prompt_builder.build(context);
            atlas_utils::log_info!(
                "[PromptBuilder] prompt size = {} chars elapsed={:?}",
                resolved.content.len(),
                __t_pb.elapsed()
            );
            (resolved.content, images, citations)
        } else {
            atlas_utils::log_info!("[Scheduler] pipeline has no Retriever step -- skipping retrieval/context/prompt build");
            (message.to_string(), images, Vec::new())
        };

        atlas_utils::log_info!("[ModelRegistry] resolving model for role {terminal_role:?}");
        let __t_mr = std::time::Instant::now();
        let model = self
            .model_registry
            .find_for_role(terminal_role)?
            .ok_or_else(|| {
                atlas_utils::log_error!(
                    "[ModelRegistry] no model assigned to role {terminal_role:?} (registry empty or nothing selected for this role) elapsed={:?}",
                    __t_mr.elapsed()
                );
                AppError::model(format!("no model currently assigned to {terminal_role:?}"))
            })?;
        atlas_utils::log_info!(
            "[ModelRegistry] selected model {} for role {terminal_role:?} elapsed={:?}",
            model.model_identifier,
            __t_mr.elapsed()
        );

        atlas_utils::log_info!(
            "[OllamaProvider] sending request model={} prompt_chars={}",
            model.model_identifier,
            prompt_content.len()
        );
        let __t_ollama = std::time::Instant::now();
        let stream = self.ollama.generate_stream(&model.model_identifier, &prompt_content, prompt_images)?;
        atlas_utils::log_info!("[OllamaProvider] request accepted, awaiting stream elapsed={:?}", __t_ollama.elapsed());

        let mut full_content = String::new();
        let mut chunk_count = 0usize;
        for chunk in stream {
            let chunk = chunk?;
            if chunk_count == 0 {
                atlas_utils::log_info!("[OllamaProvider] first response chunk received elapsed={:?}", __t_ollama.elapsed());
            }
            if !chunk.content.is_empty() {
                on_chunk(&chunk.content);
                full_content.push_str(&chunk.content);
                chunk_count += 1;
            }
        }
        atlas_utils::log_info!(
            "[OllamaProvider] stream complete chunks={} total_chars={} elapsed={:?}",
            chunk_count,
            full_content.len(),
            __t_ollama.elapsed()
        );

        let assistant_message = self.chat.append_message(ChatMessage {
            id: atlas_types::ids::ChatMessageId(0),
            session_id: session,
            role: ChatRole::Assistant,
            content: full_content,
            engine_pipeline_used: Some(format!("{pipeline:?}")),
            created_at: atlas_utils::time::now_iso8601(),
        })?;

        atlas_utils::log_info!("[Facade] chat_stream exited OK elapsed={:?}", __t0.elapsed());
        Ok((session, assistant_message, citations))
    }

    /// Quiz Generator feature (§14.1 Reasoning Engine composition; see
    /// `atlas_models::engines` module doc for why this is not a new Engine
    /// role). `topic` is folded into the retrieval query and the
    /// instruction sent to the Reasoning Engine in one pass -- the Model
    /// Scheduler pipeline for `Intent::Quiz` already routes through
    /// Retriever -> Reranker -> Reasoning (§15).
    pub fn quiz(&self, workspace_id: WorkspaceId, topic: &str, question_count: u8) -> Result<(String, Vec<Citation>), AppError> {
        let instruction = format!(
            "Generate {question_count} quiz questions (with answers) about: {topic}. Base every question strictly on the provided context."
        );
        let (output, citations) = self
            .scheduler
            .execute(&self.engine_pool, &self.retriever, workspace_id, &Intent::Quiz, &instruction, 8, None)?;
        Ok((output.content, citations))
    }

    /// Flashcard Generator feature, composed on the Tutor Engine (via
    /// `Intent::Tutoring`'s existing routing) rather than a new role.
    pub fn flashcards(&self, workspace_id: WorkspaceId, topic: &str, card_count: u8) -> Result<(String, Vec<Citation>), AppError> {
        let instruction = format!(
            "Create {card_count} flashcards (front/back pairs) covering the key facts about: {topic}. Base every card strictly on the provided context."
        );
        let (output, citations) = self.scheduler.execute(
            &self.engine_pool,
            &self.retriever,
            workspace_id,
            &Intent::Tutoring,
            &instruction,
            8,
            None,
        )?;
        Ok((output.content, citations))
    }

    /// Revision Planner feature, composed on the Planner Engine
    /// (`Intent::Planning`, which -- per §15 -- skips retrieval and instead
    /// consumes Student Memory's weakness data directly, assembled here
    /// into the instruction the Planner Engine receives).
    pub fn revision_plan(&self, workspace_id: WorkspaceId, concept_node_ids: &[atlas_types::ids::ConceptNodeId]) -> Result<String, AppError> {
        let mut weaknesses = Vec::new();
        for &id in concept_node_ids {
            if let Some(progress) = self.memory_engine.progress().get_progress(id)? {
                weaknesses.push(format!(
                    "concept {}: mastery {:.2}, weakness {:.2}, attempts {}",
                    id.0, progress.mastery_score, progress.weakness_score, progress.attempt_count
                ));
            }
        }
        let instruction = if weaknesses.is_empty() {
            "Produce a general study revision schedule for the next 7 days.".to_string()
        } else {
            format!(
                "Produce a prioritized revision schedule for the next 7 days, focusing more time on weaker concepts. Student progress:\n{}",
                weaknesses.join("\n")
            )
        };
        let (output, _citations) = self.scheduler.execute(
            &self.engine_pool,
            &self.retriever,
            workspace_id,
            &Intent::Planning,
            &instruction,
            0,
            None,
        )?;
        Ok(output.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_db::connection::SqliteConnection;

    #[test]
    fn app_facade_new_wires_every_engine_and_starts_with_empty_state() {
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));
        assert!(facade.state().active_workspace_id().unwrap().is_none());
        // Accessors return the same Arc instances constructed internally --
        // this is the whole DI contract (Governing Principle).
        assert!(Arc::strong_count(facade.events()) >= 1);
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas-core-facade-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn link_workspace_persists_scans_and_registers_a_watcher() {
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));
        let root = temp_dir("link");
        std::fs::write(root.join("a.pdf"), b"x").unwrap();

        let workspace = facade
            .link_workspace(root.to_str().unwrap(), "Test Workspace")
            .unwrap();

        assert_eq!(facade.watched_workspace_count(), 1);
        assert_eq!(
            facade.job_queue().repository().list_by_status(
                atlas_types::job::JobStatus::Queued
            ).unwrap().len(),
            1
        );

        facade.unlink_workspace(workspace.id).unwrap();
        assert_eq!(facade.watched_workspace_count(), 0);
        assert!(facade.workspace_engine().get(workspace.id).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn archive_workspace_stops_watching_and_restore_resumes_it() {
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));
        let root = temp_dir("archive-restore");

        let workspace = facade
            .link_workspace(root.to_str().unwrap(), "Archivable")
            .unwrap();
        assert_eq!(facade.watched_workspace_count(), 1);

        facade.archive_workspace(workspace.id).unwrap();
        assert_eq!(facade.watched_workspace_count(), 0);

        facade.restore_workspace(workspace.id).unwrap();
        assert_eq!(facade.watched_workspace_count(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Demonstrates the "Mock support for testing" requirement (§30): every
    /// engine `AppFacade` wires can equally be composed from the
    /// dependency-free `testing` doubles each domain crate exports,
    /// entirely without SQLite. This mirrors exactly what `AppFacade::new`
    /// does, just with different concrete adapters plugged into the same
    /// interfaces (Dependency Inversion).
    #[test]
    fn engines_can_be_composed_from_in_memory_test_doubles_instead_of_sqlite() {
        use atlas_events::InMemoryEventBus;
        use atlas_graph::testing::InMemoryGraphRepository;
        use atlas_memory::testing::{
            InMemoryAnalyticsRepository, InMemoryAnnotationRepository, InMemoryBookmarkRepository,
            InMemoryChatRepository, InMemoryLearningProgressRepository,
        };
        use atlas_workspace::testing::InMemoryWorkspaceRepository;

        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());

        let workspace_engine =
            WorkspaceEngine::new(Arc::new(InMemoryWorkspaceRepository::new()), events.clone());
        assert!(workspace_engine.repository().list().unwrap().is_empty());

        let memory_engine = MemoryEngine::new(
            Arc::new(InMemoryAnnotationRepository::new()),
            Arc::new(InMemoryBookmarkRepository::new()),
            Arc::new(InMemoryChatRepository::new()),
            Arc::new(InMemoryLearningProgressRepository::new()),
            Arc::new(InMemoryAnalyticsRepository::new()),
            events.clone(),
        );
        assert!(memory_engine
            .annotations()
            .list_for_document(atlas_types::ids::DocumentId(1))
            .unwrap()
            .is_empty());

        let graph_engine = GraphEngine::new(Arc::new(InMemoryGraphRepository::new()), events);
        assert!(graph_engine
            .repository()
            .list_nodes_for_workspace(atlas_types::ids::WorkspaceId(1))
            .unwrap()
            .is_empty());
    }

    /// A minimal, persistent, multi-request mock Ollama HTTP server used
    /// only by this test to give `OllamaEmbeddingEngine` (now the real
    /// production embedder, per Part 1) something to call. It serves the
    /// same three endpoints the real Ollama server exposes for discovery +
    /// embedding (`/api/tags`, `/api/show`, `/api/embed`), so the test
    /// still exercises the real `ModelDiscoveryService` -> Model Registry
    /// -> `OllamaEmbeddingEngine` path end-to-end, not a bypassed one.
    /// `/api/embed` computes a deterministic feature-hashed vector per
    /// input text (same technique the now-removed `HashEmbeddingEngine`
    /// used) purely so cosine similarity between related texts is
    /// meaningfully higher than between unrelated ones -- enough to prove
    /// the pipeline wires real embeddings through end-to-end, without this
    /// test depending on a real model being installed.
    fn mock_ollama_embedding_server(model_name: &'static str) -> u16 {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        fn hashed_vector(text: &str, dims: usize) -> Vec<f32> {
            let mut vector = vec![0f32; dims];
            for token in text.split_whitespace().map(|w| w.to_lowercase()) {
                let hash = atlas_utils::hashing::hash_str(&token);
                let bucket_seed = u32::from_str_radix(&hash[0..8], 16).unwrap_or(0);
                let sign_seed = u32::from_str_radix(&hash[8..16], 16).unwrap_or(0);
                let bucket = (bucket_seed as usize) % dims;
                let sign = if sign_seed % 2 == 0 { 1.0 } else { -1.0 };
                vector[bucket] += sign;
            }
            let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for v in vector.iter_mut() {
                    *v /= norm;
                }
            }
            vector
        }

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || loop {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(_) => return,
            };
            // Requests can exceed one 8KB read (a chunk-batch embed body),
            // so read until the client closes its write side rather than
            // assuming a single recv covers the whole request.
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        // A bare HTTP/1.1 request from `ureq` here always
                        // ends headers with this sequence; once we have at
                        // least the headers, a body-bearing request also
                        // carries Content-Length, which we don't bother
                        // parsing precisely -- reading until the socket
                        // would block is unreliable, so instead: stop once
                        // we can see the JSON body looks complete (starts
                        // with `{` and brace-balances), which every request
                        // this mock receives satisfies.
                        if let Some(body_start) = find_subslice(&buf, b"\r\n\r\n") {
                            let body = &buf[body_start + 4..];
                            if !body.is_empty() && braces_balanced(body) {
                                break;
                            }
                            if buf.windows(4).any(|w| w == b"GET " || w == b"GET\r") {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let request = String::from_utf8_lossy(&buf).to_string();
            let body_json: serde_json::Value = find_subslice(&buf, b"\r\n\r\n")
                .and_then(|i| serde_json::from_slice(&buf[i + 4..]).ok())
                .unwrap_or(serde_json::json!({}));

            let response_body = if request.starts_with("GET /api/tags") {
                serde_json::json!({ "models": [{ "name": model_name }] })
            } else if request.starts_with("POST /api/show") {
                serde_json::json!({
                    "capabilities": ["embedding"],
                    "model_info": { "generic.context_length": 4096 },
                })
            } else if request.starts_with("POST /api/embed") {
                let inputs: Vec<String> = body_json
                    .get("input")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let embeddings: Vec<Vec<f32>> = inputs.iter().map(|t| hashed_vector(t, 64)).collect();
                serde_json::json!({ "embeddings": embeddings })
            } else {
                serde_json::json!({})
            };

            let payload = response_body.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(response.as_bytes());
        });
        port
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn braces_balanced(body: &[u8]) -> bool {
        let text = String::from_utf8_lossy(body);
        let trimmed = text.trim_end_matches(char::from(0));
        let opens = trimmed.matches('{').count();
        let closes = trimmed.matches('}').count();
        opens > 0 && opens == closes
    }

    #[test]
    fn index_document_now_and_search_work_end_to_end_through_the_facade() {
        let connection = SqliteConnection::open(":memory:");
        let settings = atlas_db::settings_adapter::SqliteSettingsProvider::new(connection.clone());
        let mock_port = mock_ollama_embedding_server("mock-embedding");
        settings
            .set(atlas_types::settings::SettingEntry {
                key: "ollama.port".to_string(),
                value: mock_port.to_string(),
                value_type: "string".to_string(),
                scope: atlas_types::settings::SettingsScope::Global,
                workspace_id: None,
                updated_at: "t0".to_string(),
            })
            .unwrap();
        settings
            .set(atlas_types::settings::SettingEntry {
                key: "ollama.host".to_string(),
                value: "127.0.0.1".to_string(),
                value_type: "string".to_string(),
                scope: atlas_types::settings::SettingsScope::Global,
                workspace_id: None,
                updated_at: "t0".to_string(),
            })
            .unwrap();

        let facade = AppFacade::new(connection);
        // Real Model Discovery (§37.1) against the mock server, exactly as
        // the Startup Sequence (§41) would run it against a real Ollama
        // instance -- this is what populates `EngineRole::Embedding` in
        // the Model Registry that `OllamaEmbeddingEngine` reads from.
        facade.run_model_discovery().unwrap();

        let root = temp_dir("knowledge");
        std::fs::write(
            root.join("notes.md"),
            "# Gradients\n\nGradient descent minimizes a loss function iteratively.",
        )
        .unwrap();

        let workspace = facade
            .link_workspace(root.to_str().unwrap(), "Knowledge Test")
            .unwrap();

        let outcome = facade
            .index_document_now(workspace.id, "notes.md")
            .unwrap();
        assert!(matches!(
            outcome,
            atlas_indexer::pipeline::IndexOutcome::Indexed { .. }
        ));

        let (prompt, citations) = facade
            .search(workspace.id, "gradient descent loss", 5)
            .unwrap();
        assert!(prompt.to_lowercase().contains("gradient"));
        assert!(!citations.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn run_model_discovery_fails_gracefully_when_ollama_is_not_running() {
        // Deliberately point at an unroutable port (1 is reserved) rather
        // than relying on "nothing happens to be listening on Ollama's
        // default port in this environment" -- that assumption is false on
        // any dev machine that actually has Ollama installed and running,
        // which is exactly the real target environment (§45.1 Model
        // Errors; same pattern already used in atlas-models' own
        // OllamaProvider tests).
        let connection = SqliteConnection::open(":memory:");
        let settings = atlas_db::settings_adapter::SqliteSettingsProvider::new(connection.clone());
        settings
            .set(atlas_types::settings::SettingEntry {
                key: "ollama.port".to_string(),
                value: "1".to_string(),
                value_type: "string".to_string(),
                scope: atlas_types::settings::SettingsScope::Global,
                workspace_id: None,
                updated_at: "t0".to_string(),
            })
            .unwrap();

        let facade = AppFacade::new(connection);
        assert!(facade.run_model_discovery().is_err());
    }

    #[test]
    fn chat_creates_a_session_and_persists_the_user_message_even_when_the_engine_call_fails() {
        // No live Ollama/model in this test environment, so the engine
        // call itself fails -- but the Session Manager side (session
        // creation + user message persistence) must already have
        // happened, and the failure must be a clean AppError.
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));
        let err = facade
            .chat(WorkspaceId(1), None, "explain gradient descent", Intent::Tutoring, None)
            .unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);

        let sessions = facade.memory_engine().chat().list_sessions_for_workspace(WorkspaceId(1)).unwrap();
        assert_eq!(sessions.len(), 1);
        let messages = facade.memory_engine().chat().list_messages(sessions[0].id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, atlas_types::chat::ChatRole::User);
    }

    #[test]
    fn quiz_flashcards_and_revision_plan_fail_cleanly_without_a_live_model() {
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));

        assert_eq!(
            facade.quiz(WorkspaceId(1), "gradient descent", 3).unwrap_err().category,
            atlas_utils::ErrorCategory::ModelError
        );
        assert_eq!(
            facade.flashcards(WorkspaceId(1), "gradient descent", 5).unwrap_err().category,
            atlas_utils::ErrorCategory::ModelError
        );
        assert_eq!(
            facade.revision_plan(WorkspaceId(1), &[]).unwrap_err().category,
            atlas_utils::ErrorCategory::ModelError
        );
    }

    #[test]
    fn chat_stream_persists_the_user_message_before_the_streaming_call_fails() {
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));
        let mut chunks = Vec::new();
        let err = facade
            .chat_stream(WorkspaceId(1), None, "explain X", Intent::Tutoring, None, |c| chunks.push(c.to_string()))
            .unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
        assert!(chunks.is_empty());

        let sessions = facade.memory_engine().chat().list_sessions_for_workspace(WorkspaceId(1)).unwrap();
        assert_eq!(sessions.len(), 1);
    }
}
