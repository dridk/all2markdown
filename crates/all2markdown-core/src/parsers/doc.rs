use crate::extraction::SourceDocument;
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::parser::Parser;

/// The OLE2 compound-file signature every legacy Word document opens with.
const OLE2_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

pub struct DocParser;

impl Parser for DocParser {
    fn format(&self) -> Format {
        Format::DOC
    }

    fn probe(&self, source: &SourceDocument<'_>) -> Confidence {
        Confidence::certain_if(source.bytes.starts_with(&OLE2_MAGIC))
    }

    fn extract(&self, source: &SourceDocument<'_>) -> Result<String, Failure> {
        let doc = unword::parse_doc(source.bytes)
            .map_err(|e| Failure::parse(Format::DOC, format!("{e}")))?;
        Ok(doc.body_text)
    }
}
