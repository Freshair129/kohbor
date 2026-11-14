import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { parseCaptureStatus } from "../src/live/state.ts";
import { phaseReasonText } from "../src/live/captureRecovery.ts";
import { deriveMinimapPresentation } from "../src/live/minimapPresentation.ts";
import { capturePictureEnabled } from "../src/live/captureConfig.ts";
import { derivePreviewDiagnostics } from "../src/live/previewDiagnostics.ts";
import { unknownPreviewMetrics } from "../src/live/preview.ts";
import { deriveDeckState, type DeckStateInput } from "../src/deck/deckState.ts";
import { deriveHeaderState } from "../src/shell/state.ts";
import { copy } from "../src/copy.ts";

const payload = { phase: "stale", phase_reason: "occluded", paused: false, fps_measured: 0,
  frame_age_ms: 3200, calibration_generation: 7, rect_profile: "observed", auto_confirmed: false };
const th = "มีหน้าต่างอื่นทับแผนที่ (เช่น แถบงาน) — ภาพที่เห็นเป็นภาพล่าสุดก่อนถูกทับ";
const en = "Another window covers the minimap (for example the taskbar) — showing the last clean frame";

test("timeout counts accept missing/null as unknown and genuine nonnegative integer measurements", () => {
  assert.equal(parseCaptureStatus(payload)?.grab_timeouts_per_s, null);
  for (const value of [null, 0, 19]) {
    assert.equal(parseCaptureStatus({ ...payload, grab_timeouts_per_s: value })?.grab_timeouts_per_s, value);
  }
  for (const value of [-1, 0.5, "19", NaN, Infinity, false]) {
    assert.equal(parseCaptureStatus({ ...payload, grab_timeouts_per_s: value }), null);
  }
});

test("occlusion explains the held clean frame in both languages", () => {
  assert.equal(phaseReasonText("stale", "occluded", "th"), th);
  assert.equal(phaseReasonText("stale", "occluded", "en"), en);
});

test("covered picture stays enabled, dimmed and badged without an opaque stale plate", () => {
  assert.equal(capturePictureEnabled("live", false), true);
  assert.equal(capturePictureEnabled("stale", false), true);
  assert.deepEqual(deriveMinimapPresentation("stale", "occluded", true, "th"), {
    dimmed: true, coveredBadge: "ถูกบัง", staleOverlay: null,
  });
  assert.equal(deriveMinimapPresentation("stale", "occluded", true, "en").coveredBadge, "Covered");
  assert.equal(deriveMinimapPresentation("stale", "occluded", false, "th").staleOverlay, "waiting");
  assert.deepEqual(deriveMinimapPresentation("stale", null, true, "th"), {
    dimmed: true, coveredBadge: null, staleOverlay: "age",
  });
  for (const phase of ["live", "idle", "lost", "unsupported", null] as const) {
    assert.deepEqual(deriveMinimapPresentation(phase, "occluded", true, "th"), {
      dimmed: false, coveredBadge: null, staleOverlay: null,
    });
  }
});

test("occlusion is novision with fresh hero data; game staleness and debrief retain precedence", () => {
  const input: DeckStateInput = {
    runtime: { session: { phase: "live", session_epoch: 1 },
      intake: { state: "receiving", port: 3000, generation: 1, last_admitted_age_ms: 20, reason: null } },
    snapshotAvailability: "fresh", capture: parseCaptureStatus(payload),
  };
  const view = deriveDeckState(input);
  assert.equal(view.state, "novision"); assert.deepEqual(view.reason, { th, en });
  assert.equal(input.snapshotAvailability, "fresh");
  assert.equal(deriveDeckState({ ...input, snapshotAvailability: "stale" }).state, "stale");
  assert.equal(deriveDeckState({ ...input, snapshotAvailability: "unknown" }).state, "standby");
  assert.equal(deriveDeckState({ ...input, runtime: { ...input.runtime!, session: { phase: "post_game", session_epoch: 1 } },
    snapshotAvailability: "stale", hostLost: true }).state, "debrief");
});

test("covered header chip uses words and a stale tone without a running/live label", () => {
  for (const locale of ["th", "en"] as const) {
    const text = copy(locale);
    const labels = { phases: text.phases, staleSuffix: text.readiness.staleSuffix,
      capturePhases: text.shell.capturePhases, capturePaused: text.shell.capturePaused,
      captureRunning: text.shell.captureRunning, captureCovered: text.shell.captureCovered };
    assert.deepEqual(deriveHeaderState(null, parseCaptureStatus(payload), false, labels).capture,
      { label: locale === "th" ? "ถูกบัง" : "Covered", tone: "stale" });
  }
});

test("timeout diagnostic is next to host grab fps with units, unknown dash and real zero", () => {
  for (const value of [null, 0, 19]) {
    for (const locale of ["th", "en"] as const) {
      const host = parseCaptureStatus({ ...payload, grab_timeouts_per_s: value });
      const view = derivePreviewDiagnostics(host, unknownPreviewMetrics(), null, locale);
      const index = view.host.rows.findIndex(row => row.key === "grab_fps");
      assert.deepEqual(view.host.rows[index + 1], { key: "grab_timeouts_per_s", label: "grab_timeouts_per_s",
        value: value === null ? "—" : String(value), unit: locale === "th" ? "ครั้ง/วิ" : "timeouts/s" });
      assert.equal(view.host.source, locale === "th" ? "โฮสต์" : "Host");
      const idle = derivePreviewDiagnostics(host && { ...host, phase: "idle" }, unknownPreviewMetrics(), null, locale);
      assert.equal(idle.host.rows[index + 1]?.value, "—");
    }
  }
});

test("covered badge overlays the bounded stage and its canvas uses the dimming decision", () => {
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  const badge = css.match(/\.live-map-covered\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(badge, /position:\s*absolute/); assert.match(badge, /border:\s*3px double/);
  assert.match(css, /\.live-map-canvas\.is-stale\s*\{[^}]*opacity:\s*0\.42/);
  const panel = readFileSync(new URL("../src/live/MinimapPanel.tsx", import.meta.url), "utf8");
  assert.match(panel, /presentation\.dimmed\s*\?\s*" is-stale"/);
  // Round 2 wraps the retained canvas; the covered badge now belongs to that stage.
  const stage = readFileSync(new URL("../src/live/MinimapStage.tsx", import.meta.url), "utf8");
  assert.match(stage, /className="live-map-covered"/);
  assert.match(panel, /presentation\.coveredBadge/);
});
