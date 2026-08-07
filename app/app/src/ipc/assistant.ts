import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { ipcInvoke } from "@/ipc/client";
import type {
  AssistantAnswer,
  ChatMessage,
  ChatSession,
  Citation,
  FlashcardSet,
  Quiz,
  RevisionPlan,
} from "@/ipc/types";

/**
 * `assistant.*` namespace (§43.1). Mirrors backend `assistant_*` Tauri
 * commands (`app-tauri/src/commands/assistant.rs`), including the
 * Conversation Memory read path (§33.10/§33.11) and the streaming
 * counterpart which forwards tokens over Tauri events
 * (`assistant://chunk`, `assistant://done`, `assistant://error`, §12).
 */

export type AssistantIntent = "tutoring" | "factual_lookup" | "quiz" | "research" | "planning";

export interface AssistantAskArgs {
  workspaceId: number;
  question: string;
  sessionId?: number | null;
  intent?: AssistantIntent;
  images?: string[];
}

/** Non-streaming turn: waits for the full answer + citations (§43.1 `assistant.ask`). */
export function assistantAsk(args: AssistantAskArgs): Promise<AssistantAnswer> {
  return ipcInvoke<AssistantAnswer>("assistant_ask", {
    workspaceId: args.workspaceId,
    question: args.question,
    sessionId: args.sessionId ?? null,
    intent: args.intent ?? null,
    images: args.images ?? null,
  });
}

/**
 * §43.1 `assistant.cancel` (Fix 6, P1 audit: real cancellation, no longer
 * a defined "not implemented" error). `requestId` is the same id passed to
 * `assistantAskStream` for the request being cancelled -- see that
 * function's doc comment for where it comes from. A `requestId` that's
 * unknown or already finished resolves successfully (the backend
 * registry's own clean-no-op contract), so callers don't need to guard
 * against "already done" themselves.
 */
export function assistantCancel(requestId: string): Promise<void> {
  return ipcInvoke<void>("assistant_cancel", { requestId });
}

interface StreamChunkPayload {
  session_id: number;
  content: string;
}
interface StreamDonePayload {
  session_id: number;
  message: ChatMessage;
  citations: Citation[];
}
interface StreamErrorPayload {
  message: string;
}

export interface AssistantStreamHandlers {
  onChunk: (content: string) => void;
  onDone: (result: { sessionId: number; message: ChatMessage; citations: Citation[] }) => void;
  onError: (message: string) => void;
}

/**
 * Streaming turn (§43.1 `assistant.ask` streaming counterpart, Part 1
 * "Streaming responses"/"Stop generation"). Subscribes to the three
 * `assistant://*` events the backend emits, invokes `assistant_ask_stream`,
 * and returns a `requestId` (Fix 6, P1 audit) plus a `stopListening()`
 * handle. `requestId` is generated here, client-side, and is the same id
 * the backend registers the in-flight request under (`AppFacade::chat_stream`
 * via its `CancellationRegistry`) -- pass it to `assistantCancel(requestId)`
 * to actually stop generation server-side, in addition to (not instead of)
 * calling `stopListening()` to stop the UI reacting to a response already
 * in flight.
 */
export async function assistantAskStream(
  args: AssistantAskArgs,
  handlers: AssistantStreamHandlers,
): Promise<{ stopListening: () => void; requestId: string }> {
  const unlistenFns: UnlistenFn[] = [];
  let settled = false;
  const requestId = crypto.randomUUID();

  const cleanup = () => {
    for (const fn of unlistenFns) fn();
  };

  try {
    unlistenFns.push(
      await listen<StreamChunkPayload>("assistant://chunk", (event) => {
        if (!settled) handlers.onChunk(event.payload.content);
      }),
    );
    unlistenFns.push(
      await listen<StreamDonePayload>("assistant://done", (event) => {
        if (settled) return;
        settled = true;
        handlers.onDone({
          sessionId: event.payload.session_id,
          message: event.payload.message,
          citations: event.payload.citations,
        });
        cleanup();
      }),
    );
    unlistenFns.push(
      await listen<StreamErrorPayload>("assistant://error", (event) => {
        if (settled) return;
        settled = true;
        handlers.onError(event.payload.message);
        cleanup();
      }),
    );
  } catch (err) {
    // event.listen() itself failed (e.g. a missing Tauri capability
    // permission -- "event.listen not allowed") -- without this catch,
    // the failure was an *uncaught* promise rejection: assistant_ask_stream
    // was never invoked, no assistant://error ever fires, and the UI sat
    // on "Thinking..." forever with no visible error. Surface it exactly
    // like a stream error instead.
    cleanup();
    handlers.onError(err instanceof Error ? err.message : String(err));
    return { stopListening: cleanup, requestId };
  }

  ipcInvoke<void>("assistant_ask_stream", {
    workspaceId: args.workspaceId,
    question: args.question,
    sessionId: args.sessionId ?? null,
    intent: args.intent ?? null,
    images: args.images ?? null,
    requestId,
  })
    // TEMPORARY TRACE LOGGING -- confirms the invoke() promise itself
    // settles (i.e. the Tauri command returned/threw) independently of
    // the assistant://* events.
    .then(() => console.log("[IPC] assistant_ask_stream invoke resolved"))
    .catch((err) => {
      console.log("[IPC] assistant_ask_stream invoke rejected", err);
      if (settled) return;
      settled = true;
      handlers.onError(err instanceof Error ? err.message : String(err));
      cleanup();
    });

  console.log("[IPC] assistant_ask_stream invoked");

  return { stopListening: cleanup, requestId };
}

