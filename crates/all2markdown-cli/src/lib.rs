use anyhow::{anyhow, Result};
use clap::Parser;
use std::path::PathBuf;

use all2markdown_core::{extract, format_from_id, Options, SourceDocument};

#[derive(Parser)]
#[command(
    name = "all2markdown",
    version,
    about = "Extract text from documents as Markdown"
)]
struct Cli {
    /// Input file path
    #[arg(short = 'i', long = "input")]
    input: PathBuf,

    /// Format: doc, docx, rtf, pdf (detected from the content if omitted)
    #[arg(short = 'f', long = "format")]
    format: Option<String>,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let data = std::fs::read(&cli.input)?;

    let options = match cli.format.as_deref() {
        Some(id) => {
            Options::forcing(format_from_id(id).ok_or_else(|| anyhow!("unsupported format: {id}"))?)
        }
        None => Options::default(),
    };

    let source = match cli.input.file_name().and_then(|n| n.to_str()) {
        Some(name) => SourceDocument::named(name, &data),
        None => SourceDocument::from_bytes(&data),
    };

    let extraction = extract(source, &options)?;

    // Warnings go to stderr so that piping the Markdown stays clean.
    for warning in &extraction.warnings {
        eprintln!("warning: {warning}");
    }

    print!("{}", extraction.markdown);
    Ok(())
}
