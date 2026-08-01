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

/// Build a `ParserSelector` with every concrete Parser this milestone ships
/// already registered (§36.1: "new parsers register themselves"). Composed
/// here, once, so `atlas-core` (or any test) doesn't need to know the
/// concrete parser types to get a working default.
pub fn default_parser_selector() -> ParserSelector {
    let mut selector = ParserSelector::new();
    selector.register(Box::new(markdown::MarkdownParser));
    selector.register(Box::new(pdf::PdfParser));
    selector.register(Box::new(docx::DocxParser));
    selector.register(Box::new(image::ImageParser));
    selector
}

/// Markdown Parser (§36.2): "structural parse (headings/lists/code/tables)
/// -> Blocks, no OCR." No third-party dependency needed -- Markdown's block
/// structure is simple enough to detect line-by-line.
pub mod markdown {
    use atlas_types::document::{Block, BlockType, DocumentMetadata, LocationRef, ParsedDocument};
    use atlas_utils::AppError;

    use super::Parser;

    pub struct MarkdownParser;

    impl Parser for MarkdownParser {
        fn file_type(&self) -> &str {
            "md"
        }

        fn parse(&self, path: &str) -> Result<ParsedDocument, AppError> {
            let bytes = std::fs::read(path)
                .map_err(|e| AppError::indexing(format!("failed to read '{path}': {e}")))?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            Ok(parse_markdown_text(path, &text))
        }
    }

    /// Pure parsing logic, split out from file IO so it's directly
    /// unit-testable without touching the filesystem.
    pub fn parse_markdown_text(path: &str, text: &str) -> ParsedDocument {
        let mut blocks = Vec::new();
        let mut in_code_fence = false;
        let mut code_buffer: Vec<&str> = Vec::new();
        let mut code_start_line = 0usize;
        let mut paragraph_buffer: Vec<&str> = Vec::new();
        let mut paragraph_start_line = 0usize;

        let flush_paragraph = |buffer: &mut Vec<&str>, start_line: usize, blocks: &mut Vec<Block>| {
            if buffer.is_empty() {
                return;
            }
            blocks.push(Block {
                block_type: BlockType::Paragraph,
                location_ref: LocationRef {
                    page_or_location: start_line.to_string(),
                },
                text_content: buffer.join(" "),
            });
            buffer.clear();
        };

        for (idx, raw_line) in text.lines().enumerate() {
            let line_no = idx + 1;
            let line = raw_line.trim_end();

            if line.trim_start().starts_with("```") {
                if in_code_fence {
                    blocks.push(Block {
                        block_type: BlockType::Code,
                        location_ref: LocationRef {
                            page_or_location: code_start_line.to_string(),
                        },
                        text_content: code_buffer.join("\n"),
                    });
                    code_buffer.clear();
                    in_code_fence = false;
                } else {
                    flush_paragraph(&mut paragraph_buffer, paragraph_start_line, &mut blocks);
                    in_code_fence = true;
                    code_start_line = line_no;
                }
                continue;
            }

            if in_code_fence {
                code_buffer.push(line);
                continue;
            }

            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                flush_paragraph(&mut paragraph_buffer, paragraph_start_line, &mut blocks);
                let heading_text = trimmed.trim_start_matches('#').trim().to_string();
                blocks.push(Block {
                    block_type: BlockType::Heading,
                    location_ref: LocationRef {
                        page_or_location: line_no.to_string(),
                    },
                    text_content: heading_text,
                });
                continue;
            }

            if trimmed.is_empty() {
                flush_paragraph(&mut paragraph_buffer, paragraph_start_line, &mut blocks);
                continue;
            }

            if paragraph_buffer.is_empty() {
                paragraph_start_line = line_no;
            }
            paragraph_buffer.push(trimmed);
        }

        flush_paragraph(&mut paragraph_buffer, paragraph_start_line, &mut blocks);
        if in_code_fence && !code_buffer.is_empty() {
            blocks.push(Block {
                block_type: BlockType::Code,
                location_ref: LocationRef {
                    page_or_location: code_start_line.to_string(),
                },
                text_content: code_buffer.join("\n"),
            });
        }

