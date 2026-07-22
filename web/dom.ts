/** Small shared helpers for building and formatting the page. */

export function must<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing element #${id}`);
  return node as T;
}

export function element(tag: string, className?: string, text?: string): HTMLElement {
  const node = document.createElement(tag);
  if (className) node.className = className;
  // textContent, never innerHTML: column names and values come from the file.
  if (text !== undefined) node.textContent = text;
  return node;
}

const integerFormat = new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 });
const numberFormat = new Intl.NumberFormat(undefined, { maximumSignificantDigits: 6 });

export function formatInt(value: number): string {
  return integerFormat.format(value);
}

export function formatNumber(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return numberFormat.format(value);
}

export function formatOptional(value: number | null | undefined): string {
  return value == null ? "—" : formatNumber(value);
}

export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${ms.toFixed(0)} ms`;
}
