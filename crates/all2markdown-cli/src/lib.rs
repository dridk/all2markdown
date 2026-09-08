use anyhow::{anyhow, Result};
use clap::Parser;
use std::io::{BufWriter, ErrorKind, Write};
use std::path::{Path, PathBuf};

use all2markdown_core::{
    extract, extract_paths, format_from_id, to_jsonl, BatchItem, Options, SourceDocument,
};

#[derive(Parser)]
#[command(
    name = "all2markdown",
    version,
    about = "Extract text from documents as Markdown"
)]
struct Cli {
    /// A document, or a directory whose files are all extracted. Defaults to
    /// the current directory, without recursion.
    path: Option<PathBuf>,

    /// The same target, named. Kept for callers that already write `-i`.
    #[arg(short = 'i', long = "input", conflicts_with = "path")]
    input: Option<PathBuf>,

    /// Format: doc, docx, rtf, pdf, txt (detected from the content if omitted)
    #[arg(short = 'f', long = "format")]
    format: Option<String>,

    /// Worker threads. Defaults to the core count.
    #[arg(short = 'j', long = "workers")]
    workers: Option<usize>,

    /// One JSON object per document, rather than Markdown. Always on for a
    /// directory, where Markdown alone could not say which file it came from.
    #[arg(long)]
    jsonl: bool,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    let target = cli.input.or(cli.path).unwrap_or_else(|| PathBuf::from("."));

    let options = match cli.format.as_deref() {
        Some(id) => {
            Options::forcing(format_from_id(id).ok_or_else(|| anyhow!("unsupported format: {id}"))?)
        }
        None => Options::default(),
    };

    if target.is_dir() {
        return extract_directory(&target, &options, cli.workers);
    }
    extract_one(&target, &options, cli.jsonl)
}

/// Extract every file in a directory, in parallel, as JSONL on stdout.
///
/// Results are written as they arrive rather than collected: memory stays
/// bounded, and a reader downstream sees the first document before the last
/// one has been opened.
fn extract_directory(directory: &Path, options: &Options, workers: Option<usize>) -> Result<()> {
    let entries = std::fs::read_dir(directory)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path());

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    for item in extract_paths(entries, options, workers) {
        if let Err(e) = writeln!(out, "{}", to_jsonl(&item)) {
            // A closed pipe is how `head` says it has seen enough. Returning
            // here drops the results, which stops the workers.
            if e.kind() == ErrorKind::BrokenPipe {
                return Ok(());
            }
            return Err(e.into());
        }
    }
    out.flush()?;
    Ok(())
}

fn extract_one(path: &Path, options: &Options, jsonl: bool) -> Result<()> {
    let data = std::fs::read(path)?;
    let source = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => SourceDocument::named(name, &data),
        None => SourceDocument::from_bytes(&data),
    };

    if jsonl {
        let item = BatchItem {
            source: path.display().to_string(),
            result: extract(source, options),
        };
        println!("{}", to_jsonl(&item));
        return Ok(());
    }

    let extraction = extract(source, options)?;
    for warning in &extraction.warnings {
        eprintln!("warning: {warning}");
    }
    print!("{}", extraction.markdown);
    Ok(())
}
