import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const model = await import("../src/orb/orbModel.ts");
const {
  appLabel, channelById, channelLabel, chargeCue, channelNote, controllable, faderPercent, focusTargets, HIDDEN_ORB, MAX_SLOTS,
  moveSlot, panelCssWidth, parseMixer, parseMixerReply, parseOrbPrefs, parseOrbState, reasonText, setSlot, slotChoices, slotKey,
  valueText, wheelStep,
} = model;

const layout = { disc: 52, dead: 8, cy: 324, out_cx: 34, hot: { x: 8, y: 290, w: 28, h: 68 }, panel_x: 72, panel_w: 231, sp: 1, monitor_h: 1080 };

function state(overrides: Record<string, unknown> = {}) {
  return { visible: true, phase: "tucked", reason: null, dwell_ms: 1000, charge_ms: null, locked: false, panel: null, layout, ...overrides };
}

const game = { id: "game", kind: "game", state: "present", exe: "dota2.exe", percent: 80, muted: false, range: null };
const kohbor = { id: "kohbor", kind: "kohbor", state: "present", exe: null, percent: 60, muted: null, range: null };
const music = { id: "music", kind: "music", state: "present", exe: "chrome.exe", percent: 65, muted: false, range: null };
const discord = { id: "app:discord.exe", kind: "app", state: "present", exe: "discord.exe", percent: 100, muted: false, range: null };

