use crate::date;
use crate::extraction::{Extracted, Options, SourceDocument};
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::metadata::DocumentMetadata;
use crate::parser::Parser;
use docx_rs::*;
use quick_xml::events::Event;

const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// The part every DOCX carries and no other ZIP does.
const DOCX_ENTRY: &str = "word/document.xml";

/// Where a DOCX keeps what it declares about itself: the Dublin Core part,
/// and the extended part a word processor fills in. Both are separate from
/// `word/document.xml`, which is what makes reading them nearly free.
const CORE_PROPERTIES: &str = "docProps/core.xml";
const EXTENDED_PROPERTIES: &str = "docProps/app.xml";

pub struct DocxParser;

impl Parser for DocxParser {
    fn format(&self) -> Format {
        Format::DOCX
    }

    fn probe(&self, source: &SourceDocument<'_>) -> Confidence {
        Confidence::certain_if(
            source.bytes.starts_with(&ZIP_MAGIC) && zip_contains_entry(source.bytes, DOCX_ENTRY),
        )
        .max(Confidence::likely_if(source.has_extension("docx")))
    }

    fn metadata(&self, source: &SourceDocument<'_>) -> Result<DocumentMetadata, Failure> {
        let mut metadata = DocumentMetadata::default();

        for (name, value) in properties(source.bytes, CORE_PROPERTIES) {
            match name.as_str() {
                "title" => metadata.title = Some(value),
                "creator" => metadata.author = Some(value),
                "language" => metadata.language = Some(value),
                "created" => metadata.created = date::iso8601(&value),
                "modified" => metadata.modified = date::iso8601(&value),
                _ => {
                    metadata.raw.insert(name, value);
                }
            }
        }

        for (name, value) in properties(source.bytes, EXTENDED_PROPERTIES) {
            match name.as_str() {
                "Pages" => metadata.page_count = value.parse().ok(),
                _ => {
                    metadata.raw.insert(name, value);
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
        let docx =
            read_docx(source.bytes).map_err(|e| Failure::parse(Format::DOCX, format!("{e}")))?;

        let mut md = String::new();

        for child in &docx.document.children {
            match child {
                DocumentChild::Paragraph(para) => {
                    let text = extract_paragraph_text(para);
                    if text.trim().is_empty() {
                        continue;
                    }
                    if let Some(level) = detect_heading_level(para) {
                        for _ in 0..level {
                            md.push('#');
                        }
                        md.push(' ');
                    }
                    md.push_str(text.trim());
                    md.push_str("\n\n");
                }
                DocumentChild::Table(table) => {
                    for row in &table.rows {
                        match row {
                            TableChild::TableRow(tr) => {
                                for cell in &tr.cells {
                                    match cell {
                                        TableRowChild::TableCell(tc) => {
                                            for tc_child in &tc.children {
                                                if let TableCellContent::Paragraph(para) = tc_child
                                                {
                                                    let text = extract_paragraph_text(para);
                                                    if !text.trim().is_empty() {
                                                        md.push_str(text.trim());
                                                        md.push(' ');
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                md.push('\n');
                            }
                        }
                    }
                    md.push('\n');
                }
                _ => {}
            }
        }
        Ok(md.into())
    }
}

fn extract_paragraph_text(para: &Paragraph) -> String {
    let mut text = String::new();
    for child in &para.children {
        match child {
            ParagraphChild::Run(run) => {
                for rc in &run.children {
                    match rc {
                        RunChild::Text(t) => text.push_str(&t.text),
                        RunChild::Tab(_) => text.push('\t'),
                        RunChild::Break(_) => text.push('\n'),
                        // Skip Drawing and Shape to exclude textbox content
                        _ => {}
                    }
                }
            }
            ParagraphChild::Hyperlink(link) => {
                for run in &link.children {
                    if let ParagraphChild::Run(run) = run {
                        for rc in &run.children {
                            if let RunChild::Text(t) = rc {
                                text.push_str(&t.text);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    text
}

fn detect_heading_level(para: &Paragraph) -> Option<usize> {
    if let Some(ref style) = para.property.style {
        let id = style.val.to_lowercase();
        if id.starts_with("heading") || id.starts_with("titre") {
            if let Some(n) = id.chars().last().and_then(|c| c.to_digit(10)) {
                return Some(n as usize);
            }
        }
    }
    None
}

fn zip_contains_entry(data: &[u8], name: &str) -> bool {
    let cursor = std::io::Cursor::new(data);
    let Ok(mut archive) = zip::ZipArchive::new(cursor) else {
        return false;
    };
    // Bound rather than returned directly: the ZipFile borrows `archive`, and
    // a tail expression would outlive it.
    let found = archive.by_name(name).is_ok();
    found
}

/// The properties one metadata part declares, as (name, value) pairs.
///
/// Both parts are flat lists of elements holding text, so one reader serves
/// them and neither needs a schema. A part that is missing, unreadable or
/// malformed yields nothing: metadata a document does not declare is not an
/// error.
fn properties(bytes: &[u8], entry: &str) -> Vec<(String, String)> {
    let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return Vec::new();
    };
    let Ok(mut part) = archive.by_name(entry) else {
        return Vec::new();
    };
    let mut xml = String::new();
    if std::io::Read::read_to_string(&mut part, &mut xml).is_err() {
        return Vec::new();
    }

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut found = Vec::new();
    let mut depth = 0usize;
    let mut name: Option<String> = None;
    let mut text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                depth += 1;
                if depth == 2 {
                    name = Some(element.local_name().as_ref().to_owned());
                    text.clear();
                }
            }
            Ok(Event::Text(chunk)) if depth == 2 => {
                text.push_str(&chunk.xml_content(quick_xml::XmlVersion::Implicit1_0));
            }
            Ok(Event::End(_)) => {
                if depth == 2 {
                    if let Some(name) = name.take() {
                        let value = text.trim();
                        // An element a word processor left empty declares
                        // nothing, and an empty title is worse than none.
                        if !value.is_empty() {
                            found.push((name, value.to_owned()));
                        }
                    }
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    found
}
