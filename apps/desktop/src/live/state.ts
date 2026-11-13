// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type {
  CapturePhase,
  CaptureStatus,
  MetricSummary,
  ResourceMetrics,
  ResourceMetricKey,
  ResourceSummary,
  RiskState,
  VisionEvent,
} from "./types.ts";

export const UNKNOWN_AFTER_MS = 3000;
export const VISION_EVENT_MS = 5000;

const CAPTURE_PHASES = new Set<CapturePhase>([
  "unsupported",
  "calibrating",
  "live",
  "stale",
  "coverage_unavailable",
  "lost",
  "unknown",
]);
const RISK_EVENTS = new Set(["enemy_missing", "risk_alert", "risk_revision", "risk_unavailable"]);
const VISION_EVENTS = new Set<VisionEvent>(["enemy_missing", "risk_alert", "risk_revision"]);
const RESOURCE_KEYS: ResourceMetricKey[] = [
  "appGroupBytes",
  "hostGpuBytes",
  "webviewGpuBytes",
  "gpuLoadPercent",
  "cpuPercent",
  "captureFps",
  "frameAgeMs",
];

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function nullableNumber(value: unknown): number | null | undefined {
  if (value === null) return null;
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function nullableInteger(value: unknown): number | null | undefined {
  const parsed = nullableNumber(value);
  return parsed === null || parsed === undefined ? parsed : Number.isInteger(parsed) ? parsed : undefined;
}

export function parseCaptureStatus(input: unknown): CaptureStatus | null {
  if (!record(input) || typeof input.phase !== "string" || !CAPTURE_PHASES.has(input.phase as CapturePhase)) return null;
  if (!Object.hasOwn(input, "fps_measured") || !Object.hasOwn(input, "frame_age_ms") ||
      !Object.hasOwn(input, "calibration_generation") || !Object.hasOwn(input, "rect_profile") ||
      !Object.hasOwn(input, "auto_confirmed")) return null;
  const fps = nullableNumber(input.fps_measured);
  const age = nullableNumber(input.frame_age_ms);
  const generation = nullableInteger(input.calibration_generation);
  const rectProfile = input.rect_profile === null || typeof input.rect_profile === "string" ? input.rect_profile : undefined;
  if (fps === undefined || age === undefined || generation === undefined || rectProfile === undefined || typeof input.auto_confirmed !== "boolean") return null;
  return {
    phase: input.phase as CapturePhase,
    fps_measured: fps,
    frame_age_ms: age,
    rect_profile: rectProfile,
    calibration_generation: generation,
    auto_confirmed: input.auto_confirmed,
  };
}

export function parseRiskState(input: unknown): RiskState | null {
  if (!record(input)) return null;
  if (!Object.hasOwn(input, "meter") || !Object.hasOwn(input, "gsi_source") ||
      !Object.hasOwn(input, "vision_source") || !Object.hasOwn(input, "visible") ||
      !Object.hasOwn(input, "baseline") || !Object.hasOwn(input, "events")) return null;
  const meter = nullableInteger(input.meter);
  const visible = nullableInteger(input.visible);
  const baseline = nullableInteger(input.baseline);
  const gsiSource = input.gsi_source;
  const visionSource = input.vision_source;
  if (
    meter === undefined || (meter !== null && (meter < 0 || meter > 3)) ||
    visible === undefined || baseline === undefined || typeof gsiSource !== "boolean" || typeof visionSource !== "boolean"
  ) return null;
  const events = input.events;
  if (!Array.isArray(events) || events.some((event) => typeof event !== "string" || !RISK_EVENTS.has(event))) return null;
  return {
    meter,
    visible,
    baseline,
    gsi_source: gsiSource,
    vision_source: visionSource,
    events: events as RiskState["events"],
  };
}

export function parseResourceMetrics(input: unknown): ResourceMetrics | null {
  if (!record(input)) return null;
  const fields = ["working_set_bytes", "cpu_percent", "gpu_local_bytes", "gpu_group_bytes", "gpu_util", "capture_fps", "frame_age_ms"];
  if (fields.some((field) => !Object.hasOwn(input, field))) return null;
  const appGroupBytes = nullableNumber(input.working_set_bytes);
  const hostGpuBytes = nullableNumber(input.gpu_local_bytes);
  const webviewGpuBytes = nullableNumber(input.gpu_group_bytes);
  const gpuLoadPercent = nullableNumber(input.gpu_util);
  const cpuPercent = nullableNumber(input.cpu_percent);
  const captureFps = nullableNumber(input.capture_fps);
  const frameAgeMs = nullableNumber(input.frame_age_ms);
  if (
    appGroupBytes === undefined || hostGpuBytes === undefined || webviewGpuBytes === undefined ||
    gpuLoadPercent === undefined || cpuPercent === undefined || captureFps === undefined || frameAgeMs === undefined
  ) return null;
  return {
    appGroupBytes,
    hostGpuBytes,
    webviewGpuBytes,
    gpuLoadPercent,
    cpuPercent,
    captureFps,
    frameAgeMs,
  };
}

export function isFresh<T>(value: T | null, receivedAt: number | null, now: number, timeoutMs = UNKNOWN_AFTER_MS): value is T {
  return value !== null && receivedAt !== null && now - receivedAt <= timeoutMs;
}

export function riskTag(state: RiskState): "GSI + map" | "GSI only" | "—" {
  if (!state.gsi_source) return "—";
  return state.vision_source ? "GSI + map" : "GSI only";
}

export function expireVisionEvent(event: VisionEvent | null, receivedAt: number | null, now: number): VisionEvent | null {
  return event !== null && receivedAt !== null && now - receivedAt <= VISION_EVENT_MS ? event : null;
}

export function emptyResourceSummary(): ResourceSummary {
  return Object.fromEntries(RESOURCE_KEYS.map((key) => [key, { peak: null, mean: null, count: 0 }])) as ResourceSummary;
}

export function addResourceSample(summary: ResourceSummary, sample: ResourceMetrics): ResourceSummary {
  const next = { ...summary } as ResourceSummary;
  for (const key of RESOURCE_KEYS) {
    const value = sample[key];
    const previous: MetricSummary = summary[key];
    if (value === null) continue;
    next[key] = {
      peak: previous.peak === null ? value : Math.max(previous.peak, value),
      mean: ((previous.mean ?? 0) * previous.count + value) / (previous.count + 1),
      count: previous.count + 1,
    };
  }
  return next;
}

export function formatNullable(value: number | null, decimals = 0): string {
  if (value === null || !Number.isFinite(value)) return "—";
  const rounded = Number(value.toFixed(decimals));
  return String(rounded);
}

export function formatMegabytes(bytes: number | null): string {
  return bytes === null ? "—" : `${(bytes / 1_000_000).toFixed(1)} MB`;
}

export function freshResourceMetrics(
  value: ResourceMetrics | null,
  receivedAt: number | null,
  now: number,
): ResourceMetrics | null {
  return isFresh(value, receivedAt, now) ? value : null;
}
