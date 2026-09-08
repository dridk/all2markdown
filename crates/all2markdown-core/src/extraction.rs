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
}

impl<'a> SourceDocument<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Self {
        Self { bytes, name: None }
    }

    pub fn named(name: &'a str, bytes: &'a [u8]) -> Self {
        Self { bytes, name: Some(name) }
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

    pub(crate) fn file_metadata(&self) -> FileMetadata {
        FileMetadata {
            name: self.name.map(str::to_owned),
            size: Some(self.bytes.len() as u64),
            modified: None,
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
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::EmptyOutput => f.write_str("document is not empty but produced no text"),
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

/// How to extract. Grows as the milestone adds behaviour; a field appears here
/// only once something honours it.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Bypasses detection. Always wins: the caller knows things the bytes do
    /// not say.
    pub forced_format: Option<Format>,
}

impl Options {
    pub fn forcing(format: Format) -> Self {
        Self { forced_format: Some(format) }
    }
}
