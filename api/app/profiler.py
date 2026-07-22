"""The pandas implementation of the same profile.

This is the comparison baseline, so it has to be honest in both directions: it
must do the same work the Rust engine does — no more, no less — and it must do
it the way someone would actually write it in pandas, with vectorised
operations rather than a Python loop over rows.

Columns are read as strings and classified with vectorised regex matching,
because that is the only way to reproduce the engine's semantics: strict type
inference, a per-column decimal convention, and Brazilian null and boolean
tokens. Letting `read_csv` infer dtypes natively would be faster and would also
be a different computation, which would make the benchmark a lie.

Known divergences from the Rust engine, all deliberate:

- Distinct counts here are exact. The Rust engine caps its frequency table to
  bound memory on high-cardinality columns and reports the count as a floor.
  pandas therefore does slightly *more* work on wide text columns, which
  favours Rust; the README says so.
- A row with more fields than the header makes the fast C parser fail, so the
  file is re-read with the slower Python parser. Well-formed files never pay
  that cost.
"""

from __future__ import annotations

import re
from io import StringIO
from time import perf_counter

import numpy as np
import pandas as pd

from app import models
from app.dialect import decode, sniff_delimiter

# Which string backend read_csv produces. Arrow-backed strings run the `.str`
# operations as C kernels; the Python-backed default runs them one element at a
# time, which would understate what pandas can do.
STRING_DTYPE = "str"

HISTOGRAM_BINS = 24
TOP_VALUES_REPORTED = 10

NULL_TOKENS = frozenset({"", "na", "n/a", "null", "nil", "none", "nan"})
TRUE_TOKENS = frozenset({"true", "t", "yes", "sim", "verdadeiro"})
FALSE_TOKENS = frozenset({"false", "f", "no", "nao", "não", "falso"})
BOOL_TOKENS = TRUE_TOKENS | FALSE_TOKENS

MAX_BOOL_LENGTH = 10
DATE_LENGTH = 10

# Only values that actually contain a group separator are matched against
# these; the separator has to sit in a real thousands position.
DOT_GROUPED_RE = re.compile(r"[+-]?\d{1,3}(?:,\d{3})+(?:\.\d*)?(?:[eE][+-]?\d+)?")
COMMA_GROUPED_RE = re.compile(r"[+-]?\d{1,3}(?:\.\d{3})+(?:,\d+)?")
ISO_DATE_RE = re.compile(r"\d{4}-\d{2}-\d{2}")
DMY_DATE_RE = re.compile(r"(\d{2})([/-])(\d{2})\2(\d{4})")


class RaggedCounter:
    """Truncates over-long rows and counts them, like the Rust reader does."""

    def __init__(self, width: int) -> None:
        self.width = width
        self.count = 0

    def __call__(self, fields: list[str]) -> list[str]:
        self.count += 1
        return fields[: self.width]


def profile_csv(raw: bytes) -> tuple[models.Profile, models.Timings]:
    """Profile a CSV file held in memory."""
    parse_start = perf_counter()
    text, encoding = decode(raw)
    if not text.strip():
        raise ValueError("the file is empty")
    delimiter = sniff_delimiter(text)
    frame, long_rows = _read(text, delimiter)
    parse_ms = (perf_counter() - parse_start) * 1000

    aggregate_start = perf_counter()
    if frame.empty:
        raise ValueError("the header row declares no columns")

    names = _header_names(frame.iloc[0])
    body = frame.iloc[1:]
    # A missing trailing field arrives as NaN; a present but empty one is "".
    short_rows = int(body.isna().any(axis=1).sum()) if len(body) else 0

    columns = [
        _profile_column(index, name, body.iloc[:, index]) for index, name in enumerate(names)
    ]
    aggregate_ms = (perf_counter() - aggregate_start) * 1000

    profile = models.Profile(
        byte_length=len(raw),
        encoding=encoding,
        delimiter=delimiter,
        row_count=len(body),
        column_count=len(names),
        ragged_row_count=short_rows + long_rows,
        columns=columns,
    )
    timings = models.Timings(
        parse_ms=parse_ms,
        aggregate_ms=aggregate_ms,
        profile_ms=parse_ms + aggregate_ms,
    )
    return profile, timings


