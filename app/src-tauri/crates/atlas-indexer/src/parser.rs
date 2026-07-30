//! Parser Layer (§36). Each format has one `Parser` implementation, selected
//! by a registry-based `ParserSelector` (§36.1) rather than a hardcoded
//! if/else chain, so new parsers register themselves (§28, §36.2).

use atlas_types::document::ParsedDocument;
use atlas_utils::AppError;

/// Every format-specific parser conforms to this interface (§36.2, §36.3).
/// Input: a raw file handle (represented here as a path for the skeleton).
/// Output: a `ParsedDocument` (§35.1). Parsers MUST NOT chunk, embed, or
/// retrieve (§36.3).
pub trait Parser: Send + Sync {
    fn file_type(&self) -> &str;
    fn parse(&self, path: &str) -> Result<ParsedDocument, AppError>;
}

/// Registry-based selector (§36.1): file type -> registered `Parser`.
/// Concrete parser registration happens at composition time (atlas-core),
/// not hardcoded here.
pub struct ParserSelector {
    parsers: Vec<Box<dyn Parser>>,
}

impl ParserSelector {
    pub fn new() -> Self {
        Self {
            parsers: Vec::new(),
        }
    }

    pub fn register(&mut self, parser: Box<dyn Parser>) {
        self.parsers.push(parser);
    }

    pub fn resolve(&self, file_type: &str) -> Option<&dyn Parser> {
        self.parsers
            .iter()
            .find(|p| p.file_type() == file_type)
            .map(|p| p.as_ref())
    }
}

impl Default for ParserSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubParser {
        file_type: &'static str,
    }

    impl Parser for StubParser {
        fn file_type(&self) -> &str {
            self.file_type
        }

        fn parse(&self, _path: &str) -> Result<ParsedDocument, AppError> {
            unimplemented!("stub parser -- only used to exercise the registry in tests")
        }
    }

    #[test]
    fn resolve_returns_none_when_nothing_registered() {
        let selector = ParserSelector::new();
        assert!(selector.resolve("pdf").is_none());
    }

    #[test]
    fn resolve_finds_registered_parser_by_file_type() {
        let mut selector = ParserSelector::new();
        selector.register(Box::new(StubParser { file_type: "pdf" }));
        assert!(selector.resolve("pdf").is_some());
    }

    #[test]
    fn resolve_does_not_match_a_different_file_type() {
        let mut selector = ParserSelector::new();
        selector.register(Box::new(StubParser { file_type: "pdf" }));
        assert!(selector.resolve("docx").is_none());
    }

    #[test]
    fn multiple_parsers_can_be_registered_independently() {
        let mut selector = ParserSelector::new();
        selector.register(Box::new(StubParser { file_type: "pdf" }));
        selector.register(Box::new(StubParser { file_type: "docx" }));

        assert!(selector.resolve("pdf").is_some());
        assert!(selector.resolve("docx").is_some());
    }
}
