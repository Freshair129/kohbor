import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const model = await import("../src/orb/orbModel.ts");
const {
  appLabel, chargeCue, channelNote, controllable, faderPercent, HIDDEN_ORB, parseMixer, parseMixerReply, parseOrbPrefs,
  parseOrbState, reasonText, valueText, wheelStep,
} = model;

const layout = { disc: 52, dead: 8, cy: 324, out_cx: 34, hot: { x: 8, y: 290, w: 28, h: 68 }, panel_x: 72, panel_w: 220, sp: 1, monitor_h: 1080 };

function state(overrides: Record<string, unknown> = {}) {
  return { visible: true, phase: "tucked", reason: null, dwell_ms: 1000, charge_ms: null, locked: false, panel: null, layout, ...overrides };
}

function mixer(overrides: Record<string, unknown> = {}) {
  return {
    available: true, reason: null,
    game: { channel: "game", state: "present", exe: "dota2.exe", percent: 80, muted: false },
    kohbor: { channel: "kohbor", state: "present", exe: null, percent: 60, muted: null },
    music: { channel: "music", state: "present", exe: "chrome.exe", percent: 65, muted: false },
    music_choices: ["chrome.exe", "spotify.exe"], music_selected: null,
    focus: { active: false, percent: 20, prior_percent: null }, restore_pending: 0, touched: 0,
    ...overrides,
  };
}

test("orb state parses and malformed payloads hide the orb", () => {
  assert.equal(parseOrbState(state()).phase, "tucked");
  assert.deepEqual(parseOrbState(null), HIDDEN_ORB);
  assert.deepEqual(parseOrbState(state({ phase: "peek" })), HIDDEN_ORB);
  assert.deepEqual(parseOrbState(state({ layout: null })), HIDDEN_ORB, "visible without geometry is not drawn");
  assert.equal(parseOrbState(state({ visible: false, phase: "hidden", reason: "not_in_match", layout: null })).visible, false);
});

test("charge cue grows 80 to 100 percent over the dwell and holds in reduced motion", () => {
  const charging = parseOrbState(state({ phase: "charging", charge_ms: 250 }));
  assert.deepEqual(chargeCue(charging, 250, false), { scale: 0.9, opacity: 0.8, progress: 0.5 });
  assert.equal(chargeCue(charging, 5000, false).scale, 1);
  assert.equal(chargeCue(charging, 250, true).scale, 0.8, "reduced motion: no scale");
  assert.equal(chargeCue(charging, 250, true).progress, 0.5, "the ring still fills");
  assert.equal(chargeCue(parseOrbState(state()), 0, false).scale, 0.8);
  assert.equal(chargeCue(parseOrbState(state({ phase: "out" })), 0, false).scale, 1);
});

test("wheel steps 5 percent on the 5 percent grid, wheel up is louder", () => {
  assert.equal(wheelStep(65, -100), 70);
  assert.equal(wheelStep(65, 100), 60);
  assert.equal(wheelStep(63, -1), 65);
  assert.equal(wheelStep(63, 1), 60);
  assert.equal(wheelStep(100, -1), 100);
  assert.equal(wheelStep(0, 1), 0);
  assert.equal(wheelStep(40, 0), 40);
});

test("fader maps the vertical pointer position with the bottom at zero", () => {
  assert.equal(faderPercent(300, 100, 200), 0);
  assert.equal(faderPercent(100, 100, 200), 100);
  assert.equal(faderPercent(200, 100, 200), 50);
  assert.equal(faderPercent(50, 100, 200), 100);
  assert.equal(faderPercent(Number.NaN, 100, 200), 0);
  assert.equal(faderPercent(150, 100, 0), 0);
});

test("mixer replies parse and missing or unreadable sessions are never controllable", () => {
  const reply = parseMixerReply({ ok: true, code: null, mixer: mixer() });
  assert.ok(reply);
  assert.equal(controllable(reply.mixer.game), true);
  assert.equal(valueText(reply.mixer.music, "th"), "65%");
  const missing = parseMixer(mixer({ game: { channel: "game", state: "missing", exe: "dota2.exe", percent: 40, muted: false } }));
  assert.ok(missing);
  assert.equal(missing.game.percent, null, "a missing session never carries a level");
  assert.equal(controllable(missing.game), false);
  assert.equal(valueText(missing.game, "th"), "—");
  assert.deepEqual(channelNote(missing, "th"), ["ยังไม่พบเสียงของ Dota", "เกมยังไม่ได้เล่นเสียง จึงยังปรับไม่ได้"]);
  assert.equal(parseMixer(mixer({ game: { channel: "game", state: "present", exe: "dota2.exe", percent: null, muted: false } })), null,
    "a present channel without a real level is rejected");
  assert.equal(parseMixerReply({ ok: true, mixer: { available: true } }), null);
});

test("muted values and unknown kohbor mute are shown honestly", () => {
  const view = parseMixer(mixer({ music: { channel: "music", state: "present", exe: "spotify.exe", percent: 30, muted: true } }));
  assert.ok(view);
  assert.equal(valueText(view.music, "th"), "ปิด");
  assert.equal(view.kohbor.muted, null);
  assert.equal(channelNote(parseMixer(mixer({ available: false })), "en")?.[0], "Windows audio is not ready");
});

test("labels and failure reasons are readable", () => {
  assert.equal(appLabel("msedge.exe"), "Edge");
  assert.equal(appLabel("vlc.exe"), "Vlc");
  assert.equal(appLabel(null), "—");
  assert.match(reasonText("MIXER_SET_FAILED", "th") ?? "", /ค่าเดิม/);
  assert.match(reasonText("SOMETHING_NEW", "en") ?? "", /SOMETHING_NEW/);
  assert.equal(reasonText(null, "en"), null);
});

test("orb prefs parse with status", () => {
  const prefs = parseOrbPrefs({ enabled: false, dwell_ms: 1000, focus_percent: 20, music_exe: null, focus_hotkey_available: false,
    status: { running: false, phase: null, reason: null, failed_closed: false, interactions: 0, activations: 0, start_error: null, windows_only: false } });
  assert.ok(prefs);
  assert.equal(prefs.enabled, false);
  assert.equal(prefs.focus_hotkey_available, false);
  assert.equal(parseOrbPrefs({ enabled: "yes" }), null);
});

test("orb window controls never take keyboard focus", () => {
  const stage = readFileSync(new URL("../src/orb/OrbStage.tsx", import.meta.url), "utf8");
  const buttons = stage.match(/<button /g)?.length ?? 0;
  const unfocusable = stage.match(/tabIndex=\{-1\}/g)?.length ?? 0;
  assert.ok(buttons > 0 && unfocusable >= buttons + 1, `${unfocusable} tabIndex -1 for ${buttons} buttons and the fader`);
  assert.ok(!/autoFocus|current\??\.focus\(/.test(stage));
  assert.ok(!/addEventListener\("key/.test(stage), "the orb never listens to the keyboard");
});
