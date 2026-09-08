//! A Batch: every core from one process, results as they finish, and one
//! document's fate never anyone else's.

use all2markdown_core::{extract_paths, to_jsonl, BatchItem, Failure, Format, Options};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
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

/// The files directly in a directory, as a caller would hand them over.
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

fn run(directory: &Path, workers: Option<usize>) -> Vec<BatchItem> {
    extract_paths(paths_in(directory), &Options::default(), workers).collect()
}

#[test]
fn a_whole_directory_is_extracted_without_recursion() {
    let directory = corpus(&[]);
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    std::fs::write(
        directory.path().join("nested/deep.rtf"),
        fixture("1000.rtf"),
    )
    .unwrap();

    let results = run(directory.path(), None);

    assert_eq!(
        results.len(),
        4,
        "the four files, and nothing under the subdirectory"
    );
    assert!(
        !results.iter().any(|item| item.source.contains("deep.rtf")),
        "a directory is read one level deep, never walked"
    );
    for item in &results {
        let extraction = item
            .result
            .as_ref()
            .unwrap_or_else(|e| panic!("{}: {e}", item.source));
        assert!(extraction.markdown.contains("je mange du chocolat"));
    }
}

#[test]
fn every_result_reports_the_format_that_was_used() {
    let directory = corpus(&[]);
    let mut formats: Vec<Format> = run(directory.path(), None)
        .iter()
        .map(|item| item.result.as_ref().unwrap().format)
        .collect();
    formats.sort();

    assert_eq!(
        formats,
        vec![Format::DOC, Format::DOCX, Format::PDF, Format::RTF]
    );
}

#[test]
fn a_failure_is_reported_per_document_and_aborts_nothing() {
    // A file nothing can read, in the middle of four that extract fine.
    let broken: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    let directory = corpus(&[("broken.bin", &broken)]);

    let results = run(directory.path(), None);

    assert_eq!(
        results.len(),
        5,
        "the corrupt document must not stop the others"
    );
    let failures: Vec<&BatchItem> = results.iter().filter(|item| item.result.is_err()).collect();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].source.ends_with("broken.bin"));
    assert_eq!(results.iter().filter(|item| item.result.is_ok()).count(), 4);
}

#[test]
fn a_document_over_the_cap_fails_alone() {
    // Only `huge.txt` is over the line; `small.txt` sits just under it, and
    // must come through untouched.
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("huge.txt"), vec![b'x'; 8192]).unwrap();
    std::fs::write(directory.path().join("small.txt"), vec![b'y'; 1024]).unwrap();

    let results: Vec<BatchItem> = extract_paths(
        paths_in(directory.path()),
        &Options::default().with_max_size(4096),
        None,
    )
    .collect();

    let over = results
        .iter()
        .find(|item| item.source.ends_with("huge.txt"))
        .unwrap();
    assert!(matches!(over.result, Err(Failure::TooLarge { .. })));

    let under = results
        .iter()
        .find(|item| item.source.ends_with("small.txt"))
        .unwrap();
    assert_eq!(under.result.as_ref().unwrap().markdown.len(), 1024);
}

#[test]
fn results_stream_rather_than_being_collected_at_the_end() {
    // Proof that nothing waits for the whole Batch: a run that never finishes
    // still hands over its first result. Nine hundred copies is far more than
    // the queue holds, so a collecting implementation would deadlock or hang
    // here rather than answer.
    let directory = tempfile::tempdir().unwrap();
    let rtf = fixture("1000.rtf");
    for i in 0..900 {
        std::fs::write(directory.path().join(format!("{i}.rtf")), &rtf).unwrap();
    }

    let mut results = extract_paths(paths_in(directory.path()), &Options::default(), Some(2));
    let first = results
        .next()
        .expect("a first result before the last one is read");
    assert!(first.result.is_ok());

    // Dropping the iterator here is the "stop early" path: the workers find
    // the queue gone and give up rather than extracting the other 899.
    drop(results);
}

#[test]
fn throughput_grows_with_the_worker_count() {
    // PDFs, because they are the slowest of the four by an order of magnitude
    // and so put the extraction well above the cost of starting a pool —
    // otherwise this would measure thread creation rather than throughput.
    const DOCUMENTS: usize = 48;

    let directory = tempfile::tempdir().unwrap();
    let pdf = fixture("1000.pdf");
    for i in 0..DOCUMENTS {
        std::fs::write(directory.path().join(format!("{i}.pdf")), &pdf).unwrap();
    }
    let paths = paths_in(directory.path());

    let time = |workers: usize| {
        let started = Instant::now();
        let count = extract_paths(paths.clone(), &Options::default(), Some(workers)).count();
        assert_eq!(count, DOCUMENTS);
        started.elapsed()
    };

    time(2); // Warm the page cache, so the first run is not the slow one.
    let sequential = time(1);
    let parallel = time(4);

    assert!(
        parallel.as_secs_f64() < sequential.as_secs_f64() * 0.75,
        "four workers ({parallel:?}) must clearly beat one ({sequential:?})"
    );
}

