import { useThemeStore } from "@/state/theme";

/**
 * Settings (§8.2.7). This milestone wires the one setting with a real,
 * exercised round trip: theme (§13/§23), via `settings_get`/`settings_set`
 * (§43.1). Ollama connection / model-per-engine overrides / indexing
 * preferences are real backend settings rows too, but their concrete
 * setting keys and value shapes are not defined anywhere in the frontend
 * or backend commands yet -- adding UI for them here would mean
 * inventing keys, which is exactly the "no hardcoded values" this
 * project prohibits (§46.1).
 */
export function SettingsView() {
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.setTheme);

  return (
    <section aria-label="Settings" className="mx-auto max-w-2xl overflow-auto p-6">
      <h1 className="mb-6 text-xl font-semibold">Settings</h1>

      <div className="rounded-lg border p-4">
        <h2 className="mb-3 text-sm font-semibold">Appearance</h2>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm">Theme</p>
            <p className="text-xs text-muted-foreground">Persisted globally via Settings (§23).</p>
          </div>
          <div className="flex gap-1 rounded-md border p-0.5">
            <button
              type="button"
              aria-pressed={theme === "light"}
              onClick={() => setTheme("light")}
              className={`rounded px-2 py-1 text-xs ${theme === "light" ? "bg-accent" : ""}`}
            >
              Light
            </button>
            <button
              type="button"
              aria-pressed={theme === "dark"}
              onClick={() => setTheme("dark")}
              className={`rounded px-2 py-1 text-xs ${theme === "dark" ? "bg-accent" : ""}`}
            >
              Dark
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}
