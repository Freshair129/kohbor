import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";
import react from "@vitejs/plugin-react";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  overlayChipForDisplay,
  chipForStatus,
  createChipState,
  reduceChip,
  overlayShouldShow,
  criticalStackGeometry,
  criticalStripText,
  CHIP_SHOW_MS,
  DEMO_MARKER_WINDOW_MS,
  type ChipView,
} from "../src/overlayModel.ts";
import { createPlaybackState, reducePlayback, tickPlayback, type PlaybackState } from "../src/playback.ts";
const desktopRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let EventBanner: (props: { playback: PlaybackState; visible?: boolean }) => unknown;
let OverlayStage: (props: { playback: PlaybackState; chip: ChipView | null; visible: boolean }) => unknown;
let ObjectiveAlert: (props: { batch: unknown; locale: "th" | "en"; visible: boolean }) => unknown;
let closeVite: (() => Promise<void>) | undefined;

before(async () => {
  const server = await createServer({
    configFile: false,
    plugins: [react()],
    root: desktopRoot,
    appType: "custom",
    resolve: { preserveSymlinks: true },
    optimizeDeps: { noDiscovery: true, include: [] },
    server: { middlewareMode: true },
  });
  closeVite = () => server.close();
  ({ EventBanner } = await server.ssrLoadModule("/src/EventBanner.tsx"));
  ({ OverlayStage } = await server.ssrLoadModule("/src/OverlayStage.tsx"));
  ({ ObjectiveAlert } = await server.ssrLoadModule("/src/ObjectiveAlert.tsx"));
});

