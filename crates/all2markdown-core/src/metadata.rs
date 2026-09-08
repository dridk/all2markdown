use std::collections::BTreeMap;

/// What the filesystem or object store knows about a Source Document.
///
/// Always available, and never sourced from the document's own content. Kept
/// apart from DocumentMetadata so that an `author` field can never come from
/// one when the reader expects the other.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileMetadata {
    pub name: Option<String>,
    pub size: Option<u64>,
    /// Unix timestamp, in seconds.
    pub modified: Option<i64>,
}

/// What a Source Document declares about itself.
///
/// Absent from formats that carry none, which is why every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    /// Unix timestamp, in seconds.
    pub created: Option<i64>,
    /// Unix timestamp, in seconds.
    pub modified: Option<i64>,
    pub page_count: Option<u32>,
    pub language: Option<String>,
    /// Everything else the format exposes, verbatim.
    ///
    /// Ordered rather than hashed so that rendered output is reproducible
    /// between runs, which snapshot tests depend on.
    pub raw: BTreeMap<String, String>,
}

impl DocumentMetadata {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.author.is_none()
            && self.created.is_none()
            && self.modified.is_none()
            && self.page_count.is_none()
            && self.language.is_none()
            && self.raw.is_empty()
    }
}
