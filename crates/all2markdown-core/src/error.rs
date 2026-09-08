use thiserror::Error;

#[derive(Error, Debug)]
pub enum All2mdError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Unrecognized file format")]
    UnrecognizedFormat,

    #[error("File too small to detect format")]
    FileTooSmall,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
