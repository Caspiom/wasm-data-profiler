/**
 * The profile shape produced by `mirante-core` and serialised by the Wasm
 * shell. The FastAPI comparison endpoint must return this exact shape, or the
 * side-by-side comparison is meaningless.
 */

export type ColumnType = "empty" | "boolean" | "integer" | "float" | "date" | "text";
export type DecimalStyle = "dot" | "comma";
export type Encoding = "utf8" | "windows1252";

export interface NumericSummary {
  min: number | null;
  max: number | null;
  mean: number | null;
  sum: number | null;
  stddev: number | null;
}

export interface Histogram {
  min: number;
  max: number;
  counts: number[];
}

export interface ValueCount {
  value: string;
  count: number;
}

export interface TextSummary {
  minLength: number | null;
  maxLength: number | null;
  meanLength: number | null;
  distinct: number;
  /** False when the frequency table saturated, making `distinct` a floor. */
  distinctIsExact: boolean;
  topValues: ValueCount[];
}

export interface TypeCounts {
  integer: number;
  float: number;
  boolean: number;
  date: number;
}

export interface ColumnProfile {
  name: string;
  index: number;
  type: ColumnType;
  count: number;
  nullCount: number;
  typeCounts: TypeCounts;
  decimalStyle: DecimalStyle | null;
  numeric: NumericSummary | null;
  histogram: Histogram | null;
  text: TextSummary;
}

export interface Profile {
  byteLength: number;
  encoding: Encoding;
  delimiter: string;
  rowCount: number;
  columnCount: number;
  raggedRowCount: number;
  columns: ColumnProfile[];
}

/**
 * All timings are taken here in TypeScript. `std::time::Instant` does not work
 * on `wasm32-unknown-unknown`, so the Rust side never measures anything.
 */
export interface Timings {
  /** Reading the File into an ArrayBuffer. */
  readMs: number;
  /** The `profile()` call alone, copy into Wasm memory included. */
  profileMs: number;
}

export type WorkerRequest = { type: "profile"; file: File };

export type WorkerResponse =
  | { type: "ready"; version: string }
  | { type: "result"; profile: Profile; timings: Timings; fileName: string; fileSize: number }
  | { type: "error"; message: string };