#[test]
fn a_parser_that_panics_costs_one_document_rather_than_the_process() {
    // Not a hope but a caught constraint: a Parser must never panic, and when
    // one does anyway the Batch reports it as that document's Failure.
    let directory = tempfile::tempdir().unwrap();

    // `1000.pdf` truncated to its signature: pdf-extract is handed a document
    // that ends mid-header, which is exactly the malformed input a Parser is
    // required to survive.
    let mut truncated = fixture("1000.pdf");
    truncated.truncate(200);
    std::fs::write(directory.path().join("truncated.pdf"), &truncated).unwrap();
    std::fs::write(directory.path().join("sound.rtf"), fixture("1000.rtf")).unwrap();

    let results = run(directory.path(), Some(2));

    assert_eq!(
        results.len(),
        2,
        "the batch survived whatever the PDF parser did"
    );
    let sound = results
        .iter()
        .find(|item| item.source.ends_with("sound.rtf"))
        .unwrap();
    assert!(
        sound.result.is_ok(),
        "the healthy document extracted anyway"
    );
}

#[test]
fn jsonl_carries_the_text_the_format_and_the_warnings() {
    let directory = corpus(&[]);
    let results = run(directory.path(), None);
    let item = results
        .iter()
        .find(|item| item.source.ends_with("1000.docx"))
        .unwrap();

    let line: serde_json::Value = serde_json::from_str(&to_jsonl(item)).unwrap();

    assert_eq!(line["format"], "docx");
    assert!(line["text"]
        .as_str()
        .unwrap()
        .contains("je mange du chocolat"));
    assert!(line["warnings"].is_array());
    assert!(line["error"].is_null());
    assert!(line["source"].as_str().unwrap().ends_with("1000.docx"));
}

#[test]
fn jsonl_carries_a_failure_instead_of_text() {
    let broken: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    let directory = corpus(&[("broken.bin", &broken)]);
    let results = run(directory.path(), None);
    let item = results
        .iter()
        .find(|item| item.source.ends_with("broken.bin"))
        .unwrap();

    let line: serde_json::Value = serde_json::from_str(&to_jsonl(item)).unwrap();

    assert!(line["text"].is_null());
    assert!(line["format"].is_null());
    assert!(line["error"].as_str().is_some_and(|e| !e.is_empty()));
}

#[test]
fn every_jsonl_line_has_the_same_keys_whatever_happened() {
    // An output whose shape varies with its content is hostile to whatever
    // parses it downstream.
    let broken: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    let directory = corpus(&[("broken.bin", &broken)]);

    let mut shapes: Vec<Vec<String>> = run(directory.path(), None)
        .iter()
        .map(|item| {
            let line: serde_json::Value = serde_json::from_str(&to_jsonl(item)).unwrap();
            let mut keys: Vec<String> = line.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        })
        .collect();
    shapes.dedup();

    assert_eq!(
        shapes.len(),
        1,
        "every line must carry the same keys: {shapes:?}"
    );
}

#[test]
fn a_batch_that_finds_nothing_yields_nothing_rather_than_failing() {
    let directory = tempfile::tempdir().unwrap();
    let results = run(directory.path(), None);
    assert!(results.is_empty());
}

#[test]
fn the_worker_count_defaults_to_the_core_count() {
    // Not observable directly; what is observable is that omitting it works
    // and extracts everything.
    let directory = corpus(&[]);
    assert_eq!(run(directory.path(), None).len(), 4);
}

#[test]
fn a_slow_consumer_does_not_grow_the_queue_without_bound() {
    // The queue is bounded, so workers block rather than running ahead. What
    // this asserts is that they block without deadlocking: a consumer that
    // dawdles still receives everything.
    let directory = tempfile::tempdir().unwrap();
    let rtf = fixture("1000.rtf");
    for i in 0..20 {
        std::fs::write(directory.path().join(format!("{i}.rtf")), &rtf).unwrap();
    }

    let mut seen = 0;
    for item in extract_paths(paths_in(directory.path()), &Options::default(), Some(4)) {
        assert!(item.result.is_ok());
        seen += 1;
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(seen, 20);
}
