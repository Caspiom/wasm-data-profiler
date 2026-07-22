# mirante

CSV profiler that runs entirely in the browser. The engine is Rust compiled to
WebAssembly, so the file you profile never leaves your machine — there is no
upload, no server, and no account.

Drop a CSV in and you get, per column: the inferred type, null and distinct
counts, min/max/mean/standard deviation for numbers, the most frequent values,
and a histogram.

## Why it exists

Two questions, answered honestly:

1. How much CSV work can you push into a browser tab before it stops being
   pleasant? (Parsing runs in a Web Worker; the tab does not freeze.)
2. How does Rust-in-Wasm compare with pandas on the same file? An optional
   FastAPI backend runs the same profile with pandas so the two can be measured
   side by side. **Benchmark numbers are not published yet** — see below.

## Real-world CSV

The parser handles the files people actually have, not just the tidy ones:

- `;` as well as `,`, `\t` and `|`, detected from the header line
- comma as the decimal separator (`1.234,56`), per column
- windows-1252 as well as UTF-8
- a BOM at the start of the file
- rows whose field count disagrees with the header

Type inference is strict: one unparseable value keeps the whole column as text.
The profile reports how many values parsed as each type, so you can see *why* a
column stayed text instead of guessing.

## Layout

```
crates/core/   the profiling engine — pure Rust, no Wasm, no I/O
crates/wasm/   a thin #[wasm_bindgen] shell around the core
web/           index.html, main.ts, worker.ts, pkg/ (generated)
api/           optional FastAPI comparison service
```

The core knows nothing about Wasm. It takes `&[u8]` and returns a `Profile`,
runs under native `cargo test`, and never touches the clock, the filesystem, or
threads — all three of which are unavailable on `wasm32-unknown-unknown`.

## Building

```bash
cargo test -p mirante-core
cargo clippy --all-targets -- -D warnings

# Always --release: a debug Wasm build is orders of magnitude slower.
wasm-pack build crates/wasm --target web --release --out-dir ../../web/pkg

cd web && npm install && npm run build
python3 -m http.server 8080
```

## Benchmark

### What was measured

A 51.3 MB CSV: 1,500,000 rows across five columns — an integer, a float, a
low-cardinality text column, a boolean and a date. Generated with a fixed seed,
so the file is byte-identical anywhere:

```bash
python3 benchmarks/make_fixtures.py benchmarks/data

wasm-pack build crates/wasm --target nodejs --release --out-dir ../../benchmarks/pkg
node benchmarks/bench_wasm.mjs benchmarks/data/*.csv

cd api && PYTHONPATH=. uv run python ../benchmarks/bench_pandas.py ../benchmarks/data/*.csv
```

Both engines produce the same profile. That is checked, not assumed: the
comparison page in the browser diffs the two results field by field and reports
disagreements before it reports timings.

### Results

Median of 5 runs after one warm-up. AMD Ryzen 7 6800H, 14 GB RAM, Linux
7.0.11-cachyos. Rust 1.93.1 (`--release`, `opt-level = 3`, `lto = true`),
Node 26.1.0, Python 3.12.8, pandas 3.0.3, pyarrow 25.0.0.

| Engine | 51.3 MB / 1.5 M rows | 23 KB / 500 rows |
| --- | --- | --- |
| Rust → Wasm | **1,237 ms** | **2 ms** |
| pandas | 14,298 ms | 42 ms |

Roughly 11× on the large file. Two things that number does **not** include, both
in Wasm's favour:

- **The network.** Sending the 51 MB file to the pandas service and reading the
  answer back took a further 167 ms — and that was over loopback, on the same
  machine. A real deployment would be far worse. Wasm wins partly by having no
  network at all, and that is a property of where the code runs, not of how
  fast it is.
- **Distinct counts.** The Rust engine caps its frequency table at 10,000
  distinct values per column to bound memory, and reports the count as a floor
  (`distinctIsExact: false`). pandas counts exactly, so on high-cardinality text
  columns it is doing strictly more work.

### Why the pandas side is written the way it is

The baseline reproduces the engine's semantics — strict type inference, a
per-column decimal convention, Brazilian null and boolean tokens — using
vectorised pandas rather than a loop over rows. Letting `read_csv` infer dtypes
natively would be considerably faster and would also compute something else
entirely, which would make the comparison meaningless.

`pyarrow` is a declared dependency for the same reason. Without it, pandas 3
falls back to Python-backed strings and every `.str` operation runs one element
at a time; the same file then takes 21,207 ms. Publishing that number would have
credited Rust with 7 seconds that belong to a missing dependency.

### Browser numbers

The figures above are the release `.wasm` under Node's V8, which is not the same
as measuring in a browser. The page times itself with `performance.now()` and
shows the split; browser medians will be added here once they are collected the
same way — five runs, median reported.

## License

MIT.
