//! The Python face of all2markdown.
//!
//! Bytes in, an [`Extraction`](PyExtraction) out, and a Batch that takes any
//! Python iterable and hands back an iterator. The GIL is released for the
//! whole of every extraction, so that a Python thread downloading the next
//! document and the Rust threads extracting the last one make progress at
//! once, in a single process. See `examples/s3_batch.py`.
//!
//! Three things are deliberately not here: the front matter and JSONL are
//! rendered by the core's one rendering path, the Batch is the core's one
//! Batch, and a Failure is the core's Failure. This crate only crosses the
//! boundary.

use a2md::{BatchItem, FrontMatter, Options, Source, SourceDocument};
use all2markdown_core as a2md;
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyIterator, PyMemoryView, PyString, PyTuple};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

pyo3::create_exception!(
    all2markdown,
    Failure,
    PyException,
    "One document could not be extracted.\n\n\
     Raised by `extract` and `extract_bytes`, which have one document and \
     nothing else for a failure to be local to. In `extract_many` the same \
     Failure is the item's `error`, and the batch goes on."
);

/// How often the consumer of a Batch looks up from the queue to let Python
/// deliver a signal. Long enough to cost nothing, short enough that Ctrl-C
/// feels immediate.
const SIGNAL_POLL: Duration = Duration::from_millis(100);

/// How many documents the feeder may pull from the iterable ahead of the
/// workers, per worker. This, plus the results queue, is the whole of what an
/// `extract_many` holds in memory beyond the documents being extracted: the
/// input is pulled as the Batch asks for it, never all at once.
const FEED_AHEAD_PER_WORKER: usize = 2;

/// What all2markdown produced for one document, or why it could not.
///
/// One class for both outcomes, so a batch is one stream: `error` is `None`
/// for a document that extracted and its message otherwise, in which case
/// every other field but `name` is `None`.
#[pyclass(frozen, name = "Extraction", module = "all2markdown")]
struct PyExtraction {
    item: BatchItem,
}

impl PyExtraction {
    fn extraction(&self) -> Option<&a2md::Extraction> {
        self.item.result.as_ref().ok()
    }

    /// The Extraction, or the Failure raised: for the single-document calls,
    /// which have nothing else for a failure to be local to.
    fn or_raise(self) -> PyResult<Self> {
        match &self.item.result {
            Ok(_) => Ok(self),
            Err(failure) => Err(Failure::new_err(failure.to_string())),
        }
    }
}

#[pymethods]
impl PyExtraction {
    /// How the document was referred to: the path it was read from, or the
    /// name given with its bytes. `None` for bytes given no name.
    #[getter]
    fn name(&self) -> Option<&str> {
        (!self.item.source.is_empty()).then_some(self.item.source.as_str())
    }

    /// The id of the format the document was read as: "docx", "pdf", ...
    #[getter]
    fn format(&self) -> Option<&str> {
        self.extraction().map(|extraction| extraction.format.id())
    }

    /// The encoding the text was decoded from, for formats where that is a
    /// meaningful question. `None` for formats that define their own.
    #[getter]
    fn encoding(&self) -> Option<&str> {
        self.extraction()
            .and_then(|extraction| extraction.encoding.as_deref())
    }

    /// The text, as Markdown, without front matter.
    #[getter]
    fn markdown(&self) -> Option<&str> {
        self.extraction()
            .map(|extraction| extraction.markdown.as_str())
    }

    /// Why the document is suspect, if it is: extracted, but with no text,
    /// or in an encoding that was a guess.
    #[getter]
    fn warnings(&self) -> Vec<String> {
        self.extraction().map_or_else(Vec::new, |extraction| {
            extraction
                .warnings
                .iter()
                .map(ToString::to_string)
                .collect()
        })
    }

    /// Why there is no text, or `None` when there is.
    #[getter]
    fn error(&self) -> Option<String> {
        self.item.result.as_ref().err().map(ToString::to_string)
    }

    /// What the filesystem knew: `name`, `size`, `modified` (a Unix
    /// timestamp, `None` for bytes handed over in memory).
    #[getter]
    fn file_metadata<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyDict>>> {
        let Some(extraction) = self.extraction() else {
            return Ok(None);
        };
        let file = PyDict::new(py);
        file.set_item("name", extraction.file.name.as_deref())?;
        file.set_item("size", extraction.file.size)?;
        file.set_item("modified", extraction.file.modified)?;
        Ok(Some(file))
    }

