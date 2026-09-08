use all2md_core::{parse, detect_format, Format, All2mdError};
use std::path::Path;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
}

// ── Format detection ──────────────────────────────────────────────

#[test]
fn detect_doc_format() {
    let data = fixture("1000.doc");
    assert_eq!(detect_format(&data).unwrap(), Format::Doc);
}

#[test]
fn detect_docx_format() {
    let data = fixture("1000.docx");
    assert_eq!(detect_format(&data).unwrap(), Format::Docx);
}

#[test]
fn detect_rtf_format() {
    let data = fixture("1000.rtf");
    assert_eq!(detect_format(&data).unwrap(), Format::Rtf);
}

#[test]
fn detect_pdf_format() {
    let data = fixture("1000.pdf");
    assert_eq!(detect_format(&data).unwrap(), Format::Pdf);
}

#[test]
fn detect_too_small() {
    let data = b"tiny";
    assert!(matches!(detect_format(data), Err(All2mdError::FileTooSmall)));
}

#[test]
fn detect_unknown_format() {
    let data = b"This is just plain text, not a document format!!";
    assert!(matches!(detect_format(data), Err(All2mdError::UnrecognizedFormat)));
}

// ── DOC parser ────────────────────────────────────────────────────

#[test]
fn doc_contains_title() {
    let data = fixture("1000.doc");
    let md = parse(&data, Some(Format::Doc)).unwrap();
    assert!(md.contains("Concert du soir"));
}

#[test]
fn doc_contains_headings() {
    let data = fixture("1000.doc");
    let md = parse(&data, Some(Format::Doc)).unwrap();
    assert!(md.contains("# Ceci est le titre"));
    assert!(md.contains("## Sous titre"));
    assert!(md.contains("### Sous sous titre"));
}

#[test]
fn doc_contains_body_text() {
    let data = fixture("1000.doc");
    let md = parse(&data, Some(Format::Doc)).unwrap();
    assert!(md.contains("je mange du chocolat"));
    assert!(md.contains("truc muche"));
}

// ── DOCX parser ───────────────────────────────────────────────────

#[test]
fn docx_contains_title() {
    let data = fixture("1000.docx");
    let md = parse(&data, Some(Format::Docx)).unwrap();
    assert!(md.contains("Concert du soir"));
}

#[test]
fn docx_contains_headings() {
    let data = fixture("1000.docx");
    let md = parse(&data, Some(Format::Docx)).unwrap();
    assert!(md.contains("# Ceci est le titre"));
    assert!(md.contains("## Sous titre"));
    assert!(md.contains("### Sous sous titre"));
    assert!(md.contains("#### Super sous titre"));
}

#[test]
fn docx_contains_body_text() {
    let data = fixture("1000.docx");
    let md = parse(&data, Some(Format::Docx)).unwrap();
    assert!(md.contains("je mange du chocolat"));
    assert!(md.contains("truc muche"));
}

#[test]
fn docx_excludes_textbox_content() {
    let data = fixture("1000.docx");
    let md = parse(&data, Some(Format::Docx)).unwrap();
    // The PDF output shows "ZONE DE TEXTE" in textboxes.
    // DOCX parser should NOT include textbox content.
    assert!(!md.contains("ZONE DE TEXTE"), "DOCX should not contain textbox text, got:\n{md}");
}

// ── RTF parser ────────────────────────────────────────────────────

#[test]
fn rtf_contains_title() {
    let data = fixture("1000.rtf");
    let md = parse(&data, Some(Format::Rtf)).unwrap();
    assert!(md.contains("Concert du soir"));
}

#[test]
fn rtf_contains_body_text() {
    let data = fixture("1000.rtf");
    let md = parse(&data, Some(Format::Rtf)).unwrap();
    assert!(md.contains("je mange du chocolat"));
    assert!(md.contains("truc muche"));
}

#[test]
fn rtf_not_empty() {
    let data = fixture("1000.rtf");
    let md = parse(&data, Some(Format::Rtf)).unwrap();
    assert!(md.len() > 100, "RTF output too short: {} bytes", md.len());
}

// ── PDF parser ────────────────────────────────────────────────────

#[test]
fn pdf_contains_title() {
    let data = fixture("1000.pdf");
    let md = parse(&data, Some(Format::Pdf)).unwrap();
    assert!(md.contains("Concert du soir"));
}

#[test]
fn pdf_contains_body_text() {
    let data = fixture("1000.pdf");
    let md = parse(&data, Some(Format::Pdf)).unwrap();
    assert!(md.contains("je mange du chocolat"));
    assert!(md.contains("truc muche"));
}

#[test]
fn pdf_contains_headings_as_text() {
    let data = fixture("1000.pdf");
    let md = parse(&data, Some(Format::Pdf)).unwrap();
    assert!(md.contains("Ceci est le titre"));
    assert!(md.contains("Sous titre"));
}

// ── Auto-detection (parse without format) ─────────────────────────

#[test]
fn auto_detect_doc() {
    let data = fixture("1000.doc");
    let md = parse(&data, None).unwrap();
    assert!(md.contains("Concert du soir"));
}

#[test]
fn auto_detect_docx() {
    let data = fixture("1000.docx");
    let md = parse(&data, None).unwrap();
    assert!(md.contains("Concert du soir"));
}

#[test]
fn auto_detect_rtf() {
    let data = fixture("1000.rtf");
    let md = parse(&data, None).unwrap();
    assert!(md.contains("Concert du soir"));
}

#[test]
fn auto_detect_pdf() {
    let data = fixture("1000.pdf");
    let md = parse(&data, None).unwrap();
    assert!(md.contains("Concert du soir"));
}

// ── Format::from_str_loose ────────────────────────────────────────

#[test]
fn format_from_str_valid() {
    assert_eq!(Format::from_str_loose("doc").unwrap(), Format::Doc);
    assert_eq!(Format::from_str_loose("DOCX").unwrap(), Format::Docx);
    assert_eq!(Format::from_str_loose("Rtf").unwrap(), Format::Rtf);
    assert_eq!(Format::from_str_loose("PDF").unwrap(), Format::Pdf);
}

#[test]
fn format_from_str_invalid() {
    assert!(Format::from_str_loose("odt").is_err());
    assert!(Format::from_str_loose("").is_err());
}
