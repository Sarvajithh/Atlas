import { create } from "zustand";

import { settingsGet, settingsSet } from "@/ipc/settings";

/**
 * Global Resizable Panel System -- persistence layer.
 *
 * Follows the same pattern as `state/theme.ts`: no `localStorage`, panel
 * widths are cached client-side but owned by the backend `settings` table
 * via the real `settings.*` IPC commands (`settings_get`/`settings_set`).
 *
 * All panel widths are stored together as a single JSON blob under one
 * settings key (`ui.layout.panelWidths`) rather than one key per panel,
 * because the existing `settings.*` surface only exposes `get`/`set` by
 * exact key -- there is no `settings_list` to discover keys dynamically.
 * A single blob also means one IPC round trip on startup instead of one
 * per resizable panel.
 */

const LAYOUT_SETTING_KEY = "ui.layout.panelWidths";

export interface LayoutState {
  /** panelId -> width in px */
  widths: Record<string, number>;
  isLoaded: boolean;
  hydrate: () => Promise<void>;
  /** Set a panel's width. Persists to the backend (debounced per panel). */
  setWidth: (panelId: string, width: number) => void;
  /** Remove a stored override, letting the panel fall back to its default. */
  resetWidth: (panelId: string) => void;
}

// Per-panel debounce timers so rapid drag events don't spam settings_set.
const pendingWrites = new Map<string, ReturnType<typeof setTimeout>>();
const PERSIST_DEBOUNCE_MS = 250;

function persist(widths: Record<string, number>) {
  void settingsSet({
    key: LAYOUT_SETTING_KEY,
    value: JSON.stringify(widths),
    value_type: "json",
    scope: "Global",
    workspace_id: null,
    updated_at: new Date().toISOString(),
  });
}

export const useLayoutStore = create<LayoutState>((set, get) => ({
  widths: {},
  isLoaded: false,

  hydrate: async () => {
    try {
      const entry = await settingsGet(LAYOUT_SETTING_KEY);
      if (entry?.value) {
        const parsed = JSON.parse(entry.value) as unknown;
        if (parsed && typeof parsed === "object") {
          set({ widths: parsed as Record<string, number>, isLoaded: true });
          return;
        }
      }
      set({ isLoaded: true });
    } catch {
      // §45.1 Recoverable: settings backend unreachable (e.g. outside a
      // Tauri runtime during frontend-only dev) or a corrupt stored blob --
      // fall back to component-supplied defaults rather than blocking render.
      set({ widths: {}, isLoaded: true });
    }
  },

  setWidth: (panelId, width) => {
    const next = { ...get().widths, [panelId]: Math.round(width) };
    set({ widths: next });

    const existing = pendingWrites.get(panelId);
    if (existing) clearTimeout(existing);
    pendingWrites.set(
      panelId,
      setTimeout(() => {
        pendingWrites.delete(panelId);
        persist(useLayoutStore.getState().widths);
      }, PERSIST_DEBOUNCE_MS),
    );
  },

  resetWidth: (panelId) => {
    const next = { ...get().widths };
    delete next[panelId];
    set({ widths: next });
    persist(next);
  },
}));
