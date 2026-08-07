//! atlas-memory
//!
//! Student Memory domain logic (§7.3, §14, §19). Not regenerable from
//! source files — this is the user's learning history. Deleting a source
//! file or workspace link MUST NOT delete this data (§7.3).
//!
//! Defines repository interfaces only; concrete SQLite storage is provided
//! by atlas-db.

pub mod analytics_repository;
pub mod annotation_repository;
pub mod bookmark_repository;
pub mod chat_repository;
pub mod engine;
pub mod progress_repository;
pub mod testing;

pub use analytics_repository::AnalyticsRepository;
pub use annotation_repository::AnnotationRepository;
pub use bookmark_repository::BookmarkRepository;
pub use chat_repository::ChatRepository;
pub use engine::MemoryEngine;
pub use progress_repository::{LearningProgressRepository, StudyRepository};
pub use testing::{
    InMemoryAnalyticsRepository, InMemoryAnnotationRepository, InMemoryBookmarkRepository,
    InMemoryChatRepository, InMemoryLearningProgressRepository, InMemoryStudyRepository,
};
