use crate::extraction::{Extraction, Options, SourceDocument, Warning};
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::parser::Parser;
use crate::parsers;

/// Below this, a Source Document is too short to carry any signature this
/// crate knows. Only ever used to explain why nothing recognised it: the
/// Parsers are polled first, so a Parser with a shorter signature than this
/// still wins.
const MIN_IDENTIFIABLE_BYTES: usize = 8;

/// The open set of Parsers all2markdown extracts with.
///
/// Detection is a poll of every Parser rather than a central detector, and the
/// id-to-Format lookup lives here rather than on Format, because only the
/// registry can know about a Parser registered by a consumer of this crate.
pub struct Registry {
    parsers: Vec<Box<dyn Parser>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    /// An empty registry, which recognises nothing.
    pub fn new() -> Self {
        Self { parsers: Vec::new() }
    }

    /// A registry holding the Parsers this crate ships with.
    pub fn with_builtin_parsers() -> Self {
        let mut registry = Self::new();
        parsers::register_builtin(&mut registry);
        registry
    }

    /// Add a Parser. Registration order breaks ties only between *equally*
    /// confident Parsers: at equal Confidence the earlier registration wins, but
    /// a more confident Parser wins whenever it was registered.
    pub fn register(&mut self, parser: impl Parser + 'static) -> &mut Self {
        self.parsers.push(Box::new(parser));
        self
    }

    fn parsers(&self) -> impl Iterator<Item = &dyn Parser> {
        self.parsers.iter().map(Box::as_ref)
    }

    /// The Parser registered for a Supported Format, if any.
    fn parser_for(&self, format: Format) -> Option<&dyn Parser> {
        self.parsers().find(|parser| parser.format() == format)
    }

    /// Look a Supported Format up by id, case-insensitively.
    pub fn format_from_id(&self, id: &str) -> Option<Format> {
        let id = id.trim().to_ascii_lowercase();
        self.parsers().map(|parser| parser.format()).find(|format| format.id() == id)
    }

    /// Identify a Source Document by polling every Parser and keeping the most
    /// confident, ties broken by registration order.
    pub fn detect(&self, source: &SourceDocument<'_>) -> Result<Format, Failure> {
        let mut best: Option<(Confidence, Format)> = None;
        for parser in self.parsers() {
            let confidence = parser.probe(source);
            if confidence == Confidence::No {
                continue;
            }
            if best.is_none_or(|(seen, _)| confidence > seen) {
                best = Some((confidence, parser.format()));
            }
        }

        if let Some((_, format)) = best {
            return Ok(format);
        }
        if source.bytes.len() < MIN_IDENTIFIABLE_BYTES {
            return Err(Failure::FileTooSmall);
        }
        Err(Failure::UnrecognizedFormat)
    }

    /// Extract the text of one Source Document.
    pub fn extract(
        &self,
        source: SourceDocument<'_>,
        options: &Options,
    ) -> Result<Extraction, Failure> {
        let format = match options.forced_format {
            Some(forced) => forced,
            None => self.detect(&source)?,
        };

        let parser = self
            .parser_for(format)
            .ok_or_else(|| Failure::UnsupportedFormat(format.id().to_owned()))?;

        let markdown = parser.extract(&source)?;
        let document = parser.metadata(&source)?;

        let mut warnings = Vec::new();
        if !source.bytes.is_empty() && markdown.trim().is_empty() {
            warnings.push(Warning::EmptyOutput);
        }

        Ok(Extraction {
            markdown,
            format,
            encoding: None,
            file: source.file_metadata(),
            document,
            warnings,
        })
    }
}
