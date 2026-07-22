/**
 * Time the Wasm engine outside a browser.
 *
 * This measures the same release .wasm the browser loads, under Node's V8,
 * which is not the same as measuring in the browser. Numbers from here are
 * labelled as such in the README; the browser figures come from the page
 * itself, which times with performance.now().
 *
 *   wasm-pack build crates/wasm --target nodejs --release --out-dir ../../benchmarks/pkg
 *   node benchmarks/bench_wasm.mjs benchmarks/data/*.csv
 */

import { readFileSync, statSync } from "node:fs";
import { basename } from "node:path";
import { profile } from "./pkg/mirante_wasm.js";

const RUNS = 5;

console.log(`node ${process.version} | ${process.arch} ${process.platform}`);

for (const path of process.argv.slice(2)) {
  const bytes = readFileSync(path);
  profile(bytes); // warm-up

  const runs = [];
  for (let i = 0; i < RUNS; i += 1) {
    const start = performance.now();
    profile(bytes);
    runs.push(performance.now() - start);
  }
  runs.sort((a, b) => a - b);
  const median = runs[Math.floor(RUNS / 2)];
  const size = statSync(path).size / 1048576;

  console.log(
    `${basename(path).padEnd(14)} ${size.toFixed(1).padStart(7)} MB  ` +
      `median ${median.toFixed(0).padStart(9)} ms  ` +
      `min ${runs[0].toFixed(0).padStart(8)}  max ${runs[RUNS - 1].toFixed(0).padStart(8)}  n=${RUNS}`,
  );
}
