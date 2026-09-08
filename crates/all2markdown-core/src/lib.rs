//! Extracts the text of a Source Document and renders it as Markdown.
//!
//! See `CONTEXT.md` at the repo root for the vocabulary this crate speaks, and
//! `docs/adr/0001-text-first-extraction-contract.md` for what "extract" is
//! promised to mean.

mod batch;
mod date;
mod encoding;
mod envelope;
mod extraction;
mod failure;
mod format;
mod metadata;
mod parser;
mod parsers;
mod registry;
mod render;

pub use batch::{BatchItem, Results};
pub use extraction::{Extracted, Extraction, Options, SourceDocument, Warning, DEFAULT_MAX_SIZE};
pub use failure::Failure;
pub use format::{Confidence, Format};
pub use metadata::{DocumentMetadata, FileMetadata};
pub use parser::Parser;
pub use registry::Registry;
pub use render::to_jsonl;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// The registry the crate-level functions use: the Parsers this crate ships
/// with, and nothing else. A caller that registers its own Parser builds its
/// own [`Registry`] and calls it directly.
///
/// Behind an `Arc` because a Batch hands it to worker threads that outlive the
/// call that started them.
fn default_registry() -> &'static Arc<Registry> {
    static REGISTRY: OnceLock<Arc<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Arc::new(Registry::with_builtin_parsers()))
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

/// Extract many Source Documents at once, reading each from its path.
///
/// Saturates every core from one process, and returns an iterator that yields
/// results as they finish rather than a collection built at the end: a corpus
/// of a million documents does not fit in memory, and a caller who stops early
/// stops the work with it.
///
/// `workers` defaults to the core count. It is the number worth tuning, since
/// worker count multiplied by the size cap is the real memory bound.
pub fn extract_paths<I>(paths: I, options: &Options, workers: Option<usize>) -> Results
where
    I: IntoIterator<Item = PathBuf> + Send + 'static,
    I::IntoIter: Send,
{
    batch::extract_paths(Arc::clone(default_registry()), paths, options, workers)
}