/** Conversation Memory (§33.10, Part 5): sessions for a workspace, most-recently-updated first. */
export function assistantListSessions(workspaceId: number): Promise<ChatSession[]> {
  return ipcInvoke<ChatSession[]>("assistant_list_sessions", { workspaceId });
}

/** Conversation Memory (§33.11, Part 5): full message history to resume a previous chat. */
export function assistantGetSessionMessages(sessionId: number): Promise<ChatMessage[]> {
  return ipcInvoke<ChatMessage[]>("assistant_get_session_messages", { sessionId });
}

export function assistantQuiz(
  workspaceId: number,
  topic: string,
  options?: { documentId?: number | null; questionCount?: number },
): Promise<Quiz> {
  return ipcInvoke<Quiz>("assistant_quiz", {
    request: {
      workspace_id: workspaceId,
      topic,
      document_id: options?.documentId ?? null,
      question_count: options?.questionCount ?? null,
    },
  });
}

export function assistantFlashcards(
  workspaceId: number,
  topic: string,
  options?: { documentId?: number | null; cardCount?: number },
): Promise<FlashcardSet> {
  return ipcInvoke<FlashcardSet>("assistant_flashcards", {
    request: {
      workspace_id: workspaceId,
      topic,
      document_id: options?.documentId ?? null,
      card_count: options?.cardCount ?? null,
    },
  });
}

/**
 * Revision Planner (§ Learning subsystem): no longer takes a caller-picked
 * list of concept ids -- the backend derives its input from the *computed*
 * weak-topic aggregate itself (`AnalyticsRepository::list_weak_topics`),
 * so this only needs the workspace to plan for.
 */
export function assistantRevisionPlan(workspaceId: number): Promise<RevisionPlan> {
  return ipcInvoke<RevisionPlan>("assistant_revision_plan", {
    request: { workspace_id: workspaceId },
  });
}

/** Re-open a previously generated quiz by id (e.g. resuming an exam). */
export function assistantGetQuiz(quizId: number): Promise<Quiz | null> {
  return ipcInvoke<Quiz | null>("assistant_get_quiz", { quizId });
}

/** Every quiz generated for a workspace, most recent first. */
export function assistantListQuizzes(workspaceId: number): Promise<Quiz[]> {
  return ipcInvoke<Quiz[]>("assistant_list_quizzes", { workspaceId });
}

/** Every flashcard set generated for a workspace, most recent first. */
export function assistantListFlashcardSets(workspaceId: number): Promise<FlashcardSet[]> {
  return ipcInvoke<FlashcardSet[]>("assistant_list_flashcard_sets", { workspaceId });
}

/** Every revision plan generated for a workspace, most recent first. */
export function assistantListRevisionPlans(workspaceId: number): Promise<RevisionPlan[]> {
  return ipcInvoke<RevisionPlan[]>("assistant_list_revision_plans", { workspaceId });
}

/**
 * Record one answered quiz question's outcome (§ Learning subsystem
 * weak-topic detection). `topic` is the parent `Quiz.topic` -- individual
 * questions don't carry their own topic tag. Called once per question when
 * `QuizExamMode` submits an attempt; feeds the aggregate
 * `memoryListWeakTopics` (`ipc/memory.ts`) reads.
 */
export function assistantSubmitQuizAnswer(workspaceId: number, topic: string, correct: boolean): Promise<void> {
  return ipcInvoke<void>("assistant_submit_quiz_answer", {
    submission: { workspace_id: workspaceId, topic, correct },
  });
}
