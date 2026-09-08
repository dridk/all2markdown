//! Compressed Source Documents, and the cap that keeps a bomb from taking the
//! process down with it.

use all2markdown_core::{detect_format, extract, Failure, Format, Options, SourceDocument};
use std::io::Write;
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

fn gzip(payload: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(payload).unwrap();
    encoder.finish().unwrap()
}

/// The four Envelopes, each wrapping the fixture whose uncompressed twin the
/// repository also holds.
const TWINS: [(&str, &str, Format); 4] = [
    ("1000.doc.gz", "1000.doc", Format::DOC),
    ("1000.docx.zst", "1000.docx", Format::DOCX),
    ("1000.rtf.xz", "1000.rtf", Format::RTF),
    ("1000.pdf.bz2", "1000.pdf", Format::PDF),
];

#[test]
fn every_envelope_is_stripped_before_detection() {
    for (compressed, _, expected) in TWINS {
        let data = fixture(compressed);
        let detected = detect_format(&SourceDocument::named(compressed, &data))
            .unwrap_or_else(|e| panic!("detecting {compressed} failed: {e}"));
        assert_eq!(
            detected, expected,
            "{compressed} must be identified by its content"
        );
    }
}

#[test]
fn a_compressed_document_extracts_exactly_as_its_uncompressed_twin() {
    for (compressed, plain, _) in TWINS {
        let from_envelope = extract_fixture(compressed);
        let from_plain = extract_fixture(plain);
        assert_eq!(
            from_envelope.markdown, from_plain.markdown,
            "{compressed} and {plain} must produce identical text"
        );
        assert_eq!(from_envelope.format, from_plain.format);
    }
}

#[test]
fn detection_sees_the_content_including_at_the_extension_stage() {
    // No signature to go on, so only the name can decide — and the name it
    // must read is `notes.rtf`, not `notes.rtf.gz`.
    let compressed = gzip(b"not really an RTF, but named one");
    let source = SourceDocument::named("notes.rtf.gz", &compressed);
    assert_eq!(detect_format(&source).unwrap(), Format::RTF);
}

#[test]
fn file_metadata_still_describes_what_is_on_disk() {
    // The Envelope comes off for extraction, not for provenance: an archivist
    // who reads the size back must get the number `ls` would print.
    let data = fixture("1000.doc.gz");
    let extraction = extract(
        SourceDocument::named("1000.doc.gz", &data),
        &Options::default(),
    )
    .unwrap();

    assert_eq!(extraction.file.name.as_deref(), Some("1000.doc.gz"));
    assert_eq!(extraction.file.size, Some(data.len() as u64));
    assert_eq!(extraction.format, Format::DOC);
}

#[test]
fn nesting_is_refused_beyond_one_envelope() {
    let doubly = gzip(&gzip(b"a document buried two envelopes deep"));
    let failure = extract(SourceDocument::from_bytes(&doubly), &Options::default()).unwrap_err();

    assert!(
        matches!(failure, Failure::NestedEnvelope),
        "expected NestedEnvelope, got {failure}"
    );
}

#[test]
fn a_decompression_bomb_is_a_failure_rather_than_an_exhausted_machine() {
    // Ten megabytes of zeroes compress to a few kilobytes; the cap is one
    // megabyte, so decompression is stopped a tenth of the way in.
    let bomb = gzip(&vec![0u8; 10 * 1024 * 1024]);
    assert!(
        bomb.len() < 64 * 1024,
        "the bomb must be small on disk to be a bomb"
    );

    let limit = 1024 * 1024;
    let failure = extract(
        SourceDocument::from_bytes(&bomb),
        &Options::default().with_max_size(limit),
    )
    .unwrap_err();

    assert!(
        matches!(failure, Failure::TooLarge { limit: reported } if reported == limit),
        "expected TooLarge, got {failure}"
    );
}

#[test]
fn the_cap_is_the_callers_to_move() {
    let payload = vec![b'x'; 4096];
    let compressed = gzip(&payload);

    let under = extract(
        SourceDocument::named("big.txt", &compressed),
        &Options::default().with_max_size(4096),
    );
    let over = extract(
        SourceDocument::named("big.txt", &compressed),
        &Options::default().with_max_size(4095),
    );

    assert_eq!(under.unwrap().markdown.len(), 4096);
    assert!(matches!(over.unwrap_err(), Failure::TooLarge { .. }));
}

#[test]
fn the_default_cap_is_500_mb() {
    assert_eq!(
        Options::default().max_size,
        all2markdown_core::DEFAULT_MAX_SIZE
    );
    assert_eq!(all2markdown_core::DEFAULT_MAX_SIZE, 500 * 1024 * 1024);
}

#[test]
fn a_corrupt_envelope_is_a_failure_and_not_a_panic() {
    let mut corrupt = gzip(b"a perfectly ordinary little document");
    let tail = corrupt.len() - 4;
    corrupt[tail..].fill(0xAB);

    let failure = extract(SourceDocument::from_bytes(&corrupt), &Options::default()).unwrap_err();
    assert!(
        matches!(failure, Failure::Decompress { .. }),
        "expected Decompress, got {failure}"
    );
}