    /// What the document declared about itself: `title`, `author`,
    /// `created`, `modified`, `page_count`, `language`, and under `raw`
    /// everything else the format exposed, verbatim.
    #[getter]
    fn document_metadata<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyDict>>> {
        let Some(extraction) = self.extraction() else {
            return Ok(None);
        };
        let document = &extraction.document;
        let dict = PyDict::new(py);
        dict.set_item("title", document.title.as_deref())?;
        dict.set_item("author", document.author.as_deref())?;
        dict.set_item("created", document.created)?;
        dict.set_item("modified", document.modified)?;
        dict.set_item("page_count", document.page_count)?;
        dict.set_item("language", document.language.as_deref())?;
        let raw = PyDict::new(py);
        for (key, value) in &document.raw {
            raw.set_item(key, value)?;
        }
        dict.set_item("raw", raw)?;
        Ok(Some(dict))
    }

    /// The Markdown document, with the YAML front matter the command writes
    /// unless `front_matter` is false. Raises `Failure` for a document that
    /// did not extract.
    #[pyo3(signature = (front_matter=true))]
    fn to_markdown(&self, front_matter: bool) -> PyResult<String> {
        match &self.item.result {
            Ok(extraction) => Ok(a2md::to_markdown(
                extraction,
                if front_matter {
                    FrontMatter::On
                } else {
                    FrontMatter::Off
                },
            )),
            Err(failure) => Err(Failure::new_err(failure.to_string())),
        }
    }

    /// The JSON line the command writes for this document, without its
    /// newline: the same line whether it extracted or failed.
    fn to_jsonl(&self) -> String {
        a2md::to_jsonl(&self.item)
    }

    fn __repr__(&self) -> String {
        let name = self
            .name()
            .map_or("None".to_owned(), |name| format!("{name:?}"));
        match &self.item.result {
            Ok(extraction) => format!(
                "Extraction(name={name}, format={:?}, {} chars)",
                extraction.format.id(),
                extraction.markdown.chars().count()
            ),
            Err(failure) => format!("Extraction(name={name}, error={:?})", failure.to_string()),
        }
    }
}

/// The `Options` the three entry points share, from their keyword arguments.
fn options(
    format: Option<&str>,
    encoding: Option<&str>,
    strict: bool,
    max_size: u64,
) -> PyResult<Options> {
    let mut options = Options {
        strict,
        max_size,
        ..Options::default()
    };
    if let Some(id) = format {
        options.forced_format = Some(
            a2md::format_from_id(id)
                .ok_or_else(|| PyValueError::new_err(format!("unsupported format: {id}")))?,
        );
    }
    if let Some(label) = encoding {
        options.forced_encoding = Some(label.to_owned());
    }
    Ok(options)
}

/// Extract one document from its path.
///
/// Raises `Failure` when it cannot be read. `format` forces the format
/// rather than detecting it; `encoding` does the same for the text encoding;
/// `strict` refuses a document that is identified by nothing but its being
/// decodable; `max_size` caps the document, decompressed, in bytes.
#[pyfunction]
#[pyo3(signature = (path, format=None, *, encoding=None, strict=false, max_size=a2md::DEFAULT_MAX_SIZE))]
fn extract(
    py: Python<'_>,
    path: PathBuf,
    format: Option<&str>,
    encoding: Option<&str>,
    strict: bool,
    max_size: u64,
) -> PyResult<PyExtraction> {
    let options = options(format, encoding, strict, max_size)?;
    let item = py.allow_threads(move || a2md::extract_source(Source::Path(path), &options));
    PyExtraction { item }.or_raise()
}

/// Extract one document from its bytes, which never touch the disk.
///
/// `name` is what detection falls back on for the formats that carry no
/// signature, and what the front matter and the result are labelled with;
/// give the key or the file name when there is one. The other arguments are
/// those of `extract`. Raises `Failure` when the bytes cannot be read.
#[pyfunction]
#[pyo3(signature = (data, name=None, *, format=None, encoding=None, strict=false, max_size=a2md::DEFAULT_MAX_SIZE))]
fn extract_bytes(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    name: Option<String>,
    format: Option<&str>,
    encoding: Option<&str>,
    strict: bool,
    max_size: u64,
) -> PyResult<PyExtraction> {
    let options = options(format, encoding, strict, max_size)?;
    let bytes = bytes_of(data)?;
    let bytes: &[u8] = &bytes;
    let name_ref = name.as_deref();
    // `bytes` objects are borrowed, not copied: a document of hundreds of
    // megabytes is extracted from the buffer Python already holds. That is
    // sound with the GIL released because a `bytes` is immutable and the
    // caller's reference keeps it alive for the whole call.
    let result = py.allow_threads(move || {
        if bytes.len() as u64 > options.max_size {
            return Err(a2md::Failure::TooLarge {
                limit: options.max_size,
            });
        }
        let source = match name_ref {
            Some(name) => SourceDocument::named(name, bytes),
            None => SourceDocument::from_bytes(bytes),
        };
        a2md::extract(source, &options)
    });
    PyExtraction {
        item: BatchItem {
            source: name.unwrap_or_default(),
            result,
        },
    }
    .or_raise()
}

