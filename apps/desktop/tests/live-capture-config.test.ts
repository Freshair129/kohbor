import assert from "node:assert/strict";
import { test } from "node:test";

const config = await import("../src/live/captureConfig.ts");
const state = await import("../src/live/state.ts");
const copy = await import("../src/live/copy.ts");

const profile = { x: 0, y: 788, width: 292, height: 292 };

function makeConfig(overrides: Record<string, unknown> = {}) {
  return {
    schema_version: 1,
    crop: { dx: 0, dy: 0, dw: 0, dh: 0 },
    display_fps: 15,
    paused: false,
    profile_rect: profile,
    effective_rect: profile,
    proposal: null,
    proposal_status: "none",
    crop_confirmed: false,
    ...overrides,
  };
}

test("capture config parser accepts extra fields and rejects unknown or malformed contracts", () => {
  assert.deepEqual(config.parseCaptureConfig({ ...makeConfig(), future_field: "ignored" }), makeConfig());
  assert.equal(config.validateCaptureConfig(makeConfig()), true);
  assert.equal(config.parseCaptureConfig(null), null);
  assert.equal(config.parseCaptureConfig(makeConfig({ schema_version: 2 })), null);
  assert.equal(config.parseCaptureConfig(makeConfig({ display_fps: 24 })), null);
  assert.equal(config.parseCaptureConfig(makeConfig({ paused: "false" })), null);
  assert.equal(config.parseCaptureConfig(makeConfig({ crop: { dx: 0, dy: 0, dw: "0", dh: 0 } })), null);
  assert.equal(config.parseCaptureConfig(makeConfig({ crop: { dx: 201, dy: 0, dw: 0, dh: 0 } })), null);
  assert.equal(config.parseCaptureConfig(makeConfig({ effective_rect: { x: 0, y: 788, width: 63, height: 292 } })), null);
  assert.equal(config.parseCaptureConfig(makeConfig({ proposal_status: "ready" })), null);
  assert.equal(config.validateCaptureConfig(makeConfig({ crop_confirmed: "yes" })), false);
});

test("crop nudge returns a patch and clamps movement to the contract offsets", () => {
  assert.deepEqual(config.nudge(12, -8, { dx: -1, dy: 5 }), { crop: { dx: 11, dy: -3 } });
  assert.deepEqual(config.nudge(199, -199, { dx: 5, dy: -5 }), { crop: { dx: 200, dy: -200 } });
});

test("grow and shrink clamp dimensions within the effective and relative-offset bounds", () => {
  const atMaximum = makeConfig({ crop: { dx: 0, dy: 0, dw: 200, dh: 200 } });
  const smallProfile = { x: 0, y: 788, width: 200, height: 200 };
  const atMinimum = makeConfig({
    crop: { dx: 0, dy: 0, dw: -136, dh: -136 },
    profile_rect: smallProfile,
    effective_rect: { x: 0, y: 788, width: 64, height: 64 },
  });
  assert.deepEqual(config.grow(atMaximum, 2), { crop: { dw: 108, dh: 108 } });
  assert.deepEqual(config.shrink(atMinimum, 2), { crop: { dw: -136, dh: -136 } });
  assert.equal(config.grow(makeConfig({ profile_rect: null }), 2), null);
});

test("proposal acceptance uses the profile origin and dimensions and rejects out-of-contract proposals", () => {
  assert.deepEqual(
    config.applyProposal({ x: 10, y: 800, width: 290, height: 300 }, profile),
    { crop: { dx: 10, dy: 12, dw: -2, dh: 8 } },
  );
  assert.deepEqual(
    config.applyProposal({ x: -10, y: 800, width: 290, height: 300 }, { ...profile, x: -20 }),
    { crop: { dx: 10, dy: 12, dw: -2, dh: 8 } },
  );
  assert.equal(config.applyProposal({ x: 300, y: 800, width: 290, height: 300 }, profile), null);
  assert.equal(config.applyProposal({ x: 10, y: 800, width: 60, height: 300 }, profile), null);
  assert.equal(config.applyProposal({ x: 10, y: 800, width: 290, height: 300 }, null), null);
});

test("capture rates and pause transitions expose only the supported choices", () => {
  assert.deepEqual(config.CAPTURE_DISPLAY_RATES, [10, 15, 30]);
  assert.deepEqual(config.togglePause(false), { paused: true });
  assert.deepEqual(config.togglePause(true), { paused: false });
  assert.equal(config.isCapturePaused(true, false), true);
  assert.equal(config.isCapturePaused(false, true), true);
  assert.equal(config.isCapturePaused(false, false), false);
  assert.equal(config.capturePictureEnabled("live", false), true);
  assert.equal(config.capturePictureEnabled("stale", false), true);
  assert.equal(config.capturePictureEnabled("live", true), false);
  assert.equal(config.capturePictureEnabled("stale", true), false);
  assert.equal(copy.liveCopy("th").pauseCapture, "หยุดจับภาพ");
  assert.equal(copy.liveCopy("th").resumeCapture, "เริ่มจับภาพต่อ");
  assert.equal(copy.liveCopy("en").pausedCapture, "Capture paused");
});

test("capture status confirmation and capture-cost rows preserve unknown values", () => {
  const summary = state.emptyResourceSummary();
  const rows = state.captureCostRows(null, summary);
  assert.deepEqual(rows, [
    { key: "grabMsMean", current: null, peak: null, mean: null },
    { key: "analysisMsMean", current: null, peak: null, mean: null },
  ]);
  assert.equal(state.formatNullable(rows[0]!.current, 1), "—");

  const unconfirmed = state.parseCaptureStatus({
    phase: "live", fps_measured: 30, frame_age_ms: 20, rect_profile: "p1080-default-v0",
    calibration_generation: 1, auto_confirmed: true, crop_confirmed: false,
    grab_ms_mean: 1.25, analysis_ms_mean: 4.5,
  });
  assert.ok(unconfirmed);
  assert.equal(state.showAutoLayoutUnconfirmed(unconfirmed), true);
  const confirmed = state.parseCaptureStatus({
    phase: "live", fps_measured: 30, frame_age_ms: 20, rect_profile: "p1080-default-v0",
    calibration_generation: 1, auto_confirmed: true, crop_confirmed: true,
    grab_ms_mean: null, analysis_ms_mean: null,
  });
  assert.ok(confirmed);
  assert.equal(state.showAutoLayoutUnconfirmed(confirmed), false);
  assert.equal(state.parseCaptureStatus({
    phase: "live", fps_measured: 30, frame_age_ms: 20, rect_profile: "p1080-default-v0",
    calibration_generation: 1, auto_confirmed: true, crop_confirmed: null,
  }), null);
  assert.equal(state.parseCaptureStatus({
    phase: "live", fps_measured: 30, frame_age_ms: 20, rect_profile: "p1080-default-v0",
    calibration_generation: 1, auto_confirmed: true, crop_confirmed: false, paused: "false",
  }), null);
  assert.deepEqual(state.captureCostRows(unconfirmed, {
    ...summary,
    grabMsMean: { peak: 2, mean: 1.5, count: 2 },
    analysisMsMean: { peak: 8, mean: 5, count: 2 },
  }), [
    { key: "grabMsMean", current: 1.25, peak: 2, mean: 1.5 },
    { key: "analysisMsMean", current: 4.5, peak: 8, mean: 5 },
  ]);
});
