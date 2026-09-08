//! The Parser registry seam: what a consumer of this crate can do with it.

use all2markdown_core::{
    Confidence, DocumentMetadata, Extracted, Failure, Format, Options, Parser, Registry,
    SourceDocument,
};
use std::path::Path;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
}

#[test]
fn the_builtin_registry_recognizes_the_formats_this_crate_ships_with() {
    let registry = Registry::with_builtin_parsers();
    for (name, expected) in [
        ("1000.doc", Format::DOC),
        ("1000.docx", Format::DOCX),
        ("1000.rtf", Format::RTF),
        ("1000.pdf", Format::PDF),
    ] {
        let data = fixture(name);
        assert_eq!(
            registry
                .detect(&SourceDocument::named(name, &data))
                .unwrap(),
            expected
        );
    }
}

/// A Parser a consumer of this crate could write: one file, one registration
/// line, no edit to anything central.
struct MemoParser {
    format: Format,
}

impl MemoParser {
    const MAGIC: &'static [u8] = b"MEMO/1.0";

    fn new(id: &'static str) -> Self {
        Self {
            format: Format::new(id),
        }
    }
}

impl Parser for MemoParser {
    fn format(&self) -> Format {
        self.format
    }

    fn probe(&self, source: &SourceDocument<'_>) -> Confidence {
        if source.bytes.starts_with(Self::MAGIC) {
            Confidence::Certain
        } else {
            Confidence::No
        }
    }

    fn metadata(&self, _source: &SourceDocument<'_>) -> Result<DocumentMetadata, Failure> {
        Ok(DocumentMetadata {
            title: Some("Memo".to_owned()),
            ..DocumentMetadata::default()
        })
    }

    fn extract(
        &self,
        source: &SourceDocument<'_>,
        _options: &Options,
    ) -> Result<Extracted, Failure> {
        let body = &source.bytes[Self::MAGIC.len()..];
        String::from_utf8(body.to_vec())
            .map(Extracted::from)
            .map_err(|e| Failure::parse(self.format, format!("not valid UTF-8: {e}")))
    }
}

const MEMO_DOCUMENT: &[u8] = b"MEMO/1.0Buy milk.";

#[test]
fn a_registered_parser_is_detected_like_any_other() {
    let mut registry = Registry::with_builtin_parsers();
    registry.register(MemoParser::new("memo"));

    let detected = registry
        .detect(&SourceDocument::from_bytes(MEMO_DOCUMENT))
        .unwrap();
    assert_eq!(detected, Format::new("memo"));
}

#[test]
fn a_registered_parser_extracts_and_reports_its_metadata() {
    let mut registry = Registry::with_builtin_parsers();
    registry.register(MemoParser::new("memo"));

    let extraction = registry
        .extract(
            SourceDocument::from_bytes(MEMO_DOCUMENT),
            &Options::default(),
        )
        .unwrap();

    assert_eq!(extraction.markdown, "Buy milk.");
    assert_eq!(extraction.format, Format::new("memo"));
    assert_eq!(extraction.document.title.as_deref(), Some("Memo"));
}

#[test]
fn a_registered_format_can_be_looked_up_by_id() {
    let mut registry = Registry::with_builtin_parsers();
    registry.register(MemoParser::new("memo"));

    assert_eq!(registry.format_from_id("MEMO"), Some(Format::new("memo")));
    assert_eq!(registry.format_from_id("odt"), None);
}

#[test]
fn a_parser_that_declares_nothing_reports_no_document_metadata() {
    // The trait's default, and the reason a format that carries no metadata
    // stays down to two methods: this Parser implements no `metadata`.
    struct Terse;
    impl Parser for Terse {
        fn format(&self) -> Format {
            Format::new("terse")
        }
        fn probe(&self, _source: &SourceDocument<'_>) -> Confidence {
            Confidence::Certain
        }
        fn extract(
            &self,
            _source: &SourceDocument<'_>,
            _options: &Options,
        ) -> Result<Extracted, Failure> {
            Ok("a word".to_owned().into())
        }
    }

    let mut registry = Registry::new();
    registry.register(Terse);

    let extraction = registry
        .extract(
            SourceDocument::from_bytes(b"anything at all"),
            &Options::default(),
        )
        .unwrap();

    assert!(extraction.document.is_empty());
}

#[test]
fn the_most_confident_parser_wins_whatever_its_registration_order() {
    struct Hesitant;
    impl Parser for Hesitant {
        fn format(&self) -> Format {
            Format::new("hesitant")
        }
        fn probe(&self, _source: &SourceDocument<'_>) -> Confidence {
            Confidence::LastResort
        }
        fn extract(
            &self,
            _source: &SourceDocument<'_>,
            _options: &Options,
        ) -> Result<Extracted, Failure> {
            Ok(Extracted::default())
        }
    }

    let mut registry = Registry::new();
    registry.register(Hesitant);
    registry.register(MemoParser::new("memo"));

    let detected = registry
        .detect(&SourceDocument::from_bytes(MEMO_DOCUMENT))
        .unwrap();
    assert_eq!(
        detected,
        Format::new("memo"),
        "Certain must beat LastResort"
    );
}

#[test]
fn equal_confidence_is_broken_by_registration_order() {
    let mut registry = Registry::new();
    registry.register(MemoParser::new("first"));
    registry.register(MemoParser::new("second"));

    let detected = registry
        .detect(&SourceDocument::from_bytes(MEMO_DOCUMENT))
        .unwrap();
    assert_eq!(detected, Format::new("first"));
}

#[test]
fn an_empty_registry_recognizes_nothing() {
    let registry = Registry::new();
    let data = fixture("1000.docx");

    let failure = registry
        .detect(&SourceDocument::from_bytes(&data))
        .unwrap_err();
    assert!(matches!(failure, Failure::UnrecognizedFormat));
}

#[test]
fn a_forced_format_with_no_parser_fails_rather_than_falling_back() {
    let registry = Registry::with_builtin_parsers();
    let data = fixture("1000.docx");

    let failure = registry
        .extract(
            SourceDocument::from_bytes(&data),
            &Options::forcing(Format::new("odt")),
        )
        .unwrap_err();

    assert!(
        matches!(&failure, Failure::UnsupportedFormat(id) if id == "odt"),
        "expected UnsupportedFormat, got {failure}"
    );
}
