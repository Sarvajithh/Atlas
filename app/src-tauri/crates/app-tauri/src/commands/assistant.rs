//! `assistant.*` namespace (§43.1): assistant.ask, assistant.cancel, plus
//! additive extensions for the study features built on top of the frozen
//! §14.1 Engine roles (§43.2: "New commands extend an existing namespace").
//! Handlers only validate/forward/map errors (§26, §46.4) -- routing
//! through the Model Scheduler, retrieval, and Session Manager behavior all
//! live in `atlas-core`'s `AppFacade`.

use tauri::{Emitter, State, Window};

use atlas_core::AppFacade;
use atlas_models::Intent;
use atlas_types::chat::ChatMessage;
use atlas_types::ids::{ChatSessionId, ConceptNodeId, WorkspaceId};
use atlas_types::retrieval::Citation;
use atlas_utils::error::{ErrorCategory, ErrorCode};
use atlas_utils::AppError;
use serde::{Deserialize, Serialize};

/// The Intent classification a request is routed under (§15 "Intent
/// Detection"). Exposed to the UI as a plain string so the frontend doesn't
/// need to depend on `atlas_models`' enum layout directly; `parse_intent`
/// is the one place that string is interpreted.
fn parse_intent(intent: &str) -> Intent {
    match intent {
        "tutoring" => Intent::Tutoring,
        "quiz" => Intent::Quiz,
        "research" => Intent::Research,
        "planning" => Intent::Planning,
        // "factual_lookup" and anything unrecognized default to the
        // general-purpose lookup pipeline rather than erroring -- an
        // unrecognized intent string from an older frontend build should
        // degrade gracefully, not break the whole request (§45.2).
        _ => Intent::FactualLookup,
    }
}

#[derive(Debug, Serialize)]
pub struct AssistantAnswer {
    pub session_id: i64,
    pub message: ChatMessage,
    pub citations: Vec<Citation>,
}

/// §43.1 `assistant.ask`: run one turn of the assistant end-to-end
/// (Session Manager -> Model Scheduler -> Engine -> Session Manager, §15,
/// §33.10/§33.11). `session_id` is `None` to start a new conversation, or
/// an existing session id to continue one (multi-turn, §33.11).
#[tauri::command]
pub fn assistant_ask(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
    question: String,
    session_id: Option<i64>,
    intent: Option<String>,
    images: Option<Vec<String>>,
) -> Result<AssistantAnswer, AppError> {
    let (session, message, citations) = facade.chat(
        WorkspaceId(workspace_id),
        session_id.map(ChatSessionId),
        &question,
        intent.as_deref().map(parse_intent).unwrap_or(Intent::Tutoring),
        images,
    )?;
    Ok(AssistantAnswer { session_id: session.0, message, citations })
}

