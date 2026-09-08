use crate::error::All2mdError;
use crate::strategy::FormatParser;
use rtf_parser::Painter;

pub struct RtfParser;

impl FormatParser for RtfParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, All2mdError> {
        let text = std::str::from_utf8(data)
            .map_err(|e| All2mdError::ParseError(format!("RTF not valid UTF-8: {e}")))?;
        let doc = rtf_parser::RtfDocument::try_from(text)
            .map_err(|e| All2mdError::ParseError(format!("RTF: {e}")))?;

        // rtf-parser splits text into StyleBlocks by painter changes.
        // Paragraph breaks (\par) are lost when the style doesn't change.
        // We join consecutive blocks that share the same paragraph properties
        // (same font_size) into one paragraph.
        // font_size is in half-points: 24 = 12pt (body), 56 = 28pt, etc.

        let mut md = String::new();
        let mut current_line = String::new();
        let mut current_heading: Option<usize> = None;
        let mut prev_paragraph_key: Option<u16> = None;

        for block in &doc.body {
            let block_text = block.text.trim_end();
            if block_text.is_empty() {
                continue;
            }

            let para_key = block.painter.font_size;

            // New paragraph when font_size changes
            if let Some(prev_key) = prev_paragraph_key {
                if prev_key != para_key {
                    flush_paragraph(&mut md, &mut current_line, current_heading);
                    current_heading = None;
                }
            }

            if current_line.is_empty() {
                current_heading = detect_heading(&block.painter);
            }

            if !current_line.is_empty() && !current_line.ends_with(' ') {
                current_line.push(' ');
            }
            current_line.push_str(block_text);
            prev_paragraph_key = Some(para_key);
        }

        flush_paragraph(&mut md, &mut current_line, current_heading);
        Ok(md)
    }
}

fn flush_paragraph(md: &mut String, line: &mut String, heading: Option<usize>) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        line.clear();
        return;
    }
    if let Some(level) = heading {
        for _ in 0..level {
            md.push('#');
        }
        md.push(' ');
    }
    md.push_str(trimmed);
    md.push_str("\n\n");
    line.clear();
}

fn detect_heading(painter: &Painter) -> Option<usize> {
    // font_size is in half-points (24 = 12pt normal body text)
    if painter.font_size >= 48 {
        // >= 24pt
        Some(1)
    } else if painter.font_size >= 36 {
        // >= 18pt
        Some(2)
    } else if painter.font_size >= 28 {
        // >= 14pt
        Some(3)
    } else {
        None
    }
}
