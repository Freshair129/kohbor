import assert from "node:assert/strict";
import { test } from "node:test";
import { deriveHostCounterRates, derivePreviewDiagnostics, previewRateTooltip } from "../src/live/previewDiagnostics.ts";
import { parseCaptureStatus } from "../src/live/state.ts";
import { unknownPreviewMetrics } from "../src/live/preview.ts";

const capture = (changes = {}) => parseCaptureStatus({ phase: "live", fps_measured: 17, grab_fps: 30,
  sent_total: 100, grab_total: 200, frame_age_ms: 32, calibration_generation: 3,
  rect_profile: "observed", auto_confirmed: false, grab_ms_mean: 1.25, analysis_ms_mean: 4.5, ...changes })!;
const observation = (at: number | null, changes = {}) => ({ receivedAt: at, value: capture(changes) });

test("diagnostic counter deltas divide by actual renderer-clock time instead of assuming one second", () => {
  assert.deepEqual(deriveHostCounterRates(observation(1000), observation(2500, { sent_total: 127, grab_total: 245 })), {
    elapsedMs: 1500, sentDelta: 27, grabDelta: 45, sentPerSecond: 18, grabPerSecond: 30,
  });
  const rates = deriveHostCounterRates(observation(1000), observation(3000, { sent_total: 100, grab_total: 240 }));
  assert.equal(rates.sentPerSecond, 0); assert.equal(rates.grabPerSecond, 20);
});

test("missing baselines, time, resets and generation changes never produce invented rates", () => {
  for (const [previous, current] of [
    [null, observation(2000)], [observation(null), observation(2000)],
    [observation(2000), observation(2000)], [observation(2001), observation(2000)],
    [observation(1000), observation(2000, { calibration_generation: 4 })],
    [observation(1000, { phase: "idle" }), observation(2000)],
  ] as const) {
    const rates = deriveHostCounterRates(previous, current);
    assert.equal(rates.sentPerSecond, null); assert.equal(rates.grabPerSecond, null);
  }
  const reset = deriveHostCounterRates(observation(1000), observation(2000, { sent_total: 1, grab_total: 201 }));
  assert.equal(reset.sentPerSecond, null); assert.equal(reset.grabPerSecond, 1);
  const partial = deriveHostCounterRates(observation(1000, { sent_total: null }), observation(2000, { grab_total: 220 }));
  assert.equal(partial.sentPerSecond, null); assert.equal(partial.grabPerSecond, 20);
});

test("view model keeps host last-second and window measurements distinct with explicit sources and units", () => {
  const host = capture({ sent_total: 119, grab_total: 230 });
  const metrics = { ...unknownPreviewMetrics(), received_total: 120, painted_total: 119, received_fps: 19.4, painted_fps: 19.4 };
  const rates = deriveHostCounterRates(observation(1000), { value: host, receivedAt: 2000 });
  const view = derivePreviewDiagnostics(host, metrics, rates, "th");
  const hostRows = Object.fromEntries(view.host.rows.map(row => [row.key, row]));
  const windowRows = Object.fromEntries(view.window.rows.map(row => [row.key, row]));
  assert.equal(view.host.source, "โฮสต์"); assert.equal(view.window.source, "หน้าต่าง");
  assert.equal(hostRows.sent_total?.value, "119"); assert.equal(hostRows.sent_total?.unit, "เฟรม");
  assert.equal(hostRows.sent_delta_per_second?.value, "19"); assert.equal(hostRows.sent_delta_per_second?.unit, "เฟรม/วิ");
  assert.equal(hostRows.grab_delta_per_second?.value, "30");
  assert.equal(hostRows.fps_measured?.value, "17");
  assert.equal(hostRows.grab_ms_mean?.unit, "ms"); assert.equal(hostRows.analysis_ms_mean?.value, "4.5");
  assert.equal(windowRows.received_total?.value, "120"); assert.equal(windowRows.painted_total?.value, "119");
  assert.equal(windowRows.received_fps?.value, "19.4"); assert.equal(windowRows.painted_fps?.value, "19.4");
  assert.equal(windowRows.status_interval_ms?.value, "1000"); assert.equal(windowRows.status_interval_ms?.unit, "ms");
  assert.match(previewRateTooltip("th"), /โฮสต์.*วินาทีล่าสุด.*รับ\/วาด.*หน้าต่าง/);
  assert.match(previewRateTooltip("en"), /host.*last-second.*window/);
});

test("idle or missing capture displays dashes while fresh genuine zeros remain zero", () => {
  const metrics = unknownPreviewMetrics();
  for (const host of [null, capture({ phase: "idle" })]) {
    const view = derivePreviewDiagnostics(host, metrics, null, "th");
    assert.ok(view.host.rows.every(row => row.value === "—"));
    assert.ok(view.window.rows.every(row => row.value === "—"));
  }
  const zero = derivePreviewDiagnostics(capture({ sent_total: 0 }), metrics, null, "th");
  assert.equal(zero.host.rows.find(row => row.key === "sent_total")?.value, "0");
  assert.equal(zero.window.rows.find(row => row.key === "received_total")?.value, "0");
});
