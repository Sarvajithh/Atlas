import { useAppStore } from "@/state/store";
import { cn } from "@/state/utils";

/**
 * Tabs (§8.1, multi-tab documents). This milestone's tabs represent open
 * *workspace views* (there being no `document.*` IPC yet to open an
 * individual document in its own tab); the Dashboard is always tab-less
 * and reachable from the Activity Rail, matching §8.3's "never open into
 * a blank chat box" -- the app defaults to Dashboard, not a tab.
 */
export function Tabs() {
  const tabs = useAppStore((s) => s.tabs);
  const activeTabId = useAppStore((s) => s.activeTabId);
  const setActiveTab = useAppStore((s) => s.setActiveTab);
  const closeTab = useAppStore((s) => s.closeTab);
  const setCurrentView = useAppStore((s) => s.setCurrentView);

  if (tabs.length === 0) return null;

  return (
    <div role="tablist" aria-label="Open tabs" className="flex h-9 shrink-0 items-stretch border-b text-sm">
      <button
        type="button"
        role="tab"
        aria-selected={tabs.length > 0 && activeTabId === null}
        onClick={() => {
          setCurrentView("dashboard");
        }}
        className="border-r px-3 text-muted-foreground hover:bg-accent"
      >
        Dashboard
      </button>
      {tabs.map((tab) => (
        <div
          key={tab.id}
          role="tab"
          aria-selected={tab.id === activeTabId}
          className={cn(
            "flex items-center gap-2 border-r px-3",
            tab.id === activeTabId ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/60",
          )}
        >
          <button type="button" onClick={() => setActiveTab(tab.id)} className="max-w-[12rem] truncate">
            {tab.title}
          </button>
          <button
            type="button"
            aria-label={`Close ${tab.title} tab`}
            onClick={() => closeTab(tab.id)}
            className="text-muted-foreground hover:text-foreground"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