function mixer(overrides: Record<string, unknown> = {}) {
  return {
    available: true, reason: null,
    channels: [game, kohbor, music],
    slots: [{ kind: "kohbor" }, { kind: "music", exe: null }],
    app_choices: ["chrome.exe", "discord.exe", "spotify.exe"], music_selected: null,
    focus: { active: false, percent: 20, channels: [] }, restore_pending: 0, touched: 0,
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

test("mixer replies parse in slot order and missing or unreadable sessions are never controllable", () => {
  const reply = parseMixerReply({ ok: true, code: null, mixer: mixer({ channels: [game, discord, kohbor, music] }) });
  assert.ok(reply);
  assert.deepEqual(reply.mixer.channels.map((c) => c.id), ["game", "app:discord.exe", "kohbor", "music"]);
  assert.equal(controllable(channelById(reply.mixer, "game")), true);
  assert.equal(valueText(channelById(reply.mixer, "music"), "th"), "65%");
  const missingGame = { ...game, state: "missing", percent: 40 };
  const missing = parseMixer(mixer({ channels: [missingGame, kohbor, music] }));
  assert.ok(missing);
  assert.equal(missing.channels[0]?.percent, null, "a missing session never carries a level");
  assert.equal(controllable(missing.channels[0]), false);
  assert.equal(valueText(missing.channels[0] ?? null, "th"), "—");
  assert.deepEqual(channelNote(missing, "th"), ["ยังไม่พบเสียงของ Dota", "เกมยังไม่ได้เล่นเสียง จึงยังปรับไม่ได้"]);
  assert.equal(parseMixer(mixer({ channels: [{ ...game, percent: null }] })), null, "a present channel without a real level is rejected");
  assert.equal(parseMixerReply({ ok: true, mixer: { available: true } }), null);
});

test("the channel list is one to four rows, Game first, each row once", () => {
  assert.ok(parseMixer(mixer({ channels: [game] })), "Game alone");
  assert.equal(parseMixer(mixer({ channels: [] })), null);
  assert.equal(parseMixer(mixer({ channels: [kohbor, game] })), null, "Game must be first");
  assert.equal(parseMixer(mixer({ channels: [game, music, music] })), null, "no duplicates");
  const fifth = { ...discord, id: "app:spotify.exe", exe: "spotify.exe" };
  assert.equal(parseMixer(mixer({ channels: [game, kohbor, music, discord, fifth] })), null, "never more than four");
  assert.equal(parseMixer(mixer({ channels: [game, { ...discord, id: "app:../x.exe" }] })), null, "app ids are exe names");
  assert.equal(parseMixer(mixer({ channels: [game, { ...music, id: "app:chrome.exe" }] })), null, "id matches the kind");
});

test("an app with differing sessions shows its range and stays controllable without an invented value", () => {
  const mixed = { ...discord, state: "mixed", percent: 55, muted: null, range: [30, 100] };
  const view = parseMixer(mixer({ channels: [game, kohbor, music, mixed] }));
  assert.ok(view);
  const reading = channelById(view, "app:discord.exe");
  assert.equal(reading?.percent, null, "no single level for a mixed app");
  assert.deepEqual(reading?.range, [30, 100]);
  assert.equal(controllable(reading), true);
  assert.equal(valueText(reading, "en"), "30–100%");
  assert.equal(parseMixer(mixer({ channels: [game, { ...mixed, range: [90, 10] }] })), null);
  assert.equal(parseMixer(mixer({ channels: [game, { ...mixed, range: null }] })), null);
});

test("an added app without a session is explained honestly", () => {
  const gone = { ...discord, state: "missing", percent: null, muted: null };
  const view = parseMixer(mixer({ channels: [game, kohbor, music, gone] }));
  assert.ok(view);
  assert.equal(controllable(channelById(view, "app:discord.exe")), false);
  assert.deepEqual(channelNote(view, "th"), ["ยังไม่พบเสียงของ Discord", "เปิดแอปให้เล่นเสียงก่อน จึงจะปรับได้"]);
  assert.deepEqual(channelLabel(gone as never, "th"), { name: "Discord", sub: "แอป" });
});

test("Focus targets music and added apps, never Game or the Kohbor announcer", () => {
  const view = parseMixer(mixer({ channels: [game, kohbor, music, discord] }));
  assert.deepEqual(focusTargets(view).map((c) => c.id), ["music", "app:discord.exe"]);
  assert.deepEqual(focusTargets(parseMixer(mixer({ channels: [game, kohbor] }))), []);
});

test("muted values and unknown kohbor mute are shown honestly", () => {
  const view = parseMixer(mixer({ channels: [game, kohbor, { ...music, exe: "spotify.exe", percent: 30, muted: true }] }));
  assert.ok(view);
  assert.equal(valueText(channelById(view, "music"), "th"), "ปิด");
  assert.equal(channelById(view, "kohbor")?.muted, null);
  assert.equal(channelNote(parseMixer(mixer({ available: false })), "en")?.[0], "Windows audio is not ready");
});

test("panel width grows only for the fourth channel", () => {
  assert.equal(panelCssWidth(1), 154);
  assert.equal(panelCssWidth(2), 154);
  assert.equal(panelCssWidth(3), 220);
  assert.equal(panelCssWidth(4), 288);
  assert.equal(panelCssWidth(9), 288);
  // The host sizes the window for the widest panel: 288 CSS px at the 80 % zoom is 230.4 logical px (231 at 1080p).
  assert.ok(288 * 0.8 <= layout.panel_w);
});

test("slot editing keeps Game fixed, at most three rows and no duplicates", () => {
  const start = [{ kind: "kohbor" }, { kind: "music", exe: "spotify.exe" }] as const;
  const added = setSlot([...start], 2, "app:discord.exe");
  assert.deepEqual(added.map(slotKey), ["kohbor", "music", "app:discord.exe"]);
  assert.equal(setSlot(added, 3, "app:x.exe").length, MAX_SLOTS, "a fourth slot (fifth channel) is never added");
  assert.deepEqual(slotChoices(added, 2, ["chrome.exe", "discord.exe", "spotify.exe", "dota2.exe"]),
    ["", "app:chrome.exe", "app:discord.exe"], "other rows' choices, Dota and the Music app are not offered");
  const swapped = moveSlot(added, 2, -1);
  assert.deepEqual(swapped.map(slotKey), ["kohbor", "app:discord.exe", "music"]);
  assert.deepEqual(swapped[2], { kind: "music", exe: "spotify.exe" }, "Music keeps its app when moved");
  assert.deepEqual(moveSlot(added, 0, -1).map(slotKey), added.map(slotKey));
  const removed = setSlot(added, 0, "");
  assert.deepEqual(removed.map(slotKey), ["music", "app:discord.exe"]);
  assert.deepEqual(setSlot(removed, 0, "music")[0], { kind: "music", exe: "spotify.exe" });
});

test("labels and failure reasons are readable", () => {
  assert.equal(appLabel("msedge.exe"), "Edge");
  assert.equal(appLabel("vlc.exe"), "Vlc");
  assert.equal(appLabel(null), "—");
  assert.match(reasonText("MIXER_SET_FAILED", "th") ?? "", /ค่าเดิม/);
  assert.match(reasonText("ORB_SLOTS_INVALID", "th") ?? "", /4 แถว/);
  assert.match(reasonText("SOMETHING_NEW", "en") ?? "", /SOMETHING_NEW/);
  assert.equal(reasonText(null, "en"), null);
});

test("orb prefs parse with slots and status", () => {
  const prefs = parseOrbPrefs({ enabled: false, dwell_ms: 1000, focus_percent: 20, slots: [{ kind: "kohbor" }, { kind: "music", exe: null }],
    max_slots: 3, focus_hotkey_available: false,
    status: { running: false, phase: null, reason: null, failed_closed: false, interactions: 0, activations: 0, start_error: null, windows_only: false } });
  assert.ok(prefs);
  assert.equal(prefs.enabled, false);
  assert.equal(prefs.slots.length, 2);
  assert.equal(prefs.focus_hotkey_available, false);
  assert.equal(parseOrbPrefs({ enabled: "yes" }), null);
  assert.equal(parseOrbPrefs({ enabled: true, dwell_ms: 1000, focus_percent: 20, slots: [{ kind: "game" }] }), null);
  assert.equal(parseOrbPrefs({ enabled: true, dwell_ms: 1000, focus_percent: 20, music_exe: null }), null, "schema 1 shape is not accepted");
});

test("orb window controls never take keyboard focus", () => {
  const stage = readFileSync(new URL("../src/orb/OrbStage.tsx", import.meta.url), "utf8");
  const buttons = stage.match(/<button /g)?.length ?? 0;
  const unfocusable = stage.match(/tabIndex=\{-1\}/g)?.length ?? 0;
  assert.ok(buttons > 0 && unfocusable >= buttons + 1, `${unfocusable} tabIndex -1 for ${buttons} buttons and the fader`);
  assert.ok(!/autoFocus|current\??\.focus\(/.test(stage));
  assert.ok(!/addEventListener\("key/.test(stage), "the orb never listens to the keyboard");
});
