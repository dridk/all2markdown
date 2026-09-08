use all2markdown_core::{
    detect_format, extract, Confidence, Extraction, Failure, Format, Options, SourceDocument,
};
use std::path::Path;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
}

fn extract_fixture(name: &str, format: Option<Format>) -> Extraction {
    let data = fixture(name);
    let options = format.map(Options::forcing).unwrap_or_default();
    extract(SourceDocument::named(name, &data), &options)
        .unwrap_or_else(|e| panic!("extracting {name} failed: {e}"))
}

fn markdown_of(name: &str, format: Option<Format>) -> String {
    extract_fixture(name, format).markdown
}

// ── Format detection ──────────────────────────────────────────────

fn detect(name: &str) -> Result<Format, Failure> {
    let data = fixture(name);
    detect_format(&SourceDocument::from_bytes(&data))
}

#[test]
fn detect_doc_format() {
    assert_eq!(detect("1000.doc").unwrap(), Format::DOC);
}

#[test]
fn detect_docx_format() {
    assert_eq!(detect("1000.docx").unwrap(), Format::DOCX);
}

#[test]
fn detect_rtf_format() {
    assert_eq!(detect("1000.rtf").unwrap(), Format::RTF);
}

#[test]
fn detect_pdf_format() {
    assert_eq!(detect("1000.pdf").unwrap(), Format::PDF);
}

#[test]
fn detect_too_small() {
    let source = SourceDocument::from_bytes(b"tiny");
    assert!(matches!(detect_format(&source), Err(Failure::FileTooSmall)));
}

#[test]
fn detect_unknown_format() {
    // Plain text carries no signature. Step 3 makes this the last-resort text
    // Parser's job; until then it is unrecognized.
    let source = SourceDocument::from_bytes(b"This is just plain text, not a document format!!");
    assert!(matches!(detect_format(&source), Err(Failure::UnrecognizedFormat)));
}

// ── DOC parser ────────────────────────────────────────────────────

#[test]
fn doc_contains_title() {
    assert!(markdown_of("1000.doc", Some(Format::DOC)).contains("Concert du soir"));
}

#[test]
fn doc_contains_headings() {
    let md = markdown_of("1000.doc", Some(Format::DOC));
    assert!(md.contains("# Ceci est le titre"));
    assert!(md.contains("## Sous titre"));
    assert!(md.contains("### Sous sous titre"));
}

#[test]
fn doc_contains_body_text() {
    let md = markdown_of("1000.doc", Some(Format::DOC));
    assert!(md.contains("je mange du chocolat"));
    assert!(md.contains("truc muche"));
}

// ── DOCX parser ───────────────────────────────────────────────────

#[test]
fn docx_contains_title() {
    assert!(markdown_of("1000.docx", Some(Format::DOCX)).contains("Concert du soir"));
}

#[test]
fn docx_contains_headings() {
    let md = markdown_of("1000.docx", Some(Format::DOCX));
    assert!(md.contains("# Ceci est le titre"));
    assert!(md.contains("## Sous titre"));
    assert!(md.contains("### Sous sous titre"));
    assert!(md.contains("#### Super sous titre"));
}

#[test]
fn docx_contains_body_text() {
    let md = markdown_of("1000.docx", Some(Format::DOCX));
    assert!(md.contains("je mange du chocolat"));
    assert!(md.contains("truc muche"));
}

#[test]
fn docx_excludes_textbox_content() {
    let md = markdown_of("1000.docx", Some(Format::DOCX));
    // The PDF rendering shows "ZONE DE TEXTE" in textboxes; the DOCX parser
    // deliberately skips Drawing and Shape runs.
    assert!(!md.contains("ZONE DE TEXTE"), "DOCX should not contain textbox text, got:\n{md}");
}

// ── RTF parser ────────────────────────────────────────────────────

#[test]
fn rtf_contains_title() {
    assert!(markdown_of("1000.rtf", Some(Format::RTF)).contains("Concert du soir"));
}

#[test]
fn rtf_contains_body_text() {
    let md = markdown_of("1000.rtf", Some(Format::RTF));
    assert!(md.contains("je mange du chocolat"));
    assert!(md.contains("truc muche"));
}

