use anyhow::{bail, Context, Result};
use clap::error::ErrorKind as Usage;
use clap::{CommandFactory, Parser};
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, ErrorKind, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use all2markdown_core::{
    extract, extract_paths, format_from_id, inventory, inventory_paths, to_jsonl, to_markdown,
    BatchItem, Failure, FrontMatter, Options, OutputTemplate, Record, SourceDocument,
};

const EXIT_CODES: &str = "\
Exit codes:
  0  every document was extracted
  1  the command failed, or the one document asked for could not be read
  2  the arguments were refused
  3  the batch ran to its end, and some of its documents failed";

#[derive(Parser)]
#[command(
    name = "all2markdown",
    version,
    about = "Extract text from documents as Markdown",
    after_help = EXIT_CODES,
    arg_required_else_help = true
)]
struct Cli {
    /// A document, a directory whose files are all extracted without
    /// recursion, or `-` for a document read from standard input.
    path: Option<PathBuf>,

    /// The same target, named. Kept for callers that already write `-i`.
    #[arg(short = 'i', long = "input", conflicts_with = "path")]
    input: Option<PathBuf>,

    /// Read the documents to extract from FILE, one path per line; `-` reads
    /// the list from standard input. This is how `find` and `fd` compose with
    /// the command: recursion is theirs to decide, not a flag here.
    #[arg(long = "paths-from", value_name = "FILE", conflicts_with_all = ["path", "input"])]
    paths_from: Option<PathBuf>,

    /// Name the document read from standard input, as if it had been read
    /// from a file called NAME: detection uses the extension for the formats
    /// that carry no signature, and the front matter carries the name.
    #[arg(long, value_name = "NAME")]
    name: Option<String>,

    /// Directory to write one Markdown file per document into. Required for
    /// a directory or a list of paths unless --jsonl is given; a single
    /// document goes to stdout without it. Never the directory the sources
    /// are read from.
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

    /// Refuse a document nothing identifies, rather than reading it as plain
    /// text because it decodes as some.
    #[arg(long)]
    strict: bool,

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

    /// Inventory: read what each document declares about itself, without
    /// parsing its body. An order of magnitude faster than extraction. JSONL
    /// on stdout, always; there is no Markdown to write.
    #[arg(long = "metadata-only", conflicts_with_all = ["output", "template", "no_front_matter"])]
    metadata_only: bool,
}

/// Where the Source Documents come from. Told apart by the arguments alone,
/// never by looking at standard input: a document arriving there is
/// extracted only when `-` asks for it, and a list only under `--paths-from`.
enum Input {
    /// One document on disk, whose Markdown goes to stdout.
    Document(PathBuf),
    /// Every file directly in a directory.
    Directory(PathBuf),
    /// One document on standard input, under the name the caller gave it.
    Stdin { name: Option<String> },
    /// The paths listed in a file, one per line; `-` for standard input.
    List(PathBuf),
}

impl Input {
    /// Whether exactly one document was asked for, so that its Failure is the
    /// command's rather than one of a Batch.
    fn is_single(&self) -> bool {
        matches!(self, Input::Document(_) | Input::Stdin { .. })
    }
}

/// How a run ended, as its exit code tells it. A refused argument is the
/// fourth code, and it is clap's.
enum Outcome {
    /// Every document was extracted.
    Complete,
    /// The one document asked for could not be read.
    Failed,
    /// The Batch ran to its end, and some of its documents failed. Each
    /// Failure was reported where the results went; the code is what a
    /// script sees.
    Partial,
}

impl From<Outcome> for ExitCode {
    fn from(outcome: Outcome) -> ExitCode {
        match outcome {
            Outcome::Complete => ExitCode::SUCCESS,
            Outcome::Failed => ExitCode::from(1),
            Outcome::Partial => ExitCode::from(3),
        }
    }
}

/// Refuse the arguments as clap would: the message in its style, and its
/// exit code, so that a mistake this command catches and one clap catches
/// look the same to a script.
fn refuse(kind: Usage, message: impl std::fmt::Display) -> ! {
    Cli::command().error(kind, message).exit()
}