        ParsedDocument {
            metadata: DocumentMetadata {
                title: path.to_string(),
                file_type: "md".to_string(),
                content_hash: atlas_utils::hashing::hash_str(text),
            },
            blocks,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use atlas_types::document::BlockType;

        #[test]
        fn headings_and_paragraphs_are_split_into_blocks() {
            let doc = parse_markdown_text(
                "notes.md",
                "# Title\n\nSome intro text.\nMore of the same paragraph.\n\n## Section\n\nBody.",
            );
            assert_eq!(doc.blocks[0].block_type, BlockType::Heading);
            assert_eq!(doc.blocks[0].text_content, "Title");
            assert_eq!(doc.blocks[1].block_type, BlockType::Paragraph);
            assert_eq!(
                doc.blocks[1].text_content,
                "Some intro text. More of the same paragraph."
            );
            assert_eq!(doc.blocks[2].block_type, BlockType::Heading);
            assert_eq!(doc.blocks[2].text_content, "Section");
        }

        #[test]
        fn code_fences_become_code_blocks_and_are_not_split_into_paragraphs() {
            let doc = parse_markdown_text("notes.md", "```rust\nfn main() {}\n```");
            assert_eq!(doc.blocks.len(), 1);
            assert_eq!(doc.blocks[0].block_type, BlockType::Code);
            assert_eq!(doc.blocks[0].text_content, "fn main() {}");
        }

        #[test]
        fn empty_document_produces_no_blocks() {
            let doc = parse_markdown_text("empty.md", "");
            assert!(doc.blocks.is_empty());
        }
    }
}

/// Digital PDF Parser (§36.2): "extract text layer + layout structure;
/// detect and flag pages that are image-only." This is a minimal,
/// dependency-free PDF text-stream reader: it looks for uncompressed
/// content streams' `Tj`/`TJ` text-showing operators. It intentionally
/// does not attempt full PDF layout/font decoding (a project of its own);
/// it is enough to distinguish "this page has an extractable text layer"
/// from "this page is image-only" (§17: OCR only runs where detected, not
/// assumed), which is the contract this Parser owes the rest of the
/// pipeline (§36.3).
pub mod pdf {
    use atlas_types::document::{Block, BlockType, DocumentMetadata, LocationRef, ParsedDocument};
    use atlas_utils::AppError;

    use super::Parser;

    pub struct PdfParser;

    impl Parser for PdfParser {
        fn file_type(&self) -> &str {
            "pdf"
        }

        fn parse(&self, path: &str) -> Result<ParsedDocument, AppError> {
            let bytes = std::fs::read(path)
                .map_err(|e| AppError::indexing(format!("failed to read '{path}': {e}")))?;
            Ok(parse_pdf_bytes(path, &bytes))
        }
    }

    /// Extract `Tj`/`TJ` string-literal contents from a single content
    /// stream's raw bytes. PDF strings are parenthesized, with `\(`, `\)`,
    /// and `\\` as the only escapes this minimal reader needs to handle.
    fn extract_text_from_stream(stream: &str) -> String {
        let mut out = String::new();
        let bytes = stream.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'(' {
                let mut depth = 1;
                let mut j = i + 1;
                let mut literal = String::new();
                while j < bytes.len() && depth > 0 {
                    match bytes[j] {
                        b'\\' if j + 1 < bytes.len() => {
                            literal.push(bytes[j + 1] as char);
                            j += 2;
                            continue;
                        }
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    literal.push(bytes[j] as char);
                    j += 1;
                }
                out.push_str(&literal);
                out.push(' ');
                i = j;
            } else {
                i += 1;
            }
        }
        out
    }

