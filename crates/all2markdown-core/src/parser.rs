use crate::extraction::SourceDocument;
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::metadata::DocumentMetadata;

/// The unit of extension: an object that knows one Supported Format.
///
/// A Parser recognises itself, so that adding a format costs one new file and
/// one registration line rather than an edit to a central detector.
pub trait Parser: Send + Sync {
    /// The Supported Format this Parser reads.
    fn format(&self) -> Format;

    /// How sure this Parser is that the Source Document is one of its own.
    fn probe(&self, source: &SourceDocument<'_>) -> Confidence;

    /// What the Source Document declares about itself.
    ///
    /// Defaulted to nothing, which is what keeps a format that carries no
    /// metadata down to two methods.
    fn metadata(&self, _source: &SourceDocument<'_>) -> Result<DocumentMetadata, Failure> {
        Ok(DocumentMetadata::default())
    }

    /// The text of the Source Document, as Markdown.
    fn extract(&self, source: &SourceDocument<'_>) -> Result<String, Failure>;
}
