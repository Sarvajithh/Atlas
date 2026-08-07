//! Structured-output parsing/validation for the Learning subsystem's
//! generative features (Quiz Generator, Flashcard Generator, Revision
//! Planner).
//!
//! Per the implementation plan: the model is prompted (via
//! `prompt_builder.rs`) to return structured JSON, and this module parses
//! and *validates* that JSON in Rust -- a syntactically valid JSON blob
//! that doesn't actually satisfy the contract (e.g. a quiz question whose
//! `correct_answer` isn't one of its own `options`) is treated the same as
//! malformed JSON: a `Recoverable` `AppError`, never a silent pass-through
//! of unusable data.
//!
//! These `Generated*` types deliberately omit `id`/`workspace_id`/
//! `document_id`/`created_at` -- those are assigned when a repository
//! persists the result (§7's Student Memory), not by the model. The
//! `atlas_types::memory::{Quiz, FlashcardSet, RevisionPlan}` are the
//! persisted read-model shapes; these are the pre-persistence generation
//! DTOs that a repository turns into one.

use atlas_types::memory::{Flashcard, QuizQuestion, RevisionPlanItem};
use atlas_utils::{AppError, ErrorCategory, ErrorCode};

/// A parsed-and-validated quiz, prior to persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedQuiz {
    pub topic: String,
    pub questions: Vec<QuizQuestion>,
}

/// A parsed-and-validated flashcard set, prior to persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedFlashcardSet {
    pub topic: String,
    pub cards: Vec<Flashcard>,
}

/// A parsed-and-validated revision plan, prior to persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedRevisionPlan {
    pub items: Vec<RevisionPlanItem>,
}

fn parse_error(what: &str, detail: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ValidationError,
        ErrorCategory::Recoverable,
        format!("failed to parse {what} model output: {detail}"),
    )
}

fn validation_error(what: &str, detail: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ValidationError,
        ErrorCategory::Recoverable,
        format!("invalid {what} model output: {detail}"),
    )
}

/// Models frequently wrap JSON output in a markdown code fence (```json ...
/// ```) even when explicitly instructed not to. Strip that wrapper before
/// attempting to parse, rather than failing on well-formed-but-fenced JSON.
fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    let Some(after_open) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    // Skip an optional language tag on the fence's opening line (e.g. `json`).
    let after_lang = match after_open.find('\n') {
        Some(idx) => &after_open[idx + 1..],
        None => after_open,
    };
    after_lang.strip_suffix("```").unwrap_or(after_lang).trim()
}

// --- Quiz -------------------------------------------------------------

#[derive(serde::Deserialize)]
struct RawQuizQuestion {
    question: String,
    options: Vec<String>,
    correct_answer: String,
    #[serde(default)]
    source_citations: Vec<String>,
}

#[derive(serde::Deserialize)]
struct RawQuiz {
    topic: String,
    questions: Vec<RawQuizQuestion>,
}

fn validate_quiz(raw: RawQuiz) -> Result<GeneratedQuiz, AppError> {
    if raw.topic.trim().is_empty() {
        return Err(validation_error("quiz", "topic is empty"));
    }
    if raw.questions.is_empty() {
        return Err(validation_error("quiz", "questions array is empty"));
    }

    let mut questions = Vec::with_capacity(raw.questions.len());
    for (idx, q) in raw.questions.into_iter().enumerate() {
        if q.question.trim().is_empty() {
            return Err(validation_error("quiz", format!("question {idx} has empty text")));
        }
        if q.options.len() < 2 {
            return Err(validation_error(
                "quiz",
                format!("question {idx} has fewer than 2 options"),
            ));
        }
        if !q.options.iter().any(|opt| opt == &q.correct_answer) {
            return Err(validation_error(
                "quiz",
                format!("question {idx}'s correct_answer is not among its own options"),
            ));
        }
        questions.push(QuizQuestion {
            question: q.question,
            options: q.options,
            correct_answer: q.correct_answer,
            source_citations: q.source_citations,
        });
    }

    Ok(GeneratedQuiz {
        topic: raw.topic,
        questions,
    })
}

/// Parse and validate a model's raw quiz response. Returns a `Recoverable`
/// `AppError` on either malformed JSON or JSON that fails the structural
/// contract (never panics, never silently accepts a broken quiz).
pub fn parse_quiz_response(raw: &str) -> Result<GeneratedQuiz, AppError> {
    let json = strip_code_fence(raw);
    let parsed: RawQuiz = serde_json::from_str(json).map_err(|e| parse_error("quiz", e))?;
    validate_quiz(parsed)
}

