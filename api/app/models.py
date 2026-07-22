"""Response schema.

These models exist to guarantee one thing: that this service returns the exact
shape `mirante-core` serialises. A divergence in field names or nesting would
make the side-by-side comparison meaningless, so the contract is declared here
rather than left to whatever a dict happened to contain.
"""

from typing import Literal

from pydantic import BaseModel, ConfigDict
from pydantic.alias_generators import to_camel

ColumnType = Literal["empty", "boolean", "integer", "float", "date", "text"]
DecimalStyle = Literal["dot", "comma"]
Encoding = Literal["utf8", "windows1252"]


class Schema(BaseModel):
    """serde renames Rust fields to camelCase; pydantic must do the same."""

    model_config = ConfigDict(alias_generator=to_camel, populate_by_name=True)


class NumericSummary(Schema):
    min: float | None
    max: float | None
    mean: float | None
    sum: float | None
    stddev: float | None


class Histogram(Schema):
    min: float
    max: float
    counts: list[int]


class ValueCount(Schema):
    value: str
    count: int


class TextSummary(Schema):
    min_length: int | None
    max_length: int | None
    mean_length: float | None
    distinct: int
    distinct_is_exact: bool
    top_values: list[ValueCount]


class TypeCounts(Schema):
    integer: int
    float: int
    boolean: int
    date: int


class ColumnProfile(Schema):
    name: str
    index: int
    type: ColumnType
    count: int
    null_count: int
    type_counts: TypeCounts
    decimal_style: DecimalStyle | None
    numeric: NumericSummary | None
    histogram: Histogram | None
    text: TextSummary


class Profile(Schema):
    byte_length: int
    encoding: Encoding
    delimiter: str
    row_count: int
    column_count: int
    ragged_row_count: int
    columns: list[ColumnProfile]


class Timings(Schema):
    """Server-side timings, in milliseconds.

    `profile_ms` is the number comparable to the Wasm engine's: it covers
    parsing and aggregation, and nothing else. Request overhead, multipart
    decoding and JSON serialisation are excluded, and the network time is the
    caller's to measure.
    """

    parse_ms: float
    aggregate_ms: float
    profile_ms: float


class ProfileResponse(Schema):
    profile: Profile
    timings: Timings
    engine: str
    versions: dict[str, str]
