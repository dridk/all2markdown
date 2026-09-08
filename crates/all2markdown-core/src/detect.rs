use crate::error::All2mdError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Doc,
    Docx,
    Rtf,
    Pdf,
}

impl Format {
    pub fn from_str_loose(s: &str) -> Result<Self, All2mdError> {
        match s.to_lowercase().as_str() {
            "doc" => Ok(Format::Doc),
            "docx" => Ok(Format::Docx),
            "rtf" => Ok(Format::Rtf),
            "pdf" => Ok(Format::Pdf),
            _ => Err(All2mdError::UnsupportedFormat(s.to_string())),
        }
    }
}

const OLE2_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const RTF_MAGIC: &[u8] = b"{\\rtf";
const PDF_MAGIC: &[u8] = b"%PDF";

pub fn detect_format(data: &[u8]) -> Result<Format, All2mdError> {
    if data.len() < 8 {
        return Err(All2mdError::FileTooSmall);
    }

    if data.starts_with(RTF_MAGIC) {
        return Ok(Format::Rtf);
    }

    if data.starts_with(PDF_MAGIC) {
        return Ok(Format::Pdf);
    }

    if data.starts_with(&ZIP_MAGIC) {
        if zip_contains_entry(data, "word/document.xml") {
            return Ok(Format::Docx);
        }
        return Err(All2mdError::UnsupportedFormat("ZIP (not DOCX)".into()));
    }

    if data[..8] == OLE2_MAGIC {
        return Ok(Format::Doc);
    }

    Err(All2mdError::UnrecognizedFormat)
}

fn zip_contains_entry(data: &[u8], name: &str) -> bool {
    let cursor = std::io::Cursor::new(data);
    let Ok(mut archive) = zip::ZipArchive::new(cursor) else {
        return false;
    };
    let result = archive.by_name(name).is_ok();
    result
}
