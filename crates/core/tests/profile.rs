//! End-to-end profiling, including the real-world CSV cases the project targets.

use mirante_core::{ColumnType, DecimalStyle, Encoding, Profile, ProfileError, profile_csv};

fn profile(csv: &[u8]) -> Profile {
    profile_csv(csv).expect("profiling should succeed")
}

#[test]
fn profiles_a_plain_comma_file() {
    let p = profile(b"name,age\nAna,30\nJoao,41\n");
    assert_eq!(p.delimiter, ",");
    assert_eq!(p.encoding, Encoding::Utf8);
    assert_eq!(p.row_count, 2);
    assert_eq!(p.column_count, 2);
    assert_eq!(p.ragged_row_count, 0);

    assert_eq!(p.columns[0].name, "name");
    assert_eq!(p.columns[0].column_type, ColumnType::Text);

    let age = &p.columns[1];
    assert_eq!(age.column_type, ColumnType::Integer);
    assert_eq!(age.numeric.as_ref().unwrap().min, Some(30.0));
    assert_eq!(age.numeric.as_ref().unwrap().max, Some(41.0));
    assert_eq!(age.numeric.as_ref().unwrap().mean, Some(35.5));
}

#[test]
fn profiles_a_brazilian_file_end_to_end() {
    // Semicolons, comma decimals, windows-1252 accents and a BOM at once.
    let mut csv = Vec::from(b"\xEF\xBB\xBF");
    csv.extend_from_slice("produto;preco;disponivel;cadastro\n".as_bytes());
    csv.extend_from_slice("Vassoura;12,50;sim;2026-01-15\n".as_bytes());
    csv.extend_from_slice("Rodo;8,90;nao;2026-02-20\n".as_bytes());
    csv.extend_from_slice("Balde;1.234,56;sim;2026-03-05\n".as_bytes());

    let p = profile(&csv);
    assert_eq!(p.delimiter, ";");
    assert_eq!(p.row_count, 3);

    let preco = &p.columns[1];
    assert_eq!(preco.column_type, ColumnType::Float);
    assert_eq!(preco.decimal_style, Some(DecimalStyle::Comma));
    let stats = preco.numeric.as_ref().unwrap();
    assert_eq!(stats.min, Some(8.90));
    assert_eq!(stats.max, Some(1234.56));
    assert!((stats.sum.unwrap() - 1255.96).abs() < 1e-9);

    assert_eq!(p.columns[2].column_type, ColumnType::Boolean);
    assert_eq!(p.columns[3].column_type, ColumnType::Date);
}

#[test]
fn decodes_windows1252_payloads() {
    let mut csv = Vec::from(&b"cidade;uf\n"[..]);
    csv.extend_from_slice(b"S\xE3o Paulo;SP\n"); // 0xE3 is invalid UTF-8
    csv.extend_from_slice(b"Bel\xE9m;PA\n");

    let p = profile(&csv);
    assert_eq!(p.encoding, Encoding::Windows1252);
    let top = &p.columns[0].text.top_values;
    let values: Vec<&str> = top.iter().map(|v| v.value.as_str()).collect();
    assert!(values.contains(&"São Paulo"), "got {values:?}");
    assert!(values.contains(&"Belém"), "got {values:?}");
}

#[test]
fn ragged_rows_are_counted_and_still_profiled() {
    // Row two is short, row three is long.
    let p = profile(b"a,b,c\n1,2,3\n4,5\n6,7,8,9\n");
    assert_eq!(p.row_count, 3);
    assert_eq!(p.ragged_row_count, 2);
    assert_eq!(p.column_count, 3);

    // The missing third field counts as a null, the extra fourth is dropped.
    assert_eq!(p.columns[2].count, 2);
    assert_eq!(p.columns[2].null_count, 1);
    assert_eq!(p.columns[0].count, 3);
}

#[test]
fn null_tokens_are_excluded_from_statistics() {
    let p = profile(b"v,w\n10,\n,NA\nNA,null\nnull,x\n20,\n");
    let v = &p.columns[0];
    assert_eq!(v.column_type, ColumnType::Integer);
    assert_eq!(v.count, 2);
    assert_eq!(v.null_count, 3);
    assert_eq!(v.numeric.as_ref().unwrap().mean, Some(15.0));
}

