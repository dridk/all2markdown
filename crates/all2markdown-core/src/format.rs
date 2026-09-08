use std::fmt;

/// A format all2markdown can read.
///
/// Deliberately not an enum: the Parser registry is open, so a consumer of this
/// crate can register a Parser for a format this crate has never heard of. An
/// enum would close that door.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Format(&'static str);

impl Format {
    pub const DOC: Format = Format("doc");
    pub const DOCX: Format = Format("docx");
    pub const RTF: Format = Format("rtf");
    pub const PDF: Format = Format("pdf");

    /// The formats this crate ships with.
    pub const BUILTIN: &'static [Format] = &[Self::DOC, Self::DOCX, Self::RTF, Self::PDF];

    pub const fn new(id: &'static str) -> Self {
        Format(id)
    }

    pub const fn id(self) -> &'static str {
        self.0
    }

    /// Look an id up among the built-in formats, case-insensitively.
    ///
    /// Milestone 1 step 2 moves this onto the registry, which is the only thing
    /// that can know about formats registered outside this crate.
    pub fn from_id(id: &str) -> Option<Format> {
        let id = id.trim().to_ascii_lowercase();
        Self::BUILTIN.iter().copied().find(|f| f.id() == id)
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// How sure a Parser is that a Source Document is one of its own.
///
/// Ordered on purpose: detection polls every Parser and keeps the most
/// confident. `LastResort` is what the plain-text Parser answers for anything
/// decodable, which is why the fallback needs no special case in the detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    No,
    LastResort,
    Likely,
    Certain,
}
