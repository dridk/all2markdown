//! The detection cascade and encoding detection, through the public entry
//! points a caller uses.

use all2markdown_core::{
    detect_format, extract, format_from_id, Failure, Format, Options, Registry, SourceDocument,
    Warning,
};
use std::path::Path;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
}

/// French prose, long enough for statistical detection to have something to
/// work with. Encoded per test into whatever the test is about.
const FRENCH: &str = "Le café coûte cinq euros. Élodie préfère le thé très chaud, \
                      arrosé d'un peu de miel, à côté de la fenêtre où le soleil \
                      s'attarde. Août fut étouffant; l'hôtel était complet.";

fn windows_1252(text: &str) -> Vec<u8> {
    let (bytes, _, had_errors) = encoding_rs::WINDOWS_1252.encode(text);
    assert!(
        !had_errors,
        "the test text must be representable in windows-1252"
    );
    bytes.into_owned()
}

fn utf_16le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

// ── The cascade ───────────────────────────────────────────────────

#[test]
fn a_decodable_file_with_no_signature_and_no_extension_is_read_as_text() {
    let source = SourceDocument::from_bytes(b"This is just plain text, not a document format!!");
    assert_eq!(detect_format(&source).unwrap(), Format::TXT);
}

#[test]
fn a_binary_file_with_no_signature_is_a_failure_rather_than_garbage() {
    let mut binary = b"BLOB".to_vec();
    binary.extend((0u8..=255).cycle().take(4096));
    let source = SourceDocument::from_bytes(&binary);
    assert!(
        matches!(detect_format(&source), Err(Failure::UnrecognizedFormat)),
        "unrecognised binary must fail, not be read as text"
    );
}

#[test]
fn the_extension_is_consulted_only_when_no_signature_matched() {
    // A PDF misnamed `.doc`: the signature is Certain, the extension only
    // Likely, so the content wins and the name loses.
    let data = fixture("1000.pdf");
    let source = SourceDocument::named("report.doc", &data);
    assert_eq!(detect_format(&source).unwrap(), Format::PDF);
}

#[test]
fn the_extension_identifies_what_carries_no_signature() {
    // Nothing here has a signature, and the bytes alone would say `txt`. The
    // name is the only evidence there is, so it decides.
    let source = SourceDocument::named("notes.rtf", b"not really an RTF, but named one");
    assert_eq!(detect_format(&source).unwrap(), Format::RTF);
}

#[test]
fn a_forced_format_beats_every_stage_of_the_cascade() {
    let data = fixture("1000.docx");
    let result = extract(
        SourceDocument::named("1000.docx", &data),
        &Options::forcing(Format::RTF),
    );
    assert!(
        result.is_err(),
        "a Forced Format must never fall back to detection"
    );
}

#[test]
fn the_text_fallback_is_a_registered_parser_rather_than_a_branch_in_the_detector() {
    // The proof that nothing special-cases it: an empty registry, which holds
    // no Parsers at all, recognises no text either.
    let registry = Registry::new();
    let source = SourceDocument::from_bytes(b"plain text with no signature at all");
    assert!(matches!(
        registry.detect(&source),
        Err(Failure::UnrecognizedFormat)
    ));

    assert_eq!(format_from_id("txt"), Some(Format::TXT));
}

// ── Encoding ──────────────────────────────────────────────────────

#[test]
fn a_windows_1252_document_is_decoded_correctly_and_reports_its_encoding() {
    let bytes = windows_1252(FRENCH);
    let extraction = extract(
        SourceDocument::named("legacy.txt", &bytes),
        &Options::default(),
    )
    .unwrap();

    assert_eq!(extraction.markdown, FRENCH);
    assert_eq!(extraction.encoding.as_deref(), Some("windows-1252"));
}

