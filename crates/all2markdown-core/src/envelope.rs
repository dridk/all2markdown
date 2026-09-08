use crate::failure::Failure;
use std::io::{self, Write};

/// A compression wrapper around exactly one Source Document.
///
/// Not a Format: an Envelope says nothing about what it holds, and the whole
/// point of stripping it before detection is that the Supported Format is the
/// content's and never the wrapper's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Envelope {
    Gzip,
    Zstd,
    Xz,
    Bzip2,
}

impl Envelope {
    /// The signature each format opens with, and the extension that names it.
    const ALL: [(Envelope, &'static [u8], &'static str); 4] = [
        (Envelope::Gzip, &[0x1F, 0x8B], "gz"),
        (Envelope::Zstd, &[0x28, 0xB5, 0x2F, 0xFD], "zst"),
        (Envelope::Xz, &[0xFD, b'7', b'z', b'X', b'Z', 0x00], "xz"),
        (Envelope::Bzip2, b"BZh", "bz2"),
    ];

    fn of(bytes: &[u8]) -> Option<Envelope> {
        Self::ALL
            .iter()
            .find(|(_, magic, _)| bytes.starts_with(magic))
            .map(|(envelope, _, _)| *envelope)
    }

    fn name(self) -> &'static str {
        match self {
            Envelope::Gzip => "gzip",
            Envelope::Zstd => "zstd",
            Envelope::Xz => "xz",
            Envelope::Bzip2 => "bzip2",
        }
    }
}

/// Strip one Envelope, or report that there was none.
///
/// `Ok(None)` means the bytes were never compressed and are already the
/// Source Document; the caller reads them as they are rather than copying
/// them, which is what keeps an uncompressed corpus free of an extra pass.
pub(crate) fn peel(bytes: &[u8], max_size: u64) -> Result<Option<Vec<u8>>, Failure> {
    let Some(envelope) = Envelope::of(bytes) else {
        return Ok(None);
    };

    let content = decompress(envelope, bytes, max_size)?;

    // Exactly one Envelope deep. Nesting is refused rather than followed:
    // depth is what turns a size cap into a suggestion, and an archive of
    // archives breaks the one-document-one-Extraction rule anyway.
    if Envelope::of(&content).is_some() {
        return Err(Failure::NestedEnvelope);
    }

    Ok(Some(content))
}

/// The name with its Envelope extension removed, so that the extension stage
/// of detection sees `report.doc` where the filesystem holds `report.doc.gz`.
pub(crate) fn strip_extension(name: &str) -> &str {
    for (_, _, extension) in Envelope::ALL {
        if let Some(stem) = name.strip_suffix(extension) {
            if let Some(stem) = stem.strip_suffix('.') {
                if !stem.is_empty() {
                    return stem;
                }
            }
        }
    }
    name
}

fn decompress(envelope: Envelope, bytes: &[u8], max_size: u64) -> Result<Vec<u8>, Failure> {
    let mut sink = Capped::new(max_size);

    let outcome = match envelope {
        Envelope::Gzip => io::copy(&mut flate2::read::GzDecoder::new(bytes), &mut sink).map(|_| ()),
        Envelope::Zstd => match ruzstd::decoding::StreamingDecoder::new(bytes) {
            Ok(mut decoder) => io::copy(&mut decoder, &mut sink).map(|_| ()),
            Err(e) => Err(io::Error::other(e.to_string())),
        },
        Envelope::Xz => {
            let mut input = bytes;
            lzma_rs::xz_decompress(&mut input, &mut sink)
                .map_err(|e| io::Error::other(e.to_string()))
        }
        Envelope::Bzip2 => {
            io::copy(&mut bzip2_rs::DecoderReader::new(bytes), &mut sink).map(|_| ())
        }
    };

    // The cap is checked on the way through rather than afterwards, so a
    // decompression bomb is refused while it inflates instead of after it has
    // already taken the memory.
    if sink.exceeded {
        return Err(Failure::TooLarge { limit: max_size });
    }
    outcome.map_err(|e| Failure::Decompress {
        envelope: envelope.name(),
        message: e.to_string(),
    })?;

    Ok(sink.buffer)
}

/// A sink that refuses to grow past a limit.
///
/// Capping the *compressed* size would be no protection at all: a few hundred
/// bytes of gzip expand to gigabytes. So the limit lives here, where the
/// decompressed bytes actually arrive, and the buffer never holds more than
/// it.
struct Capped {
    buffer: Vec<u8>,
    limit: u64,
    exceeded: bool,
}

impl Capped {
    fn new(limit: u64) -> Self {
        Self {
            buffer: Vec::new(),
            limit,
            exceeded: false,
        }
    }
}

impl Write for Capped {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.buffer.len() as u64 + buf.len() as u64 > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("decompressed size cap exceeded"));
        }
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
