//! Per-cell classification.

use serde::Serialize;

/// The type inferred for a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColumnType {
    /// Every value was null or blank.
    Empty,
    Boolean,
    Integer,
    Float,
    Date,
    Text,
}

/// Tokens treated as missing data, compared case-insensitively.
const NULL_TOKENS: [&str; 7] = ["", "na", "n/a", "null", "nil", "none", "nan"];
const TRUE_TOKENS: [&str; 5] = ["true", "t", "yes", "sim", "verdadeiro"];
/// Comparison is ASCII-case-insensitive, which does not fold `ã`/`Ã`, so both
/// cased spellings of the accented form are listed rather than lowercasing
/// every cell.
const FALSE_TOKENS: [&str; 7] = ["false", "f", "no", "nao", "falso", "não", "nÃo"];

pub fn is_null(s: &str) -> bool {
    s.len() <= 4 && NULL_TOKENS.iter().any(|t| s.eq_ignore_ascii_case(t))
}

pub fn parse_bool(s: &str) -> Option<bool> {
    if TRUE_TOKENS.iter().any(|t| s.eq_ignore_ascii_case(t)) {
        Some(true)
    } else if FALSE_TOKENS.iter().any(|t| s.eq_ignore_ascii_case(t)) {
        Some(false)
    } else {
        None
    }
}

/// Recognises `YYYY-MM-DD`, `DD/MM/YYYY` and `DD-MM-YYYY`, with an optional
/// time part after a space or `T`.
///
/// This classifies only; no calendar arithmetic is done, so no date crate is
/// pulled in. Field ranges are checked, leap years are not.
pub fn looks_like_date(s: &str) -> bool {
    let date = s.split(['T', ' ']).next().unwrap_or(s);
    let b = date.as_bytes();
    match b.len() {
        10 => {}
        _ => return false,
    }
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(|c| c.is_ascii_digit());

    if b[4] == b'-' && b[7] == b'-' && digits(0..4) && digits(5..7) && digits(8..10) {
        return in_range(&date[5..7], 1, 12) && in_range(&date[8..10], 1, 31);
    }
    if (b[2] == b'/' || b[2] == b'-')
        && b[2] == b[5]
        && digits(0..2)
        && digits(3..5)
        && digits(6..10)
    {
        return in_range(&date[0..2], 1, 31) && in_range(&date[3..5], 1, 12);
    }
    false
}

fn in_range(s: &str, lo: u32, hi: u32) -> bool {
    s.parse::<u32>().is_ok_and(|v| v >= lo && v <= hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_tokens() {
        for s in ["", "NA", "n/a", "NULL", "None", "nan"] {
            assert!(is_null(s), "{s}");
        }
        for s in ["0", "-", "nada", "n/a/b"] {
            assert!(!is_null(s), "{s}");
        }
    }

    #[test]
    fn booleans_including_portuguese() {
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("sim"), Some(true));
        assert_eq!(parse_bool("Não"), Some(false));
        assert_eq!(parse_bool("nao"), Some(false));
        assert_eq!(parse_bool("1"), None);
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn dates() {
        assert!(looks_like_date("2026-07-22"));
        assert!(looks_like_date("22/07/2026"));
        assert!(looks_like_date("22-07-2026"));
        assert!(looks_like_date("2026-07-22T10:30:00"));
        assert!(looks_like_date("2026-07-22 10:30:00"));
    }

    #[test]
    fn non_dates() {
        for s in [
            "2026-13-01",
            "32/01/2026",
            "2026-7-2",
            "hoje",
            "",
            "2026/07/22x",
        ] {
            assert!(!looks_like_date(s), "{s}");
        }
    }
}
