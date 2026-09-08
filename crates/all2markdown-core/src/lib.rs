mod detect;
mod doc;
mod docx;
mod error;
mod pdf;
mod rtf;
mod strategy;

pub use detect::{detect_format, Format};
pub use error::All2mdError;
use strategy::FormatParser;

pub fn parse(data: &[u8], format: Option<Format>) -> Result<String, All2mdError> {
    let format = match format {
        Some(f) => f,
        None => detect_format(data)?,
    };
    let parser: Box<dyn FormatParser> = match format {
        Format::Doc => Box::new(doc::DocParser),
        Format::Docx => Box::new(docx::DocxParser),
        Format::Rtf => Box::new(rtf::RtfParser),
        Format::Pdf => Box::new(pdf::PdfParser),
    };
    parser.to_markdown(data)
}
