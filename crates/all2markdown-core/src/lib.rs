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
mod template;

pub use batch::{BatchItem, Results, Source, Timeout};
pub use extraction::{
    Extracted, Extraction, Inventory, Options, SourceDocument, Warning, DEFAULT_MAX_SIZE,
};
pub use failure::Failure;
pub use format::{Confidence, Format};
pub use metadata::{DocumentMetadata, FileMetadata};
pub use parser::Parser;
pub use registry::Registry;
pub use render::{to_jsonl, to_markdown, FrontMatter, Record};
pub use template::{OutputTemplate, TemplateError};

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
    batch::guarded(|| default_registry().extract(source, options))
}

/// Read what one Source Document declares about itself, without parsing its
/// body.
///
/// The same detection as [`extract`], and none of the extraction: the body
/// is where the time goes, and every format keeps its metadata apart from it.
pub fn inventory(source: SourceDocument<'_>, options: &Options) -> Result<Inventory, Failure> {
    batch::guarded(|| default_registry().inventory(source, options))
}

/// Identify a Source Document by polling the built-in Parsers.
pub fn detect_format(source: &SourceDocument<'_>) -> Result<Format, Failure> {
    default_registry().detect(source)
}

/// Look a built-in Supported Format up by id, case-insensitively.
pub fn format_from_id(id: &str) -> Option<Format> {
    default_registry().format_from_id(id)
}

/// Extract one Source Document from wherever it is, exactly as a Batch
/// would: a path is read under the size cap and with its modification time,
/// bytes are taken as they are, and a Parser that panics costs a Failure
/// rather than the process. The result is labelled the way the caller
/// referred to the document, as it would be in a Batch.
pub fn extract_source(source: Source, options: &Options) -> BatchItem {
    batch::extract_source(default_registry(), source, options)
}

/// Inventory one Source Document from wherever it is, exactly as a Batch
/// would.
pub fn inventory_source(source: Source, options: &Options) -> BatchItem<Inventory> {
    batch::inventory_source(default_registry(), source, options)
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

/// Extract many Source Documents at once, wherever each comes from: the same
/// Batch as [`extract_paths`], over paths and bytes alike.
///
/// This is the road in for a caller who does not have a filesystem — one
/// draining an object store, say — and it is why the input is an iterator
/// rather than a collection: the documents can be fetched as the Batch asks
/// for them, and no more than the queue holds are ever in memory at once.
pub fn extract_sources<I>(sources: I, options: &Options, workers: Option<usize>) -> Results
where
    I: IntoIterator<Item = Source> + Send + 'static,
    I::IntoIter: Send,
{
    batch::extract_sources(Arc::clone(default_registry()), sources, options, workers)
}

/// Inventory many Source Documents at once, wherever each comes from: the
/// same Batch as [`extract_sources`], reading metadata alone.
pub fn inventory_sources<I>(
    sources: I,
    options: &Options,
    workers: Option<usize>,
) -> Results<Inventory>
where
    I: IntoIterator<Item = Source> + Send + 'static,
    I::IntoIter: Send,
{
    batch::inventory_sources(Arc::clone(default_registry()), sources, options, workers)
}

/// Inventory many Source Documents at once: the same Batch as
/// [`extract_paths`], the same iterator out, reading metadata alone.
///
/// An order of magnitude faster than extraction on the same corpus, which is
/// what makes it worth having: it turns "inventory three terabytes" from an
/// overnight job into a coffee break.
pub fn inventory_paths<I>(paths: I, options: &Options, workers: Option<usize>) -> Results<Inventory>
where
    I: IntoIterator<Item = PathBuf> + Send + 'static,
    I::IntoIter: Send,
{
    batch::inventory_paths(Arc::clone(default_registry()), paths, options, workers)
}
