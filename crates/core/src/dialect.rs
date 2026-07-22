//! Delimiter sniffing.

/// Candidates in preference order; ties are broken by this order.
const CANDIDATES: [u8; 4] = [b',', b';', b'\t', b'|'];

/// Picks the delimiter by counting candidates in the header line.
///
/// Counting only outside quotes matters for headers like `"last, first";age`,
/// where the naive count would elect the comma.
pub fn sniff_delimiter(text: &str) -> u8 {
    let header = first_line(text.as_bytes());
    let mut counts = [0usize; CANDIDATES.len()];
    let mut in_quotes = false;

    for &b in header {
        if b == b'"' {
            in_quotes = !in_quotes;
            continue;
        }
        if in_quotes {
            continue;
        }
        if let Some(i) = CANDIDATES.iter().position(|&c| c == b) {
            counts[i] += 1;
        }
    }

    let mut best = 0;
    for i in 1..CANDIDATES.len() {
        if counts[i] > counts[best] {
            best = i;
        }
    }
    // A single-column file has no delimiter at all; comma is the harmless default.
    if counts[best] == 0 {
        b','
    } else {
        CANDIDATES[best]
    }
}

/// The first line, respecting quoted newlines.
fn first_line(bytes: &[u8]) -> &[u8] {
    let mut in_quotes = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b'\n' | b'\r' if !in_quotes => return &bytes[..i],
            _ => {}
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_comma() {
        assert_eq!(sniff_delimiter("a,b,c\n1,2,3"), b',');
    }

    #[test]
    fn detects_semicolon() {
        assert_eq!(sniff_delimiter("nome;idade;cidade\nAna;30;Recife"), b';');
    }

    #[test]
    fn detects_tab() {
        assert_eq!(sniff_delimiter("a\tb\tc\n1\t2\t3"), b'\t');
    }

    #[test]
    fn ignores_delimiters_inside_quotes() {
        assert_eq!(
            sniff_delimiter("\"last, first\";age\n\"Silva, Ana\";30"),
            b';'
        );
    }

    #[test]
    fn single_column_defaults_to_comma() {
        assert_eq!(sniff_delimiter("value\n1\n2"), b',');
    }

    #[test]
    fn only_the_header_line_is_considered() {
        // Commas in the body must not outvote the header's semicolons.
        assert_eq!(sniff_delimiter("a;b\n1,1;2,2\n3,3;4,4"), b';');
    }
}
