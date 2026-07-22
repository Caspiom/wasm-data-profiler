/**
 * UI wiring and rendering. All profiling happens in the worker; this file only
 * hands it a File and draws what comes back.
 */

import { setComparisonState } from "./compare.js";
import {
  element,
  formatBytes,
  formatInt,
  formatMs,
  formatNumber,
  formatOptional,
  must,
} from "./dom.js";
import type { ColumnProfile, Histogram, Profile, TextSummary, Timings, WorkerResponse } from "./types.js";

const SVG_NS = "http://www.w3.org/2000/svg";

const dropzone = must<HTMLElement>("dropzone");
const fileInput = must<HTMLInputElement>("file-input");
const statusLine = must<HTMLElement>("status");
const summary = must<HTMLElement>("summary");
const columnsSection = must<HTMLElement>("columns");
const columnsBody = must<HTMLElement>("columns-body");
const versionLabel = must<HTMLElement>("version");

/** Kept so the comparison can send the same bytes to the pandas service. */
let currentFile: File | null = null;

// `import.meta.url` keeps this working under a subpath, which is where
// GitHub Pages serves the project from.
const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });

let busy = false;

worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
  const message = event.data;
  switch (message.type) {
    case "ready":
      versionLabel.textContent = `Engine v${message.version}.`;
      break;
    case "result":
      busy = false;
      dropzone.classList.remove("is-busy");
      render(message.profile, message.timings, message.fileName, message.fileSize);
      break;
    case "error":
      busy = false;
      dropzone.classList.remove("is-busy");
      setStatus(message.message, true);
      break;
  }
};

worker.onerror = (event) => {
  busy = false;
  dropzone.classList.remove("is-busy");
  setStatus(`Worker failed: ${event.message}`, true);
};

/* ------------------------------------------------------------------ input */

dropzone.addEventListener("click", () => !busy && fileInput.click());
dropzone.addEventListener("keydown", (event) => {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    if (!busy) fileInput.click();
  }
});

fileInput.addEventListener("change", () => {
  const file = fileInput.files?.[0];
  if (file) start(file);
  // Reset so choosing the same file twice fires `change` again.
  fileInput.value = "";
});

for (const type of ["dragenter", "dragover"]) {
  dropzone.addEventListener(type, (event) => {
    event.preventDefault();
    dropzone.classList.add("is-dragging");
  });
}

for (const type of ["dragleave", "dragend"]) {
  dropzone.addEventListener(type, () => dropzone.classList.remove("is-dragging"));
}

dropzone.addEventListener("drop", (event) => {
  event.preventDefault();
  dropzone.classList.remove("is-dragging");
  const file = (event as DragEvent).dataTransfer?.files?.[0];
  if (file) start(file);
});

// Without this a missed drop navigates away and the page is replaced by the CSV.
for (const type of ["dragover", "drop"]) {
  window.addEventListener(type, (event) => event.preventDefault());
}

function start(file: File): void {
  if (busy) return;
  busy = true;
  currentFile = file;
  dropzone.classList.add("is-busy");
  summary.hidden = true;
  columnsSection.hidden = true;
  setComparisonState(null);
  setStatus(`Profiling ${file.name} (${formatBytes(file.size)})…`);
  worker.postMessage({ type: "profile", file });
}

function setStatus(text: string, isError = false): void {
  statusLine.textContent = text;
  statusLine.classList.toggle("is-error", isError);
}

/* --------------------------------------------------------------- rendering */

function render(profile: Profile, timings: Timings, fileName: string, fileSize: number): void {
  setStatus(
    `${fileName} — read in ${formatMs(timings.readMs)}, profiled in ${formatMs(timings.profileMs)}.`,
  );
  renderSummary(profile, timings, fileSize);
  renderColumns(profile);
  summary.hidden = false;
  columnsSection.hidden = false;
  if (currentFile) {
    setComparisonState({ file: currentFile, profile, profileMs: timings.profileMs });
  }
}

function renderSummary(profile: Profile, timings: Timings, fileSize: number): void {
  const encoding = profile.encoding === "utf8" ? "UTF-8" : "windows-1252";
  const delimiter = profile.delimiter === "\t" ? "tab" : profile.delimiter;

  summary.replaceChildren(
    stat("Rows", formatInt(profile.rowCount)),
    stat("Columns", formatInt(profile.columnCount)),
    stat("Size", formatBytes(fileSize)),
    stat("Encoding", encoding),
    stat("Delimiter", delimiter),
    stat(
      "Profiled in",
      formatMs(timings.profileMs),
      `${formatMs(timings.readMs)} to read the file`,
    ),
    ...(profile.raggedRowCount > 0
      ? [
          stat(
            "Ragged rows",
            formatInt(profile.raggedRowCount),
            "field count differed from the header",
          ),
        ]
      : []),
  );
}

function stat(label: string, value: string, note?: string): HTMLElement {
  const box = element("div", "stat");
  box.append(element("p", "stat__label", label), element("p", "stat__value", value));
  if (note) box.append(element("p", "stat__note", note));
  return box;
}

function renderColumns(profile: Profile): void {
  const rows: HTMLElement[] = [];
  for (const column of profile.columns) {
    const detail = detailRow(column);
    rows.push(columnRow(column, detail), detail);
  }
  columnsBody.replaceChildren(...rows);
}

