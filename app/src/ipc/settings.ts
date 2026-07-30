import { ipcInvoke } from "@/ipc/client";
import type { SettingEntry } from "@/ipc/types";

/** `settings.*` namespace (§43.1). Mirrors backend `settings_get`/`settings_set`. */
export function settingsGet(key: string): Promise<SettingEntry | null> {
  return ipcInvoke<SettingEntry | null>("settings_get", { key });
}

export function settingsSet(entry: SettingEntry): Promise<void> {
  return ipcInvoke<void>("settings_set", { entry });
}
