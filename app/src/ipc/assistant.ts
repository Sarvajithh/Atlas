import { ipcInvoke } from "@/ipc/client";

/** `assistant.*` namespace (§43.1). Mirrors backend `assistant_ask`/`assistant_cancel`. */
export function assistantAsk(question: string): Promise<string> {
  return ipcInvoke<string>("assistant_ask", { question });
}

export function assistantCancel(requestId: string): Promise<void> {
  return ipcInvoke<void>("assistant_cancel", { requestId });
}
