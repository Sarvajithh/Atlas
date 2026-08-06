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

    /// Best-effort extraction of the actual image bytes for one
    /// `BlockType::Image` block (identified by its index in
    /// `ParsedDocument::blocks`), for handing to OCR (§17). Default: not
    /// supported -- the caller (`pipeline.rs`) falls back to reading the
    /// whole file, which is only actually correct for a format where the
    /// whole file already IS a single image (`ImageParser`). Formats that
    /// can contain a real embedded image per block (e.g. `PdfParser`)
    /// override this so OCR receives an actual image rather than an
    /// unrelated raw container file it can't decode.
    fn extract_ocr_image(&self, _path: &str, _block_index: usize) -> Option<Vec<u8>> {
        None
    }

    /// Whether `extract_ocr_image` is a real per-block implementation for
    /// this format (Fix 3, P0 audit). Default `false`: the pipeline's
    /// whole-file fallback (`std::fs::read(absolute_path)`) is only
    /// correct for a parser where the whole file already IS the image
    /// (`ImageParser`). A parser that overrides `extract_ocr_image` with a
    /// real implementation (`PdfParser`) should return `true` here so a
    /// `None` result is treated as a genuine per-block OCR failure rather
    /// than silently falling through to raw whole-file bytes again.
    fn supports_ocr_image_extraction(&self) -> bool {
        false
    }
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
    selector.register(Box::new(html::HtmlParser));
    selector.register(Box::new(txt::TxtParser));
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
/// detect and flag pages that are image-only."
///
/// FIX 2 (P0 audit): the previous implementation was a dependency-free,
/// hand-rolled reader that only scanned raw page byte ranges for
/// uncompressed `Tj`/`TJ` string literals. It had no support for
/// `/FlateDecode` (zlib) content streams -- what nearly all real-world PDF
/// producers actually use -- so most real PDFs extracted no text at all
/// and were misclassified as image-only. This now uses `lopdf`, a
/// well-maintained, pure-Rust, offline-capable PDF object-graph parser (no
/// network calls, consistent with §5 local-first), for real per-page text
/// extraction through the actual PDF object/stream/filter model instead of
/// scanning raw bytes for coincidental patterns.
///
/// FIX 3 (P0 audit): `extract_ocr_image` used to search for the Nth
/// `/DCTDecode` (JPEG) stream in the raw file bytes, which returns nothing
/// for the non-JPEG image encodings scanned/handwritten PDF exporters
/// (GoodNotes, Notability, Apple Notes, Samsung Notes) actually use.
/// `pipeline.rs` then fell back to handing the OCR engine the *entire raw
/// PDF file* as if it were one image, which no OCR engine can read, so
/// every OCR-flagged block silently ended up with empty text. This now
/// walks the real page/XObject object graph via `lopdf` to find that
/// page's embedded raster image, and produces a real standalone image
/// file for the OCR engine: JPEG/JPEG2000 streams are passed through
/// as-is (their raw stream bytes already ARE a standalone image file --
/// that's what those filter names mean), and raw/FlateDecode-compressed
/// sample data is decoded and re-encoded as a real PNG using the image
/// dictionary's Width/Height/ColorSpace/BitsPerComponent.
///
/// Known limitation (kept honest rather than silently guessing, see Fix
/// 7's spirit): this extracts the page's embedded raster *image object*.
/// It does not do full vector/text page rasterization, so a page whose
/// scanned content is composed of vector ink strokes rather than a single
/// embedded raster image (some GoodNotes export modes) is not covered by
/// this fix and will still yield `None` from `extract_ocr_image` --
/// `pipeline.rs` treats that as a clean per-block OCR failure rather than
/// falling back to raw file bytes. CCITTFaxDecode/JBIG2Decode-encoded
/// embedded images (common for pure black-and-white fax-style scans) and
/// Indexed/CMYK color spaces are likewise not decoded by this fix and also
/// yield `None`. Both are called out explicitly in the Fix 7 audit report
/// rather than being silently accepted as "handled."
pub mod pdf {
    use atlas_types::document::{Block, BlockType, DocumentMetadata, LocationRef, ParsedDocument};
    use atlas_utils::AppError;
    use std::io::Read;

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

        fn extract_ocr_image(&self, path: &str, block_index: usize) -> Option<Vec<u8>> {
            let bytes = std::fs::read(path).ok()?;
            let doc = lopdf::Document::load_mem(&bytes).ok()?;
            // Same iteration order as `parse_pdf_bytes` (a `BTreeMap`
            // sorted by page number), so `block_index` lines up with the
            // page this block was produced from.
            let page_id = *doc.get_pages().values().nth(block_index)?;
            let stream = find_page_image_stream(&doc, page_id)?;
            stream_to_image_bytes(stream)
        }

