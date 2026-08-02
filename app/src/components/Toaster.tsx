import { useAppStore } from "@/state/store";

const KIND_STYLES: Record<string, string> = {
  info: "border-border bg-card text-foreground",
  success: "border-green-600/40 bg-card text-foreground",
  error: "border-destructive/50 bg-card text-destructive",
};

/** Toast Notifications (§13). Non-blocking, stacked bottom-right, self-dismissable. */
export function Toaster() {
  const toasts = useAppStore((s) => s.toasts);
  const dismissToast = useAppStore((s) => s.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div
      aria-live="assertive"
      className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-80 flex-col gap-2"
    >
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role="status"
          className={`pointer-events-auto flex items-start justify-between gap-2 rounded-md border px-3 py-2 text-sm shadow-md ${KIND_STYLES[toast.kind]}`}
        >
          <span>{toast.message}</span>
          <button
            type="button"
            aria-label="Dismiss notification"
            onClick={() => dismissToast(toast.id)}
            className="text-muted-foreground hover:text-foreground"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
