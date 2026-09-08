use crate::error::Error;
use crate::strategy::FormatParser;

pub struct DocParser;

impl FormatParser for DocParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, Error> {
        let doc = unword::parse_doc(data)
            .map_err(|e| Error::ParseError(format!("DOC: {e}")))?;
        Ok(doc.body_text)
    }
}
