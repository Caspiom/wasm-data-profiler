//! Input decoding: BOM stripping and windows-1252 transcoding.

use std::borrow::Cow;

use serde::Serialize;

/// Character encoding detected for the input file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Encoding {
    Utf8,
    Windows1252,
}

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The 0x80..=0x9F range is where windows-1252 differs from latin-1.
/// Every other byte maps to the code point of the same value.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}',
    '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
];

/// Strips a UTF-8 BOM if present.
pub fn strip_bom(input: &[u8]) -> &[u8] {
    input.strip_prefix(UTF8_BOM).unwrap_or(input)
}

/// Decodes the whole input to UTF-8 once, up front.
///
/// UTF-8 input is borrowed, so the common case copies nothing. windows-1252
/// input is transcoded into one owned buffer; that is a single allocation for
/// the file rather than one per cell, which is what the per-cell rule is about.
pub fn decode(input: &[u8]) -> (Cow<'_, str>, Encoding) {
    let input = strip_bom(input);
    match std::str::from_utf8(input) {
        Ok(s) => (Cow::Borrowed(s), Encoding::Utf8),
        Err(_) => (Cow::Owned(decode_windows1252(input)), Encoding::Windows1252),
    }
}

fn decode_windows1252(input: &[u8]) -> String {
    // Most bytes are ASCII and cost one byte out; the rest cost at most three.
    let mut out = String::with_capacity(input.len() + input.len() / 8);
    for &b in input {
        match b {
            0x00..=0x7F => out.push(b as char),
            0x80..=0x9F => out.push(CP1252_HIGH[(b - 0x80) as usize]),
            _ => out.push(b as char),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_is_stripped() {
        let (text, enc) = decode(b"\xEF\xBB\xBFname,age");
        assert_eq!(text, "name,age");
        assert_eq!(enc, Encoding::Utf8);
    }

    #[test]
    fn utf8_is_borrowed() {
        let (text, enc) = decode("preço".as_bytes());
        assert!(matches!(text, Cow::Borrowed(_)));
        assert_eq!(enc, Encoding::Utf8);
    }

    #[test]
    fn windows1252_accents_round_trip() {
        // "São Paulo" in windows-1252: 0xE3 is a-tilde.
        let (text, enc) = decode(b"S\xE3o Paulo");
        assert_eq!(text, "São Paulo");
        assert_eq!(enc, Encoding::Windows1252);
    }

    #[test]
    fn windows1252_smart_punctuation() {
        // 0x93/0x94 are curly quotes, which latin-1 does not have.
        let (text, _) = decode(b"\x93ol\xE1\x94");
        assert_eq!(text, "“olá”");
    }
}
