//! Number parsing for both the `1,234.56` and the `1.234,56` conventions.

use serde::Serialize;

/// Which character separates the fractional part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DecimalStyle {
    /// `1,234.56` — dot decimal, comma grouping.
    Dot,
    /// `1.234,56` — comma decimal, dot grouping.
    Comma,
}

impl DecimalStyle {
    fn separators(self) -> (char, char) {
        match self {
            DecimalStyle::Dot => ('.', ','),
            DecimalStyle::Comma => (',', '.'),
        }
    }
}

/// Parses an integer. Deliberately strict: sign and digits only.
///
/// Grouped values like `1.234` stay ambiguous between conventions, so they are
/// left for [`parse_float`] to resolve once the column's style is known.
pub fn parse_int(s: &str) -> Option<i64> {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.strip_prefix('+').unwrap_or(s).parse::<i64>().ok()
}

/// Parses a decimal number under the given convention.
///
/// Group separators are accepted only where they form real thousands groups,
/// which is what keeps `3,5` from being read as `35` under [`DecimalStyle::Dot`].
pub fn parse_float(s: &str, style: DecimalStyle, scratch: &mut String) -> Option<f64> {
    let (decimal, group) = style.separators();

    // Reject `inf`, `NaN` and friends: a number must start like one.
    let first = s.as_bytes().first()?;
    if !(first.is_ascii_digit() || *first == b'+' || *first == b'-' || *first == decimal as u8) {
        return None;
    }

    let (sign, body) = match s.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", s.strip_prefix('+').unwrap_or(s)),
    };

    // Fast path: nothing to rewrite, so `str::parse` handles exponents too.
    if style == DecimalStyle::Dot && !body.contains(group) {
        let value = s.strip_prefix('+').unwrap_or(s).parse::<f64>().ok()?;
        return value.is_finite().then_some(value);
    }

    let mut parts = body.splitn(2, decimal);
    let int_part = parts.next()?;
    let frac_part = parts.next();
    if body.matches(decimal).count() > 1 {
        return None;
    }
    if let Some(frac) = frac_part
        && (frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }

    scratch.clear();
    scratch.push_str(sign);
    push_ungrouped(int_part, group, scratch)?;
    if let Some(frac) = frac_part {
        scratch.push('.');
        scratch.push_str(frac);
    }

    let value = scratch.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

/// Appends `int_part`'s digits to `out`, accepting `group` only in valid
/// thousands positions: 1-3 digits, then groups of exactly 3.
fn push_ungrouped(int_part: &str, group: char, out: &mut String) -> Option<()> {
    if int_part.is_empty() {
        // Allow a bare `.5`.
        out.push('0');
        return Some(());
    }
    if !int_part.contains(group) {
        if !int_part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        out.push_str(int_part);
        return Some(());
    }

    let mut groups = int_part.split(group);
    let head = groups.next()?;
    if head.is_empty() || head.len() > 3 || !head.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    out.push_str(head);
    for g in groups {
        if g.len() != 3 || !g.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        out.push_str(g);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::DecimalStyle::{Comma, Dot};
    use super::*;

    fn f(s: &str, style: DecimalStyle) -> Option<f64> {
        parse_float(s, style, &mut String::new())
    }

    #[test]
    fn plain_integers() {
        assert_eq!(parse_int("42"), Some(42));
        assert_eq!(parse_int("-7"), Some(-7));
        assert_eq!(parse_int("+7"), Some(7));
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int("-"), None);
        assert_eq!(parse_int("1.234"), None);
        assert_eq!(parse_int("1e5"), None);
    }

    #[test]
    fn brazilian_decimals() {
        assert_eq!(f("3,5", Comma), Some(3.5));
        assert_eq!(f("1.234,56", Comma), Some(1234.56));
        assert_eq!(f("-1.234.567,89", Comma), Some(-1234567.89));
    }

    #[test]
    fn anglo_decimals() {
        assert_eq!(f("3.5", Dot), Some(3.5));
        assert_eq!(f("1,234.56", Dot), Some(1234.56));
        assert_eq!(f("1e5", Dot), Some(100000.0));
    }

    #[test]
    fn comma_decimal_is_not_read_as_grouping() {
        // The whole point: `3,5` is not thirty-five under either convention.
        assert_eq!(f("3,5", Dot), None);
        assert_eq!(f("3.5", Comma), None);
    }

    #[test]
    fn malformed_grouping_is_rejected() {
        assert_eq!(f("1,23,456", Dot), None);
        assert_eq!(f("1234,567", Dot), None);
        assert_eq!(f("1.23.456", Comma), None);
    }

    #[test]
    fn non_numbers_are_rejected() {
        for s in [
            "", "abc", "NaN", "inf", "-inf", "1,,2", "1.2.3", "12,", "R$ 10",
        ] {
            assert_eq!(f(s, Dot), None, "dot: {s}");
            assert_eq!(f(s, Comma), None, "comma: {s}");
        }
    }
}
