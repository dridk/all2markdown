use crate::failure::Failure;
use encoding_rs::{Encoding, UTF_8, WINDOWS_1252};

/// How many bytes the decodability test looks at.
///
/// A file that is text at all is text in its first few kilobytes, and reading
/// further would make the test cost proportional to the corpus rather than to
/// the number of documents in it.
const SNIFF_BYTES: usize = 8192;

/// Above this proportion of control characters, bytes are binary rather than
/// text. Loose on purpose: the test only has to separate a stray log file from
/// a format nobody registered a Parser for.
const MAX_CONTROL_RATIO: f32 = 0.05;

/// How many non-ASCII bytes a statistical guess needs before it counts as
/// evidence rather than a coin flip.
///
/// windows-1252, ISO-8859-7 and KOI8-R all map byte 0xE9 to a different
/// letter, and nothing in one accented word says which. Below this, the guess
/// is reported but flagged.
const MIN_EVIDENCE_BYTES: usize = 8;

/// Text read out of bytes, with the encoding it was read from.
pub(crate) struct Decoded {
    pub text: String,
    /// The encoding's canonical name, as the Extraction reports it.
    pub encoding: &'static str,
    /// False when the encoding was guessed statistically and the guess is
    /// shaky. A shaky guess is a Warning rather than a Failure: the text is
    /// probably right, and the caller is the one who can tell.
    pub confident: bool,
}

/// Decode bytes to text, guessing the encoding when the caller does not say.
///
/// The cascade: an explicit override, then a byte-order mark, then valid
/// UTF-8, then statistical detection, then Windows-1252 — which maps every
/// byte and so can never fail, and is what keeps a legacy corpus readable
/// rather than lost.
pub(crate) fn decode(bytes: &[u8], forced: Option<&str>) -> Result<Decoded, Failure> {
    if let Some(label) = forced {
        let encoding = Encoding::for_label(label.as_bytes())
            .ok_or_else(|| Failure::UnknownEncoding(label.to_owned()))?;
        let (text, _) = encoding.decode_with_bom_removal(bytes);
        return Ok(Decoded {
            text: text.into_owned(),
            encoding: encoding.name(),
            confident: true,
        });
    }

    if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
        let (text, _) = encoding.decode_without_bom_handling(&bytes[bom_len..]);
        return Ok(Decoded {
            text: text.into_owned(),
            encoding: encoding.name(),
            confident: true,
        });
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(Decoded {
            text: text.to_owned(),
            encoding: UTF_8.name(),
            confident: true,
        });
    }

    // chardetng always answers, and cannot say how much evidence it had:
    // `guess_assess` exists for that but returns `max >= 0` on a counter that
    // starts at 0, so it is `true` unconditionally. The judgement is ours.
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let guess = detector.guess(None, true);

    let (text, had_errors) = guess.decode_without_bom_handling(bytes);
    if had_errors {
        // The guess does not even decode its own input. Windows-1252 maps
        // every byte, so falling back to it loses no text — only certainty.
        let (text, _) = WINDOWS_1252.decode_without_bom_handling(bytes);
        return Ok(Decoded {
            text: text.into_owned(),
            encoding: WINDOWS_1252.name(),
            confident: false,
        });
    }

    let evidence = bytes
        .iter()
        .filter(|b| **b >= 0x80)
        .take(MIN_EVIDENCE_BYTES)
        .count();
    Ok(Decoded {
        text: text.into_owned(),
        encoding: guess.name(),
        confident: evidence >= MIN_EVIDENCE_BYTES,
    })
}

/// Whether these bytes read as text rather than as an unrecognised binary.
///
/// This is the whole of the last-resort stage: what it answers decides
/// between reading a file as plain text and refusing it, so it stays a
/// question about the bytes and never about the name.
pub(crate) fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    // A byte-order mark settles it before any statistics: UTF-16 text is full
    // of null bytes, which the control-character test would reject.
    if Encoding::for_bom(bytes).is_some() {
        return true;
    }

    let head = &bytes[..bytes.len().min(SNIFF_BYTES)];
    if head.contains(&0) {
        return false;
    }

    let Ok(decoded) = decode(head, None) else {
        return false;
    };
    let mut total = 0usize;
    let mut control = 0usize;
    for c in decoded.text.chars() {
        total += 1;
        if c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\u{0c}') {
            control += 1;
        }
        // U+FFFD is what a decoder emits where it gave up; it is as good a
        // sign of binary as a control character.
        if c == '\u{fffd}' {
            control += 1;
        }
    }
    if total == 0 {
        return false;
    }
    (control as f32 / total as f32) <= MAX_CONTROL_RATIO
}
