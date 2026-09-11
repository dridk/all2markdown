//! The command as a user runs it: flags, the output directory and its
//! template, what lands on stdout and what refuses to start.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
}

/// A directory holding `report.doc` and `report.pdf`: two documents that
/// differ only by extension, which is the collision the default naming must
/// make impossible.
fn corpus() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("report.doc"), fixture("1000.doc")).unwrap();
    std::fs::write(directory.path().join("report.pdf"), fixture("1000.pdf")).unwrap();
    directory
}

fn all2markdown(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_all2markdown"))
        .args(args)
        .env_remove("RUST_BACKTRACE")
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn files_under(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn a_directory_without_an_output_directory_is_refused() {
    let corpus = corpus();
    let output = all2markdown(&[corpus.path().to_str().unwrap()]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("-o"),
        "the refusal names the flag: {}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty());
    assert_eq!(
        files_under(corpus.path()).len(),
        2,
        "nothing was written among the sources"
    );
}

#[test]
fn one_markdown_file_per_document_named_after_the_source() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();

    let output = all2markdown(&[
        corpus.path().to_str().unwrap(),
        "-o",
        out.path().to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));

    let names: Vec<String> = files_under(out.path())
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["report.doc.md", "report.pdf.md"],
        "the extension stays in the name, so the two cannot overwrite one another"
    );

    let doc = std::fs::read_to_string(out.path().join("report.doc.md")).unwrap();
    assert!(doc.starts_with("---\n"), "front matter is on by default");
    assert!(doc.contains("format: \"doc\""));
    assert!(doc.contains("je mange du chocolat"));
    let pdf = std::fs::read_to_string(out.path().join("report.pdf.md")).unwrap();
    assert!(pdf.contains("format: \"pdf\""));
}

#[test]
fn the_output_is_the_same_from_one_run_to_the_next() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();
    let args = [
        corpus.path().to_str().unwrap(),
        "-o",
        out.path().to_str().unwrap(),
    ];

    assert!(all2markdown(&args).status.success());
    let first = files_under(out.path());
    assert!(all2markdown(&args).status.success());
    let second = files_under(out.path());

    assert_eq!(
        first, second,
        "no suffixes, no renames: the same names every time"
    );
}

#[test]
fn the_template_substitutes_every_variable() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();

    let output = all2markdown(&[
        corpus.path().to_str().unwrap(),
        "-o",
        out.path().to_str().unwrap(),
        "-t",
        "{format}/{stem}-{ext}-{name}.md",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));

    let mut relative: Vec<PathBuf> = files_under(out.path())
        .iter()
        .map(|p| p.strip_prefix(out.path()).unwrap().to_path_buf())
        .collect();
    relative.sort();
    assert_eq!(
        relative,
        vec![
            PathBuf::from("doc/report-doc-report.doc.md"),
            PathBuf::from("pdf/report-pdf-report.pdf.md"),
        ]
    );
}

#[test]
fn the_parent_variable_mirrors_the_source_directory() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();

    let output = all2markdown(&[
        corpus.path().to_str().unwrap(),
        "-o",
        out.path().to_str().unwrap(),
        "-t",
        "{parent}/{name}.md",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));

    // The corpus path is absolute; its root is dropped so the mirror lands
    // under the output directory rather than replacing it.
    let expected_parent = out
        .path()
        .join(corpus.path().strip_prefix("/").unwrap_or(corpus.path()));
    assert!(
        expected_parent.join("report.doc.md").is_file(),
        "expected {} under {}",
        expected_parent.display(),
        out.path().display()
    );
}

#[test]
fn an_unknown_template_variable_is_refused_before_anything_runs() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();

    let output = all2markdown(&[
        corpus.path().to_str().unwrap(),
        "-o",
        out.path().to_str().unwrap(),
        "-t",
        "{basename}.md",
    ]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("{basename}"),
        "the refusal names the variable: {}",
        stderr(&output)
    );
    assert!(
        files_under(out.path()).is_empty(),
        "a bad template is refused at startup, not mid-run"
    );
}

#[test]
fn the_output_directory_may_not_be_the_source_directory() {
    let corpus = corpus();
    let output = all2markdown(&[
        corpus.path().to_str().unwrap(),
        "-o",
        corpus.path().to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert_eq!(
        files_under(corpus.path()).len(),
        2,
        "nothing landed among the sources"
    );
}

#[test]
fn the_front_matter_can_be_switched_off() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();

    let output = all2markdown(&[
        corpus.path().to_str().unwrap(),
        "-o",
        out.path().to_str().unwrap(),
        "--no-front-matter",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));

    let doc = std::fs::read_to_string(out.path().join("report.doc.md")).unwrap();
    assert!(!doc.starts_with("---"), "{doc}");
    assert!(doc.contains("je mange du chocolat"));
}

#[test]
fn a_single_document_goes_to_stdout_with_its_front_matter() {
    let corpus = corpus();
    let document = corpus.path().join("report.doc");

    let output = all2markdown(&[document.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("---\n"));
    assert!(text.contains("name: \"report.doc\""));
    assert!(text.contains("je mange du chocolat"));

    let output = all2markdown(&[document.to_str().unwrap(), "--no-front-matter"]);
    assert!(!stdout(&output).starts_with("---"));
}

#[test]
fn a_single_document_can_be_written_to_the_output_directory_too() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();
    let document = corpus.path().join("report.pdf");

    let output = all2markdown(&[
        document.to_str().unwrap(),
        "-o",
        out.path().to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
    assert!(out.path().join("report.pdf.md").is_file());
}

#[test]
fn a_document_nothing_can_read_costs_no_file_and_stops_nothing() {
    let corpus = corpus();
    let broken: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    std::fs::write(corpus.path().join("broken.bin"), broken).unwrap();
    let out = tempfile::tempdir().unwrap();

    let output = all2markdown(&[
        corpus.path().to_str().unwrap(),
        "-o",
        out.path().to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(files_under(out.path()).len(), 2, "the two readable ones");
    assert!(
        stderr(&output).contains("broken.bin"),
        "the failure is reported: {}",
        stderr(&output)
    );
}

#[test]
fn jsonl_needs_no_output_directory_and_carries_the_raw_bag() {
    let corpus = corpus();
    let output = all2markdown(&[corpus.path().to_str().unwrap(), "--jsonl"]);
    assert!(output.status.success(), "{}", stderr(&output));

    let lines: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    let pdf = lines
        .iter()
        .find(|line| line["format"] == "pdf")
        .expect("the PDF's line");
    assert_eq!(pdf["document"]["raw"]["creator"], "Writer");
    assert!(pdf["file"]["name"] == "report.pdf");
}
