use crate::extraction::{Extracted, Options, SourceDocument};
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
    ///
    /// This is the whole detection cascade: a signature answers `Certain`, an
    /// extension `Likely`, and mere decodability `LastResort`. Because the
    /// registry keeps the most confident answer, the extension is consulted
    /// only when no signature matched, and the text fallback only when nothing
    /// else spoke at all — with no stage ordering written down anywhere.
    fn probe(&self, source: &SourceDocument<'_>) -> Confidence;

    /// What the Source Document declares about itself.
    ///
    /// Defaulted to nothing, which is what keeps a format that carries no
    /// metadata down to two methods.
    fn metadata(&self, _source: &SourceDocument<'_>) -> Result<DocumentMetadata, Failure> {
        Ok(DocumentMetadata::default())
    }

    /// The text of the Source Document, as Markdown.
    ///
    /// Returns an [`Extracted`] rather than a `String` because the encoding a
    /// document was read from is known here and nowhere else; a Parser with
    /// nothing to add returns `Ok(text.into())`.
    fn extract(&self, source: &SourceDocument<'_>, options: &Options)
        -> Result<Extracted, Failure>;
}
