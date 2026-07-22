/**
 * Side-by-side comparison against the pandas service.
 *
 * Two rules govern what this reports, both of them about not flattering the
 * Wasm engine:
 *
 * 1. Network time is measured and shown on its own line. Wasm wins partly by
 *    not having a network at all, and hiding that in a single total would be
 *    the easiest way to publish a dishonest number.
 * 2. The two profiles are diffed field by field. A speed comparison between
 *    engines that computed different things is worthless, so the page says
 *    whether they agree before it says which was faster.
 */

import { element, formatBytes, formatMs, must } from "./dom.js";
import type { Profile, ProfileResponse } from "./types.js";

/** Fields whose values may differ without the profiles disagreeing. */
const TOLERANCE = 1e-9;

export interface ComparisonState {
  file: File;
  profile: Profile;
  profileMs: number;
}

const section = must<HTMLElement>("compare");
const urlInput = must<HTMLInputElement>("api-url");
const button = must<HTMLButtonElement>("compare-button");
const resultBox = must<HTMLElement>("compare-result");

let state: ComparisonState | null = null;

button.addEventListener("click", () => void run());

/** Called after each local profile; `null` hides the section. */
export function setComparisonState(next: ComparisonState | null): void {
  state = next;
  section.hidden = next === null;
  resultBox.hidden = true;
  resultBox.replaceChildren();
}

async function run(): Promise<void> {
  if (!state) return;
  const base = urlInput.value.trim().replace(/\/$/, "");
  button.disabled = true;
  showMessage("Uploading and profiling with pandas…");

  try {
    const body = new FormData();
    body.append("file", state.file, state.file.name);

    // The clock covers everything the browser waits for: upload, parsing,
    // aggregation, JSON serialisation and download.
    const start = performance.now();
    const response = await fetch(`${base}/profile`, { method: "POST", body });
    const payload: unknown = await response.json();
    const roundTripMs = performance.now() - start;

    if (!response.ok) {
      const detail =
        typeof payload === "object" && payload !== null && "detail" in payload
          ? String((payload as { detail: unknown }).detail)
          : `HTTP ${response.status}`;
      throw new Error(detail);
    }

    render(payload as ProfileResponse, roundTripMs);
  } catch (error) {
    // A failed fetch to a plain-HTTP service from an HTTPS page is blocked as
    // mixed content, which surfaces here as an opaque network error.
    showMessage(
      `Could not reach the comparison service: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
  } finally {
    button.disabled = false;
  }
}

function render(response: ProfileResponse, roundTripMs: number): void {
  if (!state) return;
  const wasmMs = state.profileMs;
  const pandasMs = response.timings.profileMs;
  const transportMs = Math.max(0, roundTripMs - pandasMs);
  const differences = diff(state.profile, response.profile);

  const verdict = element(
    "p",
    differences.length === 0 ? "verdict is-match" : "verdict",
    differences.length === 0
      ? "Both engines produced the same profile, field for field."
      : `The engines disagree on ${differences.length} field${
          differences.length === 1 ? "" : "s"
        }: ${differences.slice(0, 5).join(", ")}${differences.length > 5 ? " …" : ""}`,
  );

  const table = element("table", "compare-table");
  const head = element("thead");
  head.append(row("th", ["", "Wasm (in this tab)", `pandas ${response.versions.pandas ?? ""}`]));
  const bodyRows = element("tbody");
  bodyRows.append(
    row("td", ["Parsing and aggregation", formatMs(wasmMs), formatMs(pandasMs)]),
    row("td", ["Network and transport", "none", formatMs(transportMs)]),
    row("td", ["Total the user waits", formatMs(wasmMs), formatMs(roundTripMs)]),
  );
  table.append(head, bodyRows);

  const caveat = element(
    "p",
    "caveat",
    `Processing alone: ${ratio(pandasMs, wasmMs)}. End to end, including the upload of ` +
      `${formatBytes(state.file.size)}: ${ratio(roundTripMs, wasmMs)}. ` +
      "One run each — not a benchmark. The published figures are medians of repeated runs.",
  );

  resultBox.replaceChildren(verdict, table, caveat);
  resultBox.hidden = false;
}

/**
 * A ratio is only worth printing when both sides were timed for long enough.
 *
 * `performance.now()` is coarse and the first millisecond of anything is
 * mostly noise, so a 1 ms measurement against a 40 ms one would advertise a
 * 40× win that the clock cannot actually support.
 */
const MEASURABLE_MS = 20;

function ratio(slower: number, faster: number): string {
  if (faster < MEASURABLE_MS || slower < MEASURABLE_MS) {
    return "too fast on this file to compare meaningfully";
  }
  const factor = slower / faster;
  return factor >= 1
    ? `Wasm was ${factor.toFixed(1)}× faster`
    : `pandas was ${(1 / factor).toFixed(1)}× faster`;
}

function row(cell: "th" | "td", texts: string[]): HTMLElement {
  const tr = element("tr");
  for (const text of texts) tr.append(element(cell, undefined, text));
  return tr;
}

function showMessage(text: string): void {
  resultBox.replaceChildren(element("p", "verdict", text));
  resultBox.hidden = false;
}

/**
 * Walks both profiles and returns the paths that disagree.
 *
 * Floats are compared with a tolerance: the two engines sum in different
 * orders, so the last bits of a mean over a million rows will not match, and
 * calling that a disagreement would be noise rather than a finding.
 */
function diff(a: unknown, b: unknown, path = "profile"): string[] {
  if (typeof a === "number" && typeof b === "number") {
    const scale = Math.max(Math.abs(a), Math.abs(b), 1);
    return Math.abs(a - b) <= TOLERANCE * scale ? [] : [path];
  }
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") {
    return a === b ? [] : [path];
  }
  if (Array.isArray(a) !== Array.isArray(b)) return [path];

  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return [`${path}.length`];
    return a.flatMap((item, i) => diff(item, b[i], `${path}[${i}]`));
  }

  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  return [...keys].flatMap((key) => diff(left[key], right[key], `${path}.${key}`));
}
