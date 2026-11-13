import assert from "node:assert/strict";
import { test } from "node:test";
import { chipForStatus, createChipState, reduceChip, overlayShouldShow, CHIP_SHOW_MS, DEMO_MARKER_WINDOW_MS } from "../src/overlayModel.ts";
import { createPlaybackState, reducePlayback, tickPlayback } from "../src/playback.ts";
const ready = { state: "ready", reason: null } as const;
const muted = { state: "unavailable", reason: "AUDIO_MUTED" } as const;
const missing = { state: "not_ready", reason: "MEDIA_UNMAPPED" } as const;
const empty = { view: null, shownUntilMs: null, lastStatusKey: null, demoMarkerAtMs: null };
const status = (state: any, media: any, now_ms: number) => reduceChip(state, { type: "status", media, now_ms });
const started = (id: string, extra: object = {}) => ({ kind: "started", playback_id: id, class: "ordinary", event_type: "multi_kill", tier: 2, visual_url: "kohbor-media://pack/video.webm", audio_duration_ms: 1200, visual_tail_ms: 200, at_ms: 1000, ...extra });
test("chip: maps unknown, unavailable and reason honestly", () => {
  assert.equal(chipForStatus(null).kind, "unknown");
  for (const reason of [null, "OTHER"]) assert.equal(chipForStatus({ state: "unavailable", reason }).kind, "unknown");
  assert.equal(chipForStatus({ state: "unavailable", reason: "CAPABILITY_UNAVAILABLE" }).kind, "no_device");
  assert.equal(chipForStatus(missing).reason, "MEDIA_UNMAPPED");
  assert.equal(chipForStatus(missing).kind, "not_ready");
});
test("chip: muted text warns critical alerts are silent", () => {
  const view = chipForStatus(muted); assert.equal(view.kind, "muted"); assert.match(view.text_th, /เสียงเตือนสำคัญ.*ไม่ดัง/);
});
test("chip: ready and unknown text never assert safety or readiness for unknown", () => {
  assert.doesNotMatch(chipForStatus(ready).text_th, /ปลอดภัย|safe|protected/i);
  assert.doesNotMatch(chipForStatus(null).text_th, /พร้อม|ready/i);
});
test("chip: first status is visible exactly eight seconds", () => {
  assert.equal(CHIP_SHOW_MS, 8000); const s = status(createChipState(), missing, 100);
  assert.equal(s.shownUntilMs, 8100); assert.equal(overlayShouldShow(createPlaybackState(), s, 8099), true); assert.equal(overlayShouldShow(createPlaybackState(), s, 8100), false);
});
test("chip: unchanged does not reshow; changed degraded status does", () => {
  let s = status(createChipState(), missing, 0); s = status(s, missing, 9000); assert.equal(overlayShouldShow(createPlaybackState(), s, 9000), false);
  s = status(s, muted, 10000); assert.equal(overlayShouldShow(createPlaybackState(), s, 10000), true); assert.equal(s.shownUntilMs, 18000);
});
test("chip: later ready does not show alone but accompanies a banner", () => {
  let s = status(createChipState(), muted, 0); s = status(s, ready, 10000);
  assert.equal(overlayShouldShow(createPlaybackState(), s, 10000), false);
  assert.equal(overlayShouldShow(reducePlayback(createPlaybackState(), started("pb_00000001")), s, 10000), true);
});
test("chip: demo marker is inclusive at 2000ms and lasts through banner", () => {
  assert.equal(DEMO_MARKER_WINDOW_MS, 2000); let s = reduceChip(createChipState(), { type: "demo_marker", now_ms: 100 });
  s = reduceChip(s, { type: "playback_started", now_ms: 2100 }); assert.equal(s.view?.kind, "demo");
  s = reduceChip(s, { type: "tick", now_ms: 20000 }); assert.equal(s.view?.kind, "demo");
});
test("chip: stale demo marker does not label playback", () => {
  let s = reduceChip(createChipState(), { type: "demo_marker", now_ms: 100 }); s = reduceChip(s, { type: "playback_started", now_ms: 2101 }); assert.notEqual(s.view?.kind, "demo");
});
test("chip: visibility requires a banner or an unexpired chip", () => {
  const p = createPlaybackState(); assert.equal(overlayShouldShow(p, empty, 0), false);
  const chip = { ...empty, view: { kind: "muted" as const, reason: "AUDIO_MUTED", text_th: "x" }, shownUntilMs: 10 };
  assert.equal(overlayShouldShow(p, chip, 9), true); assert.equal(overlayShouldShow(p, chip, 10), false);
  assert.equal(overlayShouldShow(reducePlayback(p, started("pb_00000001")), empty, 10), true);
});
test("scheduling: start replaces immediately, stale end and duplicate ignored", () => {
  let p = reducePlayback(createPlaybackState(), started("pb_00000001"));
  p = reducePlayback(p, started("pb_00000002")); assert.equal(p.visual?.playback_id, "pb_00000002");
  const same = p; p = reducePlayback(p, { kind: "ended", playback_id: "pb_00000001", reason: "cancelled_preempted", at_ms: 1100 }); assert.strictEqual(p, same);
  assert.strictEqual(reducePlayback(p, started("pb_00000002")), p);
});
test("scheduling: voice and absent visual never show a banner", () => {
  for (const extra of [{ class: "voice" }, { visual_url: null }]) assert.equal(reducePlayback(createPlaybackState(), started("pb_00000001", extra)).visual, null);
});
test("scheduling: fail safe expires at audio plus tail plus 500", () => {
  const p = reducePlayback(createPlaybackState(), started("pb_00000001")); assert.equal(p.failSafeAtMs, 2900);
  assert.notEqual(tickPlayback(p, 2899).visual, null); assert.equal(tickPlayback(p, 2900).visual, null);
});
test("scheduling: health danger then multi kill renders in arrival order", () => {
  let p = reducePlayback(createPlaybackState(), started("pb_00000001", { class: "critical", event_type: "health_danger", tier: null })); assert.equal(p.visual?.event_type, "health_danger");
  p = reducePlayback(p, started("pb_00000002", { event_type: "multi_kill", at_ms: 1001 })); assert.equal(p.visual?.event_type, "multi_kill");
  assert.equal(overlayShouldShow(p, empty, 1001), true);
});