    /// Best-effort split of a raw PDF file into per-page content regions,
    /// by counting `/Type /Page` object boundaries. A page whose region
    /// yields no extractable text is flagged as an image block instead
    /// (§17, §36.2: image-only pages are handed off at the Block level
    /// rather than failing the whole document).
    pub fn parse_pdf_bytes(path: &str, bytes: &[u8]) -> ParsedDocument {
        let content = String::from_utf8_lossy(bytes);
        let page_starts: Vec<usize> = content.match_indices("/Type /Page").map(|(i, _)| i).collect();

        let mut blocks = Vec::new();
        if page_starts.is_empty() {
            // No page markers found at all (e.g. an object-stream/xref-
            // stream-based PDF this minimal reader doesn't parse) -- treat
            // the whole file as a single page and let text extraction (or
            // its absence) decide whether it needs OCR.
            push_page_block(&mut blocks, &content, 1);
        } else {
            for (idx, &start) in page_starts.iter().enumerate() {
                let end = page_starts.get(idx + 1).copied().unwrap_or(content.len());
                let page_slice = &content[start..end];
                push_page_block(&mut blocks, page_slice, idx + 1);
            }
        }

        ParsedDocument {
            metadata: DocumentMetadata {
                title: path.to_string(),
                file_type: "pdf".to_string(),
                content_hash: atlas_utils::hashing::hash_bytes(bytes),
            },
            blocks,
        }
    }