/// Extract many documents at once, from any iterable, as an iterator.
///
/// Each item is a path (`str` or `os.PathLike`), the document's bytes, or a
/// `(name, bytes)` pair; a generator that downloads as it goes is the
/// intended input. Documents are pulled from the iterable only as the
/// workers need them, and results are yielded as they finish, in completion
/// order, so memory stays bounded whatever the size of the corpus and
/// stopping early stops the work.
///
/// A document that cannot be read is an item with its `error` set, never an
/// exception: nothing one document does interrupts the others. An exception
/// raised by the iterable itself is raised from the iterator, once the
/// documents already pulled have been yielded.
///
/// `workers` is the number of extraction threads; `None` or 0 means one per
/// core. The other arguments are those of `extract`.
#[pyfunction]
#[pyo3(signature = (items, *, workers=None, format=None, encoding=None, strict=false, max_size=a2md::DEFAULT_MAX_SIZE))]
fn extract_many(
    items: &Bound<'_, PyAny>,
    workers: Option<usize>,
    format: Option<&str>,
    encoding: Option<&str>,
    strict: bool,
    max_size: u64,
) -> PyResult<PyResults> {
    let options = options(format, encoding, strict, max_size)?;
    let iterator = items.try_iter()?.unbind();
    let threads = match workers {
        None | Some(0) => std::thread::available_parallelism().map_or(1, |count| count.get()),
        Some(workers) => workers,
    };

    // The iterable is Python's, so pulling from it needs the GIL; the Batch
    // is Rust's, and its workers must never wait on the GIL. A feeder thread
    // sits between the two: it takes the GIL for each item, converts it, and
    // hands it down a bounded channel the Batch reads from. Bounded is the
    // point — it is what keeps a generator from being drained ahead of the
    // extraction, and what makes the whole thing lazy.
    let (sender, receiver) = sync_channel::<Source>(threads * FEED_AHEAD_PER_WORKER);
    let stop = Arc::new(AtomicBool::new(false));
    let input_error = Arc::new(Mutex::new(None));
    let feeder = std::thread::spawn({
        let stop = Arc::clone(&stop);
        let input_error = Arc::clone(&input_error);
        move || feed(iterator, sender, &stop, &input_error)
    });
    let results = a2md::extract_sources(receiver, &options, Some(threads));

    Ok(PyResults {
        batch: Mutex::new(Some(Batch { results, feeder })),
        stop,
        input_error,
    })
}

/// Pull items from the Python iterable, one at a time and under the GIL,
/// into the Batch, until the iterable ends, raises, or nobody is reading.
fn feed(
    iterator: Py<PyIterator>,
    sender: SyncSender<Source>,
    stop: &AtomicBool,
    input_error: &Mutex<Option<PyErr>>,
) {
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let pulled = Python::with_gil(|py| {
            // A `Bound<PyIterator>` is itself an Iterator; the clone is a new
            // reference to the same Python object, not a copy of it.
            let mut iterator = iterator.bind(py).clone();
            match iterator.next() {
                None => Ok(None),
                Some(Ok(item)) => source_of(&item).map(Some),
                Some(Err(error)) => Err(error),
            }
        });
        match pulled {
            // Checked again after the pull: a consumer that went away while
            // this thread was waiting for the GIL, or for a slow download,
            // must not be fed one more document.
            Ok(Some(source)) => {
                if stop.load(Ordering::SeqCst) || sender.send(source).is_err() {
                    break;
                }
            }
            Ok(None) => break,
            Err(error) => {
                if let Ok(mut slot) = input_error.lock() {
                    *slot = Some(error);
                }
                break;
            }
        }
    }
    // The iterator, and through it the caller's generator, is released under
    // the GIL and now, rather than whenever some other thread next takes it.
    Python::with_gil(|py| drop(iterator.into_bound(py)));
}

/// The Batch of `extract_many`, as an iterator of `Extraction`.
///
/// Dropping it before the end stops the work: the workers find the queue
/// gone and give up, and the iterable is pulled from at most once more.
///
/// Frozen, with the moving parts behind a Mutex, because pyo3 asks a class to
/// be `Sync` and because two threads may share one iterator: each blocks on
/// the lock with the GIL released, never holding the one while waiting for
/// the other, and takes the next result to finish.
#[pyclass(frozen, name = "Results", module = "all2markdown")]
struct PyResults {
    batch: Mutex<Option<Batch>>,
    stop: Arc<AtomicBool>,
    input_error: Arc<Mutex<Option<PyErr>>>,
}

/// The threads behind a `Results`: the core Batch, and the feeder that
/// pulls the Python iterable into it.
struct Batch {
    results: a2md::Results,
    feeder: JoinHandle<()>,
}

