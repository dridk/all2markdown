use crate::batch::BatchItem;
use crate::extraction::Extraction;
use crate::metadata::{DocumentMetadata, FileMetadata};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write;

/// What an Extraction says about where it came from, in the shape both
/// renderings share.
///
/// One struct feeds the YAML front matter and the JSONL line, so the two
/// cannot drift apart: a field added here appears in both, or in neither.
/// The raw bag is the one exception, and it is opt-in rather than a second
/// struct, because a header that dumps everything a format exposes stops
/// being readable.
#[derive(Serialize)]
struct Provenance<'a> {
    format: &'a str,
    encoding: Option<&'a str>,
    file: File<'a>,
    document: Document<'a>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct File<'a> {
    name: Option<&'a str>,
    size: Option<u64>,
    modified: Option<i64>,
}

#[derive(Serialize)]
struct Document<'a> {
    title: Option<&'a str>,
    author: Option<&'a str>,
    created: Option<i64>,
    modified: Option<i64>,
    page_count: Option<u32>,
    language: Option<&'a str>,
    /// Absent from the front matter, present in JSONL.
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<&'a BTreeMap<String, String>>,
}

impl<'a> Provenance<'a> {
    fn of(extraction: &'a Extraction, raw: bool) -> Self {
        let FileMetadata {
            name,
            size,
            modified,
        } = &extraction.file;
        let DocumentMetadata {
            title,
            author,
            created,
            modified: document_modified,
            page_count,
            language,
            raw: bag,
        } = &extraction.document;
        Provenance {
            format: extraction.format.id(),
            encoding: extraction.encoding.as_deref(),
            file: File {
                name: name.as_deref(),
                size: *size,
                modified: *modified,
            },
            document: Document {
                title: title.as_deref(),
                author: author.as_deref(),
                created: *created,
                modified: *document_modified,
                page_count: *page_count,
                language: language.as_deref(),
                raw: raw.then_some(bag),
            },
            warnings: extraction
                .warnings
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

/// Whether a rendered Markdown document opens with a YAML header.
///
/// On by default: a file that carries its own provenance survives being
/// moved, renamed or handed to someone who never saw the Batch. Off for a
/// consumer that does not understand front matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrontMatter {
    #[default]
    On,
    Off,
}

/// Render an Extraction as a Markdown document.
///
/// With `FrontMatter::On`, the text is preceded by a YAML header carrying the
/// format, the encoding, the File Metadata and the normalised core of the
/// Document Metadata. The header is always present, every key in it, `null`
/// where there is nothing to say: an output whose shape varies with its
/// content is hostile to whatever parses it downstream.
pub fn to_markdown(extraction: &Extraction, front_matter: FrontMatter) -> String {
    let mut out = String::with_capacity(extraction.markdown.len() + 256);
    if front_matter == FrontMatter::On {
        out.push_str("---\n");
        out.push_str(&yaml(&Provenance::of(extraction, false)));
        out.push_str("---\n");
        if !extraction.markdown.is_empty() {
            out.push('\n');
        }
    }
    out.push_str(&extraction.markdown);
    out
}

/// One Batch result as a JSON line.
///
/// Every key is always present, `null` where there is nothing to say, for the
/// same reason the front matter is: the whole point of this format is that
/// one pass over a corpus answers both what worked and what did not.
#[derive(Serialize)]
struct Line<'a> {
    source: &'a str,
    format: Option<&'a str>,
    encoding: Option<&'a str>,
    file: Option<File<'a>>,
    document: Option<Document<'a>>,
    text: Option<&'a str>,
    warnings: Vec<String>,
    error: Option<String>,
}

/// Render one Batch result as a JSONL line, without its newline.
///
/// Carries what the front matter carries, plus the raw metadata bag, which
/// belongs to a machine-read line and not to a human-read header.
pub fn to_jsonl(item: &BatchItem) -> String {
    let line = match &item.result {
        Ok(extraction) => {
            let provenance = Provenance::of(extraction, true);
            Line {
                source: &item.source,
                format: Some(provenance.format),
                encoding: provenance.encoding,
                file: Some(provenance.file),
                document: Some(provenance.document),
                text: Some(&extraction.markdown),
                warnings: provenance.warnings,
                error: None,
            }
        }
        Err(failure) => Line {
            source: &item.source,
            format: None,
            encoding: None,
            file: None,
            document: None,
            text: None,
            warnings: Vec::new(),
            error: Some(failure.to_string()),
        },
    };

    // Serialising a struct of strings cannot fail; falling back rather than
    // unwrapping keeps the promise that one document never stops a Batch.
    serde_json::to_string(&line).unwrap_or_else(|e| {
        format!(
            r#"{{"source":{},"format":null,"encoding":null,"file":null,"document":null,"text":null,"warnings":[],"error":{}}}"#,
            serde_json::to_string(&item.source).unwrap_or_else(|_| "\"?\"".to_owned()),
            serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"?\"".to_owned()),
        )
    })
}

/// A flat-or-nested mapping of scalars as YAML, one key per line.
///
/// Written by hand rather than through a YAML crate because the shape is
/// fixed and small, and because the value goes through serde first: the same
/// `Serialize` impl that feeds the JSON line feeds this, which is what keeps
/// the two renderings from diverging. Strings are emitted as JSON string
/// literals, which YAML accepts verbatim, so nothing a document declares can
/// break the header.
fn yaml(value: &impl Serialize) -> String {
    let value = serde_json::to_value(value).unwrap_or(Value::Null);
    let mut out = String::new();
    write_yaml(&mut out, &value, 0);
    out
}

fn write_yaml(out: &mut String, value: &Value, indent: usize) {
    let Value::Object(map) = value else {
        return;
    };
    for (key, value) in map {
        let _ = write!(out, "{:indent$}{key}:", "", indent = indent);
        match value {
            Value::Object(inner) if !inner.is_empty() => {
                out.push('\n');
                write_yaml(out, value, indent + 2);
            }
            Value::Object(_) => out.push_str(" {}\n"),
            Value::Array(items) if items.is_empty() => out.push_str(" []\n"),
            Value::Array(items) => {
                out.push('\n');
                for item in items {
                    let _ = writeln!(out, "{:indent$}- {}", "", scalar(item), indent = indent + 2);
                }
            }
            scalar_value => {
                let _ = writeln!(out, " {}", scalar(scalar_value));
            }
        }
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        // A JSON string literal is a valid YAML double-quoted scalar.
        other => other.to_string(),
    }
}