after(async () => {
  await closeVite?.();
});
const ready = { state: "ready", reason: null } as const;
const muted = { state: "unavailable", reason: "AUDIO_MUTED" } as const;
const missing = { state: "not_ready", reason: "MEDIA_UNMAPPED" } as const;
const empty = { view: null, shownUntilMs: null, lastStatusKey: null, demoMarkerAtMs: null };
const status = (state: any, media: any, now_ms: number) => reduceChip(state, { type: "status", media, now_ms });
const started = (id: string, extra: object = {}) => ({ kind: "started", playback_id: id, class: "ordinary", event_type: "multi_kill", tier: 2, visual_url: "kohbor-media://pack/video.webm", audio_duration_ms: 1200, visual_tail_ms: 200, at_ms: 1000, ...extra });
const activeBannerPlayback = () => reducePlayback(createPlaybackState(), started("pb_00000001", {
  visual_url: "kohbor-media://localhost/v1/ins_0000000001/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
}));
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
test("chip: playback_ended clears the demo label for its playback", () => {
  let s = reduceChip(createChipState(), { type: "demo_marker", now_ms: 100 });
  s = reduceChip(s, { type: "playback_started", playback_id: "pb_00000001", now_ms: 200 });
  assert.equal(s.view?.kind, "demo");
  s = reduceChip(s, { type: "playback_ended", playback_id: "pb_00000001", now_ms: 500 });
  assert.equal(s.view, null);
  assert.equal(s.demoMarkerAtMs, null);
});
test("chip: playback_ended for an older id leaves the current demo label", () => {
  let s = reduceChip(createChipState(), { type: "demo_marker", now_ms: 100 });
  s = reduceChip(s, { type: "playback_started", playback_id: "pb_00000001", now_ms: 200 });
  s = reduceChip(s, { type: "demo_marker", now_ms: 300 });
  s = reduceChip(s, { type: "playback_started", playback_id: "pb_00000002", now_ms: 400 });
  const current = s;
  s = reduceChip(s, { type: "playback_ended", playback_id: "pb_00000001", now_ms: 500 });
  assert.strictEqual(s, current);
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

test("critical stack geometry scales the gap, strip and 32 px limit with the banner", () => {
  for (const monitor of [
    { x: 0, y: 0, width: 1920, height: 1080 },
    { x: 0, y: 0, width: 2560, height: 1440 },
  ]) {
    const geometry = criticalStackGeometry(monitor);
    assert.ok(geometry);
    const expectedScale = monitor.height / 1080;
    const bannerBottom = geometry.banner.y + geometry.banner.height;
    const stripBottom = geometry.strip.y + geometry.strip.height;

    assert.deepEqual([geometry.stage.width, geometry.stage.height], monitor.width === 1920 ? [306, 94] : [408, 125]);
    assert.equal(geometry.strip.x, geometry.banner.x);
    assert.equal(geometry.strip.width, geometry.banner.width);
    assert.ok(Math.abs(geometry.strip.y - bannerBottom - 4 * expectedScale) < 0.1);
    assert.ok(Math.abs(geometry.strip.height - 40 * expectedScale) < 0.1);
    assert.ok(stripBottom <= geometry.stripLimitBottom);
    assert.ok(geometry.window.y + geometry.window.height <= geometry.stripLimitBottom);
  }
});

test("critical strip copy stays Thai and event-specific", () => {
  assert.equal(criticalStripText("health_danger"), "เลือดน้อย! ถอยก่อน");
  assert.equal(criticalStripText("risk_alert"), "อันตราย! ระวังตัว");
});

test("renderer keeps the yielding banner video and renders the critical strip above it", () => {
  const banner = reducePlayback(createPlaybackState(), started("pb_00000001", {
    event_type: "hero_kill",
    visual_url: "kohbor-media://localhost/v1/ins_0000000001/" + "a".repeat(64),
    at_ms: 1000,
  }));
  const stacked = reducePlayback(banner, started("pb_00000002", {
    class: "critical",
    event_type: "health_danger",
    visual_url: "kohbor-media://localhost/v1/ins_0000000002/" + "b".repeat(64),
    at_ms: 1100,
  }));
  const html = renderToStaticMarkup(createElement(OverlayStage, {
    playback: stacked,
    chip: null,
    visible: true,
  }));

  assert.match(html, /event-overlay-banner is-yielding/);
  assert.match(html, /critical-strip/);
  assert.match(html, /เลือดน้อย! ถอยก่อน/);
  assert.match(html, /a{64}/);
  assert.doesNotMatch(html, /b{64}/);
});

test("the critical strip keeps overlay visibility after the yielded video leaves", () => {
  let playback = reducePlayback(createPlaybackState(), started("pb_00000001", {
    event_type: "hero_kill",
    audio_duration_ms: 1000,
    visual_tail_ms: 0,
    at_ms: 1000,
  }));
  playback = reducePlayback(playback, started("pb_00000002", {
    class: "critical",
    event_type: "risk_alert",
    audio_duration_ms: 5000,
    visual_tail_ms: 100,
    at_ms: 1100,
  }));
  playback = tickPlayback(playback, 2500);
  playback = tickPlayback(playback, 2680);

  assert.equal(playback.visual, null);
  assert.notEqual(playback.criticalStrip, null);
  assert.equal(overlayShouldShow(playback, empty, 2680), true);
});

test("overlay display: the benign ready chip is never shown, problem states and DEMO still are", () => {
  assert.equal(overlayChipForDisplay(null), null);
  assert.equal(overlayChipForDisplay(chipForStatus(ready)), null);
  for (const media of [missing, muted]) {
    const view = chipForStatus(media);
    assert.equal(overlayChipForDisplay(view), view);
  }
  const unknown = chipForStatus(null);
  assert.equal(overlayChipForDisplay(unknown), unknown);
  const demo = { kind: "demo", reason: null, text_th: "DEMO" } as const;
  assert.equal(overlayChipForDisplay(demo), demo);
});

test("renderer idle overlay renders no banner element", () => {
  const html = renderToStaticMarkup(createElement(OverlayStage, {
    playback: activeBannerPlayback(),
    chip: null,
    visible: false,
  }));
  assert.doesNotMatch(html, /event-overlay-banner|<video\b/);
  assert.equal(html, "");
});

test("renderer hides banner content when the host visibility gate is closed", () => {
  const html = renderToStaticMarkup(
    createElement(EventBanner, { playback: activeBannerPlayback(), visible: false }),
  );
  assert.equal(html, "");
});

test("objective alert text is rendered in the HUD card, outside the video banner", () => {
  const html = renderToStaticMarkup(createElement(OverlayStage, {
    playback: activeBannerPlayback(),
    chip: null,
    visible: true,
  }));
  assert.match(html, /event-overlay-banner/);
  assert.match(html, /<video\b/);
  assert.doesNotMatch(html, /objective-card|Water rune/);
  const objectives = [
    ["water_rune", "Water rune"],
    ["power_rune", "Power rune"],
    ["bounty_rune", "Bounty rune"],
    ["wisdom_rune", "Wisdom rune"],
    ["tormentor", "Tormentor"],
  ] as const;
  for (const [objective, name] of objectives) {
    const prewarn = {
      schema_version: 1,
      observed_clock_seconds: 120,
      alerts: [{ objective, phase: "pre_warn", lead_s: 30, occurrence_seconds: 150, due_seconds: 120 }],
    };
    const thaiCard = renderToStaticMarkup(createElement(ObjectiveAlert, { batch: prewarn, locale: "th", visible: true }));
    const englishCard = renderToStaticMarkup(createElement(ObjectiveAlert, { batch: prewarn, locale: "en", visible: true }));
    assert.match(thaiCard, new RegExp(`objective-card-${objective}`));
    assert.match(thaiCard, /<svg/);
    assert.match(thaiCard, new RegExp(`${name} จะเกิดใน 30 วินาที`));
    assert.match(englishCard, new RegExp(`${name} spawns in 30 s`));
    assert.doesNotMatch(englishCard, /[\u0e00-\u0e7f]/u);

    const spawn = {
      schema_version: 1,
      observed_clock_seconds: 120,
      alerts: [{ objective, phase: "spawn", lead_s: null, occurrence_seconds: 120, due_seconds: 120 }],
    };
    const thaiSpawn = renderToStaticMarkup(createElement(ObjectiveAlert, { batch: spawn, locale: "th", visible: true }));
    const englishSpawn = renderToStaticMarkup(createElement(ObjectiveAlert, { batch: spawn, locale: "en", visible: true }));
    assert.match(thaiSpawn, new RegExp(`${name} เกิดแล้ว`));
    assert.match(englishSpawn, new RegExp(`${name} spawned`));
    assert.doesNotMatch(englishSpawn, /[\u0e00-\u0e7f]/u);
  }

  const merged = renderToStaticMarkup(createElement(ObjectiveAlert, {
    batch: {
      schema_version: 1,
      observed_clock_seconds: 120,
      alerts: [
        { objective: "water_rune", phase: "pre_warn", lead_s: 30, occurrence_seconds: 150, due_seconds: 120 },
        { objective: "power_rune", phase: "spawn", lead_s: null, occurrence_seconds: 120, due_seconds: 120 },
      ],
    },
    locale: "th",
    visible: true,
  }));
  assert.match(merged, /Water rune จะเกิดใน 30 วินาที/);
  assert.match(merged, /\+1/);
});
