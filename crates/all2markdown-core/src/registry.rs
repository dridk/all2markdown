use crate::envelope;
use crate::extraction::{Extraction, Inventory, Options, SourceDocument, Warning};
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::metadata::FileMetadata;
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
        Self {
            parsers: Vec::new(),
        }
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
        self.parsers()
            .map(|parser| parser.format())
            .find(|format| format.id() == id)
    }

    /// Identify a Source Document by polling every Parser and keeping the most
    /// confident, ties broken by registration order.
    pub fn detect(&self, source: &SourceDocument<'_>) -> Result<Format, Failure> {
        self.detect_with(source, &Options::default())
    }

    /// Identify a Source Document, honouring the strict flag.
    ///
    /// Strips an Envelope first, so that what is identified is the content
    /// and never the wrapper.
    pub fn detect_with(
        &self,
        source: &SourceDocument<'_>,
        options: &Options,
    ) -> Result<Format, Failure> {
        match envelope::peel(source.bytes, options.max_size)? {
            Some(content) => self.poll(&unwrapped(source, &content), options),
            None => self.poll(source, options),
        }
    }

    /// Poll every Parser and keep the most confident, ties broken by
    /// registration order.
    ///
    /// Strict mode is the whole of its own implementation: it drops the
    /// `LastResort` stage and touches nothing else, so an unidentified but
    /// decodable document becomes a Failure while every other answer stands.
    fn poll(&self, source: &SourceDocument<'_>, options: &Options) -> Result<Format, Failure> {
        let floor = if options.strict {
            Confidence::LastResort
        } else {
            Confidence::No
        };

        let mut best: Option<(Confidence, Format)> = None;
        for parser in self.parsers() {
            let confidence = parser.probe(source);
            if confidence <= floor {
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
    ///
    /// An Envelope is stripped once, up front, so that everything downstream —
    /// detection, the Parsers, the Extraction — sees a `report.doc.gz` exactly
    /// as it sees a `report.doc`.
    pub fn extract(
        &self,
        source: SourceDocument<'_>,
        options: &Options,
    ) -> Result<Extraction, Failure> {
        // File Metadata describes what is on disk, so it is taken before the
        // Envelope comes off: the size is the compressed one, and the name is
        // the one that would be typed at a shell.
        let file = source.file_metadata();
        match envelope::peel(source.bytes, options.max_size)? {
            Some(content) => self.extract_content(unwrapped(&source, &content), options, file),
            None => self.extract_content(source, options, file),
        }
    }

    /// Read what one Source Document declares about itself, without parsing
    /// its body.
    ///
    /// Same Envelope handling and same detection as [`Registry::extract`];
    /// the one thing it never does is call a Parser's `extract`, which is
    /// where all the time goes.
    pub fn inventory(
        &self,
        source: SourceDocument<'_>,
        options: &Options,
    ) -> Result<Inventory, Failure> {
        let file = source.file_metadata();
        match envelope::peel(source.bytes, options.max_size)? {
            Some(content) => self.inventory_content(unwrapped(&source, &content), options, file),
            None => self.inventory_content(source, options, file),
        }
    }

    fn inventory_content(
        &self,
        source: SourceDocument<'_>,
        options: &Options,
        file: FileMetadata,
    ) -> Result<Inventory, Failure> {
        let (format, parser) = self.identify(&source, options)?;
        let document = parser.metadata(&source)?;
        Ok(Inventory {
            format,
            file,
            document,
        })
    }

    /// The Supported Format of a Source Document, forced or detected, and the
    /// Parser that reads it.
    fn identify(
        &self,
        source: &SourceDocument<'_>,
        options: &Options,
    ) -> Result<(Format, &dyn Parser), Failure> {
        let format = match options.forced_format {
            Some(forced) => forced,
            None => self.poll(source, options)?,
        };
        let parser = self
            .parser_for(format)
            .ok_or_else(|| Failure::UnsupportedFormat(format.id().to_owned()))?;
        Ok((format, parser))
    }

    fn extract_content(
        &self,
        source: SourceDocument<'_>,
        options: &Options,
        file: FileMetadata,
    ) -> Result<Extraction, Failure> {
        let (format, parser) = self.identify(&source, options)?;

        let extracted = parser.extract(&source, options)?;
        let document = parser.metadata(&source)?;

        let mut warnings = extracted.warnings;
        if !source.bytes.is_empty() && extracted.markdown.trim().is_empty() {
            warnings.push(Warning::EmptyOutput);
        }

        Ok(Extraction {
            markdown: extracted.markdown,
            format,
            encoding: extracted.encoding,
            file,
            document,
            warnings,
        })
    }
}

/// The Source Document that was inside an Envelope: the decompressed bytes,
/// under the name with the Envelope's extension taken off.
fn unwrapped<'a>(source: &SourceDocument<'a>, content: &'a [u8]) -> SourceDocument<'a> {
    SourceDocument {
        bytes: content,
        name: source.name.map(envelope::strip_extension),
        modified: source.modified,
    }
}
