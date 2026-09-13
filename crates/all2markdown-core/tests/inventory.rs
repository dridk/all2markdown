//! An Inventory: what every document declares about itself, without reading
//! any of them.

use all2markdown_core::{
    extract, extract_paths, inventory, inventory_paths, to_jsonl, BatchItem, Failure, Format,
    Inventory, Options, SourceDocument,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
}

fn inventory_fixture(name: &str) -> Inventory {
    let data = fixture(name);
    inventory(SourceDocument::named(name, &data), &Options::default())
        .unwrap_or_else(|e| panic!("inventorying {name} failed: {e}"))
}

/// A directory holding the four reference fixtures, plus whatever else the
/// test adds.
fn corpus(extra: &[(&str, &[u8])]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for name in ["1000.doc", "1000.docx", "1000.rtf", "1000.pdf"] {
        std::fs::write(directory.path().join(name), fixture(name)).unwrap();
    }
    for (name, bytes) in extra {
        std::fs::write(directory.path().join(name), bytes).unwrap();
    }
    directory
}

fn paths_in(directory: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(directory)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
}

#[test]
fn an_inventory_reports_what_an_extraction_would_without_the_text() {
    for name in ["1000.doc", "1000.docx", "1000.rtf", "1000.pdf"] {
        let data = fixture(name);
        let extraction = extract(SourceDocument::named(name, &data), &Options::default()).unwrap();
        let inventory = inventory_fixture(name);

        assert_eq!(inventory.format, extraction.format, "{name}");
        assert_eq!(inventory.file, extraction.file, "{name}");
        assert_eq!(inventory.document, extraction.document, "{name}");
    }
}

#[test]
fn the_document_metadata_comes_from_the_metadata_parts_alone() {
    let inventory = inventory_fixture("1000.docx");
    assert_eq!(inventory.document.language.as_deref(), Some("fr-FR"));
    assert_eq!(inventory.document.page_count, Some(1));
    assert_eq!(
        inventory.document.raw.get("Words").map(String::as_str),
        Some("108")
    );
}

#[test]
fn a_format_that_declares_nothing_still_yields_a_record() {
    // doc and rtf carry no metadata this crate can reach; an inventory of
    // them is a record with an empty Document Metadata, not an absence.
    for name in ["1000.doc", "1000.rtf"] {
        let inventory = inventory_fixture(name);
        assert!(inventory.document.is_empty(), "{name}");
        assert_eq!(inventory.file.name.as_deref(), Some(name));
        assert!(inventory.file.size.is_some_and(|size| size > 0));
    }
}

#[test]
fn an_envelope_is_stripped_before_the_inventory() {
    let inventory = inventory_fixture("1000.docx.zst");
    assert_eq!(inventory.format, Format::DOCX);
    assert_eq!(inventory.document.page_count, Some(1));
    // File Metadata is what is on disk: the compressed name and size.
    assert_eq!(inventory.file.name.as_deref(), Some("1000.docx.zst"));
    assert_eq!(
        inventory.file.size,
        Some(fixture("1000.docx.zst").len() as u64)
    );
}

#[test]
fn a_forced_format_is_honoured() {
    let data = fixture("1000.docx");
    let inventory = inventory(
        SourceDocument::named("1000.docx", &data),
        &Options::forcing(Format::TXT),
    )
    .unwrap();
    assert_eq!(inventory.format, Format::TXT);
    assert!(inventory.document.is_empty());
}

#[test]
fn a_document_that_cannot_be_opened_fails_alone_and_the_run_continues() {
    let broken: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    let directory = corpus(&[("broken.bin", &broken)]);
    let mut paths = paths_in(directory.path());
    paths.push(directory.path().join("missing.docx"));

    let results: Vec<BatchItem<Inventory>> =
        inventory_paths(paths, &Options::default(), None).collect();

    assert_eq!(results.len(), 6, "every path answers, whatever happened");
    assert_eq!(results.iter().filter(|item| item.result.is_ok()).count(), 4);
    let missing = results
        .iter()
        .find(|item| item.source.ends_with("missing.docx"))
        .unwrap();
    assert!(matches!(missing.result, Err(Failure::Io(_))));
    let broken = results
        .iter()
        .find(|item| item.source.ends_with("broken.bin"))
        .unwrap();
    assert!(matches!(broken.result, Err(Failure::UnrecognizedFormat)));
}

#[test]
fn an_inventory_renders_to_the_same_jsonl_line_as_an_extraction_minus_the_text() {
    let directory = corpus(&[]);
    let paths = paths_in(directory.path());

    let extracted: Vec<serde_json::Value> = extract_paths(paths.clone(), &Options::default(), None)
        .map(|item| serde_json::from_str(&to_jsonl(&item)).unwrap())
        .collect();
    let inventoried: Vec<serde_json::Value> = inventory_paths(paths, &Options::default(), None)
        .map(|item| serde_json::from_str(&to_jsonl(&item)).unwrap())
        .collect();

    for inventory in &inventoried {
        let extraction = extracted
            .iter()
            .find(|line| line["source"] == inventory["source"])
            .unwrap();
        let keys = |line: &serde_json::Value| -> Vec<String> {
            line.as_object().unwrap().keys().cloned().collect()
        };
        assert_eq!(keys(inventory), keys(extraction), "one schema for both");
        assert_eq!(inventory["format"], extraction["format"]);
        assert_eq!(inventory["file"], extraction["file"]);
        assert_eq!(inventory["document"], extraction["document"]);
        assert!(inventory["text"].is_null(), "no body was read");
        assert!(inventory["error"].is_null());
    }
}

#[test]
fn an_inventory_is_an_order_of_magnitude_faster_than_an_extraction() {
    // PDFs, the slowest of the four to extract: reading the Info dictionary
    // costs a parse of the object table, and nothing of the content streams
    // and fonts that make extraction slow. The margin must be wide enough to
    // survive a busy CI runner and narrow enough to mean something.
    const DOCUMENTS: usize = 48;

    let directory = tempfile::tempdir().unwrap();
    let pdf = fixture("1000.pdf");
    for i in 0..DOCUMENTS {
        std::fs::write(directory.path().join(format!("{i}.pdf")), &pdf).unwrap();
    }
    let paths = paths_in(directory.path());

    // Warm the page cache and the allocator, so neither run pays for it.
    let _ = inventory_paths(paths.clone(), &Options::default(), Some(2)).count();
    let _ = extract_paths(paths.clone(), &Options::default(), Some(2)).count();

    let started = Instant::now();
    assert_eq!(
        extract_paths(paths.clone(), &Options::default(), Some(2)).count(),
        DOCUMENTS
    );
    let extraction = started.elapsed();

    let started = Instant::now();
    assert_eq!(
        inventory_paths(paths, &Options::default(), Some(2)).count(),
        DOCUMENTS
    );
    let inventory = started.elapsed();

    // Reported, not just asserted: the number is the point of the mode.
    eprintln!(
        "{DOCUMENTS} PDFs: extraction {extraction:?}, inventory {inventory:?}, ratio {:.1}",
        extraction.as_secs_f64() / inventory.as_secs_f64()
    );
    assert!(
        inventory.as_secs_f64() * 10.0 < extraction.as_secs_f64(),
        "an inventory ({inventory:?}) must be ten times faster than an extraction ({extraction:?})"
    );
}
