use crate::format::Format;
use crate::metadata::{DocumentMetadata, FileMetadata};
use std::fmt;

/// A text-bearing file submitted to all2markdown.
///
/// Borrows its bytes: a Batch holds them once and hands out views, rather than
/// copying a million documents.
#[derive(Debug, Clone, Copy)]
pub struct SourceDocument<'a> {
    pub bytes: &'a [u8],
    /// The file name, when the caller knows it. Detection uses its extension
    /// for the formats that carry no signature of their own.
    pub name: Option<&'a str>,
    /// When the filesystem says the file last changed, as a Unix timestamp.
    /// Known only to a caller that read it off a disk; bytes handed over by an
    /// object store carry no such thing.
    pub modified: Option<i64>,
}

impl<'a> SourceDocument<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            name: None,
            modified: None,
        }
    }

    pub fn named(name: &'a str, bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            name: Some(name),
            modified: None,
        }
    }

    /// Record when the filesystem says the file last changed.
    pub fn modified_at(mut self, timestamp: i64) -> Self {
        self.modified = Some(timestamp);
        self
    }

    /// The name's extension, without the dot. `None` when there is no name, or
    /// no extension, or the name is a dotfile with nothing after the dot.
    pub fn extension(&self) -> Option<&'a str> {
        let name = self.name?;
        let (stem, ext) = name.rsplit_once('.')?;
        if stem.is_empty() || ext.is_empty() {
            return None;
        }
        Some(ext)
    }

    /// Whether the name's extension is the given one, case-insensitively.
    ///
    /// What a Parser answers `Likely` on: the extension stage of the cascade
    /// lives in the Parsers, so that no shared table has to learn a format's
    /// extensions when the format is added.
    pub fn has_extension(&self, expected: &str) -> bool {
        self.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(expected))
    }

    pub(crate) fn file_metadata(&self) -> FileMetadata {
        FileMetadata {
            name: self.name.map(str::to_owned),
            size: Some(self.bytes.len() as u64),
            modified: self.modified,
        }
    }
}

/// An Extraction that succeeded but looks suspect.
///
/// The only way to check the Extraction Invariant across a Batch of a million
/// documents: a caller can count the warnings instead of re-reading the corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Warning {
    /// The Source Document carried bytes, but extraction produced no text.
    EmptyOutput,
    /// The encoding was guessed statistically and the guess is shaky. The text
    /// is probably right; only the caller can tell, and this is what lets them
    /// find every questionable document with a query rather than a re-run.
    UncertainEncoding(String),
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::EmptyOutput => f.write_str("document is not empty but produced no text"),
            Warning::UncertainEncoding(encoding) => {
                write!(f, "encoding guessed as {encoding}, with low confidence")
            }
        }
    }
}

/// What a Parser produces: the text, plus what only the Parser can know about
/// how it read it.
///
/// A `String` converts into one, so a Parser whose format defines its own
/// encoding and has nothing to warn about still returns a single expression.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extracted {
    pub markdown: String,
    /// The encoding the text was decoded from, for formats where that is a
    /// meaningful question. `None` for formats that define their own.
    pub encoding: Option<String>,
    pub warnings: Vec<Warning>,
}

impl From<String> for Extracted {
    fn from(markdown: String) -> Self {
        Self {
            markdown,
            encoding: None,
            warnings: Vec::new(),
        }
    }
}

/// What all2markdown produces for one Source Document.
///
/// Never a bare string: without the format, the encoding and the warnings, a
/// caller cannot tell a genuinely empty document from a silent extraction
/// failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extraction {
    pub markdown: String,
    pub format: Format,
    /// The encoding the text was decoded from, for formats where that is a
    /// meaningful question. `None` for formats that define their own.
    pub encoding: Option<String>,
    pub file: FileMetadata,
    pub document: DocumentMetadata,
    pub warnings: Vec<Warning>,
}

/// What all2markdown produces for one Source Document when only its metadata
/// is asked for: the format, and what the filesystem and the document itself
/// declare. The body is never parsed.
///
/// Every format keeps its metadata in a stream apart from the body, which is
/// what makes an inventory of three terabytes a coffee break rather than an
/// overnight job. There is no text and no encoding here, on purpose: both
/// would require reading the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    pub format: Format,
    pub file: FileMetadata,
    pub document: DocumentMetadata,
}

/// The default cap on a decompressed Source Document: 500 MB.
///
/// The real memory bound of a Batch is this multiplied by the worker count,
/// which is why it is a number the caller can move rather than a constant.
pub const DEFAULT_MAX_SIZE: u64 = 500 * 1024 * 1024;

/// How to extract. Grows as the milestone adds behaviour; a field appears here
/// only once something honours it.
#[derive(Debug, Clone)]
pub struct Options {
    /// Bypasses detection. Always wins: the caller knows things the bytes do
    /// not say.
    pub forced_format: Option<Format>,
    /// Bypasses encoding detection, by label — `"windows-1252"`, `"utf-8"`.
    /// For the caller who knows better than the heuristic.
    pub forced_encoding: Option<String>,
    /// Removes the last-resort stage of the cascade, and nothing else: an
    /// unidentified but decodable document becomes a Failure. For the caller
    /// who wants an exact inventory rather than a permissive read.
    pub strict: bool,
    /// The cap on a decompressed Source Document, in bytes.
    pub max_size: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            forced_format: None,
            forced_encoding: None,
            strict: false,
            max_size: DEFAULT_MAX_SIZE,
        }
    }
}

impl Options {
    pub fn forcing(format: Format) -> Self {
        Self {
            forced_format: Some(format),
            ..Self::default()
        }
    }

    pub fn with_encoding(mut self, label: impl Into<String>) -> Self {
        self.forced_encoding = Some(label.into());
        self
    }

    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    pub fn with_max_size(mut self, bytes: u64) -> Self {
        self.max_size = bytes;
        self
    }
}
