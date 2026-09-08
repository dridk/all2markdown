use crate::error::Error;
use crate::strategy::FormatParser;

pub struct PdfParser;

impl FormatParser for PdfParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, Error> {
        let text = pdf_extract::extract_text_from_mem(data)
            .map_err(|e| Error::ParseError(format!("PDF: {e}")))?;
        Ok(text)
    }
}
