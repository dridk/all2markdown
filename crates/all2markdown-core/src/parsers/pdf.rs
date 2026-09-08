use crate::date;
use crate::extraction::{Extracted, Options, SourceDocument};
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::metadata::DocumentMetadata;
use crate::parser::Parser;

const PDF_MAGIC: &[u8] = b"%PDF";

/// The keys of the PDF Info dictionary, and the names this crate gives them.
const INFO_KEYS: [(&str, &str); 8] = [
    ("Title", "title"),
    ("Author", "author"),
    ("Subject", "subject"),
    ("Keywords", "keywords"),
    ("Creator", "creator"),
    ("Producer", "producer"),
    ("CreationDate", "created"),
    ("ModDate", "modified"),
];

pub struct PdfParser;

impl Parser for PdfParser {
    fn format(&self) -> Format {
        Format::PDF
    }

    fn probe(&self, source: &SourceDocument<'_>) -> Confidence {
        Confidence::certain_if(source.bytes.starts_with(PDF_MAGIC))
            .max(Confidence::likely_if(source.has_extension("pdf")))
    }

    fn metadata(&self, source: &SourceDocument<'_>) -> Result<DocumentMetadata, Failure> {
        // A second read of the file, since pdf-extract does not hand back the
        // document it parsed. Acceptable while metadata is always read
        // alongside the body; an inventory mode that reads metadata alone will
        // want this path and not the other.
        let Ok(document) = lopdf::Document::load_mem(source.bytes) else {
            return Ok(DocumentMetadata::default());
        };

        let mut metadata = DocumentMetadata {
            page_count: u32::try_from(document.get_pages().len()).ok(),
            ..DocumentMetadata::default()
        };

        let Ok(info) = document
            .trailer
            .get(b"Info")
            .and_then(|info| {
                info.as_reference()
                    .map_or(Ok(info), |id| document.get_object(id))
            })
            .and_then(|info| info.as_dict())
        else {
            return Ok(metadata);
        };

        for (key, name) in INFO_KEYS {
            let Some(value) = info.get(key.as_bytes()).ok().and_then(text_of) else {
                continue;
            };
            match name {
                "title" => metadata.title = Some(value),
                "author" => metadata.author = Some(value),
                "created" => metadata.created = date::pdf(&value),
                "modified" => metadata.modified = date::pdf(&value),
                _ => {
                    metadata.raw.insert(name.to_owned(), value);
                }
            }
        }
        Ok(metadata)
    }

    fn extract(
        &self,
        source: &SourceDocument<'_>,
        _options: &Options,
    ) -> Result<Extracted, Failure> {
        let text = pdf_extract::extract_text_from_mem(source.bytes)
            .map_err(|e| Failure::parse(Format::PDF, format!("{e}")))?;
        Ok(text.into())
    }
}

/// A PDF string, which is either UTF-16 behind a byte-order mark or bytes in
/// PDFDocEncoding — close enough to Latin-1 for every character a metadata
/// field carries.
fn text_of(object: &lopdf::Object) -> Option<String> {
    let bytes = object.as_str().ok()?;
    let text = if bytes.starts_with(&[0xFE, 0xFF]) {
        let (text, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        text.into_owned()
    } else {
        let (text, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
        text.into_owned()
    };
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}
