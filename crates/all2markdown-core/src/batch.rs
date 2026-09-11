use crate::extraction::{Extraction, Inventory, Options, SourceDocument};
use crate::failure::Failure;
use crate::registry::Registry;
use rayon::prelude::*;
use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, UNIX_EPOCH};

/// How many finished results may wait ahead of the consumer, per worker.
///
/// The queue is bounded because it is the only thing between a fast extractor
/// and an unbounded pile of Markdown in memory: workers block on a full queue
/// rather than running ahead of whoever is reading.
const QUEUE_PER_WORKER: usize = 2;

/// Where a Batch finds one Source Document.
///
/// A path is read by the worker that extracts it, so that a million paths
/// cost a million strings and never a million documents in memory. Bytes are
/// for the caller who fetched the document from somewhere that is not a
/// filesystem — an object store, a socket — and holds it already: the Batch
/// takes them as they are and never touches the disk for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Path(PathBuf),
    Bytes {
        /// The name the caller knows the document by. Detection uses its
        /// extension for the formats that carry no signature of their own.
        name: Option<String>,
        bytes: Vec<u8>,
    },
}

impl From<PathBuf> for Source {
    fn from(path: PathBuf) -> Self {
        Source::Path(path)
    }
}

impl Source {
    /// Bytes the caller has a name for.
    pub fn named(name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Source::Bytes {
            name: Some(name.into()),
            bytes,
        }
    }

    /// Bytes the caller has no name for. Detection then rests on signatures
    /// alone, which is enough for every format but the ones that have none.
    pub fn unnamed(bytes: Vec<u8>) -> Self {
        Source::Bytes { name: None, bytes }
    }
}

/// One Source Document's fate in a Batch.
///
/// Carries its own Failure rather than aborting: a corrupt document must not
/// destroy the other nine hundred thousand.
///
/// An [`Extraction`] by default; an [`Inventory`] when the Batch was asked
/// for metadata alone.
#[derive(Debug)]
pub struct BatchItem<T = Extraction> {
    /// How the caller referred to the document: the path it was read from,
    /// or the name given with its bytes. Empty for bytes given no name,
    /// which have nothing to be called by.
    pub source: String,
    pub result: Result<T, Failure>,
}

/// The results of a Batch, as they finish.
///
/// An iterator rather than a collection: a million documents cannot be held
/// in memory, and a caller who stops early must be able to stop the work too.
/// Dropping it does exactly that — the workers find the queue gone and give
/// up, promptly.
pub struct Results<T = Extraction> {
    results: Option<Receiver<BatchItem<T>>>,
    producer: Option<JoinHandle<()>>,
}

/// [`Results::next_within`] ran out of time before anything finished. Not
/// the end of the Batch: the next call may well have something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeout;

impl<T> Results<T> {
    /// The next result, unless nothing finishes within `timeout`.
    ///
    /// For the caller who cannot afford to block indefinitely — one holding
    /// a foreign runtime's lock, or watching for a signal — and would rather
    /// look around and come back. `Ok(None)` is the end of the Batch, exactly
    /// as `next` returning `None` is; `Err(Timeout)` is not.
    pub fn next_within(&mut self, timeout: Duration) -> Result<Option<BatchItem<T>>, Timeout> {
        let Some(results) = self.results.as_ref() else {
            return Ok(None);
        };
        match results.recv_timeout(timeout) {
            Ok(item) => Ok(Some(item)),
            Err(RecvTimeoutError::Disconnected) => Ok(None),
            Err(RecvTimeoutError::Timeout) => Err(Timeout),
        }
    }
}

impl<T> Iterator for Results<T> {
    type Item = BatchItem<T>;

    fn next(&mut self) -> Option<BatchItem<T>> {
        self.results.as_ref()?.recv().ok()
    }
}

impl<T> Drop for Results<T> {
    fn drop(&mut self) {
        // The order is the whole of it, and it cannot be left to field order,
        // which runs after this method: the queue has to go first, because
        // that is what tells a worker blocked on a full one that nobody is
        // reading any more. Joining first would wait forever on a thread that
        // is waiting on us.
        drop(self.results.take());
        if let Some(producer) = self.producer.take() {
            let _ = producer.join();
        }
    }
}

