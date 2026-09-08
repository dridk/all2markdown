use crate::error::All2mdError;
use crate::strategy::FormatParser;

pub struct DocParser;

impl FormatParser for DocParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, All2mdError> {
        let doc = unword::parse_doc(data)
            .map_err(|e| All2mdError::ParseError(format!("DOC: {e}")))?;
        Ok(doc.body_text)
    }
}
