//! The command as a user runs it: flags, the output directory and its
//! template, standard input, what lands on stdout, what refuses to start and
//! what the exit code says.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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
    all2markdown_in(Path::new("."), args, b"")
}

/// The command run in a directory, with bytes on its standard input.
fn all2markdown_in(cwd: &Path, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_all2markdown"))
        .args(args)
        .current_dir(cwd)
        .env_remove("RUST_BACKTRACE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    // Written from another thread: a document larger than the pipe would
    // otherwise deadlock against a child that is waiting to be read.
    let mut pipe = child.stdin.take().unwrap();
    let bytes = stdin.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(&bytes);
    });
    let output = child.wait_with_output().expect("the binary exits");
    writer.join().unwrap();
    output
}

/// The command with bytes on its standard input.
fn all2markdown_with_stdin(args: &[&str], stdin: &[u8]) -> Output {
    all2markdown_in(Path::new("."), args, stdin)
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().expect("exited rather than signalled")
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
    assert_eq!(
        exit_code(&output),
        3,
        "the batch ran to its end and one document failed: {}",
        stderr(&output)
    );
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

#[test]
fn metadata_only_inventories_a_directory_as_jsonl_without_an_output_directory() {
    let corpus = corpus();
    let output = all2markdown(&[corpus.path().to_str().unwrap(), "--metadata-only"]);
    assert!(output.status.success(), "{}", stderr(&output));

    let lines: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    for line in &lines {
        assert!(line["text"].is_null(), "no body was read: {line}");
        assert!(line["error"].is_null());
        assert!(line["file"]["size"].is_number());
    }
    let pdf = lines
        .iter()
        .find(|line| line["format"] == "pdf")
        .expect("the PDF's record");
    assert_eq!(pdf["document"]["page_count"], 1);
    assert_eq!(pdf["document"]["raw"]["creator"], "Writer");
    let doc = lines
        .iter()
        .find(|line| line["format"] == "doc")
        .expect("a format that declares nothing still has a record");
    assert!(doc["document"]["title"].is_null());
}

#[test]
fn metadata_only_refuses_markdown_output() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();

    for extra in [
        vec!["-o", out.path().to_str().unwrap()],
        vec!["--no-front-matter"],
        vec!["-t", "{name}.md"],
    ] {
        let mut args = vec![corpus.path().to_str().unwrap(), "--metadata-only"];
        args.extend(extra.iter().copied());
        let output = all2markdown(&args);
        assert!(!output.status.success(), "{extra:?} must be refused");
        assert!(stdout(&output).is_empty());
    }
    assert!(files_under(out.path()).is_empty());
}

#[test]
fn a_document_on_stdin_is_extracted_to_stdout() {
    let output = all2markdown_with_stdin(&["-"], &fixture("1000.doc"));
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));

    let text = stdout(&output);
    assert!(text.starts_with("---\n"), "front matter is on by default");
    assert!(text.contains("format: \"doc\""));
    assert!(text.contains("name: null"), "nothing named it: {text}");
    assert!(text.contains("modified: null"), "no filesystem said when");
    assert!(text.contains("je mange du chocolat"));
    assert!(
        stderr(&output).is_empty(),
        "nothing to warn about, and no progress off a terminal: {}",
        stderr(&output)
    );

    let output = all2markdown_with_stdin(&["-i", "-", "--no-front-matter"], &fixture("1000.doc"));
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    assert!(stdout(&output).contains("je mange du chocolat"));
}