def _read(text: str, delimiter: str) -> tuple[pd.DataFrame, int]:
    """Read every field as a string, header included.

    `header=None` is deliberate: pandas would rename duplicate and blank header
    fields, and the Rust engine does not.
    """
    options = {
        "sep": delimiter,
        "header": None,
        "dtype": STRING_DTYPE,
        "keep_default_na": False,
        "na_filter": False,
        "skip_blank_lines": True,
        "quotechar": '"',
    }
    try:
        return pd.read_csv(StringIO(text), engine="c", **options), 0
    except pd.errors.ParserError:
        # Some row is wider than the header. Only the Python engine can hand
        # the offending fields back to us instead of giving up.
        width = pd.read_csv(StringIO(text), engine="c", nrows=1, **options).shape[1]
        counter = RaggedCounter(width)
        frame = pd.read_csv(StringIO(text), engine="python", on_bad_lines=counter, **options)
        return frame, counter.count


def _header_names(row: pd.Series) -> list[str]:
    names = []
    for index, value in enumerate(row):
        name = "" if pd.isna(value) else str(value).strip()
        names.append(name or f"column_{index}")
    return names


def _profile_column(index: int, name: str, raw: pd.Series) -> models.ColumnProfile:
    series = raw.fillna("").astype(str).str.strip()
    lowered = series.str.lower()
    keep = ~lowered.isin(NULL_TOKENS)
    values = series[keep]
    non_null = len(values)

    dot = _parse_dot(values)
    comma = _parse_comma(values)
    dot_count = int(dot.notna().sum())
    comma_count = int(comma.notna().sum())
    int_count = _count_integers(values, dot)
    bool_count = _count_booleans(values, lowered[keep])
    date_count = _count_dates(values)

    column_type = _infer_type(non_null, bool_count, int_count, dot_count, comma_count, date_count)
    is_numeric = column_type in ("integer", "float")
    style = "comma" if comma_count > dot_count else "dot"

    numbers = (comma if style == "comma" else dot).dropna() if is_numeric else None

    return models.ColumnProfile(
        name=name,
        index=index,
        type=column_type,
        count=non_null,
        null_count=len(series) - non_null,
        type_counts=models.TypeCounts(
            integer=int_count,
            float=max(dot_count, comma_count),
            boolean=bool_count,
            date=date_count,
        ),
        decimal_style=style if is_numeric else None,
        numeric=_numeric_summary(numbers) if numbers is not None else None,
        histogram=_histogram(numbers) if numbers is not None else None,
        text=_text_summary(values),
    )


def _infer_type(
    non_null: int, boolean: int, integer: int, dot: int, comma: int, date: int
) -> models.ColumnType:
    """A column takes a type only when every non-null value fits it."""
    if non_null == 0:
        return "empty"
    if boolean == non_null:
        return "boolean"
    if integer == non_null:
        return "integer"
    if dot == non_null or comma == non_null:
        return "float"
    if date == non_null:
        return "date"
    return "text"


def _parse_dot(values: pd.Series) -> pd.Series:
    """Values read as `1,234.56`, with NaN where the text does not parse.

    Anything without a group separator goes straight to `to_numeric`, which
    parses in C and accepts exactly what Rust's `str::parse::<f64>` accepts,
    exponents included. Only the minority of values that actually contain a
    comma pay for a regex, and there the comma must sit in a real thousands
    position — otherwise `3,5` would be read as thirty-five.
    """
    out = _empty_floats(values)
    if values.empty:
        return out
    grouped = values.str.contains(",", regex=False)
    _fill(out, values[~grouped])
    _fill_grouped(out, values[grouped], DOT_GROUPED_RE, ",", "")
    return out.where(np.isfinite(out))


def _parse_comma(values: pd.Series) -> pd.Series:
    """Values read as `1.234,56`, with NaN where the text does not parse."""
    out = _empty_floats(values)
    if values.empty:
        return out
    # A dot means a possible thousands group; a trailing comma is a fractional
    # part with no digits, which neither engine accepts.
    tricky = values.str.contains(".", regex=False) | values.str.endswith(",")
    plain = values[~tricky]
    _fill(out, plain, swap=(",", "."))
    _fill_grouped(out, values[tricky], COMMA_GROUPED_RE, ".", "", swap=(",", "."))
    return out.where(np.isfinite(out))


def _empty_floats(values: pd.Series) -> pd.Series:
    return pd.Series(np.nan, index=values.index, dtype="float64")


def _fill(out: pd.Series, subset: pd.Series, swap: tuple[str, str] | None = None) -> None:
    if subset.empty:
        return
    if swap:
        subset = subset.str.replace(swap[0], swap[1], regex=False)
    out.loc[subset.index] = pd.to_numeric(subset, errors="coerce")