function columnRow(column: ColumnProfile, detail: HTMLElement): HTMLElement {
  const row = element("tr", "column-row");
  const numeric = column.numeric;

  const name = element("td", "column-name", column.name);
  name.title = column.name;

  const type = element("td");
  const tag = element("span", "type-tag", column.type);
  tag.dataset.type = column.type;
  type.append(tag);

  row.append(
    name,
    type,
    numberCell(formatInt(column.count)),
    numberCell(formatInt(column.nullCount)),
    numberCell(distinctLabel(column.text)),
    numberCell(formatOptional(numeric?.min)),
    numberCell(formatOptional(numeric?.max)),
    numberCell(formatOptional(numeric?.mean)),
    numberCell(formatOptional(numeric?.stddev)),
    histogramCell(column.histogram),
  );

  row.addEventListener("click", () => {
    detail.hidden = !detail.hidden;
  });
  return row;
}

function numberCell(text: string): HTMLElement {
  const cell = element("td", "num", text);
  if (text === "—") cell.classList.add("nil");
  return cell;
}

function histogramCell(histogram: Histogram | null): HTMLElement {
  const cell = element("td");
  if (!histogram) {
    cell.append(element("span", "nil", "—"));
    return cell;
  }
  cell.append(histogramSvg(histogram));
  return cell;
}

/** Hand-written SVG: 24 bars scaled to the tallest bin. */
function histogramSvg(histogram: Histogram): SVGElement {
  const width = 140;
  const height = 34;
  const gap = 1;
  const bins = histogram.counts.length;
  const barWidth = (width - gap * (bins - 1)) / bins;
  const peak = Math.max(...histogram.counts, 1);

  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "histogram");
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.setAttribute("role", "img");
  svg.setAttribute(
    "aria-label",
    `Distribution from ${formatNumber(histogram.min)} to ${formatNumber(histogram.max)}`,
  );

  histogram.counts.forEach((count, i) => {
    // Keep a hairline for non-empty bins so a rare value stays visible.
    const barHeight = count === 0 ? 0 : Math.max(1, (count / peak) * height);
    const rect = document.createElementNS(SVG_NS, "rect");
    rect.setAttribute("x", String(i * (barWidth + gap)));
    rect.setAttribute("y", String(height - barHeight));
    rect.setAttribute("width", String(barWidth));
    rect.setAttribute("height", String(barHeight));
    const title = document.createElementNS(SVG_NS, "title");
    title.textContent = `${formatInt(count)} rows`;
    rect.append(title);
    svg.append(rect);
  });

  return svg;
}

function detailRow(column: ColumnProfile): HTMLElement {
  const row = element("tr", "detail-row");
  row.hidden = true;
  const cell = element("td") as HTMLTableCellElement;
  cell.colSpan = 10;

  const grid = element("div", "detail-grid");
  grid.append(statsPanel(column), valuesPanel(column.text));
  cell.append(grid);
  row.append(cell);
  return row;
}

function statsPanel(column: ColumnProfile): HTMLElement {
  const panel = element("div", "detail");
  panel.append(element("h3", undefined, "Details"));

  const list = element("dl", "kv");
  const entries: Array<[string, string]> = [
    ["Parsed as integer", formatInt(column.typeCounts.integer)],
    ["Parsed as number", formatInt(column.typeCounts.float)],
    ["Parsed as boolean", formatInt(column.typeCounts.boolean)],
    ["Parsed as date", formatInt(column.typeCounts.date)],
    ["Length (min / max)", `${formatOptional(column.text.minLength)} / ${formatOptional(column.text.maxLength)}`],
    ["Mean length", formatOptional(column.text.meanLength)],
  ];
  if (column.numeric?.sum != null) entries.push(["Sum", formatNumber(column.numeric.sum)]);
  if (column.decimalStyle) {
    entries.push(["Decimal separator", column.decimalStyle === "comma" ? "comma (1.234,56)" : "dot (1,234.56)"]);
  }

  for (const [key, value] of entries) {
    list.append(element("dt", undefined, key), element("dd", undefined, value));
  }
  panel.append(list);
  return panel;
}

function valuesPanel(text: TextSummary): HTMLElement {
  const panel = element("div", "detail");
  panel.append(element("h3", undefined, "Most frequent values"));

  if (text.topValues.length === 0) {
    panel.append(element("p", "nil", "No values."));
    return panel;
  }

  const peak = text.topValues[0]?.count ?? 1;
  const list = element("div", "top-values");
  for (const { value, count } of text.topValues) {
    const item = element("div", "top-value");
    const label = element("span", "top-value__label", value === "" ? "(empty)" : value);
    label.title = value;
    label.style.setProperty("--fill", `${(count / peak) * 100}%`);
    item.append(label, element("span", "top-value__count", formatInt(count)));
    list.append(item);
  }
  panel.append(list);
  return panel;
}

/* --------------------------------------------------------------- utilities */

function distinctLabel(text: TextSummary): string {
  // The counter caps out on high-cardinality columns; say so rather than lie.
  return text.distinctIsExact ? formatInt(text.distinct) : `≥ ${formatInt(text.distinct)}`;
}