#[pymethods]
impl PyResults {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<PyExtraction>> {
        loop {
            // The wait is where the GIL goes: whoever feeds the iterable, and
            // whatever else the process is doing in Python, runs while this
            // thread has nothing to do but wait for the workers.
            let outcome = py.allow_threads(|| {
                let mut batch = self.batch.lock().unwrap_or_else(PoisonError::into_inner);
                match batch.as_mut() {
                    Some(batch) => batch.results.next_within(SIGNAL_POLL),
                    None => Ok(None),
                }
            });
            match outcome {
                Ok(Some(item)) => return Ok(Some(PyExtraction { item })),
                Ok(None) => {
                    self.finish(py);
                    let error = self
                        .input_error
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    return match error {
                        Some(error) => Err(error),
                        None => Ok(None),
                    };
                }
                // Nothing finished yet: let Python deliver a pending signal,
                // so that Ctrl-C ends a batch stuck behind a slow document
                // rather than waiting for it.
                Err(a2md::Timeout) => py.check_signals()?,
            }
        }
    }
}

impl PyResults {
    /// Stop the Batch and wait for its threads, with the GIL released.
    ///
    /// Released because the feeder may be waiting for the GIL to pull one
    /// more item, and the Batch cannot end until it has: joining it with the
    /// GIL held would wait for a thread that is waiting for us.
    fn finish(&self, py: Python<'_>) {
        self.stop.store(true, Ordering::SeqCst);
        py.allow_threads(|| {
            let batch = self
                .batch
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(Batch { results, feeder }) = batch {
                drop(results);
                let _ = feeder.join();
            }
        });
    }
}

impl Drop for PyResults {
    fn drop(&mut self) {
        let running = self
            .batch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some();
        if running {
            // Python deallocates with the GIL held, which is exactly the
            // state `finish` needs to release it from.
            Python::with_gil(|py| self.finish(py));
        }
    }
}

/// One item of the iterable given to `extract_many`, as a `Source`.
fn source_of(item: &Bound<'_, PyAny>) -> PyResult<Source> {
    if let Ok(pair) = item.downcast::<PyTuple>() {
        if pair.len() != 2 {
            return Err(PyTypeError::new_err(format!(
                "an extract_many item given as a tuple must be a (name, bytes) pair, not {} items",
                pair.len()
            )));
        }
        let name = pair.get_item(0)?;
        let name = if name.is_none() {
            None
        } else {
            Some(name.downcast::<PyString>()?.to_str()?.to_owned())
        };
        let bytes = bytes_of(&pair.get_item(1)?)?.into_owned();
        return Ok(Source::Bytes { name, bytes });
    }
    // Bytes before paths: `os.fspath` accepts a `bytes` and would read it as
    // a path, which is never what a caller handing over a document means.
    if is_bytes_like(item) {
        return Ok(Source::unnamed(bytes_of(item)?.into_owned()));
    }
    if let Ok(path) = item.extract::<PathBuf>() {
        return Ok(Source::Path(path));
    }
    Err(PyTypeError::new_err(format!(
        "an extract_many item must be a path, bytes, or a (name, bytes) pair, not {}",
        item.get_type().name()?
    )))
}

fn is_bytes_like(object: &Bound<'_, PyAny>) -> bool {
    object.is_instance_of::<PyBytes>()
        || object.is_instance_of::<PyByteArray>()
        || object.is_instance_of::<PyMemoryView>()
}

/// The bytes of a `bytes`, `bytearray` or `memoryview`, borrowed from a
/// `bytes` and copied from the two that can change under us.
fn bytes_of<'a>(object: &'a Bound<'_, PyAny>) -> PyResult<Cow<'a, [u8]>> {
    if let Ok(bytes) = object.downcast::<PyBytes>() {
        return Ok(Cow::Borrowed(bytes.as_bytes()));
    }
    if let Ok(array) = object.downcast::<PyByteArray>() {
        return Ok(Cow::Owned(array.to_vec()));
    }
    if object.is_instance_of::<PyMemoryView>() {
        let bytes = object.call_method0("tobytes")?;
        return Ok(Cow::Owned(bytes.downcast::<PyBytes>()?.as_bytes().to_vec()));
    }
    Err(PyTypeError::new_err(format!(
        "expected bytes, bytearray or memoryview, not {}",
        object.get_type().name()?
    )))
}

#[pymodule]
fn all2markdown(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(extract_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(extract_many, m)?)?;
    m.add_class::<PyExtraction>()?;
    m.add_class::<PyResults>()?;
    m.add("Failure", m.py().get_type::<Failure>())?;
    m.add("DEFAULT_MAX_SIZE", a2md::DEFAULT_MAX_SIZE)?;
    Ok(())
}
