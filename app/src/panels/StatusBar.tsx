/**
 * Status Bar (§8.1): background indexing progress, current engine
 * activity, never blocks interaction. Live status wiring (indexing events,
 * §12) is deferred to the UI implementation milestone.
 */
export function StatusBar() {
  return <footer aria-label="Status Bar" className="h-6 shrink-0 border-t" />;
}