// --- Flashcards ---------------------------------------------------------

#[derive(serde::Deserialize)]
struct RawFlashcard {
    front: String,
    back: String,
    #[serde(default)]
    source_citations: Vec<String>,
}

#[derive(serde::Deserialize)]
struct RawFlashcardSet {
    topic: String,
    cards: Vec<RawFlashcard>,
}

fn validate_flashcards(raw: RawFlashcardSet) -> Result<GeneratedFlashcardSet, AppError> {
    if raw.topic.trim().is_empty() {
        return Err(validation_error("flashcard set", "topic is empty"));
    }
    if raw.cards.is_empty() {
        return Err(validation_error("flashcard set", "cards array is empty"));
    }

    let mut cards = Vec::with_capacity(raw.cards.len());
    for (idx, c) in raw.cards.into_iter().enumerate() {
        if c.front.trim().is_empty() || c.back.trim().is_empty() {
            return Err(validation_error(
                "flashcard set",
                format!("card {idx} has an empty front or back"),
            ));
        }
        cards.push(Flashcard {
            front: c.front,
            back: c.back,
            source_citations: c.source_citations,
        });
    }

    Ok(GeneratedFlashcardSet {
        topic: raw.topic,
        cards,
    })
}

/// Parse and validate a model's raw flashcard response.
pub fn parse_flashcard_response(raw: &str) -> Result<GeneratedFlashcardSet, AppError> {
    let json = strip_code_fence(raw);
    let parsed: RawFlashcardSet = serde_json::from_str(json).map_err(|e| parse_error("flashcard", e))?;
    validate_flashcards(parsed)
}

// --- Revision plan -------------------------------------------------------

#[derive(serde::Deserialize)]
struct RawRevisionPlanItem {
    topic: String,
    recommendation: String,
    priority: u32,
}

#[derive(serde::Deserialize)]
struct RawRevisionPlan {
    items: Vec<RawRevisionPlanItem>,
}

fn validate_revision_plan(raw: RawRevisionPlan) -> Result<GeneratedRevisionPlan, AppError> {
    if raw.items.is_empty() {
        return Err(validation_error("revision plan", "items array is empty"));
    }

    let mut items = Vec::with_capacity(raw.items.len());
    for (idx, item) in raw.items.into_iter().enumerate() {
        if item.topic.trim().is_empty() {
            return Err(validation_error("revision plan", format!("item {idx} has empty topic")));
        }
        if item.recommendation.trim().is_empty() {
            return Err(validation_error(
                "revision plan",
                format!("item {idx} has empty recommendation"),
            ));
        }
        if item.priority == 0 {
            return Err(validation_error(
                "revision plan",
                format!("item {idx} has priority 0 (priorities are 1-based, lower = more urgent)"),
            ));
        }
        items.push(RevisionPlanItem {
            topic: item.topic,
            recommendation: item.recommendation,
            priority: item.priority,
        });
    }

    Ok(GeneratedRevisionPlan { items })
}

