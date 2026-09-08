//! Extracts the text of a Source Document and renders it as Markdown.
//!
//! See `CONTEXT.md` at the repo root for the vocabulary this crate speaks, and
//! `docs/adr/0001-text-first-extraction-contract.md` for what "extract" is
//! promised to mean.

mod encoding;
mod envelope;
mod extraction;
mod failure;
mod format;
mod metadata;
mod parser;
mod parsers;
mod registry;

pub use extraction::{Extracted, Extraction, Options, SourceDocument, Warning, DEFAULT_MAX_SIZE};
pub use failure::Failure;
pub use format::{Confidence, Format};
pub use metadata::{DocumentMetadata, FileMetadata};
pub use parser::Parser;
pub use registry::Registry;

use std::sync::OnceLock;

/// The registry the crate-level functions use: the Parsers this crate ships
/// with, and nothing else. A caller that registers its own Parser builds its
/// own [`Registry`] and calls it directly.
fn default_registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::with_builtin_parsers)
}

/// Extract the text of one Source Document.
///
/// Never panics on malformed input: a document that cannot be read yields a
/// Failure, so that one bad document in a Batch of a million interrupts
/// nothing.
pub fn extract(source: SourceDocument<'_>, options: &Options) -> Result<Extraction, Failure> {
    default_registry().extract(source, options)
}

/// Identify a Source Document by polling the built-in Parsers.
pub fn detect_format(source: &SourceDocument<'_>) -> Result<Format, Failure> {
    default_registry().detect(source)
}

/// Look a built-in Supported Format up by id, case-insensitively.
pub fn format_from_id(id: &str) -> Option<Format> {
    default_registry().format_from_id(id)
}