        fn supports_ocr_image_extraction(&self) -> bool {
            true
        }
    }

    /// Real per-page text extraction and page/image splitting, via
    /// `lopdf`'s object graph (Fix 2). One `Block` per page, in page
    /// order, per the existing contract; a page with no extractable text
    /// is produced as `BlockType::Image` rather than dropped, so
    /// downstream OCR-gating (`requires_ocr`) keeps working unchanged.
    pub fn parse_pdf_bytes(path: &str, bytes: &[u8]) -> ParsedDocument {
        let metadata = DocumentMetadata {
            title: path.to_string(),
            file_type: "pdf".to_string(),
            content_hash: atlas_utils::hashing::hash_bytes(bytes),
        };

        let doc = match lopdf::Document::load_mem(bytes) {
            Ok(doc) => doc,
            Err(_) => {
                // Malformed, encrypted, or otherwise not walkable as a
                // real PDF object graph. Per contract (§36.3, this fix's
                // requirement 2), this must never be silently dropped --
                // produce a single Image block so downstream OCR-gating
                // still has something to act on, rather than an empty
                // document or a panic.
                return ParsedDocument {
                    metadata,
                    blocks: vec![Block {
                        block_type: BlockType::Image,
                        location_ref: LocationRef { page_or_location: "1".to_string() },
                        text_content: String::new(),
                    }],
                };
            }
        };

        let pages = doc.get_pages();
        let mut blocks = Vec::new();
        if pages.is_empty() {
            blocks.push(Block {
                block_type: BlockType::Image,
                location_ref: LocationRef { page_or_location: "1".to_string() },
                text_content: String::new(),
            });
        } else {
            for &page_number in pages.keys() {
                let text = doc.extract_text(&[page_number]).unwrap_or_default();
                let block_type = if looks_like_real_text(&text) {
                    BlockType::Paragraph
                } else {
                    // §17/§36.2: image-only (or extraction-failed) page,
                    // flagged for OCR rather than assumed absent.
                    BlockType::Image
                };
                let text_content = if block_type == BlockType::Paragraph {
                    text.trim().to_string()
                } else {
                    String::new()
                };
                blocks.push(Block {
                    block_type,
                    location_ref: LocationRef { page_or_location: page_number.to_string() },
                    text_content,
                });
            }
        }

        ParsedDocument { metadata, blocks }
    }

    /// Sanity-check that extracted text plausibly IS text, kept from the
    /// previous implementation: even `lopdf`'s real extraction can return
    /// a near-empty or garbled string for a page with a broken/embedded
    /// font encoding table, and that must still be treated as "needs OCR"
    /// rather than stored as the page's real content.
    fn looks_like_real_text(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return false;
        }
        let total = trimmed.chars().count();
        let plausible = trimmed
            .chars()
            .filter(|c| c.is_ascii_graphic() || c.is_whitespace())
            .count();
        plausible * 10 >= total * 9
    }

    /// Find the largest (by pixel area) embedded raster Image XObject
    /// referenced by a page's resources. Scanned/handwritten PDF exports
    /// are overwhelmingly one full-page image per page, so "largest image
    /// on the page" is a reliable proxy for "the page scan" without
    /// needing a true PDF object-graph resolver of every content-stream
    /// `Do` operator invocation (this parser's documented minimal-reader
    /// scope, §36.2).
    fn find_page_image_stream(doc: &lopdf::Document, page_id: (u32, u16)) -> Option<&lopdf::Stream> {
        // `get_page_resources` only returns a *direct* (inline) Resources
        // dictionary as its first tuple element; real-world PDFs
        // overwhelmingly store `/Resources` as an indirect reference
        // instead, which only shows up in the second tuple element
        // (`resource_ids`, already-resolved object ids of every Resources
        // dict in this page's ancestry). Both must be checked, or this
        // silently finds nothing for the common indirect-reference case.
        let (inline_resources, resource_ids) = doc.get_page_resources(page_id);

        let mut xobject_dicts: Vec<&lopdf::Dictionary> = Vec::new();
        if let Some(dict) = inline_resources {
            if let Ok(xobjects) = dict.get(b"XObject").and_then(|o| o.as_dict()) {
                xobject_dicts.push(xobjects);
            }
        }
        for resource_id in resource_ids {
            let Ok(dict) = doc.get_dictionary(resource_id) else { continue };
            if let Ok(xobjects) = dict.get(b"XObject").and_then(|o| o.as_dict()) {
                xobject_dicts.push(xobjects);
            }
        }

        let mut best: Option<(&lopdf::Stream, i64)> = None;
        for xobjects in xobject_dicts {
            for (_name, value) in xobjects.iter() {
                let Ok(obj_id) = value.as_reference() else { continue };
                let Ok(object) = doc.get_object(obj_id) else { continue };
                let Ok(stream) = object.as_stream() else { continue };
                let is_image = stream
                    .dict
                    .get(b"Subtype")
                    .and_then(|o| o.as_name_str())
                    .map(|s| s == "Image")
                    .unwrap_or(false);
                if !is_image {
                    continue;
                }
                let width = stream.dict.get(b"Width").and_then(|o| o.as_i64()).unwrap_or(0);
                let height = stream.dict.get(b"Height").and_then(|o| o.as_i64()).unwrap_or(0);
                let area = width.saturating_mul(height);
                let is_bigger = best.map(|(_, best_area)| area > best_area).unwrap_or(true);
                if is_bigger {
                    best = Some((stream, area));
                }
            }
        }
        best.map(|(stream, _)| stream)
    }

    /// Convert an embedded image XObject `Stream` into standalone image
    /// bytes the OCR engine can decode directly, based on its actual
    /// encoding filter -- not assumed to always be JPEG.
    fn stream_to_image_bytes(stream: &lopdf::Stream) -> Option<Vec<u8>> {
        let filters = stream.filters().unwrap_or_default();
        // Filters are listed in decoding order (PDF spec); the last one
        // applied is the one whose semantics decide how to interpret
        // `stream.content`.
        let last_filter = filters.last().map(|s| s.as_str()).unwrap_or("");
        match last_filter {
            // A `/DCTDecode` (JPEG) or `/JPXDecode` (JPEG2000) stream's
            // raw bytes already ARE a standalone valid image file -- that
            // filter name means "this stream is encoded that way" -- no
            // re-encoding needed, just handing the bytes through.
            "DCTDecode" | "JPXDecode" => Some(stream.content.clone()),
            // Raw or zlib-compressed sample data: decode (if needed) and
            // re-encode as a real PNG using the image dict's geometry, so
            // the OCR engine receives an actual image file rather than
            // undecodable raw samples.
            "FlateDecode" | "" => {
                let raw = if last_filter == "FlateDecode" {
                    decode_zlib(&stream.content)?
                } else {
                    stream.content.clone()
                };
                encode_png_from_raw_samples(&stream.dict, &raw)
            }
            // CCITTFaxDecode / JBIG2Decode / other encodings: not decoded
            // by this fix -- a genuine, explicitly-acknowledged gap (see
            // module doc comment and the Fix 7 audit) rather than a
            // silent guess.
            _ => None,
        }
    }

    fn decode_zlib(data: &[u8]) -> Option<Vec<u8>> {
        let mut decoder = flate2::read::ZlibDecoder::new(data);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).ok()?;
        Some(out)
    }

    fn encode_png_from_raw_samples(dict: &lopdf::Dictionary, raw: &[u8]) -> Option<Vec<u8>> {
        let width = dict.get(b"Width").ok()?.as_i64().ok()?.try_into().ok()?;
        let height = dict.get(b"Height").ok()?.as_i64().ok()?.try_into().ok()?;
        let bits_per_component: u8 = dict
            .get(b"BitsPerComponent")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(8)
            .try_into()
            .ok()?;
        let color_space = dict
            .get(b"ColorSpace")
            .ok()
            .and_then(|o| o.as_name_str().ok())
            .unwrap_or("DeviceGray");

        let (color_type, bit_depth) = match (color_space, bits_per_component) {
            ("DeviceGray" | "CalGray", 1) => (png::ColorType::Grayscale, png::BitDepth::One),
            ("DeviceGray" | "CalGray", 8) => (png::ColorType::Grayscale, png::BitDepth::Eight),
            ("DeviceRGB" | "CalRGB", 8) => (png::ColorType::Rgb, png::BitDepth::Eight),
            // Indexed palettes, CMYK, 16-bit-per-component, and other
            // uncommon combinations are not handled here -- returning
            // `None` (a clean per-block OCR failure) rather than
            // guessing at a color transform that could silently corrupt
            // the image.
            _ => return None,
        };
        if width == 0 || height == 0 {
            return None;
        }

        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(color_type);
            encoder.set_depth(bit_depth);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(raw).ok()?;
        }
        Some(out)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use atlas_types::document::BlockType;
        use lopdf::dictionary;

        /// Build a minimal single-page PDF whose content stream is
        /// `/FlateDecode`-compressed, containing a `Tj` text-showing
        /// operator -- the exact real-world shape (Fix 2) the previous
        /// byte-scanning parser could never extract text from.
        fn build_flate_compressed_text_pdf(text: &str) -> Vec<u8> {
            let mut doc = lopdf::Document::with_version("1.5");
            let pages_id = doc.new_object_id();

            let font_id = doc.add_object(lopdf::dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
            });
            let resources_id = doc.add_object(lopdf::dictionary! {
                "Font" => lopdf::dictionary! { "F1" => font_id },
            });

            let content = lopdf::content::Content {
                operations: vec![
                    lopdf::content::Operation::new("BT", vec![]),
                    lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    lopdf::content::Operation::new("Td", vec![100.into(), 700.into()]),
                    lopdf::content::Operation::new(
                        "Tj",
                        vec![lopdf::Object::string_literal(text)],
                    ),
                    lopdf::content::Operation::new("ET", vec![]),
                ],
            };
            let content_bytes = content.encode().unwrap();
            let mut content_stream = lopdf::Stream::new(lopdf::dictionary! {}, content_bytes);
            // Force real zlib compression so this is a faithful
            // regression fixture for the FlateDecode bug, not an
            // uncompressed stream that the old parser could already read.
            content_stream.compress().unwrap();
            let content_id = doc.add_object(content_stream);

            let page_id = doc.add_object(lopdf::dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
            });

            doc.objects.insert(
                pages_id,
                lopdf::Object::Dictionary(lopdf::dictionary! {
                    "Type" => "Pages",
                    "Kids" => vec![page_id.into()],
                    "Count" => 1,
                    "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                }),
            );
            let catalog_id = doc.add_object(lopdf::dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            });
            doc.trailer.set("Root", catalog_id);

            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            bytes
        }

        #[test]
        fn extracts_text_from_a_flatedecode_compressed_content_stream() {
            let pdf_bytes = build_flate_compressed_text_pdf("Hello Compressed World");
            let parsed = parse_pdf_bytes("compressed.pdf", &pdf_bytes);
            assert_eq!(parsed.blocks.len(), 1);
            assert_eq!(parsed.blocks[0].block_type, BlockType::Paragraph);
            assert!(parsed.blocks[0].text_content.contains("Hello Compressed World"));
        }

        #[test]
        fn a_page_with_no_extractable_text_is_flagged_as_image() {
            // A page with no content stream at all still has to produce
            // a Block, and since it has no text it must be flagged for
            // OCR rather than silently dropped.
            let mut doc = lopdf::Document::with_version("1.5");
            let pages_id = doc.new_object_id();
            let page_id = doc.add_object(lopdf::dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
            });
            doc.objects.insert(
                pages_id,
                lopdf::Object::Dictionary(lopdf::dictionary! {
                    "Type" => "Pages",
                    "Kids" => vec![page_id.into()],
                    "Count" => 1,
                }),
            );
            let catalog_id = doc.add_object(lopdf::dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            });
            doc.trailer.set("Root", catalog_id);
            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();

            let parsed = parse_pdf_bytes("blank-page.pdf", &bytes);
            assert_eq!(parsed.blocks.len(), 1);
            assert_eq!(parsed.blocks[0].block_type, BlockType::Image);
            assert!(parsed.blocks[0].text_content.is_empty());
        }

        #[test]
        fn a_malformed_pdf_still_produces_a_single_image_block_not_a_panic_or_empty_doc() {
            let parsed = parse_pdf_bytes("not-really-a-pdf.pdf", b"this is not a pdf file at all");
            assert_eq!(parsed.blocks.len(), 1);
            assert_eq!(parsed.blocks[0].block_type, BlockType::Image);
        }

        /// Build a single-page PDF whose page has one embedded raster
        /// image XObject encoded with `/FlateDecode` (the common case for
        /// a full-page scan that isn't JPEG-compressed) and confirm
        /// `extract_ocr_image` returns real, valid PNG bytes for it, not
        /// `None` and not the raw PDF file.
        fn build_flate_image_pdf(width: u32, height: u32, gray_samples: &[u8]) -> Vec<u8> {
            let mut doc = lopdf::Document::with_version("1.5");
            let pages_id = doc.new_object_id();

            let mut image_stream = lopdf::Stream::new(
                lopdf::dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => width as i64,
                    "Height" => height as i64,
                    "ColorSpace" => "DeviceGray",
                    "BitsPerComponent" => 8,
                },
                gray_samples.to_vec(),
            );
            image_stream.compress().unwrap();
            let image_id = doc.add_object(image_stream);

            let resources_id = doc.add_object(lopdf::dictionary! {
                "XObject" => lopdf::dictionary! { "Im0" => image_id },
            });

            let page_id = doc.add_object(lopdf::dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Resources" => resources_id,
            });
            doc.objects.insert(
                pages_id,
                lopdf::Object::Dictionary(lopdf::dictionary! {
                    "Type" => "Pages",
                    "Kids" => vec![page_id.into()],
                    "Count" => 1,
                }),
            );
            let catalog_id = doc.add_object(lopdf::dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            });
            doc.trailer.set("Root", catalog_id);

            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            bytes
        }

        #[test]