/// What a Batch does to each Source Document once it is in memory.
type Job<T> = fn(&Registry, SourceDocument<'_>, &Options) -> Result<T, Failure>;

/// Extract many Source Documents at once, reading each from its path.
///
/// Runs on `workers` threads, or on one per core when that is `None`, and
/// yields results in completion order — so one slow document stalls nothing
/// behind it.
pub(crate) fn extract_paths<I>(
    registry: Arc<Registry>,
    paths: I,
    options: &Options,
    workers: Option<usize>,
) -> Results<Extraction>
where
    I: IntoIterator<Item = PathBuf> + Send + 'static,
    I::IntoIter: Send,
{
    extract_sources(
        registry,
        paths.into_iter().map(Source::Path),
        options,
        workers,
    )
}

/// Inventory many Source Documents at once: the same Batch as
/// [`extract_paths`], reading metadata alone.
pub(crate) fn inventory_paths<I>(
    registry: Arc<Registry>,
    paths: I,
    options: &Options,
    workers: Option<usize>,
) -> Results<Inventory>
where
    I: IntoIterator<Item = PathBuf> + Send + 'static,
    I::IntoIter: Send,
{
    inventory_sources(
        registry,
        paths.into_iter().map(Source::Path),
        options,
        workers,
    )
}

/// Extract many Source Documents at once, wherever each comes from.
pub(crate) fn extract_sources<I>(
    registry: Arc<Registry>,
    sources: I,
    options: &Options,
    workers: Option<usize>,
) -> Results<Extraction>
where
    I: IntoIterator<Item = Source> + Send + 'static,
    I::IntoIter: Send,
{
    run(registry, sources, options, workers, Registry::extract)
}

/// Inventory many Source Documents at once, wherever each comes from.
pub(crate) fn inventory_sources<I>(
    registry: Arc<Registry>,
    sources: I,
    options: &Options,
    workers: Option<usize>,
) -> Results<Inventory>
where
    I: IntoIterator<Item = Source> + Send + 'static,
    I::IntoIter: Send,
{
    run(registry, sources, options, workers, Registry::inventory)
}

/// Extract one Source Document exactly as a Batch would: the same read, the
/// same size cap, the same panic guard, without the threads.
pub(crate) fn extract_source(
    registry: &Registry,
    source: Source,
    options: &Options,
) -> BatchItem<Extraction> {
    run_one(registry, source, options, Registry::extract)
}

/// Inventory one Source Document exactly as a Batch would.
pub(crate) fn inventory_source(
    registry: &Registry,
    source: Source,
    options: &Options,
) -> BatchItem<Inventory> {
    run_one(registry, source, options, Registry::inventory)
}

/// The one Batch, whatever it does to each document: the same threads, the
/// same bounded queue, the same size cap and the same panic guard, so that an
/// inventory and an extraction of the same corpus cannot differ in anything
/// but the work done per document.
fn run<T, I>(
    registry: Arc<Registry>,
    sources: I,
    options: &Options,
    workers: Option<usize>,
    job: Job<T>,
) -> Results<T>
where
    T: Send + 'static,
    I: IntoIterator<Item = Source> + Send + 'static,
    I::IntoIter: Send,
{
    let threads = workers.unwrap_or_else(num_cpus).max(1);
    let (sender, receiver): (SyncSender<BatchItem<T>>, Receiver<BatchItem<T>>) =
        sync_channel(threads * QUEUE_PER_WORKER);
    let options = options.clone();

    let producer = std::thread::spawn(move || {
        let Ok(pool) = rayon::ThreadPoolBuilder::new().num_threads(threads).build() else {
            return;
        };
        pool.install(move || {
            // `try_for_each_with` rather than `for_each`: a send that fails
            // means the consumer has gone, and the right answer is to stop
            // rather than to extract another half a corpus into a dead queue.
            let _ = sources
                .into_iter()
                .par_bridge()
                .try_for_each_with(sender, |sender, source| {
                    sender
                        .send(run_one(&registry, source, &options, job))
                        .map_err(|_| ())
                });
        });
    });

    Results {
        results: Some(receiver),
        producer: Some(producer),
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism().map_or(1, |count| count.get())
}

/// One Source Document through the job, labelled the way the caller gave it.
fn run_one<T>(registry: &Registry, source: Source, options: &Options, job: Job<T>) -> BatchItem<T> {
    match source {
        Source::Path(path) => BatchItem {
            source: path.display().to_string(),
            result: read_and_run(registry, &path, options, job),
        },
        Source::Bytes { name, bytes } => {
            // Already in memory, so the cap cannot bound anything here; it is
            // applied all the same, so that a document is refused or read
            // whatever road it came in by.
            let result = if bytes.len() as u64 > options.max_size {
                Err(Failure::TooLarge {
                    limit: options.max_size,
                })
            } else {
                let source = match name.as_deref() {
                    Some(name) => SourceDocument::named(name, &bytes),
                    None => SourceDocument::from_bytes(&bytes),
                };
                guarded(|| job(registry, source, options))
            };
            BatchItem {
                source: name.unwrap_or_default(),
                result,
            }
        }
    }
}

fn read_and_run<T>(
    registry: &Registry,
    path: &Path,
    options: &Options,
    job: Job<T>,
) -> Result<T, Failure> {
    let file = std::fs::metadata(path)?;

    // Checked before the read, not after: the cap is meant to bound memory,
    // and a file already in memory is past bounding.
    if file.len() > options.max_size {
        return Err(Failure::TooLarge {
            limit: options.max_size,
        });
    }

    let bytes = std::fs::read(path)?;
    let mut source = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => SourceDocument::named(name, &bytes),
        None => SourceDocument::from_bytes(&bytes),
    };
    if let Some(modified) = modified_seconds(&file) {
        source = source.modified_at(modified);
    }
    guarded(|| job(registry, source, options))
}

/// A Parser must never panic on malformed input, and that is a constraint
/// this crate holds rather than hopes for: a panic here becomes this one
/// document's Failure instead of the whole process's last act.
pub(crate) fn guarded<T>(job: impl FnOnce() -> Result<T, Failure>) -> Result<T, Failure> {
    match catch_unwind(AssertUnwindSafe(job)) {
        Ok(result) => result,
        Err(payload) => Err(Failure::Panic(panic_message(payload))),
    }
}

fn modified_seconds(file: &std::fs::Metadata) -> Option<i64> {
    let modified = file.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "parser panicked".to_owned()
    }
}