#[test]
fn a_utf_16_document_with_a_byte_order_mark_is_decoded_correctly() {
    let bytes = utf_16le_with_bom(FRENCH);
    let extraction = extract(SourceDocument::from_bytes(&bytes), &Options::default()).unwrap();

    assert_eq!(extraction.format, Format::TXT);
    assert_eq!(extraction.markdown, FRENCH);
    assert_eq!(extraction.encoding.as_deref(), Some("UTF-16LE"));
}

#[test]
fn utf_8_is_reported_as_the_encoding_it_is() {
    let extraction = extract(
        SourceDocument::named("notes.txt", FRENCH.as_bytes()),
        &Options::default(),
    )
    .unwrap();

    assert_eq!(extraction.encoding.as_deref(), Some("UTF-8"));
    assert!(
        extraction.warnings.is_empty(),
        "a clean UTF-8 read warns about nothing"
    );
}

#[test]
fn a_well_evidenced_guess_raises_no_warning() {
    let bytes = windows_1252(FRENCH);
    let extraction = extract(
        SourceDocument::named("legacy.txt", &bytes),
        &Options::default(),
    )
    .unwrap();

    assert!(
        extraction.warnings.is_empty(),
        "a page of accented text is evidence, not a coin flip: {:?}",
        extraction.warnings
    );
}

#[test]
fn the_caller_can_override_the_detected_encoding() {
    // The same bytes, read two ways: byte 0xE9 is `é` in windows-1252 and `й`
    // in windows-1251, and only the caller can know which was meant.
    let bytes = windows_1252(FRENCH);

    let guessed = extract(SourceDocument::named("a.txt", &bytes), &Options::default()).unwrap();
    let forced = extract(
        SourceDocument::named("a.txt", &bytes),
        &Options::default().with_encoding("windows-1251"),
    )
    .unwrap();

    assert_eq!(guessed.encoding.as_deref(), Some("windows-1252"));
    assert_eq!(forced.encoding.as_deref(), Some("windows-1251"));
    assert!(
        forced.markdown.contains('й'),
        "the override must actually change the read"
    );
}

#[test]
fn an_unknown_encoding_label_is_a_failure_rather_than_a_silent_fallback() {
    let failure = extract(
        SourceDocument::named("a.txt", FRENCH.as_bytes()),
        &Options::default().with_encoding("klingon-1"),
    )
    .unwrap_err();

    assert!(
        matches!(&failure, Failure::UnknownEncoding(label) if label == "klingon-1"),
        "expected UnknownEncoding, got {failure}"
    );
}

#[test]
fn a_shaky_guess_raises_a_warning_so_it_can_be_found_with_a_query() {
    // One accented byte and nothing else to go on. Every single-byte encoding
    // maps it, each to a different letter, so the guess is a coin flip.
    let bytes = windows_1252("Café");
    let extraction = extract(
        SourceDocument::named("short.txt", &bytes),
        &Options::default(),
    )
    .unwrap();

    assert!(
        extraction
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UncertainEncoding(_))),
        "expected an UncertainEncoding warning, got {:?}",
        extraction.warnings
    );
}

// ── Strict mode ───────────────────────────────────────────────────

#[test]
fn strict_mode_turns_the_last_resort_stage_into_a_failure() {
    let source = SourceDocument::from_bytes(b"plain text with no signature at all");
    let options = Options::default().strict();

    assert_eq!(
        extract(source, &Options::default()).unwrap().format,
        Format::TXT
    );
    assert!(matches!(
        extract(source, &options),
        Err(Failure::UnrecognizedFormat)
    ));
}

#[test]
fn strict_mode_changes_nothing_else() {
    let options = Options::default().strict();

    // A signature still wins.
    let pdf = fixture("1000.pdf");
    let by_signature = extract(SourceDocument::from_bytes(&pdf), &options).unwrap();
    assert_eq!(by_signature.format, Format::PDF);

    // So does an extension, which is not the last-resort stage.
    let by_extension = extract(
        SourceDocument::named("notes.txt", FRENCH.as_bytes()),
        &options,
    )
    .unwrap();
    assert_eq!(by_extension.format, Format::TXT);
    assert_eq!(by_extension.markdown, FRENCH);
}
