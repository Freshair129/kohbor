import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const state = await import("../src/live/state.ts");
const meter = await import("../src/live/meter.ts");
const frame = await import("../src/live/frame.ts");

const capturePayload = (overrides: Record<string, unknown> = {}) => ({
  phase: "live",
  fps_measured: 30,
  frame_age_ms: 24,
  rect_profile: "1920x1080@100%",
  calibration_generation: 3,
  auto_confirmed: true,
  ...overrides,
});

const riskPayload = (overrides: Record<string, unknown> = {}) => ({
  meter: 2,
  gsi_source: true,
  vision_source: true,
  visible: 3,
  baseline: 1,
  events: ["risk_alert"],
  ...overrides,
});

const resourcePayload = (overrides: Record<string, unknown> = {}) => ({
  working_set_bytes: 1_000_000,
  cpu_percent: 2.5,
  gpu_local_bytes: 500_000,
  gpu_group_bytes: null,
  gpu_util: null,
  capture_fps: 30,
  frame_age_ms: 24,
  ...overrides,
});

function makeHeader(overrides: Partial<{
  flags: number;
  width: number;
  height: number;
  stride: number;
  session_epoch: number;
  calibration_generation: number;
  frame_seq: number;
  age_ms: number;
}> = {}) {
  return {
    flags: 0,
    width: 2,
    height: 1,
    stride: 8,
    session_epoch: 4,
    calibration_generation: 9,
    frame_seq: 11,
    age_ms: 13,
    ...overrides,
  };
}

function makeFrame(overrides: Parameters<typeof makeHeader>[0] = {}) {
  const header = makeHeader(overrides);
  return frame.createFrameBuffer(header, new Uint8Array(header.stride * header.height));
}

function corrupt(buffer: ArrayBuffer, write: (view: DataView) => void): ArrayBuffer {
  const copy = buffer.slice(0);
  write(new DataView(copy));
  return copy;
}

test("capture and risk payloads parse strictly and become unknown after 3000 ms", () => {
  const capture = state.parseCaptureStatus({ ...capturePayload(), extra_field: "ignored" });
  assert.ok(capture);
  assert.equal(capture.frame_age_ms, 24);
  assert.equal(state.parseCaptureStatus(capturePayload({ fps_measured: "30" })), null);
  assert.equal(state.parseCaptureStatus(capturePayload({ auto_confirmed: null })), null);
  assert.equal(state.isFresh(capture, 10_000, 13_000), true);
  assert.equal(state.isFresh(capture, 10_000, 13_001), false);
  assert.equal(state.isFresh(null, 10_000, 10_000), false);

  const risk = state.parseRiskState({ ...riskPayload({ meter: null, events: ["risk_unavailable"] }), extra: 1 });
  assert.ok(risk);
  assert.equal(risk.meter, null);
  assert.equal(state.parseRiskState(riskPayload({ meter: 4 })), null);
  assert.equal(state.parseRiskState(riskPayload({ vision_source: null })), null);
  assert.equal(state.parseRiskState(riskPayload({ events: ["unknown_event"] })), null);
  assert.equal(state.isFresh(risk, 10_000, 13_001), false);
});

test("frame parser accepts the 32-byte KMAP v1 header and preserves all fields", () => {
  const buffer = frame.createFrameBuffer(
    makeHeader({ flags: 5, session_epoch: 12, frame_seq: 88, age_ms: 37 }),
    new Uint8Array([1, 2, 3, 255, 4, 5, 6, 255]),
  );
  const parsed = frame.parseFrame(buffer);
  assert.ok(parsed);
  assert.equal(buffer.byteLength, 40);
  assert.deepEqual(parsed.header, {
    flags: 5,
    width: 2,
    height: 1,
    stride: 8,
    session_epoch: 12,
    calibration_generation: 9,
    frame_seq: 88,
    age_ms: 37,
    byte_length: 8,
  });
  assert.deepEqual([...parsed.bgra], [1, 2, 3, 255, 4, 5, 6, 255]);
});

test("frame parser rejects each invalid header and payload size", () => {
  const valid = makeFrame();
  assert.equal(frame.parseFrame(valid.slice(0, 31)), null); // Header shorter than 32 bytes.
  assert.equal(frame.parseFrame(corrupt(valid, (view) => view.setUint32(0, 0, true))), null); // Wrong magic.
  assert.equal(frame.parseFrame(corrupt(valid, (view) => view.setUint16(4, 2, true))), null); // Wrong version.
  assert.equal(frame.parseFrame(corrupt(valid, (view) => view.setUint16(8, 0, true))), null); // Zero width.
  assert.equal(frame.parseFrame(corrupt(valid, (view) => view.setUint16(10, 0, true))), null); // Zero height.
  assert.equal(frame.parseFrame(corrupt(valid, (view) => view.setUint16(8, 4097, true))), null); // Width > 4096.
  assert.equal(frame.parseFrame(corrupt(valid, (view) => view.setUint16(10, 4097, true))), null); // Height > 4096.
  assert.equal(frame.parseFrame(corrupt(valid, (view) => view.setUint32(12, 7, true))), null); // Stride < width * 4.
  assert.equal(frame.parseFrame(valid.slice(0, valid.byteLength - 1)), null); // Truncated payload.
  const extraByte = new Uint8Array(valid.byteLength + 1);
  extraByte.set(new Uint8Array(valid));
  assert.equal(frame.parseFrame(extraByte.buffer), null); // Extra payload byte.
  assert.equal(frame.parseFrame(new Uint8Array(valid) as unknown as ArrayBuffer), null); // Channel type is ArrayBuffer.
});

