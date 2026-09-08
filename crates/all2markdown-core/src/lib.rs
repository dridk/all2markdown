//! Extracts the text of a Source Document and renders it as Markdown.
//!
//! See `CONTEXT.md` at the repo root for the vocabulary this crate speaks, and
//! `docs/adr/0001-text-first-extraction-contract.md` for what "extract" is
//! promised to mean.

mod detect;
mod doc;
mod docx;
mod extraction;
mod failure;
mod format;
mod metadata;
mod pdf;
mod rtf;
mod strategy;

pub use detect::detect_format;
pub use extraction::{Extraction, Options, SourceDocument, Warning};
pub use failure::Failure;
pub use format::{Confidence, Format};
pub use metadata::{DocumentMetadata, FileMetadata};

use strategy::FormatParser;

/// Extract the text of one Source Document.
///
/// Never panics on malformed input: a document that cannot be read yields a
/// Failure, so that one bad document in a Batch of a million interrupts
/// nothing.
pub fn extract(source: SourceDocument<'_>, options: &Options) -> Result<Extraction, Failure> {
    let format = match options.forced_format {
        Some(forced) => forced,
        None => detect_format(&source)?,
    };

    // Replaced by the Parser registry in milestone 1 step 2, which is what
    // makes a new format a one-file change.
    let parser: Box<dyn FormatParser> = match format {
        Format::DOC => Box::new(doc::DocParser),
        Format::DOCX => Box::new(docx::DocxParser),
        Format::RTF => Box::new(rtf::RtfParser),
        Format::PDF => Box::new(pdf::PdfParser),
        other => return Err(Failure::UnsupportedFormat(other.id().to_owned())),
    };

    let markdown = parser.to_markdown(source.bytes)?;

    let mut warnings = Vec::new();
    if !source.bytes.is_empty() && markdown.trim().is_empty() {
        warnings.push(Warning::EmptyOutput);
    }

    Ok(Extraction {
        markdown,
        format,
        encoding: None,
        file: source.file_metadata(),
        document: DocumentMetadata::default(),
        warnings,
    })
}
