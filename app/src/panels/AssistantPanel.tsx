import { useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";

import {
  assistantAskStream,
  assistantCancel,
  assistantGetSessionMessages,
  assistantListSessions,
  type AssistantIntent,
} from "@/ipc/assistant";
import { documentGet } from "@/ipc/document";
import type { ChatMessage, ChatSession, Citation } from "@/ipc/types";
import { useAppStore } from "@/state/store";
import { useDocumentStore } from "@/state/documents";

/**
 * Assistant Panel (§8.1, Part 1 of the AI Assistant vertical slice):
 * dockable side panel, collapsible, never modal, never full-screen by
 * default (§8.3). Scoped to the current workspace/document context
 * (Part 3). Talks to the backend exclusively through `ipc/assistant.ts`
 * (§43.1 `assistant.*`) -- the full Retrieval -> Context Builder ->
 * Prompt Builder -> Tutor/Reasoning Engine -> Citations pipeline (§15,
 * §39, §40) already lives in `atlas-core::AppFacade`; this component only
 * renders it and manages Session Manager state (§33.10/§33.11, Part 5).
 */

interface DisplayMessage extends Omit<ChatMessage, "id"> {
  id: number | string;
  citations?: Citation[];
  pending?: boolean;
  failed?: boolean;
}

const INTENT_OPTIONS: { value: AssistantIntent; label: string }[] = [
  { value: "tutoring", label: "Tutor" },
  { value: "factual_lookup", label: "Quick answer" },
  { value: "research", label: "Research" },
];

function locationLabel(ref: string): string {
  const pageMatch = /^page:(\d+)$/.exec(ref);
  if (pageMatch) return `page ${pageMatch[1]}`;
  return ref;
}

export function AssistantPanel() {
  const activeWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const setActiveWorkspaceId = useAppStore((s) => s.setActiveWorkspaceId);
  const setCurrentView = useAppStore((s) => s.setCurrentView);
  const pushToast = useAppStore((s) => s.pushToast);

  const openTabs = useDocumentStore((s) => s.openTabs);
  const activeDocTabId = useDocumentStore((s) => s.activeTabId);
  const activeDocTab = useMemo(
    () => openTabs.find((t) => t.tabId === activeDocTabId) ?? null,
    [openTabs, activeDocTabId],
  );
  const openDocument = useDocumentStore((s) => s.openDocument);
  const navigateToLocation = useDocumentStore((s) => s.navigateToLocation);

  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [input, setInput] = useState("");
  const [intent, setIntent] = useState<AssistantIntent>("tutoring");
  const [isGenerating, setIsGenerating] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selectedText, setSelectedText] = useState<string | null>(null);
  const [showSessions, setShowSessions] = useState(false);

  const stopRef = useRef<{ stopListening: () => void; requestId: string } | null>(null);
  const lastUserMessageRef = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const localIdRef = useRef(0);

  // Part 3 "Workspace Context Awareness": track the current selection
  // inside the document area so the user can ask "explain this" without
  // manually attaching anything. Only captured while the panel would use
  // it (selection outside the app, e.g. in the assistant's own message
  // list, is intentionally not treated as document context).
  useEffect(() => {
    function onSelectionChange() {
      const text = window.getSelection()?.toString().trim();
      setSelectedText(text && text.length > 0 ? text : null);
    }
    document.addEventListener("selectionchange", onSelectionChange);
    return () => document.removeEventListener("selectionchange", onSelectionChange);
  }, []);

  // Conversation Memory (Part 5): load this workspace's sessions whenever
  // the active workspace changes, and start fresh (no session picked yet)
  // rather than silently reusing a stale session id from another
  // workspace.
  useEffect(() => {
    setSessionId(null);
    setMessages([]);
    setSessions([]);
    if (activeWorkspaceId === null) return;
    assistantListSessions(activeWorkspaceId)
      .then(setSessions)
      .catch(() => {
        // §45.1 recoverable: the session picker is supplementary; a
        // failure here shouldn't block asking a fresh question.
      });
  }, [activeWorkspaceId]);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [messages]);

  useEffect(
    () => () => {
      if (stopRef.current) {
        stopRef.current.stopListening();
        assistantCancel(stopRef.current.requestId).catch(() => {
          // §45.1 recoverable: unmounting is not the place to surface a
          // cancel failure to the user.
        });
      }
    },
    [],
  );

  async function resumeSession(id: number) {
    setLoadError(null);
    setShowSessions(false);
    try {
      const history = await assistantGetSessionMessages(id);
      setSessionId(id);
      setMessages(history.map((m) => ({ ...m })));
    } catch (err) {
      setLoadError(err instanceof Error ? err.message : String(err));
    }
  }

  function startNewChat() {
    setSessionId(null);
    setMessages([]);
    setShowSessions(false);
  }

  function buildQuestion(raw: string): string {
    // Part 3: fold in current-document context automatically so
    // "Explain this theorem" resolves without the user attaching a file.
    // The backend's retrieval is workspace-scoped (§18), not yet
    // filterable by a single document over IPC, so this is a best-effort
    // grounding hint rather than a hard filter -- disclosed here, not
    // silently assumed to restrict retrieval.
    const contextParts: string[] = [];
    if (activeDocTab) contextParts.push(`current document: ${activeDocTab.relativePath}`);
    if (selectedText) contextParts.push(`selected text: "${selectedText.slice(0, 500)}"`);
    if (contextParts.length === 0) return raw;
    return `[Context - ${contextParts.join("; ")}]\n\n${raw}`;
  }

  async function send(rawQuestion: string) {
    if (!rawQuestion.trim() || activeWorkspaceId === null || isGenerating) return;
    // TEMPORARY TRACE LOGGING -- remove once the chat pipeline is confirmed working.
    console.log("[Assistant] Message submitted", { workspaceId: activeWorkspaceId, sessionId, intent });

    const question = buildQuestion(rawQuestion);
    lastUserMessageRef.current = rawQuestion;
    setLoadError(null);
    setInput("");

    const userMsgId = `local-user-${++localIdRef.current}`;
    const assistantMsgId = `local-assistant-${localIdRef.current}`;
    const now = new Date().toISOString();

    setMessages((prev) => [
      ...prev,
      { id: userMsgId, session_id: sessionId ?? 0, role: "User", content: rawQuestion, engine_pipeline_used: null, created_at: now },
      { id: assistantMsgId, session_id: sessionId ?? 0, role: "Assistant", content: "", engine_pipeline_used: null, created_at: now, pending: true },
    ]);
    setIsGenerating(true);

    const handle = await assistantAskStream(
      { workspaceId: activeWorkspaceId, question, sessionId, intent },
      {
        onChunk: (chunk) => {
          // TEMPORARY TRACE LOGGING
          console.log("[Assistant] chunk received", { length: chunk.length });
          setMessages((prev) =>
            prev.map((m) => (m.id === assistantMsgId ? { ...m, content: m.content + chunk } : m)),
          );
        },
        onDone: (result) => {
          // TEMPORARY TRACE LOGGING
          console.log("[Assistant] done", { sessionId: result.sessionId, citations: result.citations.length });
          setSessionId(result.sessionId);
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantMsgId
                ? { ...m, id: result.message.id, content: result.message.content, citations: result.citations, pending: false }
                : m,
            ),
          );
          setIsGenerating(false);
          stopRef.current = null;
          assistantListSessions(activeWorkspaceId).then(setSessions).catch(() => {});
        },
        onError: (message) => {
          // TEMPORARY TRACE LOGGING
          console.log("[Assistant] error", { message });
          setMessages((prev) =>
            prev.map((m) => (m.id === assistantMsgId ? { ...m, pending: false, failed: true, content: m.content } : m)),
          );
          setLoadError(message);
          setIsGenerating(false);
          stopRef.current = null;
        },
      },
    );
    stopRef.current = handle;
  }

  function stopGeneration() {
    // Fix 6 (P1 audit): `assistant_cancel` now performs real backend
    // cancellation -- the model stops generating and no further chunks
    // are forwarded, not just the UI detaching from a stream it ignores.
    // `stopListening()` still runs too, so the UI stops reacting the
    // instant the button is pressed rather than waiting on the cancel
    // round-trip.
    const handle = stopRef.current;
    stopRef.current = null;
    handle?.stopListening();
    setIsGenerating(false);
    setMessages((prev) => prev.map((m) => (m.pending ? { ...m, pending: false } : m)));
    pushToast({ kind: "info", message: "Stopping generation..." });
    if (handle) {
      assistantCancel(handle.requestId).catch((err) => {
        console.log("[Assistant] assistant_cancel failed", err);
      });
    }
  }

  function retryLast() {
    if (!lastUserMessageRef.current) return;
    setMessages((prev) => {
      const lastFailedIdx = [...prev].reverse().findIndex((m) => m.role === "Assistant" && m.failed);
      if (lastFailedIdx === -1) return prev;
      const idx = prev.length - 1 - lastFailedIdx;
      return prev.slice(0, idx - 1 >= 0 && prev[idx - 1]?.role === "User" ? idx - 1 : idx);
    });
    void send(lastUserMessageRef.current);
  }

  async function copyMessage(content: string) {
    try {
      await navigator.clipboard.writeText(content);
      pushToast({ kind: "success", message: "Copied to clipboard." });
    } catch {
      pushToast({ kind: "error", message: "Couldn't copy to clipboard." });
    }
  }

  async function openCitation(citation: Citation) {
    try {
      const doc = await documentGet(citation.document_id);
      if (!doc) {
        pushToast({ kind: "error", message: "Source document is no longer available." });
        return;
      }
      setActiveWorkspaceId(doc.workspace_id);
      setCurrentView("workspace-detail");
      openDocument(doc.workspace_id, doc);
      navigateToLocation(doc.id, citation.location_ref);
    } catch (err) {
      pushToast({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    }
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send(input);
    }
  }

  if (activeWorkspaceId === null) {
    return (
      <aside aria-label="Assistant Panel" className="flex w-80 shrink-0 flex-col border-l">
        <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">
          Open a workspace to start a conversation with your tutor.
        </div>
      </aside>
    );
  }

  return (
    <aside aria-label="Assistant Panel" className="flex w-96 shrink-0 flex-col border-l bg-background">
      <header className="flex shrink-0 items-center justify-between gap-2 border-b px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <span className="text-sm font-medium">Tutor</span>
          {activeDocTab ? (
            <span className="truncate rounded-full bg-accent px-2 py-0.5 text-xs text-muted-foreground" title={activeDocTab.relativePath}>
              {activeDocTab.title}
            </span>
          ) : null}
        </div>
        <div className="flex items-center gap-1">
          <button type="button" onClick={() => setShowSessions((v) => !v)} className="rounded px-2 py-1 text-xs hover:bg-accent" aria-expanded={showSessions}>
            History
          </button>
          <button type="button" onClick={startNewChat} className="rounded px-2 py-1 text-xs hover:bg-accent">
            New
          </button>
        </div>
      </header>

      {showSessions ? (
        <div className="max-h-40 shrink-0 overflow-y-auto border-b" role="listbox" aria-label="Previous conversations">
          {sessions.length === 0 ? (
            <p className="p-3 text-xs text-muted-foreground">No previous conversations in this workspace yet.</p>
          ) : (
            sessions.map((s) => (
              <button
                key={s.id}
                type="button"
                role="option"
                aria-selected={s.id === sessionId}
                onClick={() => void resumeSession(s.id)}
                className={`block w-full truncate px-3 py-2 text-left text-xs hover:bg-accent ${s.id === sessionId ? "bg-accent" : ""}`}
              >
                {s.title || "Untitled conversation"}
              </button>
            ))
          )}
        </div>
      ) : null}

      <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-3">
        {messages.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center text-sm text-muted-foreground">
            <p>Ask about {activeDocTab ? activeDocTab.title : "your workspace"} — I'll explain, not just quote.</p>
            <p className="text-xs">Try "Explain this" or "Summarize this chapter".</p>
          </div>
        ) : (
          <ul className="flex flex-col gap-4">
            {messages.map((m) => (
              <li key={m.id} className={m.role === "User" ? "flex justify-end" : "flex justify-start"}>
                <div
                  className={`max-w-[90%] rounded-lg px-3 py-2 text-sm ${
                    m.role === "User" ? "bg-primary text-primary-foreground" : "bg-accent"
                  }`}
                >
                  {m.pending && m.content.length === 0 ? (
                    <span className="flex items-center gap-1 text-xs text-muted-foreground" role="status" aria-live="polite">
                      <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-current" />
                      Thinking…
                    </span>
                  ) : (
                    <div className="assistant-markdown text-sm">
                      <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]}>
                        {m.content}
                      </ReactMarkdown>
                    </div>
                  )}

                  {m.failed ? <p className="mt-1 text-xs text-destructive">Generation failed.</p> : null}

                  {m.role === "Assistant" && !m.pending && m.content ? (
                    <div className="mt-2 flex items-center gap-2 border-t border-border/50 pt-1.5">
                      <button type="button" onClick={() => void copyMessage(m.content)} className="text-xs text-muted-foreground hover:text-foreground">
                        Copy
                      </button>
                      {m.failed ? (
                        <button type="button" onClick={retryLast} className="text-xs text-muted-foreground hover:text-foreground">
                          Retry
                        </button>
                      ) : null}
                    </div>
                  ) : null}

                  {m.citations && m.citations.length > 0 ? (
                    <div className="mt-2 flex flex-col gap-1 border-t border-border/50 pt-1.5">
                      <span className="text-[10px] uppercase tracking-wide text-muted-foreground">Sources</span>
                      {m.citations.map((c, i) => (
                        <button
                          key={`${c.chunk_id}-${i}`}
                          type="button"
                          onClick={() => void openCitation(c)}
                          className="rounded border border-border/60 px-2 py-1 text-left text-xs hover:bg-background"
                          title={c.snippet}
                        >
                          <span className="font-medium">{locationLabel(c.location_ref)}</span>
                          <span className="ml-1 text-muted-foreground">— {c.snippet.slice(0, 80)}</span>
                        </button>
                      ))}
                    </div>
                  ) : null}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>

      {loadError ? (
        <div role="alert" className="shrink-0 border-t bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {loadError}
        </div>
      ) : null}

      <div className="shrink-0 border-t p-2">
        {selectedText ? (
          <div className="mb-1.5 flex items-center justify-between rounded bg-accent px-2 py-1 text-xs text-muted-foreground">
            <span className="truncate">Using selection: "{selectedText.slice(0, 60)}"</span>
            <button type="button" onClick={() => setSelectedText(null)} aria-label="Clear selection context" className="ml-2 shrink-0 hover:text-foreground">
              ×
            </button>
          </div>
        ) : null}

        <div className="mb-1.5 flex items-center gap-1">
          {INTENT_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => setIntent(opt.value)}
              aria-pressed={intent === opt.value}
              className={`rounded-full px-2 py-0.5 text-[11px] ${
                intent === opt.value ? "bg-primary text-primary-foreground" : "bg-accent text-muted-foreground hover:text-foreground"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>

        <div className="flex items-end gap-2">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Ask your tutor…"
            rows={2}
            aria-label="Ask the assistant"
            className="min-h-[2.5rem] flex-1 resize-none rounded-md border bg-background px-2 py-1.5 text-sm"
          />
          {isGenerating ? (
            <button type="button" onClick={stopGeneration} className="shrink-0 rounded-md border px-3 py-1.5 text-xs hover:bg-accent">
              Stop
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void send(input)}
              disabled={!input.trim()}
              className="shrink-0 rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-50"
            >
              Send
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}
