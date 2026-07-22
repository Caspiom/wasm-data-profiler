//! Profile orchestration: two passes over the decoded buffer.
//!
//! Pass one infers each column's type and accumulates its summary statistics.
//! Pass two fills the histograms, which need the min/max that pass one found.
//! Reparsing costs a second scan but keeps memory flat: no column ever holds
//! its own values.

use serde::Serialize;

use crate::decode::{self, Encoding};
use crate::dialect;
use crate::number::{self, DecimalStyle};
use crate::reader::{self, Record};
use crate::stats::{Histogram, NumericAccumulator, NumericSummary, TextAccumulator, TextSummary};
use crate::value::{self, ColumnType};

/// Why a file could not be profiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    /// The input had no bytes, or only whitespace.
    EmptyInput,
    /// A header row was found but it declared no columns.
    NoColumns,
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::EmptyInput => f.write_str("the file is empty"),
            ProfileError::NoColumns => f.write_str("the header row declares no columns"),
        }
    }
}

impl std::error::Error for ProfileError {}

/// The complete profile of one CSV file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub byte_length: usize,
    pub encoding: Encoding,
    /// The detected field separator, as a display character.
    pub delimiter: String,
    pub row_count: u64,
    pub column_count: usize,
    /// Rows whose field count differed from the header's. They are still
    /// profiled, up to the number of columns the header declares.
    pub ragged_row_count: u64,
    pub columns: Vec<ColumnProfile>,
}

/// One column's inferred type and statistics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnProfile {
    pub name: String,
    pub index: usize,
    #[serde(rename = "type")]
    pub column_type: ColumnType,
    /// Values that were present and not a null token.
    pub count: u64,
    pub null_count: u64,
    /// How each value was read; useful for seeing why a column stayed text.
    pub type_counts: TypeCounts,
    /// Set for numeric columns only.
    pub decimal_style: Option<DecimalStyle>,
    pub numeric: Option<NumericSummary>,
    pub histogram: Option<Histogram>,
    /// Set for every column: length stats and the most frequent values.
    pub text: TextSummary,
}

/// How many non-null values parsed as each type. A value can be counted more
/// than once — `1` is an integer and a float — so these do not sum to `count`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeCounts {
    pub integer: u64,
    pub float: u64,
    pub boolean: u64,
    pub date: u64,
}

/// Per-column state for pass one.
///
/// Both decimal conventions are accumulated at once because the column's
/// convention is only known after every value has been seen.
#[derive(Default)]
struct ColumnState {
    non_null: u64,
    null: u64,
    int: u64,
    float_dot: u64,
    float_comma: u64,
    boolean: u64,
    date: u64,
    acc_dot: NumericAccumulator,
    acc_comma: NumericAccumulator,
    text: TextAccumulator,
}

impl ColumnState {
    fn observe(&mut self, s: &str, scratch: &mut String) {
        if value::is_null(s) {
            self.null += 1;
            return;
        }
        self.non_null += 1;
        self.text.push(s);

        if let Some(i) = number::parse_int(s) {
            self.int += 1;
            self.float_dot += 1;
            self.float_comma += 1;
            self.acc_dot.push(i as f64);
            self.acc_comma.push(i as f64);
        } else {
            if let Some(v) = number::parse_float(s, DecimalStyle::Dot, scratch) {
                self.float_dot += 1;
                self.acc_dot.push(v);
            }
            if let Some(v) = number::parse_float(s, DecimalStyle::Comma, scratch) {
                self.float_comma += 1;
                self.acc_comma.push(v);
            }
        }

        if value::parse_bool(s).is_some() {
            self.boolean += 1;
        }
        if value::looks_like_date(s) {
            self.date += 1;
        }
    }

    /// A column takes a type only when every non-null value fits it.
    fn infer_type(&self) -> ColumnType {
        let n = self.non_null;
        if n == 0 {
            ColumnType::Empty
        } else if self.boolean == n {
            ColumnType::Boolean
        } else if self.int == n {
            ColumnType::Integer
        } else if self.float_dot == n || self.float_comma == n {
            ColumnType::Float
        } else if self.date == n {
            ColumnType::Date
        } else {
            ColumnType::Text
        }
    }

    /// The convention that read more values, preferring dot on a tie.
    fn decimal_style(&self) -> DecimalStyle {
        if self.float_comma > self.float_dot {
            DecimalStyle::Comma
        } else {
            DecimalStyle::Dot
        }
    }