/// Parse and validate a model's raw revision-plan response.
pub fn parse_revision_plan_response(raw: &str) -> Result<GeneratedRevisionPlan, AppError> {
    let json = strip_code_fence(raw);
    let parsed: RawRevisionPlan = serde_json::from_str(json).map_err(|e| parse_error("revision plan", e))?;
    validate_revision_plan(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Quiz: well-formed ------------------------------------------------

    #[test]
    fn parses_well_formed_quiz() {
        let raw = r#"{
            "topic": "Photosynthesis",
            "questions": [
                {
                    "question": "What pigment absorbs light?",
                    "options": ["Chlorophyll", "Melanin", "Keratin"],
                    "correct_answer": "Chlorophyll",
                    "source_citations": ["[1]"]
                }
            ]
        }"#;
        let quiz = parse_quiz_response(raw).unwrap();
        assert_eq!(quiz.topic, "Photosynthesis");
        assert_eq!(quiz.questions.len(), 1);
        assert_eq!(quiz.questions[0].correct_answer, "Chlorophyll");
        assert_eq!(quiz.questions[0].source_citations, vec!["[1]".to_string()]);
    }

    #[test]
    fn parses_quiz_wrapped_in_a_markdown_code_fence() {
        let raw = "```json\n{\"topic\": \"t\", \"questions\": [{\"question\": \"q\", \"options\": [\"a\", \"b\"], \"correct_answer\": \"a\"}]}\n```";
        let quiz = parse_quiz_response(raw).unwrap();
        assert_eq!(quiz.topic, "t");
    }

    #[test]
    fn quiz_source_citations_default_to_empty_when_omitted() {
        let raw = r#"{"topic": "t", "questions": [{"question": "q", "options": ["a", "b"], "correct_answer": "a"}]}"#;
        let quiz = parse_quiz_response(raw).unwrap();
        assert!(quiz.questions[0].source_citations.is_empty());
    }

    // --- Quiz: malformed / invalid -----------------------------------------

    #[test]
    fn quiz_malformed_json_is_recoverable_not_a_crash() {
        let err = parse_quiz_response("{not json at all").unwrap_err();
        assert_eq!(err.category, ErrorCategory::Recoverable);
        assert_eq!(err.code, ErrorCode::ValidationError);
    }

    #[test]
    fn quiz_correct_answer_not_in_options_is_rejected() {
        let raw = r#"{"topic": "t", "questions": [{"question": "q", "options": ["a", "b"], "correct_answer": "c"}]}"#;
        let err = parse_quiz_response(raw).unwrap_err();
        assert_eq!(err.category, ErrorCategory::Recoverable);
        assert!(err.message.contains("correct_answer"));
    }

    #[test]
    fn quiz_empty_questions_array_is_rejected() {
        let raw = r#"{"topic": "t", "questions": []}"#;
        assert!(parse_quiz_response(raw).is_err());
    }

    #[test]
    fn quiz_question_with_one_option_is_rejected() {
        let raw = r#"{"topic": "t", "questions": [{"question": "q", "options": ["a"], "correct_answer": "a"}]}"#;
        let err = parse_quiz_response(raw).unwrap_err();
        assert!(err.message.contains("fewer than 2 options"));
    }

    #[test]
    fn quiz_missing_required_field_is_rejected() {
        let raw = r#"{"topic": "t", "questions": [{"question": "q", "options": ["a", "b"]}]}"#;
        assert!(parse_quiz_response(raw).is_err());
    }

    // --- Flashcards ---------------------------------------------------------

    #[test]
    fn parses_well_formed_flashcards() {
        let raw = r#"{"topic": "Cell Biology", "cards": [{"front": "What is a ribosome?", "back": "Protein synthesis site", "source_citations": ["[2]"]}]}"#;
        let set = parse_flashcard_response(raw).unwrap();
        assert_eq!(set.topic, "Cell Biology");
        assert_eq!(set.cards.len(), 1);
        assert_eq!(set.cards[0].front, "What is a ribosome?");
    }

    #[test]
    fn flashcard_with_empty_back_is_rejected() {
        let raw = r#"{"topic": "t", "cards": [{"front": "q", "back": ""}]}"#;
        let err = parse_flashcard_response(raw).unwrap_err();
        assert_eq!(err.category, ErrorCategory::Recoverable);
    }

    #[test]
    fn flashcard_malformed_json_is_recoverable() {
        let err = parse_flashcard_response("not json").unwrap_err();
        assert_eq!(err.category, ErrorCategory::Recoverable);
    }

    // --- Revision plan --------------------------------------------------

    #[test]
    fn parses_well_formed_revision_plan() {
        let raw = r#"{"items": [{"topic": "Photosynthesis", "recommendation": "Review chapter 4 diagrams", "priority": 1}]}"#;
        let plan = parse_revision_plan_response(raw).unwrap();
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].priority, 1);
    }

    #[test]
    fn revision_plan_zero_priority_is_rejected() {
        let raw = r#"{"items": [{"topic": "t", "recommendation": "r", "priority": 0}]}"#;
        let err = parse_revision_plan_response(raw).unwrap_err();
        assert_eq!(err.category, ErrorCategory::Recoverable);
    }

    #[test]
    fn revision_plan_empty_items_is_rejected() {
        let raw = r#"{"items": []}"#;
        assert!(parse_revision_plan_response(raw).is_err());
    }

    #[test]
    fn revision_plan_malformed_json_is_recoverable() {
        let err = parse_revision_plan_response("<<not json>>").unwrap_err();
        assert_eq!(err.category, ErrorCategory::Recoverable);
        assert_eq!(err.code, ErrorCode::ValidationError);
    }
}
