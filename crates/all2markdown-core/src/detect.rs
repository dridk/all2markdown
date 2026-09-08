use crate::extraction::SourceDocument;
use crate::failure::Failure;
use crate::format::Format;

const OLE2_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const RTF_MAGIC: &[u8] = b"{\\rtf";
const PDF_MAGIC: &[u8] = b"%PDF";

/// Identify a Source Document from its content.
///
/// Milestone 1 step 2 replaces this with a poll of the Parser registry, so that
/// a new format stops needing an edit here. Step 3 adds the extension and
/// plain-text stages of the cascade.
pub fn detect_format(source: &SourceDocument<'_>) -> Result<Format, Failure> {
    let data = source.bytes;

    if data.len() < 8 {
        return Err(Failure::FileTooSmall);
    }

    if data.starts_with(RTF_MAGIC) {
        return Ok(Format::RTF);
    }

    if data.starts_with(PDF_MAGIC) {
        return Ok(Format::PDF);
    }

    if data.starts_with(&ZIP_MAGIC) {
        if zip_contains_entry(data, "word/document.xml") {
            return Ok(Format::DOCX);
        }
        return Err(Failure::UnsupportedFormat("ZIP (not DOCX)".into()));
    }

    if data[..8] == OLE2_MAGIC {
        return Ok(Format::DOC);
    }

    Err(Failure::UnrecognizedFormat)
}

fn zip_contains_entry(data: &[u8], name: &str) -> bool {
    let cursor = std::io::Cursor::new(data);
    let Ok(mut archive) = zip::ZipArchive::new(cursor) else {
        return false;
    };
    // Bound rather than returned directly: the ZipFile borrows `archive`,
    // and as a tail expression it would outlive it.
    let found = archive.by_name(name).is_ok();
    found
}