test("BGRA conversion swizzles pixels, skips row padding, and reuses its ArrayBuffer-backed destination", () => {
  const parsed = frame.parseFrame(frame.createFrameBuffer(
    makeHeader({ height: 2, stride: 12 }),
    new Uint8Array([
      3, 2, 1, 255, 6, 5, 4, 255, 0, 0, 0, 0,
      9, 8, 7, 255, 12, 11, 10, 255, 0, 0, 0, 0,
    ]),
  ));
  assert.ok(parsed);
  const destination = new Uint8ClampedArray(new ArrayBuffer(16));
  const result = frame.bgraToRgba(parsed, destination);
  assert.equal(result, destination);
  assert.deepEqual([...result], [
    1, 2, 3, 255, 4, 5, 6, 255,
    7, 8, 9, 255, 10, 11, 12, 255,
  ]);
});

test("vision event label expires after five seconds", () => {
  assert.equal(state.expireVisionEvent("enemy_missing", 20_000, 25_000), "enemy_missing");
  assert.equal(state.expireVisionEvent("enemy_missing", 20_000, 25_001), null);
});

test("risk source tags use the specified labels", () => {
  const known = state.parseRiskState(riskPayload());
  assert.ok(known);
  assert.equal(state.riskTag(known), "GSI + map");
  const gsiOnly = state.parseRiskState(riskPayload({ vision_source: false }));
  assert.ok(gsiOnly);
  assert.equal(state.riskTag(gsiOnly), "GSI only");
  const noGsi = state.parseRiskState(riskPayload({ gsi_source: false }));
  assert.ok(noGsi);
  assert.equal(state.riskTag(noGsi), "—");
});

test("meter labels match Thai and English levels and level zero stays grey", () => {
  assert.equal(meter.dangerLabel(null, "th"), "—");
  assert.equal(meter.dangerLabel(null, "en"), "—");
  assert.deepEqual([0, 1, 2, 3].map((level) => meter.dangerLabel(level, "th")), [
    "ไม่พบสัญญาณ", "ระวัง", "เสี่ยง", "อันตราย",
  ]);
  assert.deepEqual([0, 1, 2, 3].map((level) => meter.dangerLabel(level, "en")), [
    "No signal", "Caution", "Risk", "Danger",
  ]);
  assert.equal(meter.dangerTone(0), "none");
  assert.deepEqual(meter.litDangerLeds(0), [0]);
  assert.deepEqual(meter.litDangerLeds(null), []);
  const styles = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  assert.match(styles, /\.live-danger-led-none\s*\{[^}]*background:\s*var\(--stale\)/);
  assert.doesNotMatch(styles, /\.live-danger-led-none\s*\{[^}]*green/i);
});

test("nullable values display as an em dash", () => {
  assert.equal(state.formatNullable(null), "—");
  assert.equal(state.formatMegabytes(null), "—");
  assert.equal(state.formatNullable(0), "0");
  const allUnknown = state.parseResourceMetrics(resourcePayload({
    working_set_bytes: null,
    cpu_percent: null,
    gpu_local_bytes: null,
    gpu_group_bytes: null,
    gpu_util: null,
    capture_fps: null,
    frame_age_ms: null,
  }));
  assert.ok(allUnknown);
  for (const value of Object.values(allUnknown)) assert.equal(state.formatNullable(value), "—");
});

test("resource peak and mean ignore null samples and stay unknown when all values are null", () => {
  const initial = state.emptyResourceSummary();
  const first = state.parseResourceMetrics(resourcePayload({ working_set_bytes: 100, cpu_percent: 10, capture_fps: 30 }));
  assert.ok(first);
  const firstSummary = state.addResourceSample(initial, first);
  const second = state.parseResourceMetrics(resourcePayload({
    working_set_bytes: 300,
    gpu_local_bytes: null,
    cpu_percent: null,
    capture_fps: 36,
    frame_age_ms: null,
  }));
  assert.ok(second);
  const secondSummary = state.addResourceSample(firstSummary, second);
  assert.deepEqual(secondSummary.appGroupBytes, { peak: 300, mean: 200, count: 2 });
  assert.deepEqual(secondSummary.cpuPercent, { peak: 10, mean: 10, count: 1 });
  assert.deepEqual(secondSummary.captureFps, { peak: 36, mean: 33, count: 2 });
  assert.deepEqual(secondSummary.hostGpuBytes, { peak: 500_000, mean: 500_000, count: 1 });

  const allNull = state.parseResourceMetrics(resourcePayload({
    working_set_bytes: null,
    cpu_percent: null,
    gpu_local_bytes: null,
    gpu_group_bytes: null,
    gpu_util: null,
    capture_fps: null,
    frame_age_ms: null,
  }));
  assert.ok(allNull);
  const nullSummary = state.addResourceSample(initial, allNull);
  for (const metric of Object.values(nullSummary)) {
    assert.equal(metric.peak, null);
    assert.equal(metric.mean, null);
    assert.equal(metric.count, 0);
  }
  assert.equal(state.formatNullable(nullSummary.webviewGpuBytes.mean), "—");
});

test("resource bytes format in decimal MB with one decimal place", () => {
  assert.equal(state.formatMegabytes(1_234_567), "1.2 MB");
  assert.equal(state.formatMegabytes(1_000_000), "1.0 MB");
});

test("resource samples become unknown after 3000 ms and require every field", () => {
  const sample = state.parseResourceMetrics(resourcePayload());
  assert.ok(sample);
  assert.equal(state.freshResourceMetrics(sample, 0, 3000)?.appGroupBytes, 1_000_000);
  assert.equal(state.freshResourceMetrics(sample, 0, 3001), null);
  assert.equal(state.parseResourceMetrics({ working_set_bytes: 1_000_000, cpu_percent: 2.5 }), null);
  assert.equal(state.parseResourceMetrics(resourcePayload({ gpu_util: "unknown" })), null);
});
