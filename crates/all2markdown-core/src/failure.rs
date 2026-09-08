use crate::format::Format;
use thiserror::Error;

/// The absence of any Extraction for one Source Document.
///
/// Always local to a single document: a Failure never interrupts a Batch.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Failure {
    #[error("unrecognized file format")]
    UnrecognizedFormat,

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("file too small to identify")]
    FileTooSmall,

    #[error("unknown encoding: {0}")]
    UnknownEncoding(String),

    #[error("{envelope} decompression failed: {message}")]
    Decompress {
        envelope: &'static str,
        message: String,
    },

    #[error("decompressed size exceeds the {limit} byte cap")]
    TooLarge { limit: u64 },

    #[error("a compressed document may not contain another")]
    NestedEnvelope,

    #[error("{format} parser failed: {message}")]
    Parse { format: Format, message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl Failure {
    pub fn parse(format: Format, message: impl Into<String>) -> Self {
        Failure::Parse {
            format,
            message: message.into(),
        }
    }
}