#[test]
fn a_name_given_to_the_stdin_document_drives_detection_and_the_front_matter() {
    // Plain text carries no signature. Strictly, it is unrecognisable with
    // no name to go on, and recognised by its extension once it has one.
    let text = b"un texte sans signature\n";

    let output = all2markdown_with_stdin(&["-", "--strict"], text);
    assert_eq!(exit_code(&output), 1, "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
    assert!(
        stderr(&output).contains("unrecognized"),
        "{}",
        stderr(&output)
    );

    let output = all2markdown_with_stdin(&["-", "--strict", "--name", "notes.txt"], text);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let markdown = stdout(&output);
    assert!(markdown.contains("format: \"txt\""), "{markdown}");
    assert!(markdown.contains("name: \"notes.txt\""), "{markdown}");
    assert!(markdown.ends_with("un texte sans signature\n"));
}

#[test]
fn a_forced_format_applies_to_the_stdin_document() {
    // RTF has a signature; forcing txt wins over it, so the source comes out
    // verbatim, control words and all.
    let output = all2markdown_with_stdin(&["-", "-f", "txt"], &fixture("1000.rtf"));
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let markdown = stdout(&output);
    assert!(markdown.contains("format: \"txt\""), "{markdown}");
    assert!(markdown.contains("{\\rtf"), "{markdown}");
}

#[test]
fn the_stdin_document_can_be_a_jsonl_line_or_an_inventory() {
    let output = all2markdown_with_stdin(
        &["-", "--jsonl", "--name", "report.pdf"],
        &fixture("1000.pdf"),
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let line: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    assert_eq!(line["source"], "report.pdf");
    assert_eq!(line["format"], "pdf");
    assert!(line["text"]
        .as_str()
        .unwrap()
        .contains("je mange du chocolat"));

    let output = all2markdown_with_stdin(&["-", "--metadata-only"], &fixture("1000.pdf"));
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let line: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    assert_eq!(line["source"], "<stdin>");
    assert!(line["text"].is_null(), "no body was read: {line}");
    assert_eq!(line["document"]["page_count"], 1);
}

#[test]
fn a_document_on_stdin_is_never_guessed_at() {
    // Without `-`, standard input is not looked at: flags alone name no
    // target, and the run is refused rather than guessed at.
    let empty = tempfile::tempdir().unwrap();
    let output = all2markdown_in(empty.path(), &["--jsonl"], &fixture("1000.doc"));
    assert_eq!(exit_code(&output), 2, "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));

    // The two stdin modes cannot be asked for together.
    let output = all2markdown_with_stdin(&["-", "--paths-from", "-"], &fixture("1000.doc"));
    assert_eq!(exit_code(&output), 2, "{}", stderr(&output));

    // A name is for a document read from standard input and nothing else.
    let corpus = corpus();
    let document = corpus.path().join("report.doc");
    let output = all2markdown(&[document.to_str().unwrap(), "--name", "other.doc"]);
    assert_eq!(exit_code(&output), 2, "{}", stderr(&output));
    assert!(stderr(&output).contains("--name"), "{}", stderr(&output));

    // A document on standard input goes to standard output, nowhere else.
    let out = tempfile::tempdir().unwrap();
    let output = all2markdown_with_stdin(
        &["-", "-o", out.path().to_str().unwrap()],
        &fixture("1000.doc"),
    );
    assert_eq!(exit_code(&output), 2, "{}", stderr(&output));
    assert!(files_under(out.path()).is_empty());
}

#[test]
fn a_list_of_paths_on_stdin_is_extracted() {
    let corpus = corpus();
    let out = tempfile::tempdir().unwrap();
    // As `find` would print it, plus what a Windows editor or a stray key
    // could add: a carriage return, a blank line.
    let list = format!(
        "{}\r\n\n{}\n",
        corpus.path().join("report.doc").display(),
        corpus.path().join("report.pdf").display()
    );

    let output = all2markdown_with_stdin(
        &["--paths-from", "-", "-o", out.path().to_str().unwrap()],
        list.as_bytes(),
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let names: Vec<String> = files_under(out.path())
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["report.doc.md", "report.pdf.md"]);
}

#[test]
fn a_list_of_paths_can_come_from_a_file_and_stream_as_jsonl() {
    let corpus = corpus();
    let list = corpus.path().join("list.txt");
    std::fs::write(
        &list,
        format!("{}\n", corpus.path().join("report.pdf").display()),
    )
    .unwrap();

    let output = all2markdown(&["--paths-from", list.to_str().unwrap(), "--jsonl"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let lines: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "only what the list names, not the directory"
    );
    assert_eq!(lines[0]["format"], "pdf");
    assert!(lines[0]["source"].as_str().unwrap().ends_with("report.pdf"));
}

#[test]
fn a_list_of_paths_is_a_batch_and_needs_somewhere_to_write() {
    let corpus = corpus();
    let list = format!("{}\n", corpus.path().join("report.doc").display());
    let output = all2markdown_with_stdin(&["--paths-from", "-"], list.as_bytes());
    assert_eq!(exit_code(&output), 2, "{}", stderr(&output));
    assert!(stderr(&output).contains("-o"), "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
}

#[test]
fn the_exit_code_tells_a_failed_document_from_a_failed_batch() {
    let corpus = corpus();
    let broken: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    let broken_path = corpus.path().join("broken.bin");
    std::fs::write(&broken_path, &broken).unwrap();

    // The one document asked for could not be read: the command failed.
    let output = all2markdown(&[broken_path.to_str().unwrap()]);
    assert_eq!(exit_code(&output), 1, "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "no Markdown for a Failure");
    assert!(
        stderr(&output).contains("broken.bin"),
        "{}",
        stderr(&output)
    );

    let output = all2markdown_with_stdin(&["-"], &broken);
    assert_eq!(exit_code(&output), 1, "{}", stderr(&output));

    // A batch that ran to its end with one Failure in it, whichever way it
    // was read and wherever its results went.
    let output = all2markdown(&[corpus.path().to_str().unwrap(), "--jsonl"]);
    assert_eq!(exit_code(&output), 3, "{}", stderr(&output));
    assert_eq!(
        stdout(&output).lines().count(),
        3,
        "the Failure is in the stream, and nothing else stopped"
    );
    assert!(
        stderr(&output).is_empty(),
        "off a terminal, stderr stays quiet: {}",
        stderr(&output)
    );

    let list = format!(
        "{}\n{}\n",
        broken_path.display(),
        corpus.path().join("report.doc").display()
    );
    let output =
        all2markdown_with_stdin(&["--paths-from", "-", "--metadata-only"], list.as_bytes());
    assert_eq!(exit_code(&output), 3, "{}", stderr(&output));

    // Nothing failed: success, whatever the mode.
    let list = format!("{}\n", corpus.path().join("report.doc").display());
    let output = all2markdown_with_stdin(&["--paths-from", "-", "--jsonl"], list.as_bytes());
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
}

#[test]
fn warnings_go_to_stderr_and_the_markdown_stays_clean() {
    // An RTF document with no text in it: the extraction succeeds, empty,
    // and says so where a pipe will not hear it.
    let output = all2markdown_with_stdin(&["-", "--no-front-matter"], b"{\\rtf1\\ansi\\deff0 }");
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    assert!(stderr(&output).contains("warning:"), "{}", stderr(&output));
    assert!(stdout(&output).trim().is_empty(), "{}", stdout(&output));
}
