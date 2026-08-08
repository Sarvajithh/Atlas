//! Quiz / Flashcard structured shapes (§43.2 "Quiz Generator" / "Flashcard
//! Generator" feature extensions).
//!
//! Prior to this, `AppFacade::quiz`/`flashcards` returned a single opaque
//! `String` of whatever prose the model produced -- unusable for grading,
//! scoring, or spaced repetition, and undistinguishable from a plain chat
//! answer. These types give the model a fixed structured shape to fill
//! (same "structured-output Reasoning-role call" pattern
//! `atlas-graph::extraction` already uses for Concept Graph extraction),
//! so answers can actually be graded and fed into Student Memory
//! (`learning_progress`, §33.18) instead of being a dead-end transcript.

use serde::{Deserialize, Serialize};

use crate::ids::ConceptNodeId;
use crate::retrieval::Citation;

/// One multiple-choice quiz question. `correct_index` indexes into
/// `options`; kept 0-based and validated (`correct_index < options.len()`)
/// wherever this is constructed from model output, so a malformed model
/// response can be rejected rather than silently producing an ungradeable
/// or out-of-range question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedQuiz {
    pub topic: String,
    pub questions: Vec<QuizQuestion>,
    pub citations: Vec<Citation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flashcard {
    pub front: String,
    pub back: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFlashcards {
    pub topic: String,
    pub cards: Vec<Flashcard>,
    pub citations: Vec<Citation>,
}

/// Per-question grading detail returned alongside the aggregate score, so
/// the UI can show which answers were right/wrong without re-deriving it
/// client-side from the original question set (the correct answer is only
/// authoritative server-side).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizAnswerResult {
    pub question_index: usize,
    pub selected_index: Option<usize>,
    pub correct_index: usize,
    pub correct: bool,
}

/// Result of grading a completed quiz attempt (§19 Student Memory). If the
/// quiz's `topic` matches an existing Concept Graph node (case-insensitive
/// label match, same lookup Concept Extraction dedup already uses), the
/// grade is also persisted into `learning_progress` and
/// `matched_concept_node_id` is set; otherwise progress is intentionally
/// *not* fabricated for a topic string with no corresponding concept --
/// `matched_concept_node_id` is `None` and the caller can tell the two
/// cases apart rather than assuming every quiz updates memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizGradeResult {
    pub correct_count: usize,
    pub total_count: usize,
    pub score: f32,
    pub results: Vec<QuizAnswerResult>,
    pub matched_concept_node_id: Option<ConceptNodeId>,
}