    fn accumulator(&self, style: DecimalStyle) -> &NumericAccumulator {
        match style {
            DecimalStyle::Dot => &self.acc_dot,
            DecimalStyle::Comma => &self.acc_comma,
        }
    }
}

/// Profiles a CSV file held entirely in memory.
///
/// The input is borrowed, never copied, unless it needs transcoding from
/// windows-1252.
pub fn profile_csv(input: &[u8]) -> Result<Profile, ProfileError> {
    let byte_length = input.len();
    let (decoded, encoding) = decode::decode(input);
    let text: &str = decoded.as_ref();
    if text.trim().is_empty() {
        return Err(ProfileError::EmptyInput);
    }
    let delimiter = dialect::sniff_delimiter(text);

    let mut names: Vec<String> = Vec::new();
    let mut states: Vec<ColumnState> = Vec::new();
    let mut row_count = 0u64;
    let mut ragged_row_count = 0u64;
    let mut scratch = String::new();
    let mut header_seen = false;

    reader::for_each_record(text, delimiter, |rec| {
        if is_blank(&rec) {
            return;
        }
        if !header_seen {
            header_seen = true;
            names = header_names(&rec);
            states = (0..names.len()).map(|_| ColumnState::default()).collect();
            return;
        }
        row_count += 1;
        if rec.len() != names.len() {
            ragged_row_count += 1;
        }
        // Extra fields are dropped and missing ones count as null, so a ragged
        // row still contributes what it does line up with.
        for (i, state) in states.iter_mut().enumerate() {
            state.observe(rec.get(i), &mut scratch);
        }
    });

    if names.is_empty() {
        return Err(ProfileError::NoColumns);
    }

    let mut columns = build_columns(&names, &states);
    fill_histograms(text, delimiter, &mut columns, &mut scratch);

    Ok(Profile {
        byte_length,
        encoding,
        delimiter: (delimiter as char).to_string(),
        row_count,
        column_count: names.len(),
        ragged_row_count,
        columns,
    })
}

/// A record of one empty field is what a blank line parses to; it is not a row.
fn is_blank(rec: &Record<'_>) -> bool {
    rec.len() == 0 || (rec.len() == 1 && rec.get(0).is_empty())
}

fn header_names(rec: &Record<'_>) -> Vec<String> {
    (0..rec.len())
        .map(|i| {
            let raw = rec.get(i);
            if raw.is_empty() {
                format!("column_{i}")
            } else {
                raw.to_owned()
            }
        })
        .collect()
}

fn build_columns(names: &[String], states: &[ColumnState]) -> Vec<ColumnProfile> {
    names
        .iter()
        .zip(states)
        .enumerate()
        .map(|(index, (name, state))| {
            let column_type = state.infer_type();
            let numeric = matches!(column_type, ColumnType::Integer | ColumnType::Float);
            let style = state.decimal_style();
            ColumnProfile {
                name: name.clone(),
                index,
                column_type,
                count: state.non_null,
                null_count: state.null,
                type_counts: TypeCounts {
                    integer: state.int,
                    float: state.float_dot.max(state.float_comma),
                    boolean: state.boolean,
                    date: state.date,
                },
                decimal_style: numeric.then_some(style),
                numeric: numeric.then(|| state.accumulator(style).summary()),
                histogram: numeric
                    .then(|| state.accumulator(style).range())
                    .flatten()
                    .map(|(min, max)| Histogram::new(min, max)),
                text: state.text.summary(),
            }
        })
        .collect()
}

/// Pass two: replay the file and bin the numeric columns.
fn fill_histograms(text: &str, delimiter: u8, columns: &mut [ColumnProfile], scratch: &mut String) {
    let numeric: Vec<usize> = columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.histogram.is_some())
        .map(|(i, _)| i)
        .collect();
    if numeric.is_empty() {
        return;
    }

    let mut header_seen = false;
    reader::for_each_record(text, delimiter, |rec| {
        if is_blank(&rec) {
            return;
        }
        if !header_seen {
            header_seen = true;
            return;
        }
        for &i in &numeric {
            let column = &mut columns[i];
            let style = column.decimal_style.unwrap_or(DecimalStyle::Dot);
            let field = rec.get(column.index);
            if value::is_null(field) {
                continue;
            }
            let parsed = match number::parse_int(field) {
                Some(v) => Some(v as f64),
                None => number::parse_float(field, style, scratch),
            };
            if let (Some(v), Some(h)) = (parsed, column.histogram.as_mut()) {
                h.push(v);
            }
        }
    });
}
