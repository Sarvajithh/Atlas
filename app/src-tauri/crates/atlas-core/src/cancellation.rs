//! Fix 6 (P1 audit): `assistant.cancel` was a defined no-op -- this is the
//! request-lifecycle state that makes real cancellation possible.
//!
//! One `AtomicBool` flag per in-flight streaming request, keyed by the
//! `request_id` the frontend generates and passes to `assistant_ask_stream`
//! (`app/src/panels/AssistantPanel.tsx`, `app/src/ipc/assistant.ts`).
//! `AppFacade::chat_stream` registers a flag when it starts, checks it on
//! every chunk of the response, and unregisters it when the request ends
//! (success, error, or cancellation) via an RAII guard so every exit path
//! cleans up, not just the happy one. `assistant_cancel` just signals the
//! flag -- it doesn't need to know anything about Ollama, streaming, or
//! persistence.
//!
//! `Mutex<HashMap<..>>` follows the same convention already used for
//! `AppFacade::watchers`/`indexing_worker` (infrequent whole-map
//! operations -- register/unregister once per request -- not a hot path
//! needing anything fancier).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use atlas_utils::AppError;

pub struct CancellationRegistry {
    flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl CancellationRegistry {
    pub fn new() -> Self {
        Self { flags: Mutex::new(HashMap::new()) }
    }

    /// Register a new in-flight request, returning the flag its streaming
    /// loop should poll. If `request_id` is (surprisingly) already
    /// registered -- e.g. a reused id from a client bug -- this replaces
    /// the old entry with a fresh, un-cancelled flag rather than handing
    /// back a flag some unrelated earlier request might still cancel.
    pub fn register(&self, request_id: &str) -> Result<Arc<AtomicBool>, AppError> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut guard = self
            .flags
            .lock()
            .map_err(|_| AppError::user("cancellation registry lock poisoned"))?;
        guard.insert(request_id.to_string(), flag.clone());
        Ok(flag)
    }

    pub fn unregister(&self, request_id: &str) {
        // Cleanup is best-effort: a poisoned lock here shouldn't be a hard
        // error on the way out of an already-completed request (the
        // request's own result has already been determined either way).
        if let Ok(mut guard) = self.flags.lock() {
            guard.remove(request_id);
        }
    }

    /// Signal cancellation for `request_id`. Cancelling an id that isn't
    /// registered -- already completed, never existed, or a stale/unknown
    /// id from the client -- is a clean no-op, not an error: the caller's
    /// intent ("stop this, if it's still running") is already satisfied
    /// either way.
    pub fn cancel(&self, request_id: &str) -> Result<(), AppError> {
        let guard = self
            .flags
            .lock()
            .map_err(|_| AppError::user("cancellation registry lock poisoned"))?;
        if let Some(flag) = guard.get(request_id) {
            flag.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
}

impl Default for CancellationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard so `unregister` runs on every exit path of `chat_stream`
/// (success, an early `?`-propagated error, or cancellation) without
/// duplicating the cleanup call at each return point.
pub struct RegisteredRequest<'a> {
    registry: &'a CancellationRegistry,
    request_id: String,
    pub flag: Arc<AtomicBool>,
}

impl<'a> RegisteredRequest<'a> {
    pub fn new(registry: &'a CancellationRegistry, request_id: String) -> Result<Self, AppError> {
        let flag = registry.register(&request_id)?;
        Ok(Self { registry, request_id, flag })
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

impl Drop for RegisteredRequest<'_> {
    fn drop(&mut self) {
        self.registry.unregister(&self.request_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_an_unknown_request_id_is_a_clean_no_op() {
        let registry = CancellationRegistry::new();
        // No request with this id was ever registered.
        assert!(registry.cancel("unknown-request").is_ok());
    }

    #[test]
    fn cancelling_a_registered_request_sets_its_flag() {
        let registry = CancellationRegistry::new();
        let flag = registry.register("req-1").unwrap();
        assert!(!flag.load(Ordering::SeqCst));

        registry.cancel("req-1").unwrap();

        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn unregistering_then_cancelling_is_a_clean_no_op_not_an_error() {
        let registry = CancellationRegistry::new();
        let _flag = registry.register("req-2").unwrap();
        registry.unregister("req-2");

        // Already completed/cleaned-up requests behave exactly like an
        // unknown id -- cancel doesn't error just because it's "too late".
        assert!(registry.cancel("req-2").is_ok());
    }

    #[test]
    fn concurrent_requests_do_not_interfere_with_each_others_cancellation_state() {
        let registry = CancellationRegistry::new();
        let flag_a = registry.register("req-a").unwrap();
        let flag_b = registry.register("req-b").unwrap();

        registry.cancel("req-a").unwrap();

        assert!(flag_a.load(Ordering::SeqCst));
        assert!(!flag_b.load(Ordering::SeqCst));
    }

    #[test]
    fn registered_request_guard_unregisters_on_drop() {
        let registry = CancellationRegistry::new();
        {
            let _guard = RegisteredRequest::new(&registry, "req-scoped".to_string()).unwrap();
            // While the guard is alive, cancelling it has a real effect.
            registry.cancel("req-scoped").unwrap();
        }
        // Once dropped, the id is gone -- cancelling again is a no-op, not
        // an error, and doesn't resurrect or affect a new registration
        // reusing the same id.
        assert!(registry.cancel("req-scoped").is_ok());
        let flag = registry.register("req-scoped").unwrap();
        assert!(!flag.load(Ordering::SeqCst), "a fresh registration must start un-cancelled");
    }

    #[test]
    fn re_registering_the_same_id_gets_a_fresh_uncancelled_flag() {
        let registry = CancellationRegistry::new();
        let first = registry.register("req-reuse").unwrap();
        registry.cancel("req-reuse").unwrap();
        assert!(first.load(Ordering::SeqCst));

        let second = registry.register("req-reuse").unwrap();
        assert!(!second.load(Ordering::SeqCst));
    }
}