def _fill_grouped(
    out: pd.Series,
    subset: pd.Series,
    pattern: re.Pattern[str],
    group: str,
    replacement: str,
    swap: tuple[str, str] | None = None,
) -> None:
    if subset.empty:
        return
    valid = subset[subset.str.fullmatch(pattern)]
    if valid.empty:
        return
    _fill(out, valid.str.replace(group, replacement, regex=False), swap)


def _count_integers(values: pd.Series, dot: pd.Series) -> int:
    """Integers are the numbers written with digits and a sign, nothing else.

    Derived from the parse that already ran rather than from another regex
    pass: a value is an integer when it parsed and contains no separator and
    no exponent.
    """
    if values.empty:
        return 0
    plain = ~values.str.contains(r"[.,eE]", regex=True)
    return int((dot.notna() & plain).sum())


def _count_booleans(values: pd.Series, lowered: pd.Series) -> int:
    """Only short values can be boolean tokens, so most columns skip the test."""
    if values.empty:
        return 0
    candidates = lowered[values.str.len() <= MAX_BOOL_LENGTH]
    if candidates.empty:
        return 0
    return int(candidates.isin(BOOL_TOKENS).sum())


def _count_dates(values: pd.Series) -> int:
    """Recognise `YYYY-MM-DD` and `DD/MM/YYYY`, with an optional time part.

    Field ranges are checked; the calendar is not, which is what the Rust side
    does too. No date library is involved on either side. Values too short to
    be a date are filtered out first, in C, so numeric columns cost nothing.
    """
    if values.empty:
        return 0
    candidates = values[values.str.len() >= DATE_LENGTH]
    if candidates.empty:
        return 0

    heads = candidates.str.slice(0, DATE_LENGTH)
    iso = heads.str.fullmatch(ISO_DATE_RE)
    iso_valid = iso & _in_range(heads.str[5:7], 1, 12) & _in_range(heads.str[8:10], 1, 31)

    dmy = heads.str.fullmatch(DMY_DATE_RE)
    dmy_valid = dmy & _in_range(heads.str[0:2], 1, 31) & _in_range(heads.str[3:5], 1, 12)

    # A longer value is only a date if what follows the date is a time part.
    tail_ok = candidates.str.len().eq(DATE_LENGTH) | candidates.str.slice(
        DATE_LENGTH, DATE_LENGTH + 1
    ).isin(("T", " "))
    return int(((iso_valid | dmy_valid) & tail_ok).sum())


def _in_range(part: pd.Series, low: int, high: int) -> pd.Series:
    numbers = pd.to_numeric(part, errors="coerce")
    return numbers.between(low, high).fillna(False)


def _numeric_summary(numbers: pd.Series) -> models.NumericSummary:
    if numbers.empty:
        return models.NumericSummary(min=None, max=None, mean=None, sum=None, stddev=None)
    # ddof=1 is the sample standard deviation, matching Welford on the Rust side.
    stddev = float(numbers.std(ddof=1)) if len(numbers) > 1 else None
    return models.NumericSummary(
        min=float(numbers.min()),
        max=float(numbers.max()),
        mean=float(numbers.mean()),
        sum=float(numbers.sum()),
        stddev=stddev,
    )


def _histogram(numbers: pd.Series) -> models.Histogram | None:
    if numbers.empty:
        return None
    low = float(numbers.min())
    high = float(numbers.max())
    if high <= low:
        # A constant column collapses to a single bin.
        counts = [len(numbers)] + [0] * (HISTOGRAM_BINS - 1)
    else:
        # np.histogram closes the last bin on the right, as the Rust binning does.
        counts = np.histogram(numbers, bins=HISTOGRAM_BINS, range=(low, high))[0].tolist()
    return models.Histogram(min=low, max=high, counts=[int(c) for c in counts])


def _text_summary(values: pd.Series) -> models.TextSummary:
    if values.empty:
        return models.TextSummary(
            min_length=None,
            max_length=None,
            mean_length=None,
            distinct=0,
            distinct_is_exact=True,
            top_values=[],
        )

    lengths = values.str.len()
    counts = values.value_counts()
    # Descending by count, then by value, so ties are deterministic on both sides.
    ordered = (
        counts.rename_axis("value")
        .reset_index(name="count")
        .sort_values(["count", "value"], ascending=[False, True])
        .head(TOP_VALUES_REPORTED)
    )

    return models.TextSummary(
        min_length=int(lengths.min()),
        max_length=int(lengths.max()),
        mean_length=float(lengths.mean()),
        distinct=int(counts.size),
        distinct_is_exact=True,
        top_values=[
            models.ValueCount(value=str(row.value), count=int(row.count))
            for row in ordered.itertuples()
        ],
    )
