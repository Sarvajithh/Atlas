import { create } from "zustand";

import { settingsGet, settingsSet } from "@/ipc/settings";

/**
 * Theme System (§13, §23 Settings). Light/dark mode is a Global-scope
 * setting persisted through the real `settings.*` IPC commands
 * (`settings_get`/`settings_set`), matching the existing `SettingEntry`
 * shape in `ipc/types.ts`. No `localStorage` is used -- settings state is
 * owned by the backend `settings` table, this store is a cache of it.
 */
export type Theme = "light" | "dark";

const THEME_SETTING_KEY = "ui.theme";

export interface ThemeState {
  theme: Theme;
  isLoaded: boolean;
  hydrate: () => Promise<void>;
  setTheme: (theme: Theme) => Promise<void>;
}

function applyThemeClass(theme: Theme) {
  document.documentElement.classList.toggle("dark", theme === "dark");
}

export const useThemeStore = create<ThemeState>((set) => ({
  theme: "light",
  isLoaded: false,

  hydrate: async () => {
    try {
      const entry = await settingsGet(THEME_SETTING_KEY);
      const theme: Theme = entry?.value === "dark" ? "dark" : "light";
      applyThemeClass(theme);
      set({ theme, isLoaded: true });
    } catch {
      // §45.1 Recoverable: settings backend unreachable (e.g. outside a
      // Tauri runtime during frontend-only development) -- fall back to
      // light mode rather than blocking the shell from rendering.
      applyThemeClass("light");
      set({ theme: "light", isLoaded: true });
    }
  },

  setTheme: async (theme) => {
    applyThemeClass(theme);
    set({ theme });
    await settingsSet({
      key: THEME_SETTING_KEY,
      value: theme,
      value_type: "string",
      scope: "Global",
      workspace_id: null,
      updated_at: new Date().toISOString(),
    });
  },
}));
