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
    pub const TXT: Format = Format("txt");

    pub const fn new(id: &'static str) -> Self {
        Format(id)
    }

    pub const fn id(self) -> &'static str {
        self.0
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

impl Confidence {
    /// `Certain` when the condition holds, `No` otherwise.
    ///
    /// What a Parser whose format opens with a signature answers, which is most
    /// of them.
    pub fn certain_if(condition: bool) -> Confidence {
        if condition {
            Confidence::Certain
        } else {
            Confidence::No
        }
    }

    /// `Likely` when the condition holds, `No` otherwise.
    ///
    /// What a Parser answers about a file name's extension: a name is evidence,
    /// not proof, so it must lose to any signature that actually matched.
    pub fn likely_if(condition: bool) -> Confidence {
        if condition {
            Confidence::Likely
        } else {
            Confidence::No
        }
    }
}
