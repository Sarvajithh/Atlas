//! Resource Manager (§38). Owns finite local hardware resources so Engines
//! never negotiate resource access with each other directly. Does not know
//! what any Engine role "means" semantically -- only manages slots/budgets
//! (§38.2).

use atlas_utils::AppError;

/// Outcome of a slot request (§38.2).
pub enum SlotGrant {
    Granted,
    Queued,
    Denied { reason: String },
}

pub struct ResourceManager {
    /// Configurable concurrency budget (§38.1); not hardcoded.
    concurrency_budget: u32,
}

impl ResourceManager {
    pub fn new(concurrency_budget: u32) -> Self {
        Self { concurrency_budget }
    }

    pub fn concurrency_budget(&self) -> u32 {
        self.concurrency_budget
    }

    /// Request a slot for an inference call (§38.2). Actual scheduling logic
    /// is deferred to a future milestone.
    pub fn request_slot(&self) -> Result<SlotGrant, AppError> {
        Ok(SlotGrant::Granted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_budget_is_configurable_not_hardcoded() {
        assert_eq!(ResourceManager::new(4).concurrency_budget(), 4);
        assert_eq!(ResourceManager::new(1).concurrency_budget(), 1);
    }

    #[test]
    fn request_slot_grants_by_default() {
        let manager = ResourceManager::new(4);
        assert!(matches!(
            manager.request_slot().unwrap(),
            SlotGrant::Granted
        ));
    }
}