fn extract_ocr_image_returns_valid_png_bytes_for_a_flatedecode_image_page() {
    let width = 4u32;
    let height = 2u32;
    let samples = vec![10u8, 20, 30, 40, 50, 60, 70, 80]; // 4x2 grayscale
    let pdf_bytes = build_flate_image_pdf(width, height, &samples);

    let path = std::env::temp_dir().join("atlas_test_flate_image.pdf");
    let parser = PdfParser;
    std::fs::write(&path, &pdf_bytes).unwrap();
    let image_bytes = parser
        .extract_ocr_image(path.to_str().unwrap(), 0)
        .expect("expected real PNG bytes, got None");

    // Real PNG signature, not raw samples and not the PDF file.
    assert_eq!(&image_bytes[0..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn extract_ocr_image_returns_none_for_a_page_with_no_image() {
    let pdf_bytes = build_flate_compressed_text_pdf("no image here");
    let path = std::env::temp_dir().join("atlas_test_no_image.pdf");
    std::fs::write(&path, &pdf_bytes).unwrap();

    let parser = PdfParser;
    let result = parser.extract_ocr_image(path.to_str().unwrap(), 0);
    assert!(result.is_none());
    let _ = std::fs::remove_file(&path);
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

    /// Locate the bytes of `word/document.xml` inside a `.docx` ZIP
    /// container by scanning for its local file header signature and
    /// reading `compressed_size` bytes. Supports both compression method
    /// `0` (stored, verbatim) and method `8` (DEFLATE, the method used by
    /// real Word/LibreOffice/python-docx output) via
    /// [`flate2::read::DeflateDecoder`] (raw deflate, i.e. no zlib/gzip
    /// header — matches the ZIP local-file-data format). Any other
    /// compression method, or a DEFLATE stream that fails to inflate,
    /// falls back gracefully in [`parse_docx_bytes`].
    fn find_document_xml(bytes: &[u8]) -> Option<String> {
        use std::io::Read;

        const LOCAL_HEADER_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
        const STORED: u16 = 0;
        const DEFLATE: u16 = 8;

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
                if name == "word/document.xml" && data_end <= bytes.len() {
                    let raw = &bytes[data_start..data_end];
                    match compression {
                        STORED => {
                            return Some(String::from_utf8_lossy(raw).to_string());
                        }
                        DEFLATE => {
                            let mut decoder = flate2::read::DeflateDecoder::new(raw);
                            let mut out = String::new();
                            if decoder.read_to_string(&mut out).is_ok() {
                                return Some(out);
                            }
                            // Fall through to graceful degradation if the
                            // stream doesn't inflate cleanly.
                            return None;
                        }
                        _ => return None,
                    }
                }
                i = data_start.max(i + 1);
            } else {
                i += 1;
            }
        }
        None
    }

    pub fn parse_docx_bytes(path: &str, bytes: &[u8]) -> ParsedDocument {
        let paragraphs = find_document_xml(bytes)
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

        /// Build a minimal single-entry ZIP local-file record for
        /// `word/document.xml`, compressed with real DEFLATE (via
        /// `flate2::write::DeflateEncoder`), mirroring what real
        /// Word/LibreOffice/python-docx output actually looks like on
        /// disk (method 8, not method 0/stored). This is deliberately not
        /// just a raw XML string with the STORED method flipped -- it
        /// exercises the actual inflate path, matching the repro method
        /// described in `docs/fix7_audit_report.md`.
        fn build_deflate_docx_fixture(xml: &str) -> Vec<u8> {
            use flate2::write::DeflateEncoder;
            use flate2::Compression;
            use std::io::Write;

            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(xml.as_bytes()).unwrap();
            let compressed = encoder.finish().unwrap();

            let name = b"word/document.xml";
            let mut entry = Vec::new();
            entry.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]); // local file header sig
            entry.extend_from_slice(&[20, 0]); // version needed
            entry.extend_from_slice(&[0, 0]); // flags
            entry.extend_from_slice(&[8, 0]); // compression method = DEFLATE
            entry.extend_from_slice(&[0, 0]); // mod time
            entry.extend_from_slice(&[0, 0]); // mod date
            entry.extend_from_slice(&[0, 0, 0, 0]); // crc32 (unused by parser)
            entry.extend_from_slice(&(compressed.len() as u32).to_le_bytes()); // compressed size
            entry.extend_from_slice(&(xml.len() as u32).to_le_bytes()); // uncompressed size
            entry.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name length
            entry.extend_from_slice(&[0, 0]); // extra length
            entry.extend_from_slice(name);
            entry.extend_from_slice(&compressed);
            entry
        }

        #[test]
        fn deflate_compressed_document_xml_is_decompressed_and_parsed() {
            let xml = "<w:p><w:r><w:t>Real DEFLATE-compressed Word content</w:t></w:r></w:p>";
            let fixture = build_deflate_docx_fixture(xml);

            let doc = parse_docx_bytes("real.docx", &fixture);

            assert_eq!(doc.blocks.len(), 1);
            assert_eq!(doc.blocks[0].block_type, BlockType::Paragraph);
            assert_eq!(
                doc.blocks[0].text_content,
                "Real DEFLATE-compressed Word content"
            );
        }

        #[test]
        fn stored_fast_path_still_works_alongside_deflate_support() {
            // Guards against a regression where adding DEFLATE support
            // accidentally drops the pre-existing STORED (method 0) path.
            let xml = "<w:p><w:t>Stored content</w:t></w:p>";
            let name = b"word/document.xml";
            let mut entry = Vec::new();
            entry.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
            entry.extend_from_slice(&[20, 0]);
            entry.extend_from_slice(&[0, 0]);
            entry.extend_from_slice(&[0, 0]); // compression method = STORED
            entry.extend_from_slice(&[0, 0]);
            entry.extend_from_slice(&[0, 0]);
            entry.extend_from_slice(&[0, 0, 0, 0]);
            entry.extend_from_slice(&(xml.len() as u32).to_le_bytes());
            entry.extend_from_slice(&(xml.len() as u32).to_le_bytes());
            entry.extend_from_slice(&(name.len() as u16).to_le_bytes());
            entry.extend_from_slice(&[0, 0]);
            entry.extend_from_slice(name);
            entry.extend_from_slice(xml.as_bytes());

            let doc = parse_docx_bytes("stored.docx", &entry);
            assert_eq!(doc.blocks.len(), 1);
            assert_eq!(doc.blocks[0].text_content, "Stored content");
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

/// Plain Text Parser (Part 2 "Support ... TXT"). Not called out with its
/// own subsection in §36.2, but covered by §36.2's closing rule ("Future
/// Parsers MUST implement the same `Parser` interface ... and register
/// with the Parser Selector; no other layer needs to change") -- text
/// files have no headings/lists/tables to structurally parse (unlike
/// Markdown), so this only needs blank-line paragraph splitting, no OCR
/// (§36.2, same "no OCR" contract as the Markdown Parser).
pub mod txt {
    use atlas_types::document::{Block, BlockType, DocumentMetadata, LocationRef, ParsedDocument};
    use atlas_utils::AppError;

    use super::Parser;

    pub struct TxtParser;

    impl Parser for TxtParser {
        fn file_type(&self) -> &str {
            "txt"
        }

        fn parse(&self, path: &str) -> Result<ParsedDocument, AppError> {
            let bytes = std::fs::read(path)
                .map_err(|e| AppError::indexing(format!("failed to read '{path}': {e}")))?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            Ok(parse_txt_text(path, &text))
        }
    }

    /// Pure parsing logic, split out from file IO for direct unit testing
    /// (same convention as `markdown::parse_markdown_text`). Paragraphs are
    /// separated by one or more blank lines; lines within a paragraph are
    /// joined with a space (matching how the Markdown Parser treats a
    /// wrapped paragraph), since plain text has no other structural
    /// signal to split on.
    pub fn parse_txt_text(path: &str, text: &str) -> ParsedDocument {
        let mut blocks = Vec::new();
        let mut paragraph_buffer: Vec<&str> = Vec::new();
        let mut paragraph_start_line = 0usize;

        let flush = |buffer: &mut Vec<&str>, start_line: usize, blocks: &mut Vec<Block>| {
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
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                flush(&mut paragraph_buffer, paragraph_start_line, &mut blocks);
                continue;
            }
            if paragraph_buffer.is_empty() {
                paragraph_start_line = line_no;
            }
            paragraph_buffer.push(trimmed);
        }
        flush(&mut paragraph_buffer, paragraph_start_line, &mut blocks);

        ParsedDocument {
            metadata: DocumentMetadata {
                title: path.to_string(),
                file_type: "txt".to_string(),
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
        fn blank_lines_separate_paragraphs() {
            let doc = parse_txt_text("notes.txt", "First paragraph,\nstill first.\n\nSecond paragraph.");
            assert_eq!(doc.blocks.len(), 2);
            assert_eq!(doc.blocks[0].block_type, BlockType::Paragraph);
            assert_eq!(doc.blocks[0].text_content, "First paragraph, still first.");
            assert_eq!(doc.blocks[1].text_content, "Second paragraph.");
        }

        #[test]
        fn empty_document_produces_no_blocks() {
            let doc = parse_txt_text("empty.txt", "");
            assert!(doc.blocks.is_empty());
        }

        #[test]
        fn repeated_blank_lines_do_not_produce_empty_blocks() {
            let doc = parse_txt_text("notes.txt", "One.\n\n\n\nTwo.");
            assert_eq!(doc.blocks.len(), 2);
        }
    }
}

/// HTML Parser (§36.2 category; Part 2 "Support ... HTML"). Dependency-free
/// (no third-party HTML parser crate, consistent with `pdf`/`docx` above):
/// walks the byte stream once, tracking the current tag name, and buffers
/// visible text per block-level element. `<script>`/`<style>` contents are
/// dropped entirely (never visible text, never meant to be indexed).
/// Structural parse only (§36.2/§36.3: no OCR, no chunking here) --
/// `<h1>`-`<h6>` become Heading blocks, everything else block-level
/// (`<p>`, `<li>`, `<div>`, `<td>`, `<blockquote>`, `<br>`-flushed runs)
/// becomes a Paragraph block, `<pre>`/`<code>` becomes a Code block.
pub mod html {
    use atlas_types::document::{Block, BlockType, DocumentMetadata, LocationRef, ParsedDocument};
    use atlas_utils::AppError;

    use super::Parser;

    pub struct HtmlParser;

    impl Parser for HtmlParser {
        fn file_type(&self) -> &str {
            "html"
        }

        fn parse(&self, path: &str) -> Result<ParsedDocument, AppError> {
            let bytes = std::fs::read(path)
                .map_err(|e| AppError::indexing(format!("failed to read '{path}': {e}")))?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            Ok(parse_html_text(path, &text))
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum BlockKind {
        Paragraph,
        Heading,
        Code,
    }

    /// Tags whose content is never visible/indexable text.
    fn is_skipped_content_tag(name: &str) -> bool {
        matches!(name, "script" | "style" | "head" | "noscript")
    }

    fn heading_kind(name: &str) -> bool {
        matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
    }

    fn code_kind(name: &str) -> bool {
        matches!(name, "pre" | "code")
    }

    /// Tags that start a new block-level element -- encountering one
    /// flushes whatever text was accumulating for the previous block.
    fn is_block_boundary_tag(name: &str) -> bool {
        matches!(
            name,
            "p" | "div"
                | "li"
                | "td"
                | "th"
                | "tr"
                | "blockquote"
                | "section"
                | "article"
                | "br"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "pre"
                | "code"
        )
    }

    fn decode_entities(s: &str) -> String {
        s.replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
    }

    /// Pure parsing logic, split out from file IO for direct unit testing
    /// (same convention as the other structural parsers in this module).
    pub fn parse_html_text(path: &str, html: &str) -> ParsedDocument {
        let mut blocks = Vec::new();
        let mut buffer = String::new();
        let mut current_kind = BlockKind::Paragraph;
        let mut skip_depth = 0usize;
        let mut skip_tag_stack: Vec<String> = Vec::new();
        let mut block_index = 0usize;

        let flush = |buffer: &mut String, kind: BlockKind, block_index: &mut usize, blocks: &mut Vec<Block>| {
            let trimmed = buffer.trim();
            if trimmed.is_empty() {
                buffer.clear();
                return;
            }
            *block_index += 1;
            blocks.push(Block {
                block_type: match kind {
                    BlockKind::Paragraph => BlockType::Paragraph,
                    BlockKind::Heading => BlockType::Heading,
                    BlockKind::Code => BlockType::Code,
                },
                location_ref: LocationRef {
                    page_or_location: block_index.to_string(),
                },
                text_content: decode_entities(trimmed),
            });
            buffer.clear();
        };

        let chars: Vec<char> = html.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            if chars[i] == '<' {
                // Find the matching '>' for this tag (or bail to end of
                // input on a malformed/unterminated tag rather than
                // looping forever).
                let start = i;
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '>' {
                    j += 1;
                }
                let tag_content: String = chars[start + 1..j].iter().collect();
                i = if j < chars.len() { j + 1 } else { chars.len() };

                let is_closing = tag_content.starts_with('/');
                let name_part = tag_content.trim_start_matches('/');
                let tag_name: String = name_part
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_lowercase();
                if tag_name.is_empty() {
                    continue; // e.g. a comment `<!-- ... -->` or malformed tag
                }

                if is_skipped_content_tag(&tag_name) {
                    if is_closing {
                        if let Some(pos) = skip_tag_stack.iter().rposition(|t| t == &tag_name) {
                            skip_tag_stack.remove(pos);
                            skip_depth = skip_depth.saturating_sub(1);
                        }
                    } else {
                        skip_tag_stack.push(tag_name.clone());
                        skip_depth += 1;
                    }
                    continue;
                }
                if skip_depth > 0 {
                    continue;
                }

                if is_block_boundary_tag(&tag_name) {
                    flush(&mut buffer, current_kind, &mut block_index, &mut blocks);
                    current_kind = if heading_kind(&tag_name) {
                        BlockKind::Heading
                    } else if code_kind(&tag_name) {
                        BlockKind::Code
                    } else {
                        BlockKind::Paragraph
                    };
                }
                continue;
            }

            if skip_depth == 0 {
                buffer.push(chars[i]);
            }
            i += 1;
        }
        flush(&mut buffer, current_kind, &mut block_index, &mut blocks);

        ParsedDocument {
            metadata: DocumentMetadata {
                title: path.to_string(),
                file_type: "html".to_string(),
                content_hash: atlas_utils::hashing::hash_str(html),
            },
            blocks,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use atlas_types::document::BlockType;

        #[test]
        fn headings_and_paragraphs_become_distinct_blocks() {
            let doc = parse_html_text(
                "page.html",
                "<html><body><h1>Title</h1><p>Some intro text.</p></body></html>",
            );
            assert_eq!(doc.blocks.len(), 2);
            assert_eq!(doc.blocks[0].block_type, BlockType::Heading);
            assert_eq!(doc.blocks[0].text_content, "Title");
            assert_eq!(doc.blocks[1].block_type, BlockType::Paragraph);
            assert_eq!(doc.blocks[1].text_content, "Some intro text.");
        }

        #[test]
        fn script_and_style_content_is_never_indexed() {
            let doc = parse_html_text(
                "page.html",
                "<html><head><style>.x{color:red}</style></head><body><script>alert('hi')</script><p>Real content.</p></body></html>",
            );
            assert_eq!(doc.blocks.len(), 1);
            assert_eq!(doc.blocks[0].text_content, "Real content.");
        }

        #[test]
        fn html_entities_are_decoded() {
            let doc = parse_html_text("page.html", "<p>Fish &amp; Chips &mdash; caf&#39;e</p>");
            assert!(doc.blocks[0].text_content.starts_with("Fish & Chips"));
            assert!(doc.blocks[0].text_content.contains("caf'e"));
        }

        #[test]
        fn code_blocks_are_tagged_distinctly() {
            let doc = parse_html_text("page.html", "<pre><code>fn main() {}</code></pre>");
            assert!(doc.blocks.iter().any(|b| b.block_type == BlockType::Code));
        }

        #[test]
        fn empty_document_produces_no_blocks() {
            let doc = parse_html_text("empty.html", "<html><body></body></html>");
            assert!(doc.blocks.is_empty());
        }

        #[test]
        fn br_tags_split_lines_into_separate_paragraph_blocks() {
            let doc = parse_html_text("page.html", "<p>Line one<br>Line two</p>");
            assert_eq!(doc.blocks.len(), 2);
            assert_eq!(doc.blocks[0].text_content, "Line one");
            assert_eq!(doc.blocks[1].text_content, "Line two");
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
