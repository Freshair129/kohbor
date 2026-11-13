import { createFrameBuffer } from "./frame";
import type { CapturePhase, RiskState, VisionEvent } from "./types";

if (!import.meta.env.DEV) throw new Error("live mock is available only in development builds");

type MockScenario = CapturePhase | "unknown";
type Listener = (payload: unknown) => void;

const PHASES = new Set<MockScenario>([
  "live",
  "stale",
  "coverage_unavailable",
  "calibrating",
  "lost",
  "unsupported",
  "unknown",
]);
const generationFor = (scenario: MockScenario) => scenario === "calibrating" ? 8 : 7;
const captureListeners = new Set<Listener>();
const riskListeners = new Set<Listener>();
const resourceListeners = new Set<Listener>();

function initialScenario(): MockScenario {
  const value = new URLSearchParams(window.location.search).get("state");
  return value && PHASES.has(value as MockScenario) ? value as MockScenario : "live";
}

let scenario = initialScenario();
let invalidated = false;
let frameSequence = 0;

function captureStatus() {
  if (scenario === "unknown") return null;
  const measured = scenario === "live" ? 30 : scenario === "stale" ? 18 : null;
  const age = scenario === "live" ? 32 : scenario === "stale" ? 3_200 : null;
  return {
    phase: scenario,
    fps_measured: measured,
    frame_age_ms: age,
    rect_profile: "1920x1080@100%",
    calibration_generation: generationFor(scenario),
    auto_confirmed: scenario === "live" || scenario === "stale",
  };
}

function visionEventFromQuery(): VisionEvent | null {
  const value = new URLSearchParams(window.location.search).get("event");
  return value === "enemy_missing" || value === "risk_alert" || value === "risk_revision" ? value : "enemy_missing";
}

function riskState(event: VisionEvent | null): RiskState {
  const visionAvailable = scenario === "live" || scenario === "stale";
  return {
    meter: 2,
    gsi_source: scenario !== "unknown",
    vision_source: visionAvailable,
    visible: visionAvailable ? 3 : null,
    baseline: visionAvailable ? 2 : null,
    events: event ? [event] : [],
  };
}

function resourceSample() {
  return {
    working_set_bytes: 188_400_000,
    gpu_local_bytes: 96_700_000,
    gpu_group_bytes: null,
    gpu_util: null,
    cpu_percent: 2.4,
    capture_fps: scenario === "live" ? 30 : scenario === "stale" ? 18 : null,
    frame_age_ms: scenario === "live" ? 32 : scenario === "stale" ? 3_200 : null,
  };
}

function emit(listeners: Set<Listener>, value: unknown) {
  for (const listener of listeners) listener(value);
}

function makeMockFrame(): ArrayBuffer {
  const width = 256;
  const height = 256;
  const stride = width * 4;
  const pixels = new Uint8Array(stride * height);
  const dots: { x: number; y: number; color: readonly [number, number, number] }[] = [
    { x: 54, y: 68, color: [255, 122, 70] },
    { x: 178, y: 102, color: [255, 122, 70] },
    { x: 116, y: 181, color: [92, 170, 255] },
  ];
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const offset = y * stride + x * 4;
      const grid = x % 32 === 0 || y % 32 === 0;
      let red = 19 + Math.floor((x / width) * 31);
      let green = 40 + Math.floor((y / height) * 46);
      let blue = 26 + Math.floor(((x + y) / (width + height)) * 30);
      if (grid) {
        red += 15;
        green += 18;
        blue += 13;
      }
      for (const dot of dots) {
        const distance = (x - dot.x) ** 2 + (y - dot.y) ** 2;
        if (distance <= 7 ** 2) [red, green, blue] = dot.color;
      }
      pixels[offset] = blue;
      pixels[offset + 1] = green;
      pixels[offset + 2] = red;
      pixels[offset + 3] = 255;
    }
  }
  frameSequence += 1;
  return createFrameBuffer({
    flags: 0,
    width,
    height,
    stride,
    session_epoch: 1,
    calibration_generation: generationFor(scenario),
    frame_seq: frameSequence,
    age_ms: scenario === "stale" ? 3_200 : 32,
  }, pixels);
}

function publishCapture() {
  const status = captureStatus();
  if (status) emit(captureListeners, status);
}

function publishRisk(event: VisionEvent | null) {
  emit(riskListeners, riskState(event));
}

function publishResource() {
  emit(resourceListeners, resourceSample());
}

export function subscribeCapture(onPayload: Listener): () => void {
  captureListeners.add(onPayload);
  const initial = captureStatus();
  if (initial) onPayload(initial);
  const timer = window.setInterval(() => {
    const next = captureStatus();
    if (next) onPayload(next);
  }, 1000);
  return () => {
    captureListeners.delete(onPayload);
    window.clearInterval(timer);
  };
}

export function subscribeRisk(onPayload: Listener): () => void {
  riskListeners.add(onPayload);
  if (scenario !== "unknown") onPayload(riskState(visionEventFromQuery()));
  const timer = window.setInterval(() => {
    if (scenario !== "unknown") onPayload(riskState(null));
  }, 1000);
  return () => {
    riskListeners.delete(onPayload);
    window.clearInterval(timer);
  };
}

export function subscribeResources(onPayload: Listener): () => void {
  resourceListeners.add(onPayload);
  if (scenario !== "unknown") onPayload(resourceSample());
  const timer = window.setInterval(() => {
    if (scenario !== "unknown") publishResource();
  }, 1000);
  return () => {
    resourceListeners.delete(onPayload);
    window.clearInterval(timer);
  };
}

export function subscribeFrames(onFrame: (buffer: ArrayBuffer) => void): () => void {
  const publish = () => {
    if (scenario === "live" || scenario === "stale") onFrame(makeMockFrame());
  };
  publish();
  const timer = window.setInterval(() => {
    if (scenario === "live") publish();
  }, 400);
  return () => {
    window.clearInterval(timer);
  };
}

export function invalidateMock() {
  if (invalidated) return;
  invalidated = true;
  scenario = "calibrating";
  publishCapture();
  publishRisk(null);
}