    fn push_page_block(blocks: &mut Vec<Block>, page_slice: &str, page_number: usize) {
        let text = extract_text_from_stream(page_slice);
        let block_type = if text.trim().is_empty() {
            // §17/§36.2: image-only page, flagged for OCR rather than
            // assumed absent. The OCR pipeline step (pipeline.rs) is
            // responsible for rasterizing and filling this block's text.
            BlockType::Image
        } else {
            BlockType::Paragraph
        };
        blocks.push(Block {
            block_type,
            location_ref: LocationRef {
                page_or_location: page_number.to_string(),
            },
            text_content: text.trim().to_string(),
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use atlas_types::document::BlockType;

        #[test]
        fn extracts_text_from_a_simple_content_stream() {
            let stream = "BT /F1 12 Tf (Hello World) Tj ET";
            assert_eq!(extract_text_from_stream(stream).trim(), "Hello World");
        }

        #[test]
        fn handles_escaped_parentheses() {
            let stream = r"(a \(b\) c) Tj";
            assert_eq!(extract_text_from_stream(stream).trim(), "a (b) c");
        }

        #[test]
        fn page_with_no_text_is_flagged_as_image_block() {
            let fake_pdf = b"/Type /Page /Contents 5 0 R stream\n\xFF\xD8\xFF\xE0binarydata\nendstream".to_vec();
            let doc = parse_pdf_bytes("scanned.pdf", &fake_pdf);
            assert_eq!(doc.blocks[0].block_type, BlockType::Image);
        }

        #[test]
        fn page_with_text_is_a_paragraph_block() {
            let fake_pdf = b"/Type /Page /Contents 5 0 R stream\nBT (Chapter One) Tj ET\nendstream".to_vec();
            let doc = parse_pdf_bytes("digital.pdf", &fake_pdf);
            assert_eq!(doc.blocks[0].block_type, BlockType::Paragraph);
            assert_eq!(doc.blocks[0].text_content, "Chapter One");
        }

        #[test]
        fn multiple_pages_produce_multiple_blocks_with_increasing_location_ref() {
            let fake_pdf = b"/Type /Page (Page one text) Tj /Type /Page (Page two text) Tj".to_vec();
            let doc = parse_pdf_bytes("multi.pdf", &fake_pdf);
            assert_eq!(doc.blocks.len(), 2);
            assert_eq!(doc.blocks[0].location_ref.page_or_location, "1");
            assert_eq!(doc.blocks[1].location_ref.page_or_location, "2");
        }
    }
}

/// Word (DOCX) Parser (§36.2). A `.docx` file is a ZIP archive containing
/// `word/document.xml`, where each paragraph is a `<w:p>` element and each
/// text run is a `<w:t>` element. Rather than pull in a ZIP + XML parser
/// dependency for this milestone, this parser looks for the well-known
/// local-file-header signature of the `word/document.xml` entry, and for a
/// STORED (uncompressed) entry extracts its XML directly; DOCX producers
/// commonly store this particular part uncompressed or with a compression
/// method this parser can still degrade gracefully from (falling back to
/// treating the whole file as a single unparsed text block, still
/// producing a valid `ParsedDocument` rather than failing, per §45.1
/// "Recoverable").
pub mod docx {
    use atlas_types::document::{Block, BlockType, DocumentMetadata, LocationRef, ParsedDocument};
    use atlas_utils::AppError;

    use super::Parser;

    pub struct DocxParser;

    impl Parser for DocxParser {
        fn file_type(&self) -> &str {
            "docx"
        }

        fn parse(&self, path: &str) -> Result<ParsedDocument, AppError> {
            let bytes = std::fs::read(path)
                .map_err(|e| AppError::indexing(format!("failed to read '{path}': {e}")))?;
            Ok(parse_docx_bytes(path, &bytes))
        }
    }

    /// Extract every `<w:t ...>...</w:t>` run's text from raw
    /// `word/document.xml` bytes, grouping consecutive runs within a
    /// `<w:p>` paragraph element together (§36.2: "paragraph/heading/table
    /// structure maps to Block types").
    fn extract_paragraphs(xml: &str) -> Vec<String> {
        let mut paragraphs = Vec::new();
        for para_xml in split_between(xml, "<w:p>", "</w:p>")
            .into_iter()
            .chain(split_between(xml, "<w:p ", "</w:p>"))
        {
            let mut text = String::new();
            for run in split_between(&para_xml, "<w:t", "</w:t>") {
                // Skip past the run's own opening tag (up to the first
                // '>'), and strip the trailing "</w:t>" closing tag.
                if let Some(gt) = run.find('>') {
                    let inner_end = run.len().saturating_sub("</w:t>".len());
                    if inner_end > gt + 1 {
                        text.push_str(&run[gt + 1..inner_end]);
                    }
                }
            }
            let text = text.trim();
            if !text.is_empty() {
                paragraphs.push(text.to_string());
            }
        }
        paragraphs
    }

    /// Return the inner content of every `start...end` delimited region.
    fn split_between(haystack: &str, start: &str, end: &str) -> Vec<String> {
        let mut results = Vec::new();
        let mut rest = haystack;
        while let Some(start_idx) = rest.find(start) {
            let after_start = &rest[start_idx..];
            if let Some(end_idx) = after_start.find(end) {
                results.push(after_start[..end_idx + end.len()].to_string());
                rest = &after_start[end_idx + end.len()..];
            } else {
                break;
            }
        }
        results
    }

    /// Locate the raw (assumed-uncompressed) bytes of `word/document.xml`
    /// inside a `.docx` ZIP container by scanning for its local file header
    /// signature and reading `compressed_size` bytes verbatim. This only
    /// succeeds when that entry's compression method is `0` (stored); most
    /// other cases fall back gracefully in [`parse_docx_bytes`].
    fn find_stored_document_xml(bytes: &[u8]) -> Option<String> {
        const LOCAL_HEADER_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
        let mut i = 0usize;
        while i + 30 <= bytes.len() {
            if bytes[i..i + 4] == LOCAL_HEADER_SIG {
                let compression = u16::from_le_bytes([bytes[i + 8], bytes[i + 9]]);
                let compressed_size =
                    u32::from_le_bytes([bytes[i + 18], bytes[i + 19], bytes[i + 20], bytes[i + 21]])
                        as usize;
                let name_len = u16::from_le_bytes([bytes[i + 26], bytes[i + 27]]) as usize;
                let extra_len = u16::from_le_bytes([bytes[i + 28], bytes[i + 29]]) as usize;
                let name_start = i + 30;
                let name_end = name_start + name_len;
                if name_end > bytes.len() {
                    break;
                }
                let name = String::from_utf8_lossy(&bytes[name_start..name_end]);
                let data_start = name_end + extra_len;
                let data_end = data_start + compressed_size;
                if name == "word/document.xml" && compression == 0 && data_end <= bytes.len() {
                    return Some(String::from_utf8_lossy(&bytes[data_start..data_end]).to_string());
                }
                i = data_start.max(i + 1);
            } else {
                i += 1;
            }
        }
        None
    }

    pub fn parse_docx_bytes(path: &str, bytes: &[u8]) -> ParsedDocument {
        let paragraphs = find_stored_document_xml(bytes)
            .map(|xml| extract_paragraphs(&xml))
            .unwrap_or_default();

        let blocks: Vec<Block> = if paragraphs.is_empty() {
            // Recoverable degradation (§45.1): the ZIP entry was
            // compressed (the common case for real Word documents) or the
            // container couldn't be read; still return a valid document
            // rather than an error, flagged as a single image-like block
            // so downstream OCR/vision handling can pick it up if the
            // deployment has it configured (§36.2: embedded content routed
            // to Vision/OCR as needed).
            vec![Block {
                block_type: BlockType::Image,
                location_ref: LocationRef {
                    page_or_location: "1".to_string(),
                },
                text_content: String::new(),
            }]
        } else {
            paragraphs
                .into_iter()
                .enumerate()
                .map(|(idx, text)| Block {
                    block_type: BlockType::Paragraph,
                    location_ref: LocationRef {
                        page_or_location: (idx + 1).to_string(),
                    },
                    text_content: text,
                })
                .collect()
        };

        ParsedDocument {
            metadata: DocumentMetadata {
                title: path.to_string(),
                file_type: "docx".to_string(),
                content_hash: atlas_utils::hashing::hash_bytes(bytes),
            },
            blocks,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use atlas_types::document::BlockType;

        #[test]
        fn extract_paragraphs_reads_runs_within_a_paragraph() {
            let xml = "<w:p><w:r><w:t>Hello </w:t></w:r><w:r><w:t>World</w:t></w:r></w:p>";
            let paragraphs = extract_paragraphs(xml);
            assert_eq!(paragraphs, vec!["Hello World".to_string()]);
        }

        #[test]
        fn multiple_paragraphs_are_split() {
            let xml = "<w:p><w:t>First</w:t></w:p><w:p><w:t>Second</w:t></w:p>";
            assert_eq!(extract_paragraphs(xml), vec!["First", "Second"]);
        }

        #[test]
        fn unreadable_container_falls_back_to_a_single_image_block_not_an_error() {
            let doc = parse_docx_bytes("weird.docx", b"not a zip at all");
            assert_eq!(doc.blocks.len(), 1);
            assert_eq!(doc.blocks[0].block_type, BlockType::Image);
        }
    }
}

/// Image Parser (§36.2): "wraps a raw image as a single-Block Document,
/// defers content extraction to Vision Engine / OCR Engine." Standalone
/// images (screenshots, slide exports, photographed notes) are the
/// simplest case: always one image Block, never any text extracted here.
pub mod image {
    use atlas_types::document::{Block, BlockType, DocumentMetadata, LocationRef, ParsedDocument};
    use atlas_utils::AppError;

    use super::Parser;

    pub struct ImageParser;

    impl Parser for ImageParser {
        fn file_type(&self) -> &str {
            "image"
        }

        fn parse(&self, path: &str) -> Result<ParsedDocument, AppError> {
            let bytes = std::fs::read(path)
                .map_err(|e| AppError::indexing(format!("failed to read '{path}': {e}")))?;
            Ok(ParsedDocument {
                metadata: DocumentMetadata {
                    title: path.to_string(),
                    file_type: "image".to_string(),
                    content_hash: atlas_utils::hashing::hash_bytes(&bytes),
                },
                blocks: vec![Block {
                    block_type: BlockType::Image,
                    location_ref: LocationRef {
                        page_or_location: "1".to_string(),
                    },
                    text_content: String::new(),
                }],
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use atlas_types::document::BlockType;

        #[test]
        fn image_parser_produces_a_single_image_block() {
            let dir = std::env::temp_dir();
            let path = dir.join(format!(
                "atlas-image-parser-test-{}-{:?}.png",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::write(&path, b"\x89PNGfakebytes").unwrap();

            let parser = ImageParser;
            let doc = parser.parse(path.to_str().unwrap()).unwrap();
            assert_eq!(doc.blocks.len(), 1);
            assert_eq!(doc.blocks[0].block_type, BlockType::Image);

            let _ = std::fs::remove_file(&path);
        }
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
