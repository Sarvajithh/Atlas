import { invoke } from "@tauri-apps/api/core";

/**
 * Shared typed wrapper around Tauri `invoke` (§12). Frontend `state` MUST
 * NOT call `fetch`/HTTP directly; all backend communication goes through
 * this module (§10 rule).
 */
export function ipcInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}
