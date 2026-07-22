//! Record iteration over an in-memory buffer.
//!
//! Buffers are allocated once and reused for every record, so profiling a large
//! file does not allocate per cell.

use csv_core::{ReadRecordResult, ReaderBuilder};

/// One parsed record: the concatenated field bytes plus each field's end offset.
pub struct Record<'a> {
    fields: &'a [u8],
    ends: &'a [usize],
}

impl<'a> Record<'a> {
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    /// The `i`th field, trimmed, as a string slice.
    ///
    /// The buffer holds UTF-8 because the input was decoded up front, but
    /// quoting can split a multi-byte character across the copy in principle,
    /// so this falls back to an empty field rather than panicking.
    pub fn get(&self, i: usize) -> &'a str {
        let end = match self.ends.get(i) {
            Some(&e) => e,
            None => return "",
        };
        let start = if i == 0 { 0 } else { self.ends[i - 1] };
        std::str::from_utf8(&self.fields[start..end])
            .unwrap_or("")
            .trim()
    }
}

/// Calls `f` for every record in `input`.
pub fn for_each_record<F>(input: &str, delimiter: u8, mut f: F)
where
    F: FnMut(Record<'_>),
{
    let mut rdr = ReaderBuilder::new().delimiter(delimiter).build();
    let mut fields = vec![0u8; 64 * 1024];
    let mut ends = vec![0usize; 256];
    let (mut pos, mut nfields, mut nends) = (0usize, 0usize, 0usize);
    let input = input.as_bytes();

    loop {
        let (result, nin, nout, nend) =
            rdr.read_record(&input[pos..], &mut fields[nfields..], &mut ends[nends..]);
        pos += nin;
        nfields += nout;
        nends += nend;

        match result {
            // Only reachable once the whole input is consumed; the next call
            // sees an empty slice and flushes any trailing record.
            ReadRecordResult::InputEmpty => {}
            ReadRecordResult::OutputFull => fields.resize(fields.len() * 2, 0),
            ReadRecordResult::OutputEndsFull => ends.resize(ends.len() * 2, 0),
            ReadRecordResult::Record => {
                f(Record {
                    fields: &fields[..nfields],
                    ends: &ends[..nends],
                });
                nfields = 0;
                nends = 0;
            }
            ReadRecordResult::End => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(input: &str, delimiter: u8) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        for_each_record(input, delimiter, |r| {
            out.push((0..r.len()).map(|i| r.get(i).to_string()).collect());
        });
        out
    }

    #[test]
    fn splits_records_and_fields() {
        assert_eq!(
            collect("a,b\n1,2\n3,4", b','),
            vec![vec!["a", "b"], vec!["1", "2"], vec!["3", "4"]]
        );
    }

    #[test]
    fn trailing_newline_produces_no_empty_record() {
        assert_eq!(
            collect("a,b\n1,2\n", b','),
            vec![vec!["a", "b"], vec!["1", "2"]]
        );
    }

    #[test]
    fn quoted_fields_keep_delimiters_and_newlines() {
        let rows = collect("a;b\n\"x;y\";\"line1\nline2\"", b';');
        assert_eq!(rows[1], vec!["x;y", "line1\nline2"]);
    }

    #[test]
    fn fields_are_trimmed() {
        assert_eq!(
            collect("a , b \n 1 ,2", b','),
            vec![vec!["a", "b"], vec!["1", "2"]]
        );
    }

    #[test]
    fn ragged_rows_keep_their_own_field_count() {
        let rows = collect("a,b,c\n1,2\n1,2,3,4", b',');
        assert_eq!(rows[1].len(), 2);
        assert_eq!(rows[2].len(), 4);
    }

    #[test]
    fn crlf_is_handled() {
        assert_eq!(
            collect("a,b\r\n1,2\r\n", b','),
            vec![vec!["a", "b"], vec!["1", "2"]]
        );
    }

    #[test]
    fn buffers_grow_for_large_records() {
        // Wider than the initial 256-slot ends buffer and the 64 KiB field buffer.
        let header = (0..500)
            .map(|i| format!("c{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let row = (0..500)
            .map(|_| "x".repeat(300))
            .collect::<Vec<_>>()
            .join(",");
        let rows = collect(&format!("{header}\n{row}"), b',');
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].len(), 500);
        assert_eq!(rows[1][499].len(), 300);
    }
}
