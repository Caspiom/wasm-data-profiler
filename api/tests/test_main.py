"""Endpoint tests.

The point of these is the wire format. The comparison is only meaningful if
this service returns exactly what the Wasm engine returns, so the JSON keys are
asserted directly rather than through the pydantic models.
"""

from fastapi.testclient import TestClient

from app.main import app

client = TestClient(app)

CSV = b"produto;preco\nVassoura;12,50\nRodo;8,90\n"


def test_health() -> None:
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json()["engine"] == "pandas"


def test_profile_returns_camel_case_keys() -> None:
    response = client.post("/profile", files={"file": ("brasil.csv", CSV, "text/csv")})
    assert response.status_code == 200
    payload = response.json()

    profile = payload["profile"]
    assert set(profile) == {
        "byteLength",
        "encoding",
        "delimiter",
        "rowCount",
        "columnCount",
        "raggedRowCount",
        "columns",
    }
    assert set(profile["columns"][0]) == {
        "name",
        "index",
        "type",
        "count",
        "nullCount",
        "typeCounts",
        "decimalStyle",
        "numeric",
        "histogram",
        "text",
    }
    assert set(profile["columns"][1]["numeric"]) == {"min", "max", "mean", "sum", "stddev"}
    assert set(profile["columns"][0]["text"]) == {
        "minLength",
        "maxLength",
        "meanLength",
        "distinct",
        "distinctIsExact",
        "topValues",
    }


def test_profile_values_are_correct() -> None:
    response = client.post("/profile", files={"file": ("brasil.csv", CSV, "text/csv")})
    profile = response.json()["profile"]
    assert profile["delimiter"] == ";"
    assert profile["rowCount"] == 2
    assert profile["columns"][1]["type"] == "float"
    assert profile["columns"][1]["decimalStyle"] == "comma"
    assert profile["columns"][1]["numeric"]["max"] == 12.50


def test_absent_values_are_null_not_missing() -> None:
    # The Wasm shell is configured to emit null for None as well. A key that is
    # simply absent on one side would show up as a phantom difference.
    response = client.post("/profile", files={"file": ("brasil.csv", CSV, "text/csv")})
    text_column = response.json()["profile"]["columns"][0]
    assert text_column["numeric"] is None
    assert text_column["histogram"] is None
    assert text_column["decimalStyle"] is None


def test_timings_are_reported() -> None:
    response = client.post("/profile", files={"file": ("brasil.csv", CSV, "text/csv")})
    timings = response.json()["timings"]
    assert timings["profileMs"] > 0
    # The comparable figure is the sum of its parts, and nothing else.
    assert abs(timings["profileMs"] - (timings["parseMs"] + timings["aggregateMs"])) < 1e-6


def test_empty_file_is_rejected_with_a_reason() -> None:
    response = client.post("/profile", files={"file": ("empty.csv", b"", "text/csv")})
    assert response.status_code == 422
    assert "empty" in response.json()["detail"]