#[derive(Debug, Clone, Serialize)]
struct StreamChunkPayload {
    session_id: i64,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
struct StreamDonePayload {
    session_id: i64,
    message: ChatMessage,
    citations: Vec<Citation>,
}

#[derive(Debug, Clone, Serialize)]
struct StreamErrorPayload {
    message: String,
}

/// §43.1 `assistant.ask` streaming counterpart (§12: "use Tauri's event
/// system to stream progress/tokens back to the frontend"; requirement
/// "Stream responses to the frontend"). Runs synchronously on the IPC
/// thread (Ollama streaming reads are already non-blocking per-chunk;
/// `AppFacade::chat_stream` is the one place token-by-token forwarding
/// happens) and emits three possible event types on `window`:
/// `assistant://chunk`, `assistant://done`, `assistant://error`.
#[tauri::command]
pub fn assistant_ask_stream(
    facade: State<'_, AppFacade>,
    window: Window,
    workspace_id: i64,
    question: String,
    session_id: Option<i64>,
    intent: Option<String>,
    images: Option<Vec<String>>,
) -> Result<(), AppError> {
    let resolved_intent = intent.as_deref().map(parse_intent).unwrap_or(Intent::Tutoring);

    let result = facade.chat_stream(
        WorkspaceId(workspace_id),
        session_id.map(ChatSessionId),
        &question,
        resolved_intent,
        images,
        |chunk: &str| {
            // A stream chunk failing to emit is a UI/transport concern, not
            // a reason to abort generation (§45.2: don't let a
            // non-critical failure silently break the whole operation) --
            // logged, not propagated.
            if let Err(err) = window.emit(
                "assistant://chunk",
                StreamChunkPayload {
                    session_id: session_id.unwrap_or(0),
                    content: chunk.to_string(),
                },
            ) {
                atlas_utils::log_warn!("failed to emit assistant://chunk: {err}");
            }
        },
    );

    match result {
        Ok((session, message, citations)) => {
            let _ = window.emit(
                "assistant://done",
                StreamDonePayload { session_id: session.0, message, citations },
            );
            Ok(())
        }
        Err(err) => {
            let _ = window.emit("assistant://error", StreamErrorPayload { message: err.message.clone() });
            Err(err)
        }
    }
}

/// §43.1 `assistant.cancel`. Full in-flight cancellation (aborting a
/// streaming Ollama request mid-generation from a second IPC call) needs a
/// per-request cancellation-token registry that doesn't exist yet -- this
/// remains a defined, honest "not implemented" error (§45.2: no bare
/// silent no-op) rather than a fabricated success, so the UI can surface
/// it truthfully instead of assuming cancellation took effect.
#[tauri::command]
pub fn assistant_cancel(_request_id: String) -> Result<(), AppError> {
    Err(AppError::new(
        ErrorCode::EngineError,
        ErrorCategory::Recoverable,
        "assistant.cancel: in-flight request cancellation is not implemented in this milestone",
    ))
}

#[derive(Debug, Deserialize)]
pub struct QuizRequest {
    pub workspace_id: i64,
    pub topic: String,
    pub question_count: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct GeneratedContent {
    pub content: String,
    pub citations: Vec<Citation>,
}

/// Quiz Generator feature (additive extension of `assistant.*`, §43.2).
/// Composed on the Reasoning Engine via `atlas-core`'s `AppFacade::quiz`
/// (see that method and the `atlas_models::engines` module doc for why
/// this isn't a new §14.1 Engine role).
#[tauri::command]
pub fn assistant_quiz(facade: State<'_, AppFacade>, request: QuizRequest) -> Result<GeneratedContent, AppError> {
    let (content, citations) = facade.quiz(WorkspaceId(request.workspace_id), &request.topic, request.question_count.unwrap_or(5))?;
    Ok(GeneratedContent { content, citations })
}

#[derive(Debug, Deserialize)]
pub struct FlashcardsRequest {
    pub workspace_id: i64,
    pub topic: String,
    pub card_count: Option<u8>,
}

/// Flashcard Generator feature, composed on the Tutor Engine.
#[tauri::command]
pub fn assistant_flashcards(facade: State<'_, AppFacade>, request: FlashcardsRequest) -> Result<GeneratedContent, AppError> {
    let (content, citations) = facade.flashcards(WorkspaceId(request.workspace_id), &request.topic, request.card_count.unwrap_or(10))?;
    Ok(GeneratedContent { content, citations })
}

#[derive(Debug, Deserialize)]
pub struct RevisionPlanRequest {
    pub workspace_id: i64,
    pub concept_node_ids: Vec<i64>,
}

/// Revision Planner feature, composed on the Planner Engine, consuming
/// Student Memory's weakness data for the given concepts (§19).
#[tauri::command]
pub fn assistant_revision_plan(facade: State<'_, AppFacade>, request: RevisionPlanRequest) -> Result<String, AppError> {
    let ids: Vec<ConceptNodeId> = request.concept_node_ids.into_iter().map(ConceptNodeId).collect();
    facade.revision_plan(WorkspaceId(request.workspace_id), &ids)
}
