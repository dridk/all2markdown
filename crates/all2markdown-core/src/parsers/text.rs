use crate::encoding;
use crate::extraction::{Extracted, Options, SourceDocument, Warning};
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::parser::Parser;

/// The extensions that name plain text outright. Deliberately short: a format
/// that merely *is* text, like Markdown or CSV, is its own Parser, not an
/// alias of this one.
const TEXT_EXTENSIONS: [&str; 2] = ["txt", "text"];

/// The last resort of the detection cascade.
///
/// Nothing about it is special-cased in the registry: it is registered like
/// any other Parser and simply answers `LastResort` where the others answer
/// `No`, and the Confidence ordering does the rest. That is what makes strict
/// mode a one-line rule rather than a second detector.
pub struct TextParser;

impl Parser for TextParser {
    fn format(&self) -> Format {
        Format::TXT
    }

    fn probe(&self, source: &SourceDocument<'_>) -> Confidence {
        if TEXT_EXTENSIONS.iter().any(|ext| source.has_extension(ext)) {
            return Confidence::Likely;
        }
        if encoding::looks_like_text(source.bytes) {
            return Confidence::LastResort;
        }
        Confidence::No
    }

    fn extract(
        &self,
        source: &SourceDocument<'_>,
        options: &Options,
    ) -> Result<Extracted, Failure> {
        let decoded = encoding::decode(source.bytes, options.forced_encoding.as_deref())?;
        let warnings = if decoded.confident {
            Vec::new()
        } else {
            vec![Warning::UncertainEncoding(decoded.encoding.to_owned())]
        };
        Ok(Extracted {
            markdown: decoded.text,
            encoding: Some(decoded.encoding.to_owned()),
            warnings,
        })
    }
}
