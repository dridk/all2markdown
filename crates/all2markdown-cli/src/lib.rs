use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use all2md_core::{parse, Format};

#[derive(Parser)]
#[command(name = "all2md", version, about = "Convert documents to Markdown")]
struct Cli {
    /// Input file path
    #[arg(short = 'i', long = "input")]
    input: PathBuf,

    /// Format: doc, docx, rtf, pdf (auto-detected if omitted)
    #[arg(short = 'f', long = "format")]
    format: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let data = std::fs::read(&cli.input)?;
    let format = cli
        .format
        .map(|f| Format::from_str_loose(&f))
        .transpose()?;
    let markdown = parse(&data, format)?;
    print!("{}", markdown);
    Ok(())
}
