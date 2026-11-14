export type LogLevel = "error" | "warn" | "info" | "debug";
export type LogFilter = "all" | LogLevel;
export interface LogEntry {
  seq: number;
  ts: string | number;
  level: LogLevel;
  module: string;
  event: string;
  msg: string;
  fields: Record<string, unknown>;
}
export interface LogTailResponse { entries: LogEntry[]; last_seq: number; dropped: boolean }
export const LOG_ROW_LIMIT = 1000;
const severity: Record<LogLevel, number> = { error: 0, warn: 1, info: 2, debug: 3 };
const object = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const sequence = (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

export function parseLogTail(value: unknown): LogTailResponse | null {
  if (!object(value) || !Array.isArray(value.entries) || !sequence(value.last_seq) || typeof value.dropped !== "boolean") return null;
  const lastSeq = value.last_seq;
  if (!value.entries.every(entry => object(entry) && sequence(entry.seq) && entry.seq <= lastSeq
    && (typeof entry.ts === "string" || (typeof entry.ts === "number" && Number.isFinite(entry.ts)))
    && typeof entry.level === "string" && Object.hasOwn(severity, entry.level)
    && typeof entry.module === "string" && typeof entry.event === "string" && typeof entry.msg === "string" && object(entry.fields))) return null;
  return { entries: value.entries as LogEntry[], last_seq: lastSeq, dropped: value.dropped };
}

const oneLine = (value: string | number): string => String(value).replace(/\r?\n|\r/g, "\\n");
export function formatLogBody(entry: LogEntry): string {
  let fields = "—";
  try { fields = JSON.stringify(entry.fields); } catch { /* An unformattable field stays unknown. */ }
  return [entry.module, entry.event, entry.msg].map(oneLine).join(" ") + ` ${fields}`;
}

export function formatLogLine(entry: LogEntry): string {
  return `${oneLine(entry.ts)} ${entry.level} ${formatLogBody(entry)}`;
}

export function filterLogEntries(entries: readonly LogEntry[], level: LogFilter, text: string): LogEntry[] {
  const needle = text.trim().toLocaleLowerCase();
  return entries.filter(entry => (level === "all" || severity[entry.level] <= severity[level])
    && (!needle || formatLogLine(entry).toLocaleLowerCase().includes(needle)));
}

export function copyLogLines(entries: readonly LogEntry[], count: number): string {
  return entries.slice(-Math.max(1, Math.floor(count))).map(formatLogLine).join("\n");
}

export function mergeLogEntries(previous: readonly LogEntry[], incoming: readonly LogEntry[]): LogEntry[] {
  const bySeq = new Map(previous.map(entry => [entry.seq, entry]));
  for (const entry of incoming) bySeq.set(entry.seq, entry);
  return [...bySeq.values()].sort((a, b) => a.seq - b.seq).slice(-LOG_ROW_LIMIT);
}

export function shouldFollowLogs(box: { scrollTop: number; scrollHeight: number; clientHeight: number }): boolean {
  return box.scrollHeight - box.clientHeight - box.scrollTop <= 2;
}