#[test]
fn a_blank_line_is_not_an_empty_value() {
    // In a single-column file the two are the same bytes. Blank line wins,
    // so a trailing newline never invents a null row.
    let p = profile(b"v\n10\n\n20\n");
    assert_eq!(p.row_count, 2);
    assert_eq!(p.columns[0].null_count, 0);
}

#[test]
fn one_bad_value_keeps_the_column_text() {
    let p = profile(b"v\n1\n2\nabc\n");
    let v = &p.columns[0];
    assert_eq!(v.column_type, ColumnType::Text);
    assert_eq!(v.type_counts.integer, 2);
    assert!(v.numeric.is_none());
    assert!(v.histogram.is_none());
}

#[test]
fn histogram_covers_every_numeric_value() {
    let rows: String = (1..=100).map(|i| format!("{i}\n")).collect();
    let p = profile(format!("v\n{rows}").as_bytes());
    let h = p.columns[0].histogram.as_ref().unwrap();
    assert_eq!(h.min, 1.0);
    assert_eq!(h.max, 100.0);
    assert_eq!(h.counts.iter().sum::<u64>(), 100);
}

#[test]
fn quoted_fields_with_separators_stay_intact() {
    let p = profile("nome;obs\n\"Silva, Ana\";\"linha 1\nlinha 2\"\n".as_bytes());
    assert_eq!(p.delimiter, ";");
    assert_eq!(p.row_count, 1);
    assert_eq!(p.columns[0].text.top_values[0].value, "Silva, Ana");
}

#[test]
fn all_null_column_is_typed_empty() {
    let p = profile(b"a,b\n1,\n2,NA\n");
    assert_eq!(p.columns[1].column_type, ColumnType::Empty);
    assert_eq!(p.columns[1].count, 0);
    assert!(p.columns[1].numeric.is_none());
}

#[test]
fn blank_lines_are_not_rows() {
    let p = profile(b"a\n1\n\n\n2\n");
    assert_eq!(p.row_count, 2);
    assert_eq!(p.ragged_row_count, 0);
}

#[test]
fn header_only_file_has_columns_and_no_rows() {
    let p = profile(b"a,b,c\n");
    assert_eq!(p.row_count, 0);
    assert_eq!(p.column_count, 3);
    assert_eq!(p.columns[0].column_type, ColumnType::Empty);
}

#[test]
fn unnamed_header_fields_get_positional_names() {
    let p = profile(b"a,,c\n1,2,3\n");
    assert_eq!(p.columns[1].name, "column_1");
}

#[test]
fn empty_input_is_an_error_not_a_panic() {
    assert_eq!(profile_csv(b"").unwrap_err(), ProfileError::EmptyInput);
    assert_eq!(
        profile_csv(b"   \n\n").unwrap_err(),
        ProfileError::EmptyInput
    );
}

#[test]
fn comma_decimal_is_not_read_as_thousands() {
    // Three-point-five, not thirty-five. Semicolons because with a comma
    // delimiter the file would genuinely be two integer columns.
    let p = profile(b"v;w\n3,5;a\n4,5;b\n");
    assert_eq!(p.columns[0].column_type, ColumnType::Float);
    assert_eq!(p.columns[0].decimal_style, Some(DecimalStyle::Comma));
    assert_eq!(p.columns[0].numeric.as_ref().unwrap().max, Some(4.5));
}

#[test]
fn byte_length_reports_the_original_input() {
    let csv = b"a\n1\n";
    assert_eq!(profile(csv).byte_length, csv.len());
}

#[test]
fn malformed_input_never_panics() {
    // Unbalanced quotes, stray control bytes, lone separators, truncated UTF-8.
    let cases: [&[u8]; 6] = [
        b"a,b\n\"unclosed,2\n",
        b"\x00\x01\x02\n\x03",
        b",,,\n,,,\n",
        b"a\n\xC3",
        b"\"\n",
        b";;;",
    ];
    for case in cases {
        let _ = profile_csv(case);
    }
}
