/**
 * Assistant Panel (§8.1): dockable side panel, collapsible, never modal,
 * never full-screen by default (§8.3). Scoped to the current
 * document/workspace context. Chat UI and IPC wiring are deferred to the
 * UI implementation milestone.
 */
export function AssistantPanel() {
  return <aside aria-label="Assistant Panel" className="w-80 shrink-0 border-l" />;
}
