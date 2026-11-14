import assert from "node:assert/strict";
import { test } from "node:test";
import { deriveCaptureDisplay } from "../src/live/captureDisplay.ts";
import { parseCaptureStatus } from "../src/live/state.ts";
import { unknownPreviewMetrics, previewRateText } from "../src/live/preview.ts";
import { capturePictureEnabled } from "../src/live/captureConfig.ts";
import { phaseReasonText } from "../src/live/captureRecovery.ts";

const status = (phase: string) => parseCaptureStatus({ phase, phase_reason: "waiting_for_game", fps_measured: 15,
  frame_age_ms: 32, calibration_generation: 7, rect_profile: "observed", auto_confirmed: false,
  grab_fps: 30, sent_total: 500, grab_total: 1000 });

test("idle display masks sent, received and painted counters without resetting real measurements", () => {
  const capture = status("idle")!;
  const metrics = { ...unknownPreviewMetrics(), received_total: 480, painted_total: 470, received_fps: 15, painted_fps: 14 };
  const display = deriveCaptureDisplay(capture, metrics);
  assert.equal(display.capture?.sent_total, null); assert.equal(display.capture?.grab_total, null);
  assert.equal(display.metrics.received_total, null); assert.equal(display.metrics.painted_total, null);
  assert.equal(previewRateText(display.capture?.fps_measured ?? null, display.metrics, "th"), "ส่ง — · รับ — · วาด — fps");
  assert.equal(capture.sent_total, 500); assert.equal(metrics.received_total, 480);
  assert.equal(capturePictureEnabled("idle", false), false);
});

test("active capture keeps genuine zero counters and an idle missing reason stays unknown", () => {
  const capture = status("live")!;
  const display = deriveCaptureDisplay(capture, unknownPreviewMetrics());
  assert.equal(display.capture, capture); assert.equal(display.metrics.painted_total, 0);
  assert.equal(phaseReasonText("idle", null, "th"), "รอเกม — ยังไม่ได้รับเหตุผลจากระบบหลัก");
});

test("missing capture status and inactive previews never turn initial zero counters into measurements", () => {
  for (const capture of [null, status("unsupported"), status("calibrating")]) {
    const display = deriveCaptureDisplay(capture, unknownPreviewMetrics());
    assert.equal(display.metrics.received_total, null);
    assert.equal(display.metrics.painted_total, null);
  }
});
