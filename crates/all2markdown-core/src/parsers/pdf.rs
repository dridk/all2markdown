use crate::extraction::{Extracted, Options, SourceDocument};
use crate::failure::Failure;
use crate::format::{Confidence, Format};
use crate::parser::Parser;

const PDF_MAGIC: &[u8] = b"%PDF";

pub struct PdfParser;

impl Parser for PdfParser {
    fn format(&self) -> Format {
        Format::PDF
    }

    fn probe(&self, source: &SourceDocument<'_>) -> Confidence {
        Confidence::certain_if(source.bytes.starts_with(PDF_MAGIC))
            .max(Confidence::likely_if(source.has_extension("pdf")))
    }

    fn extract(
        &self,
        source: &SourceDocument<'_>,
        _options: &Options,
    ) -> Result<Extracted, Failure> {
        let text = pdf_extract::extract_text_from_mem(source.bytes)
            .map_err(|e| Failure::parse(Format::PDF, format!("{e}")))?;
        Ok(text.into())
    }
}
