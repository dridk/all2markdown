use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use std::io::{BufWriter, ErrorKind, Write};
use std::path::{Path, PathBuf};

use all2markdown_core::{
    extract_paths, format_from_id, to_jsonl, to_markdown, BatchItem, FrontMatter, Options,
    OutputTemplate, Results,
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

    /// Directory to write one Markdown file per document into. Required for
    /// a directory unless --jsonl is given; a single document goes to stdout
    /// without it. Never the directory the sources are read from.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Name of the produced file, relative to the output directory. Variables:
    /// {name} (report.doc), {stem} (report), {ext} (doc), {parent} (the
    /// source's directory, made relative), {format} (the format it was read
    /// as). The default keeps the source extension, so report.doc and
    /// report.pdf cannot overwrite one another.
    #[arg(
        short = 't',
        long = "template",
        default_value = OutputTemplate::DEFAULT,
        value_parser = OutputTemplate::parse
    )]
    template: OutputTemplate,

    /// Format: doc, docx, rtf, pdf, txt (detected from the content if omitted)
    #[arg(short = 'f', long = "format")]
    format: Option<String>,

    /// Worker threads. Defaults to the core count.
    #[arg(short = 'j', long = "workers")]
    workers: Option<usize>,

    /// One JSON object per document on stdout, rather than Markdown files.
    /// Carries what the front matter carries, plus the raw metadata bag.
    #[arg(long, conflicts_with = "output")]
    jsonl: bool,

    /// Leave the YAML header off the Markdown, for consumers that do not
    /// understand front matter.
    #[arg(long = "no-front-matter")]
    no_front_matter: bool,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    let target = cli.input.or(cli.path).unwrap_or_else(|| PathBuf::from("."));
    let is_directory = target.is_dir();

    let options = match cli.format.as_deref() {
        Some(id) => {
            Options::forcing(format_from_id(id).ok_or_else(|| anyhow!("unsupported format: {id}"))?)
        }
        None => Options::default(),
    };
    let front_matter = if cli.no_front_matter {
        FrontMatter::Off
    } else {
        FrontMatter::On
    };

    // Everything that must be refused is refused here, before a document is
    // opened: a Batch of a million must not fail on its last document for a
    // mistake visible on its first.
    let output = match cli.output {
        Some(output) if is_directory => Some(prepare_output(&output, &target)?),
        Some(output) => Some(prepare_output(
            &output,
            target.parent().unwrap_or(Path::new(".")),
        )?),
        None if is_directory && !cli.jsonl => bail!(
            "an output directory is required: -o <DIR> to write one Markdown file per \
             document, or --jsonl for one JSON line per document on stdout"
        ),
        None => None,
    };

    // One document or a whole directory, the same Batch: the file metadata,
    // the size cap and the panic guard come with it, so the two cannot differ.
    let entries: Vec<PathBuf> = if is_directory {
        files_in(&target)?
    } else {
        vec![target]
    };
    let results = extract_paths(entries, &options, cli.workers);

    if cli.jsonl {
        return stream_jsonl(results);
    }
    match output {
        Some(output) => write_files(results, &output, &cli.template, front_matter),
        None => print_one(results, front_matter),
    }
}

/// The files directly in a directory, in the order the filesystem lists them.
fn files_in(directory: &Path) -> Result<Vec<PathBuf>> {
    Ok(std::fs::read_dir(directory)
        .with_context(|| format!("cannot read {}", directory.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .collect())
}

/// Create the output directory if need be, and refuse one that is the
/// directory the sources sit in: Markdown must never land among them, where
/// the next run would read it back as a document.
fn prepare_output(output: &Path, sources: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output)
        .with_context(|| format!("cannot create output directory {}", output.display()))?;
    let output_canonical = output.canonicalize()?;
    if let Ok(sources_canonical) = sources.canonicalize() {
        if output_canonical == sources_canonical {
            bail!(
                "the output directory {} is where the sources are read from; \
                 Markdown must not be written among them",
                output.display()
            );
        }
    }
    Ok(output_canonical)
}

/// One Markdown file per document under the output directory, named by the
/// template.
///
/// A Failure is reported on stderr and produces no file: the absence of a
/// file is the honest output for a document nothing could read, and stderr
/// is the only channel left when stdout carries nothing.
fn write_files(
    results: Results,
    output: &Path,
    template: &OutputTemplate,
    front_matter: FrontMatter,
) -> Result<()> {
    for item in results {
        let extraction = match &item.result {
            Ok(extraction) => extraction,
            Err(failure) => {
                eprintln!("error: {}: {failure}", item.source);
                continue;
            }
        };
        let destination = output.join(template.render(Path::new(&item.source), extraction.format));
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        std::fs::write(&destination, to_markdown(extraction, front_matter))
            .with_context(|| format!("cannot write {}", destination.display()))?;
        for warning in &extraction.warnings {
            eprintln!("warning: {}: {warning}", item.source);
        }
    }
    Ok(())
}

/// One JSON line per document on stdout.
///
/// Results are written as they arrive rather than collected: memory stays
/// bounded, and a reader downstream sees the first document before the last
/// one has been opened.
fn stream_jsonl(results: Results) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    for item in results {
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

/// A single document's Markdown on stdout. Its Failure, if any, is the
/// command's: there is no other document for it to be local to.
fn print_one(results: Results, front_matter: FrontMatter) -> Result<()> {
    let Some(BatchItem { source, result }) = results.into_iter().next() else {
        bail!("nothing to extract");
    };
    let extraction = result.with_context(|| source.clone())?;
    for warning in &extraction.warnings {
        eprintln!("warning: {warning}");
    }
    print!("{}", to_markdown(&extraction, front_matter));
    Ok(())
}
