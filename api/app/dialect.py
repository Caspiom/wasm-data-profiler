"""Decoding and delimiter sniffing.

Deliberately a mirror of `crates/core/src/decode.rs` and `dialect.rs`. If the
two disagree about the delimiter or the encoding they are not profiling the
same file, and the comparison says nothing.
"""

CANDIDATES = (",", ";", "\t", "|")


def decode(raw: bytes) -> tuple[str, str]:
    """Decode to text, reporting which encoding was used.

    UTF-8 is tried first, exactly as the Rust side does; anything that fails
    validation is treated as windows-1252, which cannot fail.
    """
    if raw.startswith(b"\xef\xbb\xbf"):
        raw = raw[3:]
    try:
        return raw.decode("utf-8"), "utf8"
    except UnicodeDecodeError:
        return raw.decode("cp1252", errors="replace"), "windows1252"


def sniff_delimiter(text: str) -> str:
    """Pick the delimiter by counting candidates in the header line.

    Counting outside quotes only, so a header like `"last, first";age` is not
    read as comma-separated. Ties go to the earliest candidate.
    """
    header = _first_line(text)
    counts = []
    for candidate in CANDIDATES:
        total = 0
        in_quotes = False
        for char in header:
            if char == '"':
                in_quotes = not in_quotes
            elif char == candidate and not in_quotes:
                total += 1
        counts.append(total)

    best = max(range(len(CANDIDATES)), key=lambda i: (counts[i], -i))
    # A single-column file has no delimiter; comma is the harmless default.
    return CANDIDATES[best] if counts[best] else ","


def _first_line(text: str) -> str:
    in_quotes = False
    for i, char in enumerate(text):
        if char == '"':
            in_quotes = not in_quotes
        elif char in "\r\n" and not in_quotes:
            return text[:i]
    return text
