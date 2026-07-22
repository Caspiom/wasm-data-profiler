"""These mirror `crates/core/tests/profile.rs` case for case.

Both suites assert the same expectations, so the two engines are pinned to one
specification rather than to each other. If one drifts, its own suite fails.
"""

import contextlib

from app.profiler import profile_csv


def profile(raw: bytes):  # noqa: ANN201 - the model type is an implementation detail here
    result, _ = profile_csv(raw)
    return result


def test_profiles_a_plain_comma_file() -> None:
    p = profile(b"name,age\nAna,30\nJoao,41\n")
    assert p.delimiter == ","
    assert p.encoding == "utf8"
    assert p.row_count == 2
    assert p.column_count == 2
    assert p.ragged_row_count == 0

    assert p.columns[0].name == "name"
    assert p.columns[0].type == "text"

    age = p.columns[1]
    assert age.type == "integer"
    assert age.numeric.min == 30.0
    assert age.numeric.max == 41.0
    assert age.numeric.mean == 35.5


def test_profiles_a_brazilian_file_end_to_end() -> None:
    csv = (
        b"\xef\xbb\xbf" + b"produto;preco;disponivel;cadastro\n"
        b"Vassoura;12,50;sim;2026-01-15\n"
        b"Rodo;8,90;nao;2026-02-20\n"
        b"Balde;1.234,56;sim;2026-03-05\n"
    )
    p = profile(csv)
    assert p.delimiter == ";"
    assert p.row_count == 3

    preco = p.columns[1]
    assert preco.type == "float"
    assert preco.decimal_style == "comma"
    assert preco.numeric.min == 8.90
    assert preco.numeric.max == 1234.56
    assert abs(preco.numeric.sum - 1255.96) < 1e-9

    assert p.columns[2].type == "boolean"
    assert p.columns[3].type == "date"


def test_decodes_windows1252_payloads() -> None:
    csv = b"cidade;uf\nS\xe3o Paulo;SP\nBel\xe9m;PA\n"
    p = profile(csv)
    assert p.encoding == "windows1252"
    values = [v.value for v in p.columns[0].text.top_values]
    assert "São Paulo" in values
    assert "Belém" in values


def test_ragged_rows_are_counted_and_still_profiled() -> None:
    p = profile(b"a,b,c\n1,2,3\n4,5\n6,7,8,9\n")
    assert p.row_count == 3
    assert p.ragged_row_count == 2
    assert p.column_count == 3
    assert p.columns[2].count == 2
    assert p.columns[2].null_count == 1
    assert p.columns[0].count == 3


def test_null_tokens_are_excluded_from_statistics() -> None:
    p = profile(b"v,w\n10,\n,NA\nNA,null\nnull,x\n20,\n")
    v = p.columns[0]
    assert v.type == "integer"
    assert v.count == 2
    assert v.null_count == 3
    assert v.numeric.mean == 15.0


def test_a_blank_line_is_not_an_empty_value() -> None:
    p = profile(b"v\n10\n\n20\n")
    assert p.row_count == 2
    assert p.columns[0].null_count == 0


def test_one_bad_value_keeps_the_column_text() -> None:
    p = profile(b"v\n1\n2\nabc\n")
    v = p.columns[0]
    assert v.type == "text"
    assert v.type_counts.integer == 2
    assert v.numeric is None
    assert v.histogram is None


def test_histogram_covers_every_numeric_value() -> None:
    rows = "".join(f"{i}\n" for i in range(1, 101))
    p = profile(f"v\n{rows}".encode())
    h = p.columns[0].histogram
    assert h.min == 1.0
    assert h.max == 100.0
    assert sum(h.counts) == 100
    assert len(h.counts) == 24


def test_quoted_fields_with_separators_stay_intact() -> None:
    p = profile(b'nome;obs\n"Silva, Ana";"linha 1\nlinha 2"\n')
    assert p.delimiter == ";"
    assert p.row_count == 1
    assert p.columns[0].text.top_values[0].value == "Silva, Ana"


def test_all_null_column_is_typed_empty() -> None:
    p = profile(b"a,b\n1,\n2,NA\n")
    assert p.columns[1].type == "empty"
    assert p.columns[1].count == 0
    assert p.columns[1].numeric is None


def test_header_only_file_has_columns_and_no_rows() -> None:
    p = profile(b"a,b,c\n")
    assert p.row_count == 0
    assert p.column_count == 3
    assert p.columns[0].type == "empty"


def test_unnamed_header_fields_get_positional_names() -> None:
    p = profile(b"a,,c\n1,2,3\n")
    assert p.columns[1].name == "column_1"


def test_comma_decimal_is_not_read_as_thousands() -> None:
    # Three-point-five, not thirty-five. Semicolons because with a comma
    # delimiter the file would genuinely be two integer columns.
    p = profile(b"v;w\n3,5;a\n4,5;b\n")
    assert p.columns[0].type == "float"
    assert p.columns[0].decimal_style == "comma"
    assert p.columns[0].numeric.max == 4.5


def test_byte_length_reports_the_original_input() -> None:
    csv = b"a\n1\n"
    assert profile(csv).byte_length == len(csv)


def test_empty_input_is_rejected() -> None:
    for raw in (b"", b"   \n\n"):
        try:
            profile(raw)
        except ValueError:
            continue
        raise AssertionError(f"expected a ValueError for {raw!r}")


def test_malformed_input_does_not_crash() -> None:
    for raw in (b'a,b\n"unclosed,2\n', b",,,\n,,,\n", b'"\n', b";;;"):
        # Rejecting these is fine; crashing on them is not.
        with contextlib.suppress(ValueError):
            profile(raw)
