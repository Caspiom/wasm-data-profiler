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

Not yet published. Numbers will appear here only once they come from real runs
of a release build, reported as the median of at least five executions, with the
machine, browser, versions and dataset size declared — and with the network time
of the pandas comparison broken out separately, since Wasm wins partly by not
having a network at all.

## License

MIT.