#[test]
fn rtf_not_empty() {
    let md = markdown_of("1000.rtf", Some(Format::RTF));
    assert!(md.len() > 100, "RTF output too short: {} bytes", md.len());
}

// ── PDF parser ────────────────────────────────────────────────────

#[test]
fn pdf_contains_title() {
    assert!(markdown_of("1000.pdf", Some(Format::PDF)).contains("Concert du soir"));
}

#[test]
fn pdf_contains_body_text() {
    let md = markdown_of("1000.pdf", Some(Format::PDF));
    assert!(md.contains("je mange du chocolat"));
    assert!(md.contains("truc muche"));
}

#[test]
fn pdf_contains_headings_as_text() {
    let md = markdown_of("1000.pdf", Some(Format::PDF));
    assert!(md.contains("Ceci est le titre"));
    assert!(md.contains("Sous titre"));
}

// ── Auto-detection ────────────────────────────────────────────────

#[test]
fn auto_detect_doc() {
    assert!(markdown_of("1000.doc", None).contains("Concert du soir"));
}

#[test]
fn auto_detect_docx() {
    assert!(markdown_of("1000.docx", None).contains("Concert du soir"));
}

#[test]
fn auto_detect_rtf() {
    assert!(markdown_of("1000.rtf", None).contains("Concert du soir"));
}

#[test]
fn auto_detect_pdf() {
    assert!(markdown_of("1000.pdf", None).contains("Concert du soir"));
}

// ── The Extraction carries more than the text ─────────────────────

#[test]
fn extraction_reports_the_format_it_used() {
    assert_eq!(extract_fixture("1000.docx", None).format, Format::DOCX);
}

#[test]
fn extraction_carries_file_metadata() {
    let extraction = extract_fixture("1000.docx", None);
    assert_eq!(extraction.file.name.as_deref(), Some("1000.docx"));
    assert_eq!(extraction.file.size, Some(fixture("1000.docx").len() as u64));
}

#[test]
fn a_healthy_document_raises_no_warning() {
    assert!(extract_fixture("1000.docx", None).warnings.is_empty());
}

#[test]
fn forced_format_beats_detection() {
    // A DOCX is a ZIP; forcing RTF must not silently fall back to detection.
    let data = fixture("1000.docx");
    let result = extract(SourceDocument::from_bytes(&data), &Options::forcing(Format::RTF));
    assert!(result.is_err(), "forcing a wrong format must fail, not re-detect");
}

// ── Format ────────────────────────────────────────────────────────

#[test]
fn format_from_id_is_case_insensitive() {
    assert_eq!(Format::from_id("doc"), Some(Format::DOC));
    assert_eq!(Format::from_id("DOCX"), Some(Format::DOCX));
    assert_eq!(Format::from_id("Rtf"), Some(Format::RTF));
    assert_eq!(Format::from_id(" PDF "), Some(Format::PDF));
}

#[test]
fn format_from_id_rejects_the_unknown() {
    assert_eq!(Format::from_id("odt"), None);
    assert_eq!(Format::from_id(""), None);
}

#[test]
fn format_displays_as_its_id() {
    assert_eq!(Format::DOCX.to_string(), "docx");
}

// ── Confidence ────────────────────────────────────────────────────

#[test]
fn confidence_orders_from_no_to_certain() {
    // Detection keeps the highest, so the ordering is load-bearing.
    assert!(Confidence::Certain > Confidence::Likely);
    assert!(Confidence::Likely > Confidence::LastResort);
    assert!(Confidence::LastResort > Confidence::No);
}

// ── SourceDocument ────────────────────────────────────────────────

#[test]
fn extension_is_read_from_the_name() {
    assert_eq!(SourceDocument::named("report.docx", b"").extension(), Some("docx"));
    assert_eq!(SourceDocument::named("archive.tar.gz", b"").extension(), Some("gz"));
    assert_eq!(SourceDocument::named("README", b"").extension(), None);
    assert_eq!(SourceDocument::named(".gitignore", b"").extension(), None);
    assert_eq!(SourceDocument::from_bytes(b"").extension(), None);
}
