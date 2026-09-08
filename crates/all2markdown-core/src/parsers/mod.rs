//! The Parsers this crate ships with.
//!
//! Adding a format means adding one file here and one line to
//! [`register_builtin`].

mod doc;
mod docx;
mod pdf;
mod rtf;
mod text;

use crate::registry::Registry;

/// Registration order matters: it breaks ties between equally confident
/// Parsers.
pub(crate) fn register_builtin(registry: &mut Registry) {
    registry.register(doc::DocParser);
    registry.register(docx::DocxParser);
    registry.register(rtf::RtfParser);
    registry.register(pdf::PdfParser);
    registry.register(text::TextParser);
}
