use crate::extraction::SourceDocument;
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::parser::Parser;
use docx_rs::*;

const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// The part every DOCX carries and no other ZIP does.
const DOCX_ENTRY: &str = "word/document.xml";

pub struct DocxParser;

impl Parser for DocxParser {
    fn format(&self) -> Format {
        Format::DOCX
    }

    fn probe(&self, source: &SourceDocument<'_>) -> Confidence {
        Confidence::certain_if(
            source.bytes.starts_with(&ZIP_MAGIC) && zip_contains_entry(source.bytes, DOCX_ENTRY),
        )
    }

    fn extract(&self, source: &SourceDocument<'_>) -> Result<String, Failure> {
        let docx = read_docx(source.bytes)
            .map_err(|e| Failure::parse(Format::DOCX, format!("{e}")))?;

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
                                                if let TableCellContent::Paragraph(para) = tc_child {
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
        Ok(md)
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
