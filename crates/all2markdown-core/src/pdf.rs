use crate::error::All2mdError;
use crate::strategy::FormatParser;

pub struct PdfParser;

impl FormatParser for PdfParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, All2mdError> {
        let text = pdf_extract::extract_text_from_mem(data)
            .map_err(|e| All2mdError::ParseError(format!("PDF: {e}")))?;
        Ok(text)
    }
}
