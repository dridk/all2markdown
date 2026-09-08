//! What each Supported Format declares about itself, and the line between
//! that and what the filesystem knows.

use all2markdown_core::{extract, Options, SourceDocument};
use std::path::Path;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
}

fn extract_fixture(name: &str) -> all2markdown_core::Extraction {
    let data = fixture(name);
    extract(SourceDocument::named(name, &data), &Options::default())
        .unwrap_or_else(|e| panic!("extracting {name} failed: {e}"))
}

#[test]
fn docx_reports_what_its_metadata_parts_declare() {
    let document = extract_fixture("1000.docx").document;

    assert_eq!(document.language.as_deref(), Some("fr-FR"));
    assert_eq!(document.page_count, Some(1));
    // 2023-08-28T21:25:50Z and 2023-08-28T23:41:20Z
    assert_eq!(document.created, Some(1_693_257_950));
    assert_eq!(document.modified, Some(1_693_266_080));
}

#[test]
fn docx_keeps_everything_else_in_the_raw_bag() {
    let document = extract_fixture("1000.docx").document;

    assert_eq!(document.raw.get("revision").map(String::as_str), Some("15"));
    assert_eq!(document.raw.get("Words").map(String::as_str), Some("108"));
    assert!(document
        .raw
        .get("Application")
        .is_some_and(|application| application.contains("LibreOffice")));
}

#[test]
fn pdf_reports_what_its_info_dictionary_declares() {
    let document = extract_fixture("1000.pdf").document;

    assert_eq!(document.page_count, Some(1));
    assert_eq!(
        document.raw.get("creator").map(String::as_str),
        Some("Writer")
    );
    assert!(document
        .raw
        .get("producer")
        .is_some_and(|producer| producer.contains("LibreOffice")));
    assert!(
        document.created.is_some(),
        "the PDF declares a CreationDate"
    );
}

#[test]
fn a_format_that_declares_nothing_yields_empty_metadata_rather_than_an_error() {
    // Neither unword nor rtf-parser hands back a document's declared
    // metadata, so doc and rtf report none — which is the defaulted
    // behaviour, not a failure.
    for name in ["1000.doc", "1000.rtf"] {
        let extraction = extract_fixture(name);
        assert!(
            extraction.document.is_empty(),
            "{name} should declare no Document Metadata, got {:?}",
            extraction.document
        );
        assert!(
            !extraction.markdown.is_empty(),
            "{name} must still extract its text"
        );
    }
}

#[test]
fn file_metadata_and_document_metadata_stay_apart() {
    let extraction = extract_fixture("1000.docx");

    // The name is the filesystem's and reaches only File Metadata; the title
    // is the document's and reaches only Document Metadata. This fixture
    // leaves its title empty, and nothing fills it in from the name.
    assert_eq!(extraction.file.name.as_deref(), Some("1000.docx"));
    assert_eq!(extraction.document.title, None);
    assert_eq!(extraction.document.author, None);
}

#[test]
fn the_raw_bag_keeps_a_deterministic_order() {
    let first: Vec<_> = extract_fixture("1000.docx")
        .document
        .raw
        .into_iter()
        .collect();
    let second: Vec<_> = extract_fixture("1000.docx")
        .document
        .raw
        .into_iter()
        .collect();

    assert_eq!(first, second);
    let mut sorted = first.clone();
    sorted.sort_by(|(a, _), (b, _)| a.cmp(b));
    assert_eq!(first, sorted, "the raw bag must iterate in key order");
}

#[test]
fn reading_metadata_does_not_change_the_extracted_text() {
    // The metadata stream is separate from the body in every format here, so
    // reading one must leave the other untouched.
    for name in ["1000.doc", "1000.docx", "1000.rtf", "1000.pdf"] {
        let extraction = extract_fixture(name);
        assert!(
            extraction.markdown.contains("je mange du chocolat"),
            "{name} lost body text"
        );
    }
}