pub fn run() -> ExitCode {
    match try_run() {
        Ok(outcome) => outcome.into(),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn try_run() -> Result<Outcome> {
    let cli = Cli::parse();

    let input = match (cli.paths_from, cli.input.or(cli.path), cli.name) {
        (Some(list), _, None) => Input::List(list),
        (None, Some(target), name) if target == Path::new("-") => Input::Stdin { name },
        (_, _, Some(_)) => refuse(
            Usage::ArgumentConflict,
            "--name names a document read from standard input; pass `-` as the document",
        ),
        // Flags alone name no target. `arg_required_else_help` shows the help
        // for a bare run; this is the run that carries flags and nothing to
        // read, which guessing at the current directory would turn into a
        // dump of it.
        (None, None, None) => refuse(
            Usage::MissingRequiredArgument,
            "no document was named: give a document, a directory, `-` for a document \
             read from standard input, or --paths-from for a list of paths",
        ),
        (None, Some(target), None) => {
            if target.is_dir() {
                Input::Directory(target)
            } else {
                Input::Document(target)
            }
        }
    };

    let mut options = match cli.format.as_deref() {
        Some(id) => {
            let format = format_from_id(id).unwrap_or_else(|| {
                refuse(Usage::InvalidValue, format!("unsupported format: {id}"))
            });
            Options::forcing(format)
        }
        None => Options::default(),
    };
    if cli.strict {
        options = options.strict();
    }
    let front_matter = if cli.no_front_matter {
        FrontMatter::Off
    } else {
        FrontMatter::On
    };

    // Everything that must be refused is refused here, before a document is
    // opened: a Batch of a million must not fail on its last document for a
    // mistake visible on its first.
    let output = match (cli.output, &input) {
        (Some(output), Input::Directory(target)) => Some(prepare_output(&output, target)?),
        (Some(output), Input::Document(target)) => Some(prepare_output(
            &output,
            target.parent().unwrap_or(Path::new(".")),
        )?),
        // A list may name files from anywhere, so there is no one source
        // directory to keep the output out of.
        (Some(output), Input::List(_)) => Some(create_output(&output)?),
        (Some(_), Input::Stdin { .. }) => refuse(
            Usage::ArgumentConflict,
            "a document read from standard input goes to standard output; redirect it",
        ),
        (None, _) if !input.is_single() && !cli.jsonl && !cli.metadata_only => refuse(
            Usage::MissingRequiredArgument,
            "an output directory is required: -o <DIR> to write one Markdown file per \
             document, or --jsonl for one JSON line per document on stdout",
        ),
        (None, _) => None,
    };

    // The document on standard input is the one thing that cannot go through
    // the Batch, which reads from paths; it takes the same size cap, checked
    // before the bytes are held rather than after.
    let input = match input {
        Input::Stdin { name } => {
            let single = true;
            if cli.metadata_only {
                let failed = stream_jsonl([read_stdin(name, &options, inventory)?])?;
                return Ok(outcome(failed, single));
            }
            let item = read_stdin(name, &options, extract)?;
            if cli.jsonl {
                let failed = stream_jsonl([item])?;
                return Ok(outcome(failed, single));
            }
            return print_one([item], front_matter);
        }
        other => other,
    };

    // One document, a whole directory or a list of paths, the same Batch:
    // the file metadata, the size cap and the panic guard come with it, so
    // the three cannot differ.
    let single = input.is_single();
    let entries: Box<dyn Iterator<Item = PathBuf> + Send> = match input {
        Input::Document(target) => Box::new(std::iter::once(target)),
        Input::Directory(target) => Box::new(files_in(&target)?.into_iter()),
        Input::List(list) => Box::new(paths_listed_in(&list)?),
        Input::Stdin { .. } => unreachable!("handled above"),
    };
    if cli.metadata_only {
        let failed = stream_jsonl(inventory_paths(entries, &options, cli.workers))?;
        return Ok(outcome(failed, single));
    }
    let results = extract_paths(entries, &options, cli.workers);

    if cli.jsonl {
        let failed = stream_jsonl(results)?;
        return Ok(outcome(failed, single));
    }
    match output {
        Some(output) => {
            let failed = write_files(results, &output, &cli.template, front_matter)?;
            Ok(outcome(failed, single))
        }
        None => print_one(results, front_matter),
    }
}

fn outcome(failed: usize, single: bool) -> Outcome {
    match (failed, single) {
        (0, _) => Outcome::Complete,
        (_, true) => Outcome::Failed,
        (_, false) => Outcome::Partial,
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

/// The paths listed in a file, one per line, yielded as they are read rather
/// than collected: a list piped from `find` is consumed as `find` produces
/// it, so extraction starts before the walk is over and a million lines are
/// never held at once. Blank lines are skipped, and a trailing carriage
/// return is not part of a path.
fn paths_listed_in(list: &Path) -> Result<impl Iterator<Item = PathBuf> + Send + 'static> {
    let reader: Box<dyn BufRead + Send> = if list == Path::new("-") {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(list).with_context(|| {
            format!("cannot read the path list {}", list.display())
        })?))
    };
    Ok(reader
        .lines()
        .map_while(|line| match line {
            Ok(line) => Some(line),
            Err(e) => {
                eprintln!("error: the path list stopped being readable: {e}");
                None
            }
        })
        .filter_map(|line| {
            let path = line.trim_end_matches('\r');
            (!path.is_empty()).then(|| PathBuf::from(path))
        }))
}

/// The document on standard input, read whole under the size cap, then given
/// to the job: extraction or inventory, whichever the command asked for.
///
/// The cap is applied on the way in: one byte past it is enough to refuse,
/// so a stream with no end costs the cap and not the machine.
fn read_stdin<T>(
    name: Option<String>,
    options: &Options,
    job: fn(SourceDocument<'_>, &Options) -> Result<T, Failure>,
) -> Result<BatchItem<T>> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(options.max_size + 1)
        .read_to_end(&mut bytes)
        .context("cannot read standard input")?;

    let result = if bytes.len() as u64 > options.max_size {
        Err(Failure::TooLarge {
            limit: options.max_size,
        })
    } else {
        let source = match name.as_deref() {
            Some(name) => SourceDocument::named(name, &bytes),
            None => SourceDocument::from_bytes(&bytes),
        };
        job(source, options)
    };
    Ok(BatchItem {
        source: name.unwrap_or_else(|| "<stdin>".to_owned()),
        result,
    })
}

/// Create the output directory if need be.
fn create_output(output: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output)
        .with_context(|| format!("cannot create output directory {}", output.display()))?;
    Ok(output.canonicalize()?)
}

