use crate::failure::Failure;
use crate::format::Format;
use crate::strategy::FormatParser;

pub struct DocParser;

impl FormatParser for DocParser {
    fn to_markdown(&self, data: &[u8]) -> Result<String, Failure> {
        let doc = unword::parse_doc(data)
            .map_err(|e| Failure::parse(Format::DOC, format!("{e}")))?;
        Ok(doc.body_text)
    }
}
