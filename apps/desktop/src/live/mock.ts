import { createFrameBuffer } from "./frame";
import { applyProposal, type CaptureConfig, type CaptureConfigPatch } from "./captureConfig";
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
let mockPhaseReason = new URLSearchParams(window.location.search).get("reason");
if (mockPhaseReason !== null) scenario = scenarioForReason(mockPhaseReason);
let invalidated = mockPhaseReason === "user_invalidated";
let frameSequence = 0;
const mockProfileRect = { x: 0, y: 788, width: 292, height: 292 };
const mockOutputBounds = { width: 1920, height: 1080 };
let mockCaptureConfig: CaptureConfig = {
  schema_version: 1,
  crop: { dx: 0, dy: 0, dw: 0, dh: 0 },
  display_fps: 15,
  paused: false,
  profile_rect: mockProfileRect,
  effective_rect: mockProfileRect,
  proposal: null,
  proposal_status: "none",
  crop_confirmed: false,
};

function copyMockCaptureConfig(): CaptureConfig {
  return {
    ...mockCaptureConfig,
    crop: { ...mockCaptureConfig.crop },
    profile_rect: mockCaptureConfig.profile_rect ? { ...mockCaptureConfig.profile_rect } : null,
    effective_rect: mockCaptureConfig.effective_rect ? { ...mockCaptureConfig.effective_rect } : null,
    proposal: mockCaptureConfig.proposal ? { ...mockCaptureConfig.proposal } : null,
  };
}

function updateMockConfig(patch: CaptureConfigPatch): CaptureConfig {
  const crop = patch.crop ? { ...mockCaptureConfig.crop, ...patch.crop } : mockCaptureConfig.crop;
  const profile = mockCaptureConfig.profile_rect;
  const effective = profile
    ? {
        x: profile.x + crop.dx,
        y: profile.y + crop.dy,
        width: profile.width + crop.dw,
        height: profile.height + crop.dh,
      }
    : null;
  if (patch.crop && effective && (
    effective.x < 0 || effective.y < 0 ||
    effective.x + effective.width > mockOutputBounds.width ||
    effective.y + effective.height > mockOutputBounds.height ||
    effective.width < 64 || effective.width > 400 || effective.height < 64 || effective.height > 400
  )) throw new Error("CAPTURE_CONFIG_INVALID");

  mockCaptureConfig = {
    ...mockCaptureConfig,
    crop,
    display_fps: patch.display_fps ?? mockCaptureConfig.display_fps,
    paused: patch.paused ?? mockCaptureConfig.paused,
    crop_confirmed: patch.crop ? false : mockCaptureConfig.crop_confirmed,
    effective_rect: effective,
  };
  if (patch.crop) recoverMockUserInvalidation();
  publishCapture();
  return copyMockCaptureConfig();
}

