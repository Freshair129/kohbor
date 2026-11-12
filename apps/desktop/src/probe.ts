export const MEDIA_PROBE_TIMEOUT_MS = 5_000;

export interface MediaProbeItem {
  path_sha256: string;
  url: string;
  duration_ms: number;
  visual_tail_ms: number;
}

export interface MediaProbeRequest {
  install_id: string;
  revision: number;
  items: MediaProbeItem[];
}

export interface MediaProbeContext {
  install_id: string;
  revision: number;
  path_sha256: string;
}

export interface MediaProbeReport extends MediaProbeContext {
  ok: boolean;
  duration_ms: number;
}

/** The pending command returns item-level context so a reloaded renderer can report each item. */
export interface MediaProbePendingItem extends MediaProbeItem, MediaProbeContext {}

export type MediaProbePendingResponse = MediaProbeRequest | MediaProbePendingItem[];

const INSTALL_ID = /^ins_[0-9]{10}$/;
const PATH_SHA256 = /^[0-9a-f]{64}$/;
const LOGICAL_URL = /^kohbor-media:\/\/localhost\/v1\/(ins_[0-9]{10})\/([0-9a-f]{64})$/;

function objectRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => Object.hasOwn(value, key));
}

function safeInteger(value: unknown, minimum: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= minimum;
}

function parseItem(value: unknown, installId: string): MediaProbeItem | null {
  const item = objectRecord(value);
  if (!item || !hasExactKeys(item, ["path_sha256", "url", "duration_ms", "visual_tail_ms"])) return null;
  if (typeof item.path_sha256 !== "string" || !PATH_SHA256.test(item.path_sha256)) return null;
  if (typeof item.url !== "string") return null;

  // This logical URL is converted by the DOM glue with toWebviewUrl; the media-src CSP entry belongs to S3-5l.
  const logicalMatch = LOGICAL_URL.exec(item.url);
  if (!logicalMatch || logicalMatch[0] !== item.url || logicalMatch[1] !== installId || logicalMatch[2] !== item.path_sha256) {
    return null;
  }
  if (!safeInteger(item.duration_ms, 1) || !safeInteger(item.visual_tail_ms, 0)) return null;

  return {
    path_sha256: item.path_sha256,
    url: item.url,
    duration_ms: item.duration_ms,
    visual_tail_ms: item.visual_tail_ms,
  };
}

/** Parse one native probe-request payload and discard only items that fail strict validation. */
export function parseProbeRequest(value: unknown): MediaProbeRequest | null {
  const request = objectRecord(value);
  if (!request || !hasExactKeys(request, ["install_id", "revision", "items"])) return null;
  if (typeof request.install_id !== "string" || !INSTALL_ID.test(request.install_id)) return null;
  if (!safeInteger(request.revision, 1) || !Array.isArray(request.items)) return null;

  return {
    install_id: request.install_id,
    revision: request.revision,
    items: request.items.flatMap((item) => {
      const parsed = parseItem(item, request.install_id as string);
      return parsed ? [parsed] : [];
    }),
  };
}

/** Parse pending items with their report context, or a full request envelope if supplied. */
export function parsePendingProbeRequests(value: unknown): MediaProbeRequest[] {
  const envelope = parseProbeRequest(value);
  if (envelope) return [envelope];
  if (!Array.isArray(value)) return [];

  const grouped = new Map<string, MediaProbeRequest>();
  for (const rawItem of value) {
    const item = objectRecord(rawItem);
    if (!item || !hasExactKeys(item, [
      "install_id", "revision", "path_sha256", "url", "duration_ms", "visual_tail_ms",
    ])) continue;
    if (typeof item.install_id !== "string" || !INSTALL_ID.test(item.install_id)) continue;
    if (!safeInteger(item.revision, 1)) continue;
    const parsedItem = parseItem({
      path_sha256: item.path_sha256,
      url: item.url,
      duration_ms: item.duration_ms,
      visual_tail_ms: item.visual_tail_ms,
    }, item.install_id);
    if (!parsedItem) continue;

    const key = `${item.install_id}:${item.revision}`;
    const existing = grouped.get(key);
    if (existing) existing.items.push(parsedItem);
    else grouped.set(key, { install_id: item.install_id, revision: item.revision, items: [parsedItem] });
  }
  return [...grouped.values()];
}

/** Build the closed renderer report; failures always carry a zero duration. */
export function buildProbeReport(context: MediaProbeContext, ok: boolean, duration_ms: number): MediaProbeReport {
  const reportOk = ok && Number.isSafeInteger(duration_ms) && duration_ms > 0;
  return {
    install_id: context.install_id,
    revision: context.revision,
    path_sha256: context.path_sha256,
    ok: reportOk,
    duration_ms: reportOk ? duration_ms : 0,
  };
}

/** Convert HTMLMediaElement.duration (seconds) to a reportable integer millisecond value. */
export function durationSecondsToMilliseconds(seconds: number): number | null {
  if (!Number.isFinite(seconds) || seconds <= 0) return null;
  const milliseconds = Math.round(seconds * 1_000);
  return Number.isSafeInteger(milliseconds) && milliseconds > 0 ? milliseconds : null;
}

export interface ProbeClock {
  setTimeout(callback: () => void, delay_ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

export type ProbeTimedResult<T> =
  | { status: "resolved"; value: T }
  | { status: "rejected"; error: unknown }
  | { status: "timeout" };

/** Race decode work against a clock supplied by the caller, making the 5 s bound testable. */
export function withProbeTimeout<T>(
  work: Promise<T>,
  clock: ProbeClock,
  timeout_ms = MEDIA_PROBE_TIMEOUT_MS,
): Promise<ProbeTimedResult<T>> {
  return new Promise((resolve) => {
    let settled = false;
    let timer: unknown;
    const finish = (result: ProbeTimedResult<T>) => {
      if (settled) return;
      settled = true;
      clock.clearTimeout(timer);
      resolve(result);
    };
    timer = clock.setTimeout(() => finish({ status: "timeout" }), timeout_ms);
    work.then(
      (value) => finish({ status: "resolved", value }),
      (error: unknown) => finish({ status: "rejected", error }),
    );
  });
}

/** Queue asynchronous probes without exceeding the requested number of concurrent decodes. */
export function createProbeQueue<T>(concurrency: number, worker: (item: T) => Promise<void>): (items: readonly T[]) => void {
  if (!Number.isSafeInteger(concurrency) || concurrency < 1) throw new RangeError("concurrency must be a positive integer");
  const pending: T[] = [];
  let active = 0;

  const drain = () => {
    while (active < concurrency && pending.length > 0) {
      const item = pending.shift() as T;
      active += 1;
      void Promise.resolve()
        .then(() => worker(item))
        .catch(() => undefined)
        .finally(() => {
          active -= 1;
          drain();
        });
    }
  };

  return (items) => {
    pending.push(...items);
    drain();
  };
}