/// Create the output directory if need be, and refuse one that is the
/// directory the sources sit in: Markdown must never land among them, where
/// the next run would read it back as a document.
fn prepare_output(output: &Path, sources: &Path) -> Result<PathBuf> {
    let output_canonical = create_output(output)?;
    if let Ok(sources_canonical) = sources.canonicalize() {
        if output_canonical == sources_canonical {
            refuse(
                Usage::ValueValidation,
                format!(
                    "the output directory {} is where the sources are read from; \
                     Markdown must not be written among them",
                    output.display()
                ),
            );
        }
    }
    Ok(output_canonical)
}

/// One Markdown file per document under the output directory, named by the
/// template. Returns how many documents failed.
///
/// A Failure is reported on stderr and produces no file: the absence of a
/// file is the honest output for a document nothing could read, and stderr
/// is the only channel left when stdout carries nothing.
fn write_files(
    results: impl IntoIterator<Item = BatchItem>,
    output: &Path,
    template: &OutputTemplate,
    front_matter: FrontMatter,
) -> Result<usize> {
    let mut progress = Progress::new();
    for item in results {
        let extraction = match &item.result {
            Ok(extraction) => extraction,
            Err(failure) => {
                progress.report(format_args!("error: {}: {failure}", item.source));
                progress.tick(false);
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
            progress.report(format_args!("warning: {}: {warning}", item.source));
        }
        progress.tick(true);
    }
    Ok(progress.finish())
}

/// One JSON line per document on stdout. Returns how many documents failed.
///
/// Results are written as they arrive rather than collected: memory stays
/// bounded, and a reader downstream sees the first document before the last
/// one has been opened.
fn stream_jsonl<T: Record>(results: impl IntoIterator<Item = BatchItem<T>>) -> Result<usize> {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut progress = Progress::new();

    for item in results {
        if let Err(e) = writeln!(out, "{}", to_jsonl(&item)) {
            // A closed pipe is how `head` says it has seen enough. Returning
            // here drops the results, which stops the workers.
            if e.kind() == ErrorKind::BrokenPipe {
                return Ok(progress.finish());
            }
            return Err(e.into());
        }
        progress.tick(item.result.is_ok());
    }
    out.flush()?;
    Ok(progress.finish())
}

/// A single document's Markdown on stdout. Its Failure, if any, is the
/// command's: there is no other document for it to be local to. Its
/// Warnings go to stderr, so the Markdown a pipe receives is nothing but
/// Markdown.
fn print_one(
    results: impl IntoIterator<Item = BatchItem>,
    front_matter: FrontMatter,
) -> Result<Outcome> {
    let Some(BatchItem { source, result }) = results.into_iter().next() else {
        bail!("nothing to extract");
    };
    let extraction = result.with_context(|| source.clone())?;
    for warning in &extraction.warnings {
        eprintln!("warning: {warning}");
    }
    print!("{}", to_markdown(&extraction, front_matter));
    Ok(Outcome::Complete)
}

/// How far a Batch has come, on stderr, and only when a person is watching:
/// a pipe or a log gets the errors and the warnings and nothing else.
///
/// One line, redrawn in place and at most a few times a second, so that a
/// million documents cost a few thousand writes rather than a million.
struct Progress {
    terminal: bool,
    done: usize,
    failed: usize,
    drawn_at: Option<Instant>,
}

impl Progress {
    const REDRAW_EVERY: Duration = Duration::from_millis(100);

    fn new() -> Self {
        Self {
            terminal: io::stderr().is_terminal(),
            done: 0,
            failed: 0,
            drawn_at: None,
        }
    }

    /// Count one more document, and redraw if it has been a while.
    fn tick(&mut self, succeeded: bool) {
        self.done += 1;
        if !succeeded {
            self.failed += 1;
        }
        if self.terminal
            && self
                .drawn_at
                .is_none_or(|at| at.elapsed() >= Self::REDRAW_EVERY)
        {
            self.draw();
        }
    }

    /// Print one line for a document, above the progress rather than through
    /// it: the line in progress is cleared first and redrawn after.
    fn report(&mut self, message: std::fmt::Arguments<'_>) {
        if self.terminal {
            eprint!("\r\x1b[2K");
        }
        eprintln!("{message}");
        if self.terminal {
            self.draw();
        }
    }

    /// The final count, and the line ended. Returns how many failed.
    fn finish(mut self) -> usize {
        if self.terminal {
            self.draw();
            eprintln!();
        }
        self.failed
    }

    fn draw(&mut self) {
        eprint!("\r{} documents, {} failed", self.done, self.failed);
        self.drawn_at = Some(Instant::now());
    }
}
