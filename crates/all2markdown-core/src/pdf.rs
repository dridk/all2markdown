use crate::failure::Failure;
use crate::format::Format;
use crate::strategy::FormatParser;

pub struct PdfParser;

impl FormatParser for PdfParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, Failure> {
        let text = pdf_extract::extract_text_from_mem(data)
            .map_err(|e| Failure::parse(Format::PDF, format!("{e}")))?;
        Ok(text)
    }
}