function captureStatus() {
  if (scenario === "unknown") return null;
  const measured = !mockCaptureConfig.paused && scenario === "live" ? mockCaptureConfig.display_fps : null;
  const age = scenario === "live" ? 32 : scenario === "stale" ? 3_200 : null;
  return {
    phase: scenario,
    fps_measured: measured,
    grab_fps: measured,
    sent_total: frameSequence,
    grab_total: frameSequence,
    frame_age_ms: age,
    rect_profile: "1920x1080@100%",
    calibration_generation: generationFor(scenario),
    auto_confirmed: scenario === "live" || scenario === "stale",
    crop_confirmed: mockCaptureConfig.crop_confirmed,
    display_fps: mockCaptureConfig.display_fps,
    paused: mockCaptureConfig.paused,
    phase_reason: mockPhaseReason ?? (mockCaptureConfig.paused ? "paused" : scenario === "calibrating" ? "calibrating" : scenario === "lost" ? "reopening" : null),
    grab_ms_mean: scenario === "live" && !mockCaptureConfig.paused ? 1.5 : null,
    analysis_ms_mean: scenario === "live" && !mockCaptureConfig.paused ? 4.2 : null,
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
  const captureActive = !mockCaptureConfig.paused && scenario === "live";
  const captureStale = !mockCaptureConfig.paused && scenario === "stale";
  return {
    working_set_bytes: 188_400_000,
    gpu_local_bytes: 96_700_000,
    gpu_group_bytes: null,
    gpu_util: null,
    cpu_percent: 2.4,
    capture_fps: captureActive ? mockCaptureConfig.display_fps : null,
    frame_age_ms: captureActive ? 32 : captureStale ? 3_200 : null,
  };
}

function emit(listeners: Set<Listener>, value: unknown) {
  for (const listener of listeners) listener(value);
}

function makeMockFrame(): ArrayBuffer {
  const width = mockCaptureConfig.effective_rect?.width ?? mockProfileRect.width;
  const height = mockCaptureConfig.effective_rect?.height ?? mockProfileRect.height;
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
  let disposed = false;
  let timer = 0;
  const publish = () => {
    if (disposed) return;
    if (!mockCaptureConfig.paused && scenario === "live") onFrame(makeMockFrame());
    timer = window.setTimeout(publish, 1_000 / mockCaptureConfig.display_fps);
  };
  if (!mockCaptureConfig.paused && scenario === "stale") onFrame(makeMockFrame());
  publish();
  return () => {
    disposed = true;
    window.clearTimeout(timer);
  };
}

export function invalidateMock() {
  invalidated = true;
  mockPhaseReason = "user_invalidated";
  scenario = "unsupported";
  publishCapture();
  publishRisk(null);
}

function scenarioForReason(reason: string): MockScenario {
  if (reason === "paused" || reason === "calibrating") return "calibrating";
  if (reason === "reopening" || reason === "access_lost") return "lost";
  return "unsupported";
}

/** Explicit development controls; this module is guarded by import.meta.env.DEV. */
export function setMockCaptureReason(selection: string) {
  mockPhaseReason = selection === "live" || selection === "none" ? null : selection;
  invalidated = selection === "user_invalidated";
  scenario = selection === "live" ? "live" : selection === "none" ? "unsupported" : scenarioForReason(selection);
  publishCapture();
  publishRisk(null);
}

function recoverMockUserInvalidation() {
  if (!invalidated) return;
  invalidated = false;
  mockPhaseReason = null;
  scenario = "calibrating";
  publishCapture();
  publishRisk(null);
  window.setTimeout(() => {
    if (!invalidated && mockPhaseReason === null && scenario === "calibrating") {
      scenario = "live";
      publishCapture();
      publishRisk(null);
    }
  }, 300);
}

export async function getMockCaptureConfig(): Promise<CaptureConfig> {
  return copyMockCaptureConfig();
}

export async function invokeMockCaptureConfig(command: string, args?: Record<string, unknown>): Promise<CaptureConfig> {
  if (command === "capture_get_config") return copyMockCaptureConfig();
  if (command === "capture_retry") {
    recoverMockUserInvalidation();
    return copyMockCaptureConfig();
  }
  if (command === "capture_set_config") {
    const patch = args?.patch;
    if (typeof patch !== "object" || patch === null || Array.isArray(patch)) throw new Error("CAPTURE_CONFIG_INVALID");
    return updateMockConfig(patch as CaptureConfigPatch);
  }
  if (command === "capture_reset_crop") {
    return updateMockConfig({ crop: { dx: 0, dy: 0, dw: 0, dh: 0 } });
  }
  if (command === "capture_propose_crop") {
    mockCaptureConfig = { ...mockCaptureConfig, proposal: null, proposal_status: "searching" };
    window.setTimeout(() => {
      const notFound = new URLSearchParams(window.location.search).get("proposal") === "not_found";
      mockCaptureConfig = {
        ...mockCaptureConfig,
        proposal: notFound ? null : { x: 0, y: 800, width: 280, height: 274 },
        proposal_status: notFound ? "not_found" : "found",
      };
    }, 500);
    return copyMockCaptureConfig();
  }
  if (command === "capture_accept_proposal") {
    const proposal = mockCaptureConfig.proposal;
    const patch = proposal && applyProposal(proposal, mockCaptureConfig.profile_rect);
    if (!patch?.crop) throw new Error("CAPTURE_CONFIG_INVALID");
    updateMockConfig(patch);
    mockCaptureConfig = {
      ...mockCaptureConfig,
      crop_confirmed: true,
      proposal: null,
      proposal_status: "none",
    };
    publishCapture();
    return copyMockCaptureConfig();
  }
  if (command === "capture_confirm_crop") {
    mockCaptureConfig = { ...mockCaptureConfig, crop_confirmed: true };
    recoverMockUserInvalidation();
    publishCapture();
    return copyMockCaptureConfig();
  }
  throw new Error(`UNKNOWN_MOCK_COMMAND:${command}`);
}
